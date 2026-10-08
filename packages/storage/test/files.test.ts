import { afterEach, describe, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { drizzle } from "drizzle-orm/bun-sqlite";
import { sql } from "drizzle-orm";
import { definePlugin } from "@lenso/core/plugin";
import { startApp } from "@lenso/core";
import { createFilesPlugin, type FileQueries, type FileRecord } from "../src/files";
import { createSqliteFileQueries, fileSchema } from "../src/sqlite";
import { StorageError, type ObjectMetadata, type ObjectStorage } from "../src/index";

const cleanups: (() => void | Promise<void>)[] = [];
afterEach(async () => {
  for (const cleanup of cleanups.splice(0).reverse()) await cleanup();
});
const body = (text: string) => new Response(text).body!;
const input = {
  storageId: "objects",
  filename: "../not-an-object-key.txt",
  contentType: "text/plain",
  ownerId: "alice",
};

async function fixture(
  options: {
    deny?: boolean;
    noAuthorizer?: boolean;
    unmigrated?: boolean;
    wrapQueries?: (queries: FileQueries) => FileQueries;
  } = {},
) {
  const client = new Database(":memory:");
  cleanups.push(() => client.close());
  const db = drizzle(client, { schema: fileSchema });
  const migration = await Bun.file(
    new URL("../migrations/sqlite/0001_files.sql", import.meta.url),
  ).text();
  if (!options.unmigrated) db.run(sql.raw(migration));
  const objects = new Map<string, { metadata: ObjectMetadata; text: string }>();
  const controls = {
    deleteError: false,
    deletes: 0,
    headWait: undefined as Promise<void> | undefined,
    getIfMatch: undefined as string | undefined,
  };
  const storage: ObjectStorage = {
    id: "objects",
    capabilities: {
      provider: "s3",
      signedUpload: true,
      signedDownload: true,
      rangeRead: false,
      conditionalRead: true,
      pagination: "key",
      uploadCancellation: "abort",
      uploadRequiresSize: false,
    },
    async put(value) {
      if (objects.has(value.key)) throw new StorageError("conflict", "Exists");
      const text = await new Response(value.body).text();
      const metadata = {
        key: value.key,
        size: new TextEncoder().encode(text).length,
        contentType: value.contentType!,
        etag: "snapshot",
      };
      objects.set(value.key, { metadata, text });
      return metadata;
    },
    async get(key, readOptions) {
      controls.getIfMatch = readOptions?.ifMatch;
      const object = objects.get(key);
      if (!object) throw new StorageError("not-found", "Missing");
      if (readOptions?.ifMatch && readOptions.ifMatch !== object.metadata.etag)
        throw new StorageError("conflict", "Changed");
      return { metadata: object.metadata, body: body(object.text) };
    },
    async head(key) {
      await controls.headWait;
      return objects.get(key)?.metadata ?? null;
    },
    async delete(key) {
      controls.deletes++;
      if (controls.deleteError) throw new Error("cleanup");
      return { outcome: objects.delete(key) ? "deleted" : "not-found" };
    },
    async list() {
      return { objects: [] };
    },
    async signUpload(value) {
      return {
        url: "https://objects.invalid/upload",
        method: "PUT",
        headers: { "if-none-match": "*" },
        expiresAt: new Date(Date.now() + value.expiresIn * 1000 - 10),
        conditions: {
          createOnly: true,
          contentType: value.contentType,
          maxBytes: value.maxBytes,
          sizeEnforcement: "completion-check",
        },
      };
    },
    async signDownload(value) {
      return {
        url: "https://objects.invalid/download",
        method: "GET",
        headers: {},
        expiresAt: new Date(Date.now() + value.expiresIn * 1000),
        conditions: { ifMatch: value.ifMatch, sizeEnforcement: "none" },
      };
    },
  };
  const database = definePlugin({ id: "db", setup: () => db });
  const storageRef = definePlugin({ id: "objects", setup: () => storage });
  const queries = createSqliteFileQueries(db);
  const plugin = createFilesPlugin({
    id: "files",
    database,
    storages: [storageRef],
    queries: () => options.wrapQueries?.(queries) ?? queries,
    authorize: options.noAuthorizer
      ? undefined
      : ({ access, file }) => !options.deny && access === file.ownerId,
  });
  const app = await startApp({ plugins: [database, storageRef, plugin] });
  cleanups.push(() => app.stop());
  return { files: app.get(plugin), queries, db, migration, objects, controls, storage };
}

describe("file records", () => {
  test("migration is explicit, ordinary streams roundtrip, keys are independent and reads pin etag", async () => {
    const f = await fixture({ unmigrated: true });
    await expect(f.files.upload("alice", { ...input, body: body("hi") })).rejects.toThrow();
    f.db.run(sql.raw(f.migration));
    const file = await f.files.upload("alice", { ...input, body: body("hi"), size: 2 });
    expect(file.state).toBe("ready");
    expect(file.fileId).toMatch(/^[a-f0-9-]{36}$/);
    expect(file.objectKey).not.toContain(input.filename);
    expect(await new Response((await f.files.read("alice", file.fileId)).body).text()).toBe("hi");
    expect(f.controls.getIfMatch).toBe("snapshot");
    await expect(f.files.metadata("mallory", file.fileId)).rejects.toMatchObject({
      code: "forbidden",
    });
  });

  test("authorization defaults to deny", async () => {
    const f = await fixture({ noAuthorizer: true });
    await expect(f.files.upload("alice", { ...input, body: body("x") })).rejects.toMatchObject({
      code: "forbidden",
    });
    expect(f.objects.size).toBe(0);
  });

  test("locked input and source cleanup failures persist failed rather than uploading", async () => {
    let inserted: FileRecord | undefined;
    const f = await fixture({
      wrapQueries: (queries) => ({
        ...queries,
        async insert(file) {
          inserted = file;
          await queries.insert(file);
        },
      }),
    });
    const locked = body("hi");
    const reader = locked.getReader();
    try {
      await expect(f.files.upload("alice", { ...input, body: locked })).rejects.toThrow();
      expect((await f.queries.get(inserted!.fileId))?.state).toBe("failed");
    } finally {
      reader.releaseLock();
    }

    const cleanupFailure = new Error("source cleanup");
    const result = await f.files
      .upload("alice", {
        ...input,
        maxBytes: 1,
        body: new ReadableStream({
          pull(controller) {
            controller.enqueue(new Uint8Array(2));
          },
          cancel() {
            throw cleanupFailure;
          },
        }),
      })
      .catch((error: unknown) => error);
    expect(result).toBeInstanceOf(AggregateError);
    expect((result as AggregateError).errors[0]).toMatchObject({ code: "too-large" });
    expect((result as AggregateError).errors[1]).toBe(cleanupFailure);
    expect((await f.queries.get(inserted!.fileId))?.state).toBe("failed");
    expect(f.objects.size).toBe(0);
  });

  test.each(["provider", "conflict"] as const)(
    "write failure %s compensates owned keys but preserves conflicting objects",
    async (code) => {
      const f = await fixture();
      const put = f.storage.put;
      f.storage.put = async (value) => {
        await put(value);
        throw new StorageError(code, "Write acknowledgement failed");
      };
      await expect(f.files.upload("alice", { ...input, body: body("hi") })).rejects.toMatchObject({
        code,
      });
      expect(f.controls.deletes).toBe(code === "provider" ? 1 : 0);
      expect(f.objects.size).toBe(code === "provider" ? 0 : 1);
    },
  );

  test.each(["missing", "forged", "oversize", "no-etag", "wrong-size"] as const)(
    "direct completion rejects %s provider metadata",
    async (reason) => {
      const f = await fixture();
      const { file } = await f.files.beginUpload("alice", {
        ...input,
        size: reason === "wrong-size" ? 1 : undefined,
        maxBytes: 2,
        expiresIn: 60,
      });
      if (reason !== "missing")
        f.objects.set(file.objectKey, {
          text: "abc",
          metadata: {
            key: file.objectKey,
            contentType: reason === "forged" ? "application/forged" : input.contentType,
            size: reason === "oversize" ? 3 : 2,
            etag: reason === "no-etag" ? undefined : "value",
          },
        });
      await expect(f.files.completeUpload("alice", file.fileId)).rejects.toThrow();
      expect((await f.queries.get(file.fileId))?.state).toBe("failed");
      expect(f.objects.size).toBe(0);
    },
  );

  test("direct uploads reject unsafe adapter capabilities and links", async () => {
    const f = await fixture();
    Object.defineProperty(f.storage, "capabilities", {
      value: { ...f.storage.capabilities, conditionalRead: false },
    });
    await expect(
      f.files.beginUpload("alice", { ...input, maxBytes: 3, expiresIn: 60 }),
    ).rejects.toMatchObject({ code: "unsupported" });
    Object.defineProperty(f.storage, "capabilities", {
      value: { ...f.storage.capabilities, conditionalRead: true },
    });
    const sign = f.storage.signUpload;
    f.storage.signUpload = async (value) => {
      const link = await sign(value);
      link.conditions.createOnly = false;
      return link;
    };
    await expect(
      f.files.beginUpload("alice", { ...input, maxBytes: 3, expiresIn: 60 }),
    ).rejects.toMatchObject({ code: "unsupported" });
  });

  test("direct completion ignores caller metadata and publishes only verified head", async () => {
    const f = await fixture();
    const { file } = await f.files.beginUpload("alice", {
      ...input,
      size: 2,
      maxBytes: 3,
      expiresIn: 60,
    });
    f.objects.set(file.objectKey, {
      text: "hi",
      metadata: { key: file.objectKey, size: 2, contentType: input.contentType, etag: "real" },
    });
    const ready = await f.files.completeUpload("alice", file.fileId);
    expect(ready.etag).toBe("real");
    await expect(f.files.delete("alice", file.fileId)).rejects.toMatchObject({ code: "conflict" });
    f.objects.get(file.objectKey)!.metadata.etag = "tampered";
    await expect(f.files.read("alice", file.fileId)).rejects.toMatchObject({ code: "conflict" });
  });

  test("a completed direct object can be verified after its PUT credential expires", async () => {
    const f = await fixture();
    const { file } = await f.files.beginUpload("alice", { ...input, maxBytes: 3, expiresIn: 60 });
    const expired = { ...file, revision: file.revision + 1, uploadExpiresAt: Date.now() - 1 };
    expect(await f.queries.transition(file.fileId, file.revision, expired)).toBe(true);
    f.objects.set(file.objectKey, {
      text: "hi",
      metadata: { key: file.objectKey, size: 2, contentType: input.contentType, etag: "real" },
    });
    expect((await f.files.completeUpload("alice", file.fileId)).state).toBe("ready");
    expect((await f.files.delete("alice", file.fileId)).state).toBe("deleted");
  });

  test("failed ready persistence compensates the owned object and retains recovery errors", async () => {
    let inserted: FileRecord | undefined;
    const failure = new Error("metadata");
    const f = await fixture({
      wrapQueries: (queries) => ({
        ...queries,
        async insert(file) {
          inserted = file;
          await queries.insert(file);
        },
        async transition(id, revision, next) {
          if (next.state === "ready") throw failure;
          return queries.transition(id, revision, next);
        },
      }),
    });
    await expect(f.files.upload("alice", { ...input, body: body("hi") })).rejects.toBe(failure);
    expect(f.objects.size).toBe(0);
    expect((await f.queries.get(inserted!.fileId))?.state).toBe("failed");
    f.controls.deleteError = true;
    try {
      await f.files.upload("alice", { ...input, body: body("hi") });
      throw new Error("Expected failure");
    } catch (error) {
      expect(error).toBeInstanceOf(AggregateError);
      expect((error as AggregateError).errors[0]).toBe(failure);
      expect((error as AggregateError).errors[1].message).toBe("cleanup");
    }
  });

  test("DB failure during recovery is retained after original metadata failure", async () => {
    const original = new Error("ready persistence");
    const recovery = new Error("failed persistence");
    const f = await fixture({
      wrapQueries: (queries) => ({
        ...queries,
        async transition(id, revision, next) {
          if (next.state === "ready") throw original;
          if (next.state === "failed") throw recovery;
          return queries.transition(id, revision, next);
        },
      }),
    });
    try {
      await f.files.upload("alice", { ...input, body: body("hi") });
      throw new Error("Expected failure");
    } catch (error) {
      expect((error as AggregateError).errors).toEqual([original, recovery]);
    }
    // Recovery could not claim a failed state. Preserve the object for reconciliation.
    expect(f.objects.size).toBe(1);
  });

  test.each(["stream", "direct"] as const)(
    "lost ready DB response preserves the published %s object",
    async (mode) => {
      let inserted: FileRecord | undefined;
      const lostResponse = new Error("DB committed but response was lost");
      const f = await fixture({
        wrapQueries: (queries) => ({
          ...queries,
          async insert(file) {
            inserted = file;
            await queries.insert(file);
          },
          async transition(id, revision, next) {
            const committed = await queries.transition(id, revision, next);
            if (next.state === "ready" && committed) throw lostResponse;
            return committed;
          },
        }),
      });
      let operation: Promise<unknown>;
      if (mode === "stream") operation = f.files.upload("alice", { ...input, body: body("hi") });
      else {
        const { file } = await f.files.beginUpload("alice", {
          ...input,
          maxBytes: 3,
          expiresIn: 60,
        });
        f.objects.set(file.objectKey, {
          text: "hi",
          metadata: { key: file.objectKey, size: 2, contentType: input.contentType, etag: "real" },
        });
        operation = f.files.completeUpload("alice", file.fileId);
      }
      const result = await operation.catch((error: unknown) => error);
      expect(result).toBeInstanceOf(AggregateError);
      expect((result as AggregateError).errors[0]).toBe(lostResponse);
      expect((await f.queries.get(inserted!.fileId))?.state).toBe("ready");
      expect(f.controls.deletes).toBe(0);
      expect(await new Response((await f.files.read("alice", inserted!.fileId)).body).text()).toBe(
        "hi",
      );
    },
  );

  test("delete failure is retriable and every retry remains authorized; success leaves tombstone", async () => {
    const f = await fixture();
    const file = await f.files.upload("alice", { ...input, body: body("hi") });
    f.controls.deleteError = true;
    await expect(f.files.delete("alice", file.fileId)).rejects.toThrow("cleanup");
    expect((await f.queries.get(file.fileId))?.state).toBe("deleting");
    await expect(f.files.delete("mallory", file.fileId)).rejects.toMatchObject({
      code: "forbidden",
    });
    f.controls.deleteError = false;
    expect((await f.files.delete("alice", file.fileId)).state).toBe("deleted");
    expect((await f.files.delete("alice", file.fileId)).state).toBe("deleted");
    await expect(f.files.read("alice", file.fileId)).rejects.toMatchObject({ code: "conflict" });
  });

  test("persisted CAS prevents concurrent completion and deletion during completion", async () => {
    const f = await fixture();
    const { file } = await f.files.beginUpload("alice", { ...input, maxBytes: 3, expiresIn: 60 });
    f.objects.set(file.objectKey, {
      text: "hi",
      metadata: { key: file.objectKey, size: 2, contentType: input.contentType, etag: "real" },
    });
    let release!: () => void;
    f.controls.headWait = new Promise<void>((resolve) => {
      release = resolve;
    });
    const completing = f.files.completeUpload("alice", file.fileId);
    await new Promise((resolve) => setTimeout(resolve, 1));
    await expect(f.files.completeUpload("alice", file.fileId)).rejects.toMatchObject({
      code: "conflict",
    });
    await expect(f.files.delete("alice", file.fileId)).rejects.toMatchObject({ code: "conflict" });
    release();
    expect((await completing).state).toBe("ready");
  });
});
