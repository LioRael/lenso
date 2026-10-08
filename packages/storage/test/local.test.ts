import { afterEach, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdtemp, readdir, rm, symlink, unlink, writeFile, mkdir, rename } from "node:fs/promises";
import { join, resolve } from "node:path";
import { startApp } from "@lenso/core";
import { createLocalStoragePlugin } from "../src/local";
import { StorageError, type ObjectStorage, type StorageErrorCode } from "../src/index";

const cleanups: (() => void | Promise<void>)[] = [];
const roots: string[] = [];
afterEach(async () => {
  for (const cleanup of cleanups.splice(0)) await cleanup();
  for (const root of roots.splice(0)) await rm(root, { recursive: true, force: true });
});

async function fixture() {
  // macOS /tmp is a symlink; use the real checkout ancestry.
  const base = await mkdtemp(join(resolve("."), ".storage-test-"));
  roots.push(base);
  const root = join(base, "objects");
  return { base, root, storage: await instance(root, "one") };
}

async function instance(root: string, id: string): Promise<ObjectStorage> {
  const plugin = createLocalStoragePlugin({ id, root });
  const app = await startApp({ plugins: [plugin] });
  cleanups.push(() => app.stop());
  return app.get(plugin);
}

function chunks(...values: string[]): ReadableStream<Uint8Array> {
  let index = 0;
  return new ReadableStream(
    {
      pull(controller) {
        if (index === values.length) controller.close();
        else controller.enqueue(new TextEncoder().encode(values[index++]));
      },
    },
    { highWaterMark: 0 },
  );
}

function name(key: string) {
  return createHash("sha256").update(key).digest("hex");
}

async function errorCode(promise: Promise<unknown>, code: StorageErrorCode) {
  try {
    await promise;
    throw new Error("Expected rejection");
  } catch (error) {
    expect(error).toBeInstanceOf(StorageError);
    expect((error as StorageError).code).toBe(code);
  }
}

test("multi-chunk streaming roundtrip persists metadata across instances", async () => {
  const { root, storage } = await fixture();
  const text = "a".repeat(200_000) + "middle" + "z".repeat(100_000);
  const metadata = await storage.put({
    key: "photos/é.jpg",
    body: chunks("a".repeat(200_000), "middle", "z".repeat(100_000)),
    size: text.length,
    contentType: "image/jpeg",
    customMetadata: { owner: "alice" },
  });
  const other = await instance(root, "two");
  expect(await other.head(metadata.key)).toEqual(metadata);
  const download = await other.get(metadata.key, { ifMatch: metadata.etag });
  expect(await new Response(download.body).text()).toBe(text);
  expect(download.metadata).toEqual(metadata);
  expect(metadata.etag).toBe(`"${createHash("sha256").update(text).digest("hex")}"`);
  expect((await readdir(root)).length).toBe(1);
});

test("create-only duplicate including competing instances never overwrites", async () => {
  const { root, storage } = await fixture();
  const other = await instance(root, "two");
  const results = await Promise.allSettled([
    storage.put({ key: "same", body: chunks("first") }),
    other.put({ key: "same", body: chunks("second") }),
  ]);
  expect(results.filter((result) => result.status === "fulfilled")).toHaveLength(1);
  const loser = results.find((result) => result.status === "rejected") as PromiseRejectedResult;
  expect(loser.reason.code).toBe("conflict");
  const original = await new Response((await storage.get("same")).body).text();
  await errorCode(other.put({ key: "same", body: chunks("replacement") }), "conflict");
  expect(await new Response((await other.get("same")).body).text()).toBe(original);
  expect(await readdir(root)).toEqual([name("same")]);
});

test("ranges, conditional reads, empty files and unsupported signing", async () => {
  const { storage } = await fixture();
  const metadata = await storage.put({ key: "a", body: chunks("abc", "def") });
  const range = await storage.get("a", {
    range: { offset: 2, length: 99 },
    ifMatch: metadata.etag,
  });
  expect(range.range).toEqual({ offset: 2, length: 4 });
  expect(await new Response(range.body).text()).toBe("cdef");
  expect(
    await new Response((await storage.get("a", { range: { offset: 1, length: 2 } })).body).text(),
  ).toBe("bc");
  await errorCode(storage.get("a", { ifMatch: '"wrong"' }), "conflict");
  await errorCode(storage.get("a", { range: { offset: 6 } }), "invalid-input");
  await errorCode(storage.get("a", { range: { offset: -1 } }), "invalid-input");
  await errorCode(storage.get("a", { range: { offset: 0, length: 0 } }), "invalid-input");
  await storage.put({ key: "empty", body: chunks(), maxBytes: 0, size: 0 });
  expect(await new Response((await storage.get("empty")).body).text()).toBe("");
  await errorCode(storage.signDownload({ key: "a", expiresIn: 60 }), "unsupported");
  await errorCode(
    storage.signUpload({ key: "a", expiresIn: 60, contentType: "text/plain" }),
    "unsupported",
  );
});

test("list key pagination, prefix and idempotent delete", async () => {
  const { root, storage } = await fixture();
  for (const key of ["a/3", "b", "a/1", "a/2"]) await storage.put({ key, body: chunks(key) });
  await mkdir(join(root, ".tmp-owned"));
  const first = await storage.list({ prefix: "a/", limit: 2 });
  expect(first.objects.map((object) => object.key)).toEqual(["a/1", "a/2"]);
  expect(first.cursor).toBe("a/2");
  const second = await storage.list({ prefix: "a/", cursor: first.cursor, limit: 2 });
  expect(second.objects.map((object) => object.key)).toEqual(["a/3"]);
  expect(second.cursor).toBeUndefined();
  expect(await storage.delete("a/1")).toEqual({ outcome: "deleted" });
  expect(await storage.delete("a/1")).toEqual({ outcome: "not-found" });
  expect(await storage.head("a/1")).toBeNull();
  await errorCode(storage.get("a/1"), "not-found");
  await errorCode(storage.list({ limit: 0 }), "invalid-input");
  await errorCode(storage.list({ cursor: "../a" }), "invalid-key");
});

test("deletion retries recover complete and partially removed per-key tombs", async () => {
  const { root, storage } = await fixture();
  for (const partial of [false, true]) {
    const key = partial ? "partial" : "complete";
    await storage.put({ key, body: chunks("private bytes") });
    const tomb = join(root, `.delete-${name(key)}`);
    await rename(join(root, name(key)), tomb); // Crash/failure after logical removal.
    if (partial) await unlink(join(tomb, "metadata.json"));
    expect(await storage.list()).toEqual({ objects: [] });
    expect(await storage.delete(key)).toEqual({ outcome: "deleted" });
    expect(await storage.delete(key)).toEqual({ outcome: "not-found" });
    expect(await readdir(root)).toEqual([]);
  }
});

test("upload limits, mismatched size and source failures leave no artifacts", async () => {
  const { storage, root } = await fixture();
  await errorCode(
    storage.put({ key: "over", body: chunks("abc", "def"), maxBytes: 4 }),
    "too-large",
  );
  await errorCode(storage.put({ key: "short", body: chunks("abc"), size: 5 }), "invalid-input");
  await errorCode(storage.put({ key: "long", body: chunks("abcdef"), size: 5 }), "invalid-input");
  await errorCode(storage.put({ key: "invalid", body: chunks("x"), size: -1 }), "invalid-input");
  let calls = 0;
  const cause = new Error("source failed");
  const body = new ReadableStream<Uint8Array>({
    pull(controller) {
      if (calls++ === 0) controller.enqueue(new Uint8Array([1, 2]));
      else controller.error(cause);
    },
  });
  const error = await storage.put({ key: "failed", body }).catch((failure: unknown) => failure);
  expect(error).toBeInstanceOf(StorageError);
  expect((error as StorageError).cause).toBe(cause);
  expect(await readdir(root)).toEqual([]);
});

test("abort interrupts a source blocked in read and removes partial upload", async () => {
  const { storage, root } = await fixture();
  const abort = new AbortController();
  let started!: () => void;
  const ready = new Promise<void>((complete) => {
    started = complete;
  });
  let cancelled = false;
  const body = new ReadableStream<Uint8Array>(
    {
      pull() {
        started();
        return new Promise<void>(() => {});
      },
      cancel() {
        cancelled = true;
      },
    },
    { highWaterMark: 0 },
  );
  const upload = storage.put({ key: "blocked", body, signal: abort.signal });
  await ready;
  abort.abort();
  await errorCode(upload, "aborted");
  expect(cancelled).toBe(true);
  expect(await readdir(root)).toEqual([]);
  const before = new AbortController();
  before.abort();
  await errorCode(storage.put({ key: "pre", body: chunks("x"), signal: before.signal }), "aborted");
}, 2000);

test("rejects traversal and unsafe root ancestry", async () => {
  const { storage, base, root } = await fixture();
  for (const key of ["../secret", "/absolute", "a/../b", "a//b", "a\\b", "C:foo", "a\0b", ""]) {
    await errorCode(storage.put({ key, body: chunks("x") }), "invalid-key");
    await errorCode(storage.get(key), "invalid-key");
    await errorCode(storage.head(key), "invalid-key");
    await errorCode(storage.delete(key), "invalid-key");
  }
  await symlink(root, join(base, "link"));
  await errorCode(instance(join(base, "link", "nested"), "unsafe"), "forbidden");
  await rename(root, join(base, "real"));
  await symlink(join(base, "real"), root);
  await errorCode(storage.head("a"), "forbidden");
});

test("rejects object/payload/metadata symlinks and untrusted list entries", async () => {
  const { storage, base, root } = await fixture();
  await storage.put({ key: "a", body: chunks("value") });
  const payload = join(root, name("a"), "payload");
  await writeFile(join(base, "secret"), "secret");
  await unlink(payload);
  await symlink(join(base, "secret"), payload);
  await errorCode(storage.get("a"), "forbidden");
  await errorCode(storage.head("a"), "forbidden");
  await errorCode(storage.delete("a"), "forbidden");
  await errorCode(storage.list(), "forbidden");
  await unlink(payload);
  await writeFile(payload, "value");
  const metadata = join(root, name("a"), "metadata.json");
  await rename(metadata, join(base, "metadata"));
  await symlink(join(base, "metadata"), metadata);
  await errorCode(storage.head("a"), "forbidden");
  await unlink(metadata);
  await rename(join(base, "metadata"), metadata);
  await rename(join(root, name("a")), join(base, "object"));
  await symlink(join(base, "object"), join(root, name("a")));
  await errorCode(storage.get("a"), "forbidden");
  await unlink(join(root, name("a")));
  await writeFile(join(root, "unexpected"), "ignored?");
  await errorCode(storage.list(), "forbidden");
  await unlink(join(root, "unexpected"));
  await symlink(base, join(root, ".tmp-unsafe"));
  await errorCode(storage.list(), "forbidden");
});

test("metadata key cannot be forged by moving an object directory", async () => {
  const { storage, root } = await fixture();
  await storage.put({ key: "original", body: chunks("value") });
  await rename(join(root, name("original")), join(root, name("forged")));
  await errorCode(storage.get("forged"), "provider");
  await errorCode(storage.list(), "provider");
});

test("an open download keeps its snapshot after delete and recreate", async () => {
  const { storage } = await fixture();
  const first = await storage.put({ key: "a", body: chunks("first") });
  const download = await storage.get("a", { ifMatch: first.etag });
  await storage.delete("a");
  const second = await storage.put({ key: "a", body: chunks("other") });
  expect(second.etag).not.toBe(first.etag);
  expect(await new Response(download.body).text()).toBe("first");
  await errorCode(storage.get("a", { ifMatch: first.etag }), "conflict");
});

test("incomplete committed objects are errors, not absent", async () => {
  const { root, storage } = await fixture();
  await storage.put({ key: "a", body: chunks("value") });
  await unlink(join(root, name("a"), "payload"));
  await errorCode(storage.head("a"), "forbidden");
  await errorCode(storage.delete("a"), "forbidden");
  await errorCode(storage.get("a"), "forbidden");
});

test("cleanup aborts unconsumed downloads without deleting stored objects", async () => {
  const { root, storage } = await fixture();
  await storage.put({ key: "a", body: chunks("value") });
  const download = await storage.get("a");
  await cleanups.shift()!();
  await errorCode(new Response(download.body).text(), "aborted");
  const other = await instance(root, "other");
  expect(await new Response((await other.get("a")).body).text()).toBe("value");
});

test("cleanup interrupts an owned blocked upload and removes only its partial artifacts", async () => {
  const { storage, root } = await fixture();
  let started!: () => void;
  const ready = new Promise<void>((complete) => {
    started = complete;
  });
  const body = new ReadableStream<Uint8Array>(
    {
      pull() {
        started();
        return new Promise<void>(() => {});
      },
    },
    { highWaterMark: 0 },
  );
  const upload = storage.put({ key: "blocked", body });
  const rejected = errorCode(upload, "aborted");
  await ready;
  await cleanups.shift()!();
  await rejected;
  expect(await readdir(root)).toEqual([]);
}, 2000);
