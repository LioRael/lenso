import { afterAll, afterEach, beforeAll, describe, expect, test } from "bun:test";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";
import { createTaskQueue } from "@lenso/tasks";
import { createD1TaskProvider, type D1Database } from "@lenso/tasks/d1";
import { createD1MediaStore } from "../src/d1";
import { createTasksMediaAdapter } from "../src/tasks";
import { notesFixture } from "./notes-fixture";

let mf: Miniflare;
let database: D1Database & import("../src/d1").D1Database;
const cleanups: (() => Promise<void>)[] = [];
beforeAll(async () => {
  mf = new Miniflare({
    ...convertV4MiniflareOptions({
      modules: true,
      script: "export default { fetch() { return new Response('media-control-only'); } };",
      compatibilityDate: "2026-10-06",
      compatibilityFlags: ["nodejs_compat"],
      d1Databases: { DB: crypto.randomUUID() },
      d1Persist: false,
    }),
    host: "127.0.0.1",
    port: 0,
    telemetry: { enabled: false },
  });
  database = await mf.getD1Database("DB");
  for (const path of [
    "../../tasks/migrations/d1/0001_tasks.sql",
    "../migrations/d1/0001_media.sql",
  ]) {
    const sql = await Bun.file(new URL(path, import.meta.url)).text();
    for (const statement of sql
      .replace(/--[^\n]*/g, "")
      .split(";")
      .map((value) => value.trim())
      .filter(Boolean)) {
      await database.prepare(statement).all();
    }
  }
}, 30_000);
afterEach(async () => {
  for (const close of cleanups.splice(0).reverse()) await close();
});
afterAll(async () => {
  await mf?.dispose();
});
async function fixture() {
  const result = await notesFixture(database);
  cleanups.push(result.close);
  return result;
}
function latch() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

describe("Notes attachment, real Files/local Storage, local D1 Tasks, native Bun executor", () => {
  test("authorized existing attachment produces a private real thumbnail through Tasks", async () => {
    const f = await fixture();
    const requests = await Promise.all([f.request(), f.request(), f.request()]);
    expect(new Set(requests.map((value) => value.id)).size).toBe(1);
    const pending = requests[0]!;
    expect(pending.state).toBe("pending");
    const row = await f.store.get(pending.id);
    const queued = await f.queue.get(row!.jobId!);
    expect(queued?.state).toBe("pending");
    const entered = latch();
    const release = latch();
    f.controls.beforeRegister = async () => {
      entered.resolve();
      await release.promise;
    };
    const running = f.run();
    try {
      await entered.promise;
      expect(await f.media.status(f.alice, pending.id)).toMatchObject({
        state: "running",
        stage: "register",
      });
      const artifacts = await f.store.artifacts(pending.id);
      expect(artifacts).toHaveLength(1);
      await expect(f.files.read(f.alice, artifacts[0]!.fileId)).rejects.toMatchObject({
        code: "forbidden",
      });
    } finally {
      release.resolve();
      await running;
    }
    const ready = await f.media.status(f.alice, pending.id);
    expect(ready).toMatchObject({
      state: "ready",
      metadata: { format: "png", width: 80, height: 40 },
      cleanupPending: false,
    });
    expect((await f.queue.get(row!.jobId!))?.state).toBe("succeeded");
    const result = await f.media.result(f.alice, pending.id);
    const file = await f.files.metadata(f.alice, result.fileId);
    expect(file).toMatchObject({
      ownerId: "alice",
      tenantId: "local-notes",
      contentType: "image/webp",
      state: "ready",
    });
    const download = await f.files.read(f.alice, result.fileId);
    const bytes = new Uint8Array(await new Response(download.body).arrayBuffer());
    expect(new TextDecoder().decode(bytes.subarray(0, 4))).toBe("RIFF");
    const sharp = (await import("sharp")).default;
    expect(await sharp(bytes).metadata()).toMatchObject({
      format: "webp",
      width: 32,
      height: 32,
      hasAlpha: true,
    });
    expect(f.controls.puts).toBe(2); // existing source + one accepted derivative
    await expect(
      f.files.read(f.actor("alice", "other-tenant"), result.fileId),
    ).rejects.toMatchObject({ code: "forbidden" });
    await expect(f.files.read(f.actor("bob"), result.fileId)).rejects.toMatchObject({
      code: "forbidden",
    });
    f.controls.deriveDenied = true;
    await expect(f.files.read(f.alice, result.fileId)).rejects.toMatchObject({ code: "forbidden" });
    await expect(f.media.result(f.alice, pending.id)).rejects.toMatchObject({ code: "forbidden" });
  }, 30_000);

  test("metadata-only request decodes through Tasks without creating another file", async () => {
    const f = await fixture();
    const request = await f.media.requestMetadata(f.alice, f.note.attachments[0]!);
    await f.run();
    expect(await f.media.status(f.alice, request.id)).toMatchObject({
      state: "ready",
      result: null,
      metadata: { format: "png" },
    });
    expect(f.controls.puts).toBe(1);
  }, 30_000);

  test("raw JSON actor, cross-tenant source, and missing derive permission are denied", async () => {
    const f = await fixture();
    await expect(
      f.media.request(
        { ...f.alice },
        { source: f.note.attachments[0]!, preset: "notes-attachment" },
      ),
    ).rejects.toMatchObject({ code: "forbidden" });
    await expect(
      f.media.request(f.actor("alice", "other"), {
        source: f.note.attachments[0]!,
        preset: "notes-attachment",
      }),
    ).rejects.toMatchObject({ code: "forbidden" });
    f.controls.deriveDenied = true;
    await expect(f.request()).rejects.toMatchObject({ code: "forbidden" });
    expect(f.controls.puts).toBe(1);
  }, 30_000);

  test.each(["before", "after"] as const)(
    "upload %s provider acknowledgement fails without a published result",
    async (failure) => {
      const f = await fixture();
      const request = await f.request();
      f.controls.uploadError = failure;
      await f.run();
      const record = await f.store.get(request.id);
      expect(record?.state).not.toBe("ready");
      expect(record?.result).toBeNull();
      const artifacts = await f.store.artifacts(request.id);
      expect(artifacts).toHaveLength(1);
      expect(artifacts[0]?.state).toBe("deleted");
      expect((await f.objects.list()).objects).toHaveLength(1);
      await expect(f.files.read(f.alice, artifacts[0]!.fileId)).rejects.toMatchObject({
        code: "forbidden",
      });
    },
    30_000,
  );

  test("registration failure keeps a journal and explicit recovery collects failed cleanup", async () => {
    const f = await fixture();
    const request = await f.request();
    f.controls.registerError = "before";
    f.controls.deleteError = true;
    await f.run();
    const status = await f.media.status(f.alice, request.id);
    expect(status).toMatchObject({
      state: "pending",
      result: null,
      cleanupPending: true,
      error: { stage: "register", code: "dependency" },
    });
    const artifact = (await f.store.artifacts(request.id))[0]!;
    expect(artifact.state).toBe("discarding");
    await expect(f.files.read(f.alice, artifact.fileId)).rejects.toMatchObject({
      code: "forbidden",
    });
    f.controls.deleteError = false;
    expect(await f.media.recover(request.id)).toContainEqual({
      fileId: artifact.fileId,
      state: "deleted",
    });
    expect((await f.objects.list()).objects).toHaveLength(1);
    expect((await f.media.status(f.alice, request.id)).cleanupPending).toBe(false);
  }, 30_000);

  test("ambiguous committed registration is retained and not compensated", async () => {
    const f = await fixture();
    const request = await f.request();
    f.controls.registerError = "after";
    await f.run();
    expect((await f.media.status(f.alice, request.id)).state).toBe("ready");
    expect(f.controls.deletes).toBe(0);
    expect((await f.objects.list()).objects).toHaveLength(2);
  }, 30_000);

  test("revocation after upload blocks registration and cleanup uses separate restricted delegate", async () => {
    const f = await fixture();
    const request = await f.request();
    f.controls.afterPut = async () => {
      f.controls.revoked = true;
    };
    await f.run();
    expect(await f.store.get(request.id)).toMatchObject({
      state: "failed",
      result: null,
      error: { code: "forbidden" },
    });
    expect((await f.objects.list()).objects).toHaveLength(1);
    expect((await f.store.artifacts(request.id))[0]?.state).toBe("deleted");
  }, 30_000);

  test("source deletion invalidates delivery through Media and raw Files", async () => {
    const f = await fixture();
    const request = await f.request();
    await f.run();
    const result = await f.media.result(f.alice, request.id);
    await f.files.delete(f.alice, f.source.fileId);
    await expect(f.media.result(f.alice, request.id)).rejects.toMatchObject({
      code: "source-changed",
    });
    await expect(f.files.read(f.alice, result.fileId)).rejects.toMatchObject({
      code: "source-changed",
    });
    // Source deletion does not invent a second delete owner or auto-cascade.
    expect((await f.objects.list()).objects).toHaveLength(1);
  }, 30_000);

  test("pending cancellation prevents Tasks execution", async () => {
    const f = await fixture();
    const request = await f.request();
    expect((await f.media.cancel(f.alice, request.id)).state).toBe("cancelled");
    await f.run();
    expect(await f.store.artifacts(request.id)).toHaveLength(0);
    expect(f.controls.puts).toBe(1);
  }, 30_000);

  test("running cancellation rejects an already uploaded but unregistered file", async () => {
    const f = await fixture();
    const request = await f.request();
    const entered = latch();
    const release = latch();
    f.controls.afterPut = async () => {
      entered.resolve();
      await release.promise;
    };
    const work = f.run();
    try {
      await entered.promise;
      const value = await f.media.cancel(f.alice, request.id);
      expect(value.cancelRequested).toBe(true);
      expect(value.result).toBeNull();
    } finally {
      release.resolve();
      await work;
    }
    expect(await f.store.get(request.id)).toMatchObject({ state: "cancelled", result: null });
    expect((await f.objects.list()).objects).toHaveLength(1);
    expect((await f.store.artifacts(request.id))[0]?.state).toBe("deleted");
  }, 30_000);

  test("late superseded provider PUT is removed by revisiting the deletion tombstone", async () => {
    const f = await fixture();
    const request = await f.request();
    const jobId = (await f.store.get(request.id))!.jobId!;
    const entered = latch();
    const release = latch();
    f.controls.beforePut = async () => {
      f.controls.beforePut = undefined;
      entered.resolve();
      await release.promise;
    };
    const old = f.media.execute(request.id, {
      jobId,
      attempt: 1,
      signal: new AbortController().signal,
    });
    let oldFileId = "";
    try {
      await entered.promise;
      oldFileId = (await f.store.artifacts(request.id))[0]!.fileId;
      await f.media.execute(request.id, {
        jobId,
        attempt: 2,
        signal: new AbortController().signal,
      });
      expect((await f.store.getArtifact(oldFileId))?.state).toBe("deleted");
    } finally {
      release.resolve();
      await old;
    }
    expect((await f.objects.list()).objects).toHaveLength(2);
    expect(f.controls.deletes).toBeGreaterThanOrEqual(2);
    expect((await f.store.getArtifact(oldFileId))?.state).toBe("deleted");
    const result = await f.media.result(f.alice, request.id);
    expect(result.fileId).not.toBe(oldFileId);
    await expect(f.files.read(f.alice, oldFileId)).rejects.toMatchObject({ code: "forbidden" });
  }, 30_000);

  test("an old source revision cannot be reused even when its etag stays the same", async () => {
    const f = await fixture();
    const old = f.note.attachments[0]!;
    expect(
      await f.journal.queries.transition(f.source.fileId, f.source.revision, {
        ...f.source,
        revision: f.source.revision + 1,
        updatedAt: Date.now(),
      }),
    ).toBe(true);
    await expect(
      f.media.request(f.alice, { source: old, preset: "notes-attachment" }),
    ).rejects.toMatchObject({ code: "source-changed" });
    const latest = await f.files.metadata(f.alice, f.source.fileId);
    const accepted = await f.media.request(f.alice, {
      source: { fileId: latest.fileId, version: { revision: latest.revision, etag: latest.etag! } },
      preset: "notes-attachment",
    });
    expect(accepted.state).toBe("pending");
  }, 30_000);

  test("real corrupt image fails its Tasks handler without a business success illusion", async () => {
    const f = await fixture();
    const file = await f.files.upload(f.alice, {
      storageId: f.source.storageId,
      filename: "broken.jpg",
      contentType: "image/jpeg",
      ownerId: f.alice.subjectId,
      tenantId: f.alice.tenantId,
      maxBytes: 1024,
      body: new Blob([new Uint8Array([255, 216, 255, 0, 0])]).stream(),
    });
    const request = await f.media.request(f.alice, {
      source: { fileId: file.fileId, version: { revision: file.revision, etag: file.etag! } },
      preset: "notes-attachment",
    });
    await f.run();
    expect(await f.media.status(f.alice, request.id)).toMatchObject({
      state: "failed",
      result: null,
      error: { code: "invalid-image", stage: "decode", retryable: false },
    });
    expect(await f.store.artifacts(request.id)).toHaveLength(0);
  }, 30_000);

  test("D1 dedup survives a new producer and retry leaves one winning registered file", async () => {
    const f = await fixture();
    const request = await f.request();
    const jobId = (await f.store.get(request.id))!.jobId!;
    const producer = createTaskQueue({
      provider: await createD1TaskProvider({ database, queueName: f.queueName }),
      tasks: [f.task],
    });
    try {
      expect(await producer.identity()).toEqual(await f.queue.identity());
      expect(await createTasksMediaAdapter(producer, f.task).ensure(request.id)).toBe(jobId);
    } finally {
      await producer.close();
    }
    f.controls.uploadError = "after";
    await f.run();
    expect(await f.queue.get(jobId)).toMatchObject({ state: "pending", attempt: 1 });
    expect((await f.store.get(request.id))?.result).toBeNull();
    f.controls.uploadError = "";
    f.controls.clock += 60_000;
    await f.run();
    expect(await f.queue.get(jobId)).toMatchObject({ state: "succeeded", attempt: 2 });
    expect((await f.media.status(f.alice, request.id)).state).toBe("ready");
    const artifacts = await f.store.artifacts(request.id);
    expect(artifacts).toHaveLength(2);
    expect(artifacts.filter((artifact) => artifact.state === "staged")).toHaveLength(1);
    expect((await f.objects.list()).objects).toHaveLength(2);
  }, 30_000);
});

test("Media D1 store performs real conditional writes in local workerd", async () => {
  const f = await fixture();
  const request = await f.request();
  const record = (await f.store.get(request.id))!;
  const store = createD1MediaStore(database);
  expect(await Promise.all([store.insert(record), store.insert(record)])).toEqual([true, false]);
  const next = { ...record, revision: record.revision + 1 };
  const races = await Promise.all([
    store.replace(record.id, record.revision, next),
    store.replace(record.id, record.revision, next),
  ]);
  expect(races.filter(Boolean)).toHaveLength(1);
  const artifact = {
    fileId: crypto.randomUUID(),
    derivationId: record.id,
    revision: 0,
    state: "staged" as const,
    fence: 1,
    executionId: "worker",
    createdAt: Date.now(),
  };
  await store.insertArtifact(artifact);
  expect(await store.getArtifact(artifact.fileId)).toEqual(artifact);
  expect(await store.artifacts(record.id)).toEqual([artifact]);
}, 30_000);
