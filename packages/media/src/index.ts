import { definePlugin, type Plugin, type PluginContext } from "@lenso/core/plugin";
import { classify, MediaError } from "./errors";
import type {
  FileReference,
  MediaOptions,
  MediaRecord,
  MediaRequest,
  MediaScope,
  MediaStage,
  MediaStatus,
  Preset,
} from "./contracts";
import type { Recipe } from "./processor";

export type * from "./contracts";
export type * from "./processor";
export { defaultProcessorLimits, ProcessorError } from "./processor";
export { MediaError, mediaErrorDiagnostic } from "./errors";

const retryable = new Set(["dependency", "timeout", "unavailable"]);
const terminal = new Set(["ready", "failed", "cancelled"]);

function text(value: unknown): value is string {
  return (
    typeof value === "string" &&
    value.length > 0 &&
    value.length <= 256 &&
    !Array.from(value).some(
      (character) => character.charCodeAt(0) < 32 || character.charCodeAt(0) === 127,
    )
  );
}
function exactKeys(value: unknown, allowed: readonly string[]) {
  if (
    !value ||
    typeof value !== "object" ||
    Array.isArray(value) ||
    ![Object.prototype, null].includes(Object.getPrototypeOf(value)) ||
    Object.keys(value).some((key) => !allowed.includes(key))
  )
    throw new MediaError("invalid-input");
}
export function validateReference(reference: FileReference) {
  exactKeys(reference, ["fileId", "version"]);
  exactKeys(reference.version, ["revision", "etag"]);
  if (
    !text(reference.fileId) ||
    reference.fileId.includes("://") ||
    !Number.isSafeInteger(reference.version.revision) ||
    reference.version.revision < 0 ||
    !text(reference.version.etag)
  )
    throw new MediaError("invalid-input");
}
function sameScope(a: MediaScope, b: MediaScope) {
  return (
    a.tenantId === b.tenantId && a.subjectId === b.subjectId && a.isolationId === b.isolationId
  );
}
function validatePreset(preset: Preset) {
  exactKeys(preset, [
    "name",
    "version",
    "width",
    "height",
    "fit",
    "formats",
    "defaultFormat",
    "quality",
    "metadata",
    "animation",
  ]);
  if (
    !text(preset.name) ||
    preset.name === "metadata" ||
    !text(preset.version) ||
    !Number.isInteger(preset.width) ||
    preset.width < 1 ||
    preset.width > 4096 ||
    !Number.isInteger(preset.height) ||
    preset.height < 1 ||
    preset.height > 4096 ||
    !["cover", "inside", "contain"].includes(preset.fit) ||
    !Array.isArray(preset.formats) ||
    preset.formats.length < 1 ||
    preset.formats.length > 3 ||
    preset.formats.some((format) => !["jpeg", "png", "webp"].includes(format)) ||
    !preset.formats.includes(preset.defaultFormat) ||
    !Number.isInteger(preset.quality) ||
    preset.quality < 1 ||
    preset.quality > 100 ||
    preset.metadata !== "strip" ||
    preset.animation !== "reject"
  )
    throw new MediaError("invalid-input");
}

export function createMedia<Access>(options: MediaOptions<Access>) {
  const presets = new Map<string, Preset>();
  for (const declaration of options.presets) {
    validatePreset(declaration);
    if (presets.has(declaration.name)) throw new MediaError("invalid-input");
    presets.set(declaration.name, structuredClone(declaration));
  }
  const maxInputBytes = options.maxInputBytes ?? 16 * 1024 * 1024;
  const timeoutMs = options.timeoutMs ?? 30_000;
  if (
    !text(options.processorVersion) ||
    (options.processor && options.processor.version !== options.processorVersion) ||
    !Number.isSafeInteger(maxInputBytes) ||
    maxInputBytes < 1 ||
    maxInputBytes > 64 * 1024 * 1024 ||
    !Number.isInteger(timeoutMs) ||
    timeoutMs < 1 ||
    timeoutMs > 300_000
  )
    throw new MediaError("invalid-input");

  async function scope(access: Access) {
    const result = await options.scope(access);
    if (![result?.tenantId, result?.subjectId, result?.isolationId].every(text))
      throw new MediaError("forbidden");
    return {
      tenantId: result.tenantId,
      subjectId: result.subjectId,
      isolationId: result.isolationId,
    };
  }
  async function load(id: string) {
    if (!/^[a-f0-9]{64}$/.test(id)) throw new MediaError("invalid-input");
    const record = await options.store.get(id);
    if (!record) throw new MediaError("not-found");
    return record;
  }
  async function source(
    access: Access,
    reference: FileReference,
    partition: MediaScope,
    signal?: AbortSignal,
  ) {
    const file = await options.storage.source(access, reference, signal);
    if (
      file.tenantId !== partition.tenantId ||
      !file.ownerId ||
      !(await options.authorizeDerive(access, Object.freeze(structuredClone(file))))
    )
      throw new MediaError("forbidden");
    return file;
  }
  async function authorize(access: Access, record: MediaRecord) {
    if (!sameScope(await scope(access), record.scope)) throw new MediaError("forbidden");
    const file = await source(access, record.source, record.scope);
    if (file.ownerId !== record.sourceOwnerId || file.storageId !== record.sourceStorageId)
      throw new MediaError("source-changed");
  }
  async function mutate(id: string, change: (current: MediaRecord) => Partial<MediaRecord> | null) {
    for (let attempt = 0; attempt < 16; attempt++) {
      const current = await load(id);
      const patch = change(current);
      if (!patch) return current;
      const next = { ...current, ...patch, revision: current.revision + 1, updatedAt: Date.now() };
      if (await options.store.replace(id, current.revision, next)) return next;
    }
    throw new MediaError("lease-lost");
  }
  async function project(record: MediaRecord): Promise<MediaStatus> {
    const artifacts = await options.store.artifacts(record.id);
    return {
      id: record.id,
      state: record.state,
      stage: record.stage,
      cancelRequested: record.cancelRequested,
      metadata: record.metadata,
      result: record.result,
      error: record.error,
      cleanupPending: artifacts.some(
        (item) => item.state !== "deleted" && item.fileId !== record.result?.fileId,
      ),
    };
  }
  async function submit(
    access: Access,
    reference: FileReference,
    preset: { name: string; version: string },
    recipe: Recipe | null,
  ) {
    validateReference(reference);
    const partition = await scope(access);
    const file = await source(access, reference, partition);
    const queueIdentity = await options.tasks.identity();
    const identity = JSON.stringify([
      "media/1",
      partition.tenantId,
      partition.subjectId,
      partition.isolationId,
      reference.fileId,
      reference.version.revision,
      reference.version.etag,
      file.storageId,
      file.ownerId,
      preset.name,
      preset.version,
      recipe,
      options.processorVersion,
      queueIdentity,
    ]);
    const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(identity));
    const id = Array.from(new Uint8Array(digest), (byte) =>
      byte.toString(16).padStart(2, "0"),
    ).join("");
    const now = Date.now();
    await options.store.insert({
      id,
      revision: 0,
      scope: partition,
      source: structuredClone(reference),
      sourceOwnerId: file.ownerId!,
      sourceStorageId: file.storageId,
      preset,
      recipe,
      processorVersion: options.processorVersion,
      queueIdentity,
      jobId: null,
      fence: 0,
      executionId: null,
      state: "pending",
      stage: "queued",
      cancelRequested: false,
      metadata: null,
      result: null,
      error: null,
      createdAt: now,
      updatedAt: now,
    });
    // Repeat after an ambiguous enqueue response: the durable Tasks key keeps one job.
    const jobId = await options.tasks.ensure(id);
    const record = await mutate(id, (current) => (current.jobId === jobId ? null : { jobId }));
    // Reused ready results follow the same live delivery gate as status/result.
    return status(access, record.id);
  }
  async function reconcile(record: MediaRecord) {
    if (terminal.has(record.state)) return record;
    const job = await options.tasks.job(record.id);
    if (
      (job && ["failed", "cancelled", "succeeded"].includes(job.state)) ||
      (!job && record.jobId)
    ) {
      const next: MediaRecord = {
        ...record,
        revision: record.revision + 1,
        updatedAt: Date.now(),
        state: job?.state === "cancelled" ? "cancelled" : "failed",
        executionId: null,
        error: record.error ?? {
          code: job?.state === "cancelled" ? "cancelled" : "dependency",
          stage: record.stage,
          retryable: job !== null && job.state !== "cancelled",
        },
      };
      // Only reconcile the exact observed generation. Never overwrite a newer claim.
      await options.store.replace(record.id, record.revision, next);
      return load(record.id);
    }
    return record;
  }
  async function recover(id: string) {
    await reconcile(await load(id));
    const outcomes: { fileId: string; state: "deleted" | "retained" | "failed" }[] = [];
    for (const candidate of await options.store.artifacts(id)) {
      const record = await load(id);
      if (
        record.result?.fileId === candidate.fileId ||
        (record.executionId === candidate.executionId && !terminal.has(record.state))
      ) {
        outcomes.push({ fileId: candidate.fileId, state: "retained" });
        continue;
      }
      let artifact = candidate;
      if (artifact.state !== "discarding") {
        artifact = { ...artifact, revision: artifact.revision + 1, state: "discarding" };
        if (!(await options.store.replaceArtifact(artifact.fileId, candidate.revision, artifact)))
          continue;
      }
      try {
        const access = await options.delegate(Object.freeze(structuredClone(record)), "cleanup");
        await options.storage.discard(access, artifact);
        const next = { ...artifact, revision: artifact.revision + 1, state: "deleted" as const };
        const changed = await options.store.replaceArtifact(
          artifact.fileId,
          artifact.revision,
          next,
        );
        if (!changed && (await options.store.getArtifact(artifact.fileId))?.state !== "deleted") {
          throw new MediaError("dependency");
        }
        outcomes.push({ fileId: artifact.fileId, state: "deleted" });
      } catch {
        outcomes.push({ fileId: artifact.fileId, state: "failed" });
      }
    }
    return outcomes;
  }
  async function status(access: Access, id: string) {
    let record = await load(id);
    await authorize(access, record);
    record = await reconcile(record);
    if (record.result) await options.storage.result(access, record.result);
    return project(record);
  }
  async function execute(
    id: string,
    context: { jobId: string; attempt: number; signal: AbortSignal },
  ) {
    let record = await load(id);
    if (
      record.queueIdentity !== (await options.tasks.identity()) ||
      (await options.tasks.ensure(id)) !== context.jobId
    )
      throw new MediaError("forbidden");
    if (!Number.isSafeInteger(context.attempt) || context.attempt < 1)
      throw new MediaError("invalid-input");
    function claimable(current: MediaRecord) {
      return (
        current.state !== "ready" &&
        current.state !== "cancelled" &&
        (current.state !== "failed" || current.error?.retryable) &&
        current.fence < context.attempt
      );
    }
    if (!claimable(record)) return;
    const executionId = crypto.randomUUID();
    record = await mutate(id, (current) => {
      if (!claimable(current)) return null;
      return {
        state: "running",
        fence: context.attempt,
        executionId,
        jobId: context.jobId,
        error: null,
      };
    });
    if (record.executionId !== executionId) return;
    const controller = new AbortController();
    const abort = () => controller.abort(context.signal.reason);
    context.signal.addEventListener("abort", abort, { once: true });
    if (context.signal.aborted) abort();
    let timedOut = false;
    const timer = setTimeout(() => {
      timedOut = true;
      controller.abort();
    }, timeoutMs);
    let stage: MediaStage = "download";
    async function active(nextStage?: MediaStage) {
      controller.signal.throwIfAborted();
      record = await mutate(id, (current) => {
        if (current.executionId !== executionId || current.state !== "running")
          throw new MediaError("lease-lost");
        if (current.cancelRequested) throw new MediaError("cancelled");
        return nextStage ? { stage: nextStage } : null;
      });
    }
    try {
      await active(stage);
      if (!options.processor || options.processor.version !== record.processorVersion)
        throw new MediaError("unavailable");
      const access = await options.delegate(Object.freeze(structuredClone(record)), "execute");
      await authorize(access, record);
      const input = await options.storage.download(
        access,
        record.source,
        maxInputBytes,
        controller.signal,
      );
      stage = "decode";
      await active(stage);
      const metadata = await options.processor.inspect(input, controller.signal);
      let result: FileReference | null = null;
      if (record.recipe) {
        stage = "transform";
        await active(stage);
        const output = await options.processor.transform(input, record.recipe, controller.signal);
        stage = "upload";
        await active(stage);
        await authorize(access, record);
        result = await options.storage.upload(
          access,
          record,
          output.bytes,
          output.metadata,
          controller.signal,
        );
      }
      stage = "register";
      await active(stage);
      // Revalidate requester authority and source immutability immediately before publication.
      await authorize(access, record);
      await active();
      await mutate(id, (current) => {
        controller.signal.throwIfAborted();
        if (
          current.executionId !== executionId ||
          current.state !== "running" ||
          current.cancelRequested
        ) {
          throw new MediaError(current.cancelRequested ? "cancelled" : "lease-lost");
        }
        return { state: "ready", executionId: null, result, metadata, error: null };
      });
    } catch (cause) {
      let error = timedOut ? new MediaError("timeout", { cause }) : classify(cause);
      if (controller.signal.aborted && !timedOut) error = new MediaError("cancelled", { cause });
      const current = await load(id);
      if (current.state === "ready") return;
      const userCancelled = current.cancelRequested;
      if (userCancelled) error = new MediaError("cancelled", { cause });
      const canRetry = !userCancelled && (retryable.has(error.code) || context.signal.aborted);
      await mutate(id, (value) => {
        // A lost register acknowledgement may follow a committed result. Never compensate it.
        if (value.executionId !== executionId || value.state === "ready") return null;
        return {
          state: userCancelled ? "cancelled" : canRetry ? "pending" : "failed",
          executionId: null,
          error: { code: error.code, stage, retryable: canRetry },
        };
      });
      if (canRetry) throw error;
    } finally {
      clearTimeout(timer);
      context.signal.removeEventListener("abort", abort);
      await recover(id);
    }
  }
  return {
    async request(access: Access, input: MediaRequest) {
      exactKeys(input, ["source", "preset", "format"]);
      const preset = presets.get(input.preset);
      if (!preset) throw new MediaError("invalid-input");
      const format = input.format ?? preset.defaultFormat;
      if (!preset.formats.includes(format)) throw new MediaError("invalid-input");
      const recipe: Recipe = {
        width: preset.width,
        height: preset.height,
        fit: preset.fit,
        format,
        quality: preset.quality,
        metadata: preset.metadata,
        animation: preset.animation,
      };
      return submit(access, input.source, { name: preset.name, version: preset.version }, recipe);
    },
    requestMetadata(access: Access, reference: FileReference) {
      return submit(access, reference, { name: "metadata", version: "1" }, null);
    },
    async authorizeDelivery(access: Access, id: string) {
      await authorize(access, await load(id));
    },
    status,
    async result(access: Access, id: string) {
      const value = await status(access, id);
      if (value.state !== "ready" || !value.result) throw new MediaError("not-found");
      return value.result;
    },
    async cancel(access: Access, id: string) {
      const record = await load(id);
      await authorize(access, record);
      await mutate(id, (current) =>
        terminal.has(current.state)
          ? null
          : {
              cancelRequested: true,
              ...(current.state === "pending"
                ? { state: "cancelled" as const, executionId: null }
                : {}),
            },
      );
      await options.tasks.cancel(id);
      await recover(id);
      return status(access, id);
    },
    async retry(access: Access, id: string) {
      const record = await load(id);
      await authorize(access, record);
      if (record.state !== "failed" || !record.error?.retryable || record.cancelRequested)
        return false;
      // Queue acceptance comes first. A newer attempt may claim retryable failure
      // directly if the response or this conditional projection is lost.
      const accepted = await options.tasks.retry(id);
      if (accepted)
        await options.store.replace(id, record.revision, {
          ...record,
          revision: record.revision + 1,
          updatedAt: Date.now(),
          state: "pending",
          executionId: null,
        });
      return accepted;
    },
    execute,
    /** Trusted recovery entry; adapters must not expose it without an operator authorization boundary. */
    recover,
  };
}

export type Media<Access> = ReturnType<typeof createMedia<Access>>;
export function createMediaPlugin<Access>(options: {
  id: string;
  requires: readonly Plugin<unknown>[];
  setup(context: PluginContext): MediaOptions<Access> | Promise<MediaOptions<Access>>;
}): Plugin<Media<Access>> {
  return definePlugin({
    id: options.id,
    requires: options.requires,
    async setup(context) {
      return createMedia(await options.setup(context));
    },
  });
}
