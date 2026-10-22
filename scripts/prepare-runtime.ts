import { createHash } from "node:crypto";
import { execFile, spawn } from "node:child_process";
import { promisify } from "node:util";
import {
  cp,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rename,
  rm,
  writeFile,
} from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import pinnedSnapshot from "./runtime-sdk.json";

const owner = "lenso-runtime-sdk-preparation-v1\n";
const maxArchiveBytes = 20 * 1024 * 1024;
const downloadTimeoutMs = 60_000;
type Pin = typeof pinnedSnapshot;
type Run = (args: string[], cwd: string, signal?: AbortSignal) => Promise<void>;
export type ArchiveFetcher = (url: string, options?: RequestInit) => Promise<Response>;

async function exists(path: string) {
  try {
    return await lstat(path);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return undefined;
    throw error;
  }
}

function sha256(bytes: Uint8Array) {
  return createHash("sha256").update(bytes).digest("hex");
}

async function download(pin: Pin, fetcher: ArchiveFetcher) {
  if (
    pin.repoURL !== "https://github.com/LioRael/lenso-runtime" ||
    !/^[a-f0-9]{40}$/.test(pin.commit) ||
    !/^[a-f0-9]{64}$/.test(pin.hash) ||
    pin.version !== "0.1.0"
  ) {
    throw new Error("Invalid Runtime SDK snapshot pin");
  }
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), downloadTimeoutMs);
  let reader: ReadableStreamDefaultReader<Uint8Array> | undefined;
  try {
    const response = await fetcher(
      `https://codeload.github.com/LioRael/lenso-runtime/tar.gz/${pin.commit}`,
      { signal: controller.signal, redirect: "error" },
    );
    if (!response.ok || !response.body) throw new Error("Runtime SDK archive download failed");
    if (Number(response.headers.get("content-length")) > maxArchiveBytes) {
      throw new Error("Runtime SDK archive exceeds byte limit");
    }
    reader = response.body.getReader();
    const chunks: Uint8Array[] = [];
    let size = 0;
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > maxArchiveBytes) throw new Error("Runtime SDK archive exceeds byte limit");
      chunks.push(value);
    }
    const bytes = Buffer.concat(chunks, size);
    if (sha256(bytes) !== pin.hash) throw new Error("Runtime SDK archive checksum mismatch");
    return bytes;
  } finally {
    controller.abort();
    await reader?.cancel().catch(() => {});
    clearTimeout(timeout);
  }
}

export const runRuntimeCommand: Run = async (args, cwd, signal) => {
  signal?.throwIfAborted();
  const child = spawn(process.execPath, args, {
    cwd,
    stdio: ["ignore", "inherit", "inherit"],
    detached: process.platform !== "win32",
  });
  let timer: ReturnType<typeof setTimeout> | undefined;
  const kill = (value: NodeJS.Signals) => {
    try {
      if (process.platform !== "win32" && child.pid) process.kill(-child.pid, value);
      else child.kill(value);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ESRCH") throw error;
    }
  };
  const cancel = () => {
    kill("SIGTERM");
    timer ??= setTimeout(() => kill("SIGKILL"), 5000);
  };
  signal?.addEventListener("abort", cancel, { once: true });
  if (signal?.aborted) cancel();
  try {
    const result = await new Promise<{ code: number | null; signal: NodeJS.Signals | null }>(
      (accept, reject) => {
        child.once("error", reject);
        child.once("close", (code, exitSignal) => accept({ code, signal: exitSignal }));
      },
    );
    signal?.throwIfAborted();
    if (result.code !== 0) {
      throw new Error(
        `Runtime SDK command failed (exit ${result.code ?? "none"}${result.signal ? `, signal ${result.signal}` : ""}): bun ${args.join(" ")}`,
      );
    }
  } finally {
    signal?.removeEventListener("abort", cancel);
    if (timer) {
      kill("SIGKILL");
      clearTimeout(timer);
    }
  }
};

async function fileHashes(root: string, prefix = ""): Promise<Record<string, string>> {
  const hashes: Record<string, string> = {};
  const entries = await readdir(join(root, prefix), { withFileTypes: true });
  entries.sort((a, b) => a.name.localeCompare(b.name, "en"));
  for (const entry of entries) {
    const relative = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory()) Object.assign(hashes, await fileHashes(root, relative));
    else if (entry.isFile()) hashes[relative] = sha256(await readFile(join(root, relative)));
    else throw new Error("Runtime SDK output must contain only regular files and directories");
  }
  return hashes;
}

async function copyLocalRuntime(directory: string, destination: string) {
  const root = resolve(directory);
  if (!(await lstat(root)).isDirectory())
    throw new Error("Runtime root must be a regular directory");
  const { stdout } = await promisify(execFile)("git", ["ls-files", "-z"], {
    cwd: root,
    timeout: 10_000,
    maxBuffer: 1024 * 1024,
  });
  const files: Record<string, string> = {};
  let size = 0;
  const paths = stdout
    .split("\0")
    .filter(Boolean)
    .filter(
      (path) =>
        /^(package\.json|bun\.lock|tsconfig\.json|Cargo\.(toml|lock)|tools\/generate\.ts)$/.test(
          path,
        ) ||
        /^(sdk\/typescript\/[^/]+\.(ts|json)|crates\/[^/]+\/(Cargo\.toml|(src|tests)\/[^/]+\.rs)|examples\/batch\/(Cargo\.toml|src\/(bin\/)?[^/]+\.rs)|examples\/contracts\/[^/]+\.json|examples\/generated\/[^/]+\.(rs|ts))$/.test(
          path,
        ),
    )
    .sort();
  // Shared candidate schema cases are an explicit source input, even before Git staging.
  const schemaCases = "crates/sdk/tests/schema-cases.json";
  if (await exists(join(root, schemaCases))) paths.push(schemaCases);
  paths.sort();
  if (paths.length > 4096) throw new Error("Local Runtime source exceeds file limit");
  for (const path of paths) {
    const parts = path.split("/");
    if (parts.some((part) => !part || part === "." || part === "..") || path.includes("\\")) {
      throw new Error("Unexpected local Runtime path");
    }
    for (let i = 1; i <= parts.length; i++) {
      const entry = await lstat(join(root, ...parts.slice(0, i)));
      if (entry.isSymbolicLink() || (i === parts.length ? !entry.isFile() : !entry.isDirectory())) {
        throw new Error("Local Runtime source must contain only regular files and directories");
      }
    }
    const bytes = await Bun.file(join(root, path))
      .slice(0, maxArchiveBytes + 1)
      .bytes();
    size += bytes.length;
    if (size > maxArchiveBytes) throw new Error("Local Runtime source exceeds byte limit");
    files[path] = sha256(bytes);
    await mkdir(dirname(join(destination, path)), { recursive: true });
    await writeFile(join(destination, path), bytes);
  }
  const manifest = JSON.parse(await readFile(join(destination, "package.json"), "utf8"));
  if (manifest.name !== "lenso-runtime-workspace" || manifest.private !== true) {
    throw new Error("Local source must be lenso-runtime-workspace");
  }
  return { kind: "local", root, files, sha256: sha256(Buffer.from(JSON.stringify(files))) };
}

// Pin and command injection remain fixture-only; local source requires explicit opt-in.
export async function prepareRuntime(
  root = resolve(import.meta.dir, ".."),
  dependencies: { fetch?: ArchiveFetcher; run?: Run; pin?: Pin } = {},
  options: { runtimeRoot?: string; signal?: AbortSignal } = {},
) {
  options.signal?.throwIfAborted();
  root = resolve(root);
  const manifest = JSON.parse(await readFile(join(root, "package.json"), "utf8"));
  if (manifest.name !== "lenso-workspace" || manifest.private !== true) {
    throw new Error("Preparation root must be the consuming lenso-workspace");
  }
  const expectedBun = manifest.engines?.bun ?? "1.4.4";
  if (Bun.version !== expectedBun) {
    throw new Error(`Runtime SDK preparation requires Bun ${expectedBun}`);
  }
  const pin = dependencies.pin ?? pinnedSnapshot;
  // Authenticate the complete response before extracting files or executing snapshot code.
  const bytes = options.runtimeRoot ? undefined : await download(pin, dependencies.fetch ?? fetch);
  const output = join(root, ".runtime-deps");
  const existing = await exists(output);
  if (existing) {
    const marker = await exists(join(output, ".owner"));
    if (
      !existing.isDirectory() ||
      existing.isSymbolicLink() ||
      !marker?.isFile() ||
      marker.isSymbolicLink() ||
      (await readFile(join(output, ".owner"), "utf8")) !== owner
    ) {
      throw new Error("Refusing to replace unowned .runtime-deps");
    }
  } else {
    await mkdir(output);
    await writeFile(join(output, ".owner"), owner, { flag: "wx" });
  }
  const lock = join(output, ".prepare-lock");
  await mkdir(lock); // An existing lock fails closed; never remove another invocation's lock.
  let temporary: string | undefined;
  try {
    for (const name of ["source", "sdk", "provenance.json"]) {
      const stat = await exists(join(output, name));
      if (
        stat &&
        (stat.isSymbolicLink() ||
          (name === "provenance.json" ? !stat.isFile() : !stat.isDirectory()))
      ) {
        throw new Error(`Refusing to replace unexpected .runtime-deps/${name}`);
      }
    }
    temporary = await mkdtemp(join(output, ".prepare-"));
    const source = join(temporary, "source");
    await mkdir(source);
    const local = options.runtimeRoot
      ? await copyLocalRuntime(options.runtimeRoot, source)
      : undefined;
    if (bytes) {
      const archive = new Bun.Archive(Bun.gunzipSync(bytes));
      const prefix = `lenso-runtime-${pin.commit}/`;
      for (const [name, file] of await archive.files()) {
        const relative = name.slice(prefix.length);
        if (
          !name.startsWith(prefix) ||
          !relative ||
          relative.split("/").some((part) => part === ".." || part === "." || part === "") ||
          relative.includes("\\")
        ) {
          throw new Error("Unexpected Runtime SDK archive path");
        }
        const destination = join(source, relative);
        await mkdir(dirname(destination), { recursive: true });
        await writeFile(destination, await file.bytes());
      }
    }
    const execute = dependencies.run ?? runRuntimeCommand;
    options.signal?.throwIfAborted();
    await execute(["install", "--frozen-lockfile", "--ignore-scripts"], source, options.signal);
    const sdkSource = join(source, "sdk/typescript");
    // Run both tools through this Bun executable, including TypeScript's Node-shebang
    // entry. The runner needs no implicit Node installation or package build script.
    await execute(
      [
        "build",
        "index.ts",
        "schema.ts",
        "--outdir",
        "dist",
        "--target",
        "bun",
        "--packages",
        "external",
        "--splitting",
      ],
      sdkSource,
      options.signal,
    );
    await execute(
      [join(source, "node_modules/typescript/bin/tsc"), "-p", "tsconfig.build.json"],
      sdkSource,
      options.signal,
    );
    options.signal?.throwIfAborted();
    const sdkManifest = JSON.parse(await readFile(join(sdkSource, "package.json"), "utf8"));
    if (
      sdkManifest.name !== "@lenso/runtime-sdk" ||
      sdkManifest.version !== pin.version ||
      sdkManifest.private
    ) {
      throw new Error("Built Runtime SDK does not match the public package pin");
    }
    const sdk = join(temporary, "sdk");
    await mkdir(sdk);
    delete sdkManifest.scripts;
    await writeFile(join(sdk, "package.json"), `${JSON.stringify(sdkManifest, null, 2)}\n`);
    await cp(join(sdkSource, "dist"), join(sdk, "dist"), { recursive: true, dereference: false });
    const hashes = await fileHashes(sdk);
    for (const file of ["dist/index.js", "dist/index.d.ts", "dist/schema.js", "dist/schema.d.ts"]) {
      if (!hashes[file]) throw new Error(`Built Runtime SDK is missing ${file}`);
    }
    await writeFile(
      join(temporary, "provenance.json"),
      `${JSON.stringify({ ...(local ? { source: local, version: pin.version } : pin), bun: Bun.version, files: hashes }, null, 2)}\n`,
    );
    // Stage both source and package before replacing any previous ready state.
    options.signal?.throwIfAborted();
    const moved: string[] = [];
    const installed: string[] = [];
    try {
      for (const name of ["source", "sdk", "provenance.json"]) {
        if (await exists(join(output, name))) {
          await rename(join(output, name), join(temporary, `previous-${name}`));
          moved.push(name);
        }
        await rename(join(temporary, name), join(output, name));
        installed.push(name);
      }
    } catch (error) {
      for (const name of installed.reverse()) {
        await rename(join(output, name), join(temporary, name));
      }
      for (const name of moved.reverse()) {
        await rename(join(temporary, `previous-${name}`), join(output, name));
      }
      throw error;
    }
  } finally {
    if (temporary) await rm(temporary, { recursive: true, force: true });
    await rm(lock, { recursive: true });
  }
}

if (import.meta.main) {
  const args = process.argv.slice(2);
  const options = new Map<string, string>();
  let valid = args.length % 2 === 0;
  for (let i = 0; i < args.length; i += 2) {
    const key = args[i]!;
    if (
      !["--root", "--runtime-root"].includes(key) ||
      options.has(key) ||
      !args[i + 1] ||
      args[i + 1]!.startsWith("--")
    )
      valid = false;
    options.set(key, args[i + 1]!);
  }
  if (!valid) {
    console.error(
      "Usage: bun scripts/prepare-runtime.ts [--root <lenso-workspace>] [--runtime-root <runtime-checkout>]",
    );
    process.exitCode = 1;
  } else {
    try {
      await prepareRuntime(
        options.get("--root"),
        {},
        { runtimeRoot: options.get("--runtime-root") },
      );
      console.log(
        "Runtime SDK 0.1.0 prepared in .runtime-deps/sdk; source in .runtime-deps/source",
      );
    } catch (error) {
      console.error(error instanceof Error ? error.message : "Runtime SDK preparation failed");
      process.exitCode = 1;
    }
  }
}
