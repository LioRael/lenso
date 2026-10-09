import { mkdtemp, readdir, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  defaultProcessorLimits,
  ProcessorError,
  type ImageMetadata,
  type ImageProcessor,
  type ProcessorErrorCode,
  type ProcessorLimits,
  type Recipe,
} from "./processor";

const headerLimit = 4096;
const codes = new Set<ProcessorErrorCode>([
  "invalid-image",
  "unsupported-image",
  "limit-exceeded",
  "timeout",
  "cancelled",
  "unavailable",
]);

export interface BunImageProcessorOptions {
  limits?: Partial<ProcessorLimits>;
  /** Waiting requests, in addition to running children. Defaults to concurrency. */
  maxQueue?: number;
}

function validateRecipe(recipe: Recipe, limits: ProcessorLimits): void {
  const keys = ["width", "height", "fit", "format", "quality", "metadata", "animation"];
  if (
    !recipe ||
    typeof recipe !== "object" ||
    Reflect.ownKeys(recipe).some((key) => typeof key !== "string" || !keys.includes(key)) ||
    !Number.isSafeInteger(recipe.width) ||
    recipe.width < 1 ||
    !Number.isSafeInteger(recipe.height) ||
    recipe.height < 1 ||
    recipe.width > limits.maxWidth ||
    recipe.height > limits.maxHeight ||
    recipe.width * recipe.height > limits.maxPixels ||
    !["cover", "inside", "contain"].includes(recipe.fit) ||
    !["jpeg", "png", "webp"].includes(recipe.format) ||
    !Number.isSafeInteger(recipe.quality) ||
    recipe.quality < 1 ||
    recipe.quality > 100 ||
    recipe.metadata !== "strip" ||
    recipe.animation !== "reject"
  )
    throw new ProcessorError("limit-exceeded");
}

async function diskBytes(directory: string): Promise<number> {
  let bytes = 0;
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) bytes += await diskBytes(path);
    else bytes += (await stat(path)).size;
  }
  return bytes;
}

async function readBounded(stream: ReadableStream<Uint8Array>, limit: number): Promise<Uint8Array> {
  const reader = stream.getReader();
  const chunks: Uint8Array[] = [];
  let length = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      length += value.byteLength;
      if (length > limit) throw new ProcessorError("limit-exceeded");
      chunks.push(value);
    }
  } finally {
    reader.releaseLock();
  }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return bytes;
}

function response(bytes: Uint8Array): { bytes: Uint8Array; metadata: ImageMetadata } {
  const newline = bytes.indexOf(10);
  if (newline < 0 || newline > headerLimit) throw new ProcessorError("unavailable");
  let header: { code?: ProcessorErrorCode; metadata: ImageMetadata; length: number };
  try {
    header = JSON.parse(new TextDecoder().decode(bytes.subarray(0, newline)));
  } catch {
    throw new ProcessorError("unavailable");
  }
  if (header.code) throw new ProcessorError(codes.has(header.code) ? header.code : "unavailable");
  const m = header.metadata;
  if (
    !m ||
    !["jpeg", "png", "webp"].includes(m.format) ||
    m.mime !== `image/${m.format}` ||
    !Number.isSafeInteger(m.width) ||
    m.width < 1 ||
    !Number.isSafeInteger(m.height) ||
    m.height < 1 ||
    !Number.isSafeInteger(m.orientation) ||
    m.orientation < 1 ||
    m.orientation > 8 ||
    typeof m.hasAlpha !== "boolean" ||
    m.frames !== 1 ||
    header.length !== bytes.length - newline - 1
  )
    throw new ProcessorError("unavailable");
  return { metadata: m, bytes: bytes.slice(newline + 1) };
}

export function createBunImageProcessor(options: BunImageProcessorOptions = {}): ImageProcessor {
  const limits = { ...defaultProcessorLimits, ...options.limits };
  if (limits.maxInputBytes > 256 * 1024 * 1024) throw new ProcessorError("limit-exceeded");
  for (const [key, value] of Object.entries(limits)) {
    if (
      !(key in defaultProcessorLimits) ||
      !Number.isSafeInteger(value) ||
      value < (key === "maxTempBytes" ? 0 : 1)
    )
      throw new ProcessorError("limit-exceeded");
  }
  const maxQueue = options.maxQueue ?? limits.concurrency;
  if (!Number.isSafeInteger(maxQueue) || maxQueue < 0) throw new ProcessorError("limit-exceeded");
  let active = 0;
  const queue: Array<() => void> = [];

  async function acquire(signal: AbortSignal): Promise<() => void> {
    if (signal.aborted) throw new ProcessorError("cancelled");
    if (active < limits.concurrency) active++;
    else {
      if (queue.length >= maxQueue) throw new ProcessorError("limit-exceeded");
      await new Promise<void>((resolve, reject) => {
        const grant = () => {
          clearTimeout(timer);
          signal.removeEventListener("abort", abort);
          resolve();
        };
        const remove = (code: "timeout" | "cancelled") => {
          const index = queue.indexOf(grant);
          if (index >= 0) queue.splice(index, 1);
          clearTimeout(timer);
          signal.removeEventListener("abort", abort);
          reject(new ProcessorError(code));
        };
        const abort = () => remove("cancelled");
        const timer = setTimeout(() => remove("timeout"), limits.timeoutMs);
        signal.addEventListener("abort", abort, { once: true });
        queue.push(grant);
      });
    }
    return () => {
      const next = queue.shift();
      if (next) next();
      else active--;
    };
  }

  async function run(input: Uint8Array, signal: AbortSignal, recipe?: Recipe) {
    if (!(input instanceof Uint8Array) || input.byteLength < 1)
      throw new ProcessorError("invalid-image");
    if (input.byteLength > limits.maxInputBytes) throw new ProcessorError("limit-exceeded");
    if (input.byteLength > limits.maxMemoryBytes) throw new ProcessorError("limit-exceeded");
    if (recipe) {
      validateRecipe(recipe, limits);
      recipe = { ...recipe };
      if (recipe.width * recipe.height * 4 > limits.maxMemoryBytes)
        throw new ProcessorError("limit-exceeded");
    }
    const started = performance.now();
    const release = await acquire(signal);
    let directory: string | undefined;
    let child: Bun.Subprocess<"pipe", "pipe", "ignore"> | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let monitor: ReturnType<typeof setInterval> | undefined;
    let failure: ProcessorError | undefined;
    let polling = false;
    const stop = (code: ProcessorErrorCode) => {
      failure ??= new ProcessorError(code);
      child?.kill("SIGKILL");
    };
    const abort = () => stop("cancelled");
    try {
      if (signal.aborted) throw new ProcessorError("cancelled");
      directory = await mkdtemp(join(tmpdir(), "lenso-media-"));
      if (signal.aborted) throw new ProcessorError("cancelled");
      const entry = new URL(
        import.meta.url.endsWith(".ts") ? "./sharp-child.ts" : "./sharp-child.js",
        import.meta.url,
      );
      child = Bun.spawn(
        [
          process.execPath,
          "--no-install",
          "--no-env-file",
          "--no-orphans",
          decodeURIComponent(entry.pathname),
        ],
        {
          cwd: directory,
          env: {
            TMPDIR: directory,
            TMP: directory,
            TEMP: directory,
            VIPS_CONCURRENCY: "1",
            VIPS_DISC_THRESHOLD: String(limits.maxMemoryBytes),
            MALLOC_ARENA_MAX: "2",
          },
          stdin: "pipe",
          stdout: "pipe",
          stderr: "ignore",
        },
      );
      signal.addEventListener("abort", abort, { once: true });
      timer = setTimeout(
        () => stop("timeout"),
        Math.max(1, limits.timeoutMs - (performance.now() - started)),
      );
      // Disk and RSS are observed budgets, not an OS sandbox. Each job owns its temp directory.
      monitor = setInterval(async () => {
        if (polling) return;
        polling = true;
        try {
          if ((await diskBytes(directory!)) > limits.maxTempBytes) stop("limit-exceeded");
        } catch {
          stop("unavailable");
        } finally {
          polling = false;
        }
      }, 10);
      const output = readBounded(
        child.stdout as ReadableStream<Uint8Array>,
        limits.maxOutputBytes + headerLimit + 1,
      );
      const io = (async () => {
        const header = new TextEncoder().encode(
          JSON.stringify({
            op: recipe ? "transform" : "inspect",
            limits,
            recipe,
            length: input.length,
          }) + "\n",
        );
        await child!.stdin!.write(header);
        await child!.stdin!.write(input);
        await child!.stdin!.end();
        return await output;
      })();
      // Register both rejection handlers immediately: EPIPE and overlong stdout can race.
      const guarded = io.catch((error) => {
        stop(error instanceof ProcessorError ? error.code : "unavailable");
        throw error;
      });
      output.catch((error) => stop(error instanceof ProcessorError ? error.code : "unavailable"));
      const [result, exit] = await Promise.allSettled([guarded, child.exited]);
      if (failure) throw failure;
      if (exit.status === "fulfilled" && exit.value === 73)
        throw new ProcessorError("limit-exceeded");
      if (exit.status !== "fulfilled" || exit.value !== 0) throw new ProcessorError("unavailable");
      const usage = child.resourceUsage();
      if (usage && usage.maxRSS > limits.maxMemoryBytes) throw new ProcessorError("limit-exceeded");
      if ((await diskBytes(directory)) > limits.maxTempBytes)
        throw new ProcessorError("limit-exceeded");
      if (result.status !== "fulfilled") throw new ProcessorError("unavailable");
      return response(result.value);
    } catch (error) {
      throw error instanceof ProcessorError ? error : new ProcessorError("unavailable");
    } finally {
      clearTimeout(timer);
      clearInterval(monitor);
      signal.removeEventListener("abort", abort);
      if (child) {
        if (child.exitCode === null) child.kill("SIGKILL");
        await child.exited;
      }
      while (polling) await Bun.sleep(1);
      try {
        if (directory) await rm(directory, { recursive: true, force: true });
      } finally {
        release();
      }
    }
  }

  return {
    version: "sharp-0.35.5/vips-8.18.7/protocol-1",
    async inspect(input, signal) {
      return (await run(input, signal)).metadata;
    },
    async transform(input, recipe, signal) {
      validateRecipe(recipe, limits);
      return run(input, signal, recipe);
    },
  };
}
