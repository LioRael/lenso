import { Database } from "bun:sqlite";
import { afterEach, describe, expect, test } from "bun:test";
import { createMedia, MediaError } from "../src";
import { createSqliteMediaStore } from "../src/sqlite";
import type {
  MediaArtifact,
  MediaRecord,
  MediaStorage,
  MediaTasks,
  Preset,
} from "../src/contracts";
import { ProcessorError, type ImageMetadata, type ImageProcessor } from "../src/processor";

const databases: Database[] = [];
const source = { fileId: "source", version: { revision: 1, etag: "source-v1" } };
const metadata: ImageMetadata = {
  format: "png",
  mime: "image/png",
  width: 80,
  height: 60,
  orientation: 1,
  hasAlpha: true,
  frames: 1,
};
const preset: Preset = {
  name: "thumbnail",
  version: "2",
  width: 32,
  height: 32,
  fit: "inside",
  formats: ["webp", "jpeg"],
  defaultFormat: "webp",
  quality: 80,
  metadata: "strip",
  animation: "reject",
};

afterEach(() => {
  for (const db of databases.splice(0)) db.close();
});

function fixture(
  overrides: {
    processor?: ImageProcessor | null;
    fault?: string;
    replaceThenThrow?: boolean;
    preset?: Preset;
    processorVersion?: string;
    timeoutMs?: number;
  } = {},
) {
  const db = new Database(":memory:");
  databases.push(db);
  db.exec(`
    CREATE TABLE media (id TEXT PRIMARY KEY NOT NULL, revision INTEGER NOT NULL, record TEXT NOT NULL);
    CREATE TABLE media_artifacts (file_id TEXT PRIMARY KEY NOT NULL, derivation_id TEXT NOT NULL REFERENCES media(id),
      revision INTEGER NOT NULL, record TEXT NOT NULL);
    CREATE INDEX media_artifacts_derivation_id ON media_artifacts (derivation_id, file_id);
  `);
  const baseStore = createSqliteMediaStore(db);
  const store = {
    ...baseStore,
    async replace(id: string, revision: number, next: MediaRecord) {
      const changed = await baseStore.replace(id, revision, next);
      if (overrides.replaceThenThrow && next.state === "ready" && changed)
        throw new Error("lost acknowledgement");
      return changed;
    },
  };
  const jobs = new Map<
    string,
    {
      id: string;
      state: "pending" | "running" | "succeeded" | "failed" | "cancelled";
      attempt: number;
    }
  >();
  const tasks: MediaTasks = {
    async identity() {
      return "tasks-q1";
    },
    async ensure(id) {
      let job = jobs.get(id);
      if (!job) {
        job = { id: `job-${id}`, state: "pending", attempt: 0 };
        jobs.set(id, job);
      }
      return job.id;
    },
    async job(id) {
      const job = jobs.get(id);
      return job ? { state: job.state, cancelRequested: job.state === "cancelled" } : null;
    },
    async cancel(id) {
      const job = jobs.get(id);
      if (job) job.state = "cancelled";
    },
    async retry(id) {
      const job = jobs.get(id);
      if (!job) return false;
      job.state = "pending";
      return true;
    },
  };
  let currentScope = { tenantId: "tenant", subjectId: "person", isolationId: "default" };
  let sourceOwner = "owner";
  let sourceStorage = "storage-source";
  let permission = true;
  const calls: string[] = [];
  const storage: MediaStorage<string> = {
    async source(_access, reference) {
      calls.push("source");
      if (overrides.fault === "source") throw new Error("source unavailable");
      if (!permission) throw new MediaError("forbidden");
      return {
        fileId: reference.fileId,
        tenantId: currentScope.tenantId,
        ownerId: sourceOwner,
        storageId: sourceStorage,
      } as never;
    },
    async download() {
      calls.push("download");
      if (overrides.fault === "download") throw new Error("download");
      return new Uint8Array([1]);
    },
    async upload(_access, record) {
      calls.push("upload");
      if (overrides.fault === "upload") throw new Error("upload");
      const artifact: MediaArtifact = {
        fileId: `result-file-${record.fence}`,
        derivationId: record.id,
        fence: record.fence,
        executionId: record.executionId!,
        revision: 0,
        state: "staged",
        createdAt: Date.now(),
      };
      await store.insertArtifact(artifact);
      return { fileId: artifact.fileId, version: { revision: 1, etag: "result-v1" } };
    },
    async result() {
      calls.push("result");
    },
    async discard(_access, artifact) {
      calls.push(`discard:${artifact.fileId}`);
    },
  };
  const processor: ImageProcessor | undefined =
    overrides.processor === null
      ? undefined
      : (overrides.processor ?? {
          version: overrides.processorVersion ?? "processor-v1",
          async inspect() {
            calls.push("inspect");
            if (overrides.fault === "inspect") throw new Error("inspect");
            return metadata;
          },
          async transform() {
            calls.push("transform");
            if (overrides.fault === "transform") throw new Error("transform");
            return { bytes: new Uint8Array([2]), metadata };
          },
        });
  const service = createMedia<string>({
    store,
    storage,
    tasks,
    presets: [overrides.preset ?? preset],
    processorVersion: overrides.processorVersion ?? "processor-v1",
    processor,
    timeoutMs: overrides.timeoutMs,
    scope: async () => currentScope,
    authorizeDerive: async () => true,
    delegate: async () => "delegated",
  });
  return {
    service,
    store,
    tasks,
    jobs,
    calls,
    storage,
    processor,
    overrides,
    setPermission: (value: boolean) => {
      permission = value;
    },
    setScope: (value: typeof currentScope) => {
      currentScope = value;
    },
    changeSource: () => {
      sourceOwner = "changed-owner";
    },
    changeStorage: () => {
      sourceStorage = "changed-storage";
    },
  };
}

describe("media service", () => {
  test("transitions from queued to ready and publishes metadata and the result", async () => {
    const f = fixture();
    const queued = await f.service.request("access", { source, preset: preset.name });
    expect(queued.state).toBe("pending");
    const record = (await f.store.get(queued.id)) as MediaRecord;
    expect(record.recipe?.format).toBe("webp");
    const jobId = await f.tasks.ensure(queued.id);
    const job = f.jobs.get(queued.id)!;
    job.state = "running";
    await f.service.execute(queued.id, { jobId, attempt: 1, signal: new AbortController().signal });
    const ready = await f.service.status("access", queued.id);
    expect(ready).toMatchObject({
      state: "ready",
      stage: "register",
      metadata,
      result: { fileId: "result-file-1" },
    });
    expect(await f.service.result("access", queued.id)).toEqual(ready.result!);
  });

  test("metadata requests create a metadata-only job", async () => {
    const f = fixture();
    const queued = await f.service.requestMetadata("access", source);
    expect((await f.store.get(queued.id))?.recipe).toBeNull();
    await f.service.execute(queued.id, {
      jobId: await f.tasks.ensure(queued.id),
      attempt: 1,
      signal: new AbortController().signal,
    });
    expect(await f.service.status("access", queued.id)).toMatchObject({
      state: "ready",
      result: null,
      metadata,
    });
    expect(f.calls).not.toContain("transform");
  });

  test("rejects unknown request fields, URLs, and unsupported formats", async () => {
    const f = fixture();
    await expect(
      f.service.request("access", {
        source,
        preset: preset.name,
        url: "https://example.test",
      } as never),
    ).rejects.toMatchObject({ code: "invalid-input" });
    await expect(
      f.service.request("access", {
        source: { ...source, fileId: "https://example.test" },
        preset: preset.name,
      }),
    ).rejects.toMatchObject({ code: "invalid-input" });
    await expect(
      f.service.request("access", { source, preset: preset.name, format: "gif" as never }),
    ).rejects.toMatchObject({ code: "invalid-input" });
  });

  test("normalizes default format identity and partitions identity by recipe, source version, processor, and scope", async () => {
    const f = fixture();
    const first = await f.service.request("access", { source, preset: preset.name });
    const same = await f.service.request("access", { source, preset: preset.name, format: "webp" });
    expect(same.id).toBe(first.id);
    expect(
      (await f.service.request("access", { source, preset: preset.name, format: "jpeg" })).id,
    ).not.toBe(first.id);
    expect(
      (
        await f.service.request("access", {
          source: { ...source, version: { revision: 2, etag: "source-v2" } },
          preset: preset.name,
        })
      ).id,
    ).not.toBe(first.id);
    f.setScope({ tenantId: "tenant-2", subjectId: "person", isolationId: "default" });
    expect((await f.service.request("access", { source, preset: preset.name })).id).not.toBe(
      first.id,
    );
    f.setScope({ tenantId: "tenant", subjectId: "other", isolationId: "default" });
    expect((await f.service.request("access", { source, preset: preset.name })).id).not.toBe(
      first.id,
    );
  });

  test("concurrent duplicate submissions share one identity and task", async () => {
    const f = fixture();
    const [a, b] = await Promise.all([
      f.service.request("access", { source, preset: preset.name }),
      f.service.request("access", { source, preset: preset.name }),
    ]);
    expect(a.id).toBe(b.id);
    expect(f.jobs.size).toBe(1);
    expect((await f.store.get(a.id))?.jobId).toBe(`job-${a.id}`);
  });

  test("absence of a processor fails instead of reporting ready", async () => {
    const f = fixture({ processor: null });
    const queued = await f.service.requestMetadata("access", source);
    await expect(
      f.service.execute(queued.id, {
        jobId: await f.tasks.ensure(queued.id),
        attempt: 1,
        signal: new AbortController().signal,
      }),
    ).rejects.toMatchObject({ code: "unavailable" });
    f.jobs.get(queued.id)!.state = "failed";
    expect(await f.service.status("access", queued.id)).toMatchObject({
      state: "failed",
      result: null,
      error: { code: "unavailable" },
    });
  });

  test.each(["download", "inspect", "transform", "upload"] as const)(
    "does not publish ready after %s failure",
    async (fault) => {
      const f = fixture({ fault });
      const queued = await f.service.request("access", { source, preset: preset.name });
      await expect(
        f.service.execute(queued.id, {
          jobId: await f.tasks.ensure(queued.id),
          attempt: 1,
          signal: new AbortController().signal,
        }),
      ).rejects.toMatchObject({ code: "dependency" });
      expect((await f.store.get(queued.id))?.state).toBe("pending");
      expect((await f.store.get(queued.id))?.result).toBeNull();
    },
  );

  test("a committed ready write survives an ambiguous acknowledgement", async () => {
    const f = fixture({ replaceThenThrow: true });
    const queued = await f.service.request("access", { source, preset: preset.name });
    await f.service.execute(queued.id, {
      jobId: await f.tasks.ensure(queued.id),
      attempt: 1,
      signal: new AbortController().signal,
    });
    expect((await f.store.get(queued.id))?.state).toBe("ready");
    expect(f.calls.some((call) => call.startsWith("discard"))).toBe(false);
  });

  test("preset version, normalized dimensions, and processor version invalidate identity", async () => {
    const baseline = await fixture().service.request("access", { source, preset: preset.name });
    for (const options of [
      { preset: { ...preset, version: "3" } },
      { preset: { ...preset, width: 24 } },
      { processorVersion: "processor-v2" },
    ]) {
      const f = fixture(options);
      expect((await f.service.request("access", { source, preset: preset.name })).id).not.toBe(
        baseline.id,
      );
    }
    expect(() => fixture({ preset: { ...preset, quality: 200 } })).toThrow(MediaError);
    expect(() => fixture({ preset: { ...preset, width: 10000 } })).toThrow(MediaError);
  });

  test("retry accepts the queue first and a newer attempt reuses the same identity", async () => {
    const f = fixture({ fault: "transform" });
    const queued = await f.service.request("access", { source, preset: preset.name });
    const jobId = await f.tasks.ensure(queued.id);
    await expect(
      f.service.execute(queued.id, { jobId, attempt: 1, signal: new AbortController().signal }),
    ).rejects.toThrow();
    f.jobs.get(queued.id)!.state = "failed";
    expect((await f.service.status("access", queued.id)).state).toBe("failed");
    const retry = f.tasks.retry;
    f.tasks.retry = async (id) => {
      expect((await f.store.get(id))?.state).toBe("failed");
      expect((await f.service.status("access", id)).state).toBe("failed");
      return retry(id);
    };
    f.overrides.fault = "";
    expect(await f.service.retry("access", queued.id)).toBe(true);
    await f.service.execute(queued.id, { jobId, attempt: 2, signal: new AbortController().signal });
    await f.service.execute(queued.id, { jobId, attempt: 2, signal: new AbortController().signal });
    expect(await f.service.result("access", queued.id)).toMatchObject({ fileId: "result-file-2" });
    expect(f.jobs.size).toBe(1);
    expect(f.calls.filter((value) => value === "upload")).toHaveLength(1);
  });

  test("lost queue retry acknowledgement still permits the accepted newer attempt", async () => {
    const f = fixture({ fault: "download" });
    const queued = await f.service.request("access", { source, preset: preset.name });
    const jobId = await f.tasks.ensure(queued.id);
    await expect(
      f.service.execute(queued.id, { jobId, attempt: 1, signal: new AbortController().signal }),
    ).rejects.toThrow();
    f.jobs.get(queued.id)!.state = "failed";
    await f.service.status("access", queued.id);
    const retry = f.tasks.retry;
    f.tasks.retry = async (id) => {
      await retry(id);
      throw new Error("ack lost");
    };
    await expect(f.service.retry("access", queued.id)).rejects.toThrow();
    f.overrides.fault = "";
    await f.service.execute(queued.id, { jobId, attempt: 2, signal: new AbortController().signal });
    expect((await f.store.get(queued.id))?.state).toBe("ready");
  });

  test("permission revocation before execution and before delivery is enforced", async () => {
    const f = fixture();
    const queued = await f.service.request("access", { source, preset: preset.name });
    f.setPermission(false);
    await f.service.execute(queued.id, {
      jobId: await f.tasks.ensure(queued.id),
      attempt: 1,
      signal: new AbortController().signal,
    });
    expect(await f.store.get(queued.id)).toMatchObject({
      state: "failed",
      error: { code: "forbidden" },
      result: null,
    });
    expect(f.calls).not.toContain("download");
    const g = fixture();
    const accepted = await g.service.request("access", { source, preset: preset.name });
    await g.service.execute(accepted.id, {
      jobId: await g.tasks.ensure(accepted.id),
      attempt: 1,
      signal: new AbortController().signal,
    });
    g.setPermission(false);
    await expect(g.service.result("access", accepted.id)).rejects.toMatchObject({
      code: "forbidden",
    });
  });

  test("source owner/storage changes fail before publication", async () => {
    for (const kind of ["owner", "storage"]) {
      const f = fixture();
      const queued = await f.service.request("access", { source, preset: preset.name });
      if (kind === "owner") f.changeSource();
      else f.changeStorage();
      await f.service.execute(queued.id, {
        jobId: await f.tasks.ensure(queued.id),
        attempt: 1,
        signal: new AbortController().signal,
      });
      expect(await f.store.get(queued.id)).toMatchObject({
        state: "failed",
        error: { code: "source-changed" },
      });
    }
  });

  test("a repeated ready request still authorizes the derived result", async () => {
    const f = fixture();
    const queued = await f.service.request("access", { source, preset: preset.name });
    await f.service.execute(queued.id, {
      jobId: await f.tasks.ensure(queued.id),
      attempt: 1,
      signal: new AbortController().signal,
    });
    f.storage.result = async () => {
      throw new MediaError("forbidden");
    };
    await expect(
      f.service.request("access", { source, preset: preset.name }),
    ).rejects.toMatchObject({ code: "forbidden" });
    expect(f.jobs.size).toBe(1);
    expect(f.calls.filter((call) => call === "upload")).toHaveLength(1);
  });

  test("decode failure is terminal and is not retried as a dependency outage", async () => {
    const f = fixture();
    f.processor!.inspect = async () => {
      throw new ProcessorError("invalid-image");
    };
    const queued = await f.service.requestMetadata("access", source);
    await f.service.execute(queued.id, {
      jobId: await f.tasks.ensure(queued.id),
      attempt: 1,
      signal: new AbortController().signal,
    });
    expect(await f.store.get(queued.id)).toMatchObject({
      state: "failed",
      result: null,
      error: { code: "invalid-image", retryable: false },
    });
    expect(await f.service.retry("access", queued.id)).toBe(false);
  });

  test("execution deadline aborts the processor and never publishes ready", async () => {
    const f = fixture({ timeoutMs: 10 });
    f.processor!.inspect = (_bytes, signal) =>
      new Promise((_resolve, reject) => {
        signal.addEventListener("abort", () => reject(signal.reason), { once: true });
      });
    const queued = await f.service.requestMetadata("access", source);
    await expect(
      f.service.execute(queued.id, {
        jobId: await f.tasks.ensure(queued.id),
        attempt: 1,
        signal: new AbortController().signal,
      }),
    ).rejects.toMatchObject({ code: "timeout" });
    expect(await f.store.get(queued.id)).toMatchObject({
      state: "pending",
      result: null,
      error: { code: "timeout" },
    });
  });

  test("cancel during upload denies publication, discards artifacts, and repeats tombstone cleanup", async () => {
    const f = fixture();
    const queued = await f.service.request("access", { source, preset: preset.name });
    const original = f.storage.upload;
    const entered = latch();
    const release = latch();
    f.storage.upload = async (...args) => {
      const value = await original(...args);
      entered.resolve();
      await release.promise;
      return value;
    };
    const work = f.service.execute(queued.id, {
      jobId: await f.tasks.ensure(queued.id),
      attempt: 1,
      signal: new AbortController().signal,
    });
    try {
      await entered.promise;
      expect((await f.service.cancel("access", queued.id)).cancelRequested).toBe(true);
    } finally {
      release.resolve();
      await work;
    }
    expect(await f.store.get(queued.id)).toMatchObject({ state: "cancelled", result: null });
    expect((await f.store.artifacts(queued.id))[0]?.state).toBe("deleted");
    const before = f.calls.filter((call) => call.startsWith("discard")).length;
    await f.service.recover(queued.id);
    expect(f.calls.filter((call) => call.startsWith("discard")).length).toBe(before + 1);
  });

  test("higher attempt fences a delayed writer without discarding the winning result", async () => {
    const f = fixture();
    const queued = await f.service.request("access", { source, preset: preset.name });
    const original = f.storage.upload;
    const entered = latch();
    const release = latch();
    f.storage.upload = async (...args) => {
      const value = await original(...args);
      if (args[1].fence === 1) {
        entered.resolve();
        await release.promise;
      }
      return value;
    };
    const jobId = await f.tasks.ensure(queued.id);
    const first = f.service.execute(queued.id, {
      jobId,
      attempt: 1,
      signal: new AbortController().signal,
    });
    try {
      await entered.promise;
      await f.service.execute(queued.id, {
        jobId,
        attempt: 2,
        signal: new AbortController().signal,
      });
    } finally {
      release.resolve();
      await first;
    }
    expect((await f.store.get(queued.id))?.result?.fileId).toBe("result-file-2");
    expect((await f.store.getArtifact("result-file-1"))?.state).toBe("deleted");
    expect((await f.store.getArtifact("result-file-2"))?.state).toBe("staged");
    expect(f.calls).not.toContain("discard:result-file-2");
  });

  test("trusted recovery reconciles a final crash without needing an authorized status call", async () => {
    const f = fixture();
    const queued = await f.service.request("access", { source, preset: preset.name });
    const record = (await f.store.get(queued.id))!;
    await f.store.replace(record.id, record.revision, {
      ...record,
      revision: record.revision + 1,
      state: "running",
      fence: 1,
      executionId: "crashed",
    });
    await f.store.insertArtifact({
      fileId: "crash-residue",
      derivationId: record.id,
      revision: 0,
      fence: 1,
      executionId: "crashed",
      state: "staged",
      createdAt: 1,
    });
    f.jobs.get(queued.id)!.state = "failed";
    f.setPermission(false);
    expect(await f.service.recover(queued.id)).toContainEqual({
      fileId: "crash-residue",
      state: "deleted",
    });
    expect((await f.store.get(queued.id))?.state).toBe("failed");
  });

  test("a stale terminal Tasks sample cannot clobber a newly claimed generation", async () => {
    const f = fixture();
    const queued = await f.service.requestMetadata("access", source);
    const entered = latch();
    const release = latch();
    f.tasks.job = async () => {
      entered.resolve();
      await release.promise;
      return { state: "failed", cancelRequested: false };
    };
    const sampled = f.service.status("access", queued.id);
    await entered.promise;
    const record = (await f.store.get(queued.id))!;
    await f.store.replace(record.id, record.revision, {
      ...record,
      revision: record.revision + 1,
      state: "running",
      fence: 2,
      executionId: "new-generation",
    });
    release.resolve();
    expect((await sampled).state).toBe("running");
    expect((await f.store.get(queued.id))?.executionId).toBe("new-generation");
  });

  test("worker abort during the final awaited lookup cannot publish", async () => {
    const f = fixture();
    const queued = await f.service.request("access", { source, preset: preset.name });
    const controller = new AbortController();
    const get = f.store.get;
    let registerReads = 0;
    f.store.get = async (id) => {
      const value = await get(id);
      if (value?.state === "running" && value.stage === "register" && ++registerReads === 2)
        controller.abort();
      return value;
    };
    await expect(
      f.service.execute(queued.id, {
        jobId: await f.tasks.ensure(queued.id),
        attempt: 1,
        signal: controller.signal,
      }),
    ).rejects.toMatchObject({ code: "cancelled" });
    expect(await f.store.get(queued.id)).toMatchObject({ state: "pending", result: null });
    expect((await f.store.artifacts(queued.id))[0]?.state).toBe("deleted");
  });

  test("trusted recovery reconciles a metadata-only crash with no artifact rows", async () => {
    const f = fixture();
    const queued = await f.service.requestMetadata("access", source);
    const record = (await f.store.get(queued.id))!;
    await f.store.replace(record.id, record.revision, {
      ...record,
      revision: record.revision + 1,
      state: "running",
      fence: 1,
      executionId: "crashed",
    });
    f.jobs.get(queued.id)!.state = "failed";
    f.setPermission(false);
    expect(await f.service.recover(queued.id)).toEqual([]);
    expect((await f.store.get(queued.id))?.state).toBe("failed");
  });
});

function latch() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
