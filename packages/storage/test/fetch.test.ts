import { expect, test } from "bun:test";
import { createFileDownloadHandler, createFileUploadHandler } from "../src/fetch";
import { StorageError } from "../src/index";
import type { Files, FileRecord } from "../src/files";

function fixture() {
  const file: FileRecord = {
    fileId: "id",
    storageId: "private",
    objectKey: "generated",
    filename: "photo.txt",
    contentType: "text/plain",
    ownerId: "alice",
    tenantId: null,
    state: "ready",
    revision: 1,
    size: 5,
    expectedSize: null,
    maxBytes: 10,
    etag: "etag",
    uploadExpiresAt: null,
    createdAt: 0,
    updatedAt: 0,
  };
  const events: string[] = [];
  function authorize(access: string) {
    if (access !== file.ownerId) throw new StorageError("forbidden", "Denied");
  }
  const files: Files<string> = {
    async metadata(access) {
      authorize(access);
      events.push("metadata");
      return file;
    },
    async read(access) {
      authorize(access);
      events.push("read");
      return {
        metadata: { key: file.objectKey, size: 5, contentType: file.contentType, etag: file.etag! },
        body: new Response("hello").body!,
      };
    },
    async delete(access) {
      authorize(access);
      events.push("delete");
      return { ...file, state: "deleted" };
    },
    async upload(access, input) {
      authorize(access);
      expect(input.ownerId).toBe("alice");
      expect(input.storageId).toBe("private");
      expect(await new Response(input.body).text()).toBe("hello");
      events.push("upload");
      return file;
    },
    async beginUpload() {
      throw new Error("unused");
    },
    async completeUpload() {
      throw new Error("unused");
    },
    async signDownload() {
      throw new Error("unused");
    },
  };
  const authenticate = (request: Request) => request.headers.get("authorization") ?? "anonymous";
  return { files, events, authenticate };
}

test("raw GET HEAD DELETE authenticate and invoke authorized service operations", async () => {
  const f = fixture();
  const handler = createFileDownloadHandler({ ...f, fileId: () => "id" });
  const context = (method: string, actor = "alice") => ({
    request: new Request("https://app.invalid/files/id", {
      method,
      headers: { authorization: actor },
    }),
  });
  const response = await handler(context("GET"));
  expect(await response!.text()).toBe("hello");
  expect(response!.headers.get("content-type")).toBe("text/plain");
  expect(response!.headers.get("etag")).toBe("etag");
  const head = await handler(context("HEAD"));
  expect(head!.body).toBeNull();
  expect(head!.headers.get("content-length")).toBe("5");
  expect((await handler(context("DELETE")))!.status).toBe(204);
  expect((await handler(context("GET", "mallory")))!.status).toBe(403);
  expect(f.events).toEqual(["read", "metadata", "delete"]);
  expect(await handler(context("POST"))).toBeUndefined();
});

test("raw upload streams bytes; app selects ownership and storage instead of caller fields", async () => {
  const f = fixture();
  const handler = createFileUploadHandler({
    ...f,
    input: (_request, access) => ({
      storageId: "private",
      ownerId: access,
      filename: "app.txt",
      contentType: "text/plain",
      maxBytes: 10,
    }),
  });
  const request = new Request(
    "https://app.invalid/upload?ownerId=mallory&key=arbitrary&storageId=public",
    {
      method: "PUT",
      headers: { authorization: "alice" },
      body: "hello",
    },
  );
  const response = await handler({ request });
  expect(response!.status).toBe(201);
  expect((await response!.json()).fileId).toBe("id");
  expect(f.events).toEqual(["upload"]);
  expect(
    (await handler({
      request: new Request("https://app.invalid/upload", { method: "PUT", body: "hello" }),
    }))!.status,
  ).toBe(403);
});
