import { afterEach, expect, test } from "bun:test";
import { S3Client } from "@aws-sdk/client-s3";
import { startApp } from "@lenso/core";
import { createS3StoragePlugin } from "../src/s3";

const cleanups: (() => void | Promise<void>)[] = [];
afterEach(async () => {
  for (const cleanup of cleanups.splice(0).reverse()) await cleanup();
});
const body = (value: string | Uint8Array<ArrayBuffer>) => new Blob([value]).stream();
const xml = (value: string, status = 200) =>
  new Response(value, { status, headers: { "content-type": "application/xml" } });
const error = (code: string, status: number) =>
  xml(`<Error><Code>${code}</Code><Message>private provider detail</Message></Error>`, status);

async function fixture(
  options: {
    multipart?: boolean;
    completeFailure?: boolean;
    partFailure?: boolean;
    ignoreReadConditions?: boolean;
  } = {},
) {
  const objects = new Map<string, { bytes: Uint8Array<ArrayBuffer>; contentType: string }>();
  const parts = new Map<number, Uint8Array>();
  const requests: { method: string; url: URL; headers: Headers }[] = [];
  const server = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    async fetch(request) {
      const url = new URL(request.url);
      requests.push({ method: request.method, url, headers: request.headers });
      const key = decodeURIComponent(url.pathname.replace(/^\/bucket\//, ""));
      if (key === "forbidden") return error("AccessDenied", 403);
      if (url.searchParams.get("list-type") === "2") {
        return xml(
          `<ListBucketResult><IsTruncated>true</IsTruncated><NextContinuationToken>opaque+/=</NextContinuationToken>${[
            ...objects.entries(),
          ]
            .map(
              ([objectKey, object]) =>
                `<Contents><Key>${objectKey}</Key><Size>${object.bytes.length}</Size><ETag>"etag"</ETag></Contents>`,
            )
            .join("")}</ListBucketResult>`,
        );
      }
      if (request.method === "POST" && url.searchParams.has("uploads")) {
        return xml(
          "<InitiateMultipartUploadResult><Bucket>bucket</Bucket><Key>large</Key><UploadId>upload</UploadId></InitiateMultipartUploadResult>",
        );
      }
      if (url.searchParams.has("uploadId")) {
        if (request.method === "DELETE") {
          parts.clear();
          return new Response(null, { status: 204 });
        }
        if (request.method === "PUT") {
          if (options.partFailure) return error("InternalError", 500);
          parts.set(
            Number(url.searchParams.get("partNumber")),
            new Uint8Array(await request.arrayBuffer()),
          );
          return new Response(null, { headers: { etag: '"part"' } });
        }
        if (request.method === "POST") {
          await request.arrayBuffer();
          if (options.completeFailure) return error("PreconditionFailed", 412);
          if (request.headers.get("if-none-match") !== "*") return error("MissingCondition", 400);
          return xml(
            '<CompleteMultipartUploadResult><Bucket>bucket</Bucket><Key>large</Key><ETag>"multipart"</ETag></CompleteMultipartUploadResult>',
          );
        }
      }
      if (request.method === "PUT") {
        const bytes = new Uint8Array(await request.arrayBuffer());
        if (request.headers.get("if-none-match") !== "*") return error("MissingCondition", 400);
        if (objects.has(key)) return error("PreconditionFailed", 412);
        objects.set(key, { bytes, contentType: request.headers.get("content-type")! });
        return new Response(null, { headers: { etag: '"etag"', "x-amz-version-id": "version" } });
      }
      if (request.method === "DELETE") {
        objects.delete(key);
        return new Response(null, { status: 204 });
      }
      const object = objects.get(key);
      if (!object) return error("NoSuchKey", 404);
      if (
        !options.ignoreReadConditions &&
        request.headers.has("if-match") &&
        request.headers.get("if-match") !== '"etag"'
      )
        return error("PreconditionFailed", 412);
      const headers = {
        "content-length": String(object.bytes.length),
        "content-type": object.contentType,
        etag: '"etag"',
        "x-amz-version-id": "version",
        "x-amz-meta-owner": "test",
      };
      if (request.method === "HEAD") return new Response(null, { headers });
      const range = request.headers.get("range")?.match(/^bytes=(\d+)-(\d*)$/);
      if (range && !options.ignoreReadConditions) {
        const start = Number(range[1]);
        const end = Math.min(Number(range[2] || object.bytes.length - 1), object.bytes.length - 1);
        return new Response(object.bytes.slice(start, end + 1), {
          status: 206,
          headers: {
            ...headers,
            "content-length": String(end - start + 1),
            "content-range": `bytes ${start}-${end}/${object.bytes.length}`,
          },
        });
      }
      return new Response(object.bytes, { headers });
    },
  });
  cleanups.push(() => server.stop(true));
  const client = new S3Client({
    endpoint: server.url.href,
    region: "us-east-1",
    forcePathStyle: true,
    maxAttempts: 1,
    credentials: { accessKeyId: "test", secretAccessKey: "test" },
    requestChecksumCalculation: "WHEN_REQUIRED",
  });
  cleanups.push(() => client.destroy());
  let destroyed = false;
  const destroy = client.destroy.bind(client);
  client.destroy = () => {
    destroyed = true;
    destroy();
  };
  const plugin = createS3StoragePlugin({
    id: "s3",
    bucket: "bucket",
    client,
    multipart: options.multipart,
  });
  const app = await startApp({ plugins: [plugin] });
  cleanups.push(() => app.stop());
  return { storage: app.get(plugin), requests, objects, parts, app, destroyed: () => destroyed };
}

test("an endpoint ignoring range and etag constraints cannot silently downgrade a download", async () => {
  const f = await fixture({ ignoreReadConditions: true });
  await f.storage.put({ key: "file", body: body("abcdef") });
  await expect(f.storage.get("file", { range: { offset: 1, length: 2 } })).rejects.toMatchObject({
    code: "unsupported",
  });
  await expect(f.storage.get("file", { ifMatch: '"wrong"' })).rejects.toMatchObject({
    code: "conflict",
  });
});

test("real AWS SDK over localhost: conditional create, metadata, ranges, list HEADs, delete", async () => {
  const f = await fixture();
  expect(
    await f.storage.put({ key: "file", body: body("abcdef"), contentType: "text/plain", size: 6 }),
  ).toMatchObject({
    size: 6,
    contentType: "text/plain",
    etag: '"etag"',
    versionId: "version",
  });
  await expect(f.storage.put({ key: "file", body: body("changed") })).rejects.toMatchObject({
    code: "conflict",
  });
  expect(new TextDecoder().decode(f.objects.get("file")!.bytes)).toBe("abcdef");
  const download = await f.storage.get("file", {
    ifMatch: '"etag"',
    range: { offset: 1, length: 3 },
  });
  expect(await new Response(download.body).text()).toBe("bcd");
  expect(download.metadata.size).toBe(6);
  expect(download.range).toEqual({ offset: 1, length: 3 });
  await expect(f.storage.get("file", { ifMatch: '"other"' })).rejects.toMatchObject({
    code: "conflict",
  });
  expect((await f.storage.list({ cursor: "previous" })).objects[0]).toMatchObject({
    contentType: "text/plain",
    customMetadata: { owner: "test" },
  });
  expect((await f.storage.list()).cursor).toBe("opaque+/=");
  expect(f.requests.some((r) => r.url.searchParams.get("continuation-token") === "previous")).toBe(
    true,
  );
  expect(await f.storage.head("missing")).toBeNull();
  expect(await f.storage.delete("file")).toEqual({ outcome: "absent-or-deleted" });
  await f.app.stop();
  expect(f.destroyed()).toBe(false);
});

test("real lib-storage multipart carries conditional completion and 5 MiB parts", async () => {
  const f = await fixture();
  const size = 6 * 1024 * 1024;
  expect(await f.storage.put({ key: "large", body: body(new Uint8Array(size)) })).toMatchObject({
    size,
    etag: '"multipart"',
  });
  expect(f.parts.get(1)?.length).toBe(5 * 1024 * 1024);
  expect(f.parts.get(2)?.length).toBe(1024 * 1024);
  const complete = f.requests.find(
    (r) => r.method === "POST" && r.url.searchParams.has("uploadId"),
  );
  expect(complete?.headers.get("if-none-match")).toBe("*");
});

for (const mode of ["completeFailure", "partFailure"] as const) {
  test(`real SDK ${mode} aborts multipart without deleting an object`, async () => {
    const f = await fixture({ [mode]: true });
    await expect(
      f.storage.put({ key: "large", body: body(new Uint8Array(6 * 1024 * 1024)) }),
    ).rejects.toMatchObject({
      code: mode === "completeFailure" ? "conflict" : "provider",
    });
    expect(
      f.requests.some((r) => r.method === "DELETE" && r.url.searchParams.has("uploadId")),
    ).toBe(true);
    expect(
      f.requests.some((r) => r.method === "DELETE" && !r.url.searchParams.has("uploadId")),
    ).toBe(false);
    expect(f.parts.size).toBe(0);
  });
}

test("limits, size mismatch and cancellation stop sources without writes", async () => {
  const f = await fixture();
  await expect(
    f.storage.put({ key: "large", body: body("abcdef"), maxBytes: 5 }),
  ).rejects.toMatchObject({ code: "too-large" });
  await expect(
    f.storage.put({ key: "mismatch", body: body("abc"), size: 4 }),
  ).rejects.toMatchObject({ code: "invalid-input" });
  let cancelled = false;
  const controller = new AbortController();
  const source = new ReadableStream<Uint8Array>({
    pull: () => new Promise(() => {}),
    cancel() {
      cancelled = true;
    },
  });
  const pending = f.storage.put({ key: "blocked", body: source, signal: controller.signal });
  controller.abort();
  await expect(pending).rejects.toMatchObject({ code: "aborted" });
  expect(cancelled).toBe(true);
  expect(f.objects.size).toBe(0);
});

test("single PUT mode streams known size and rejects unknown size before reading", async () => {
  const f = await fixture({ multipart: false });
  expect(f.storage.capabilities.uploadRequiresSize).toBe(true);
  await expect(f.storage.put({ key: "missing-size", body: body("abc") })).rejects.toMatchObject({
    code: "invalid-input",
  });
  expect(await f.storage.put({ key: "single", body: body("abc"), size: 3 })).toMatchObject({
    size: 3,
  });
  expect(f.requests.some((r) => r.url.searchParams.has("uploads"))).toBe(false);
});

test("presigner signs required headers and reports maxBytes as completion-check only", async () => {
  const f = await fixture();
  const link = await f.storage.signUpload({
    key: "signed",
    contentType: "image/png",
    maxBytes: 10,
    expiresIn: 60,
  });
  expect(link.conditions).toMatchObject({
    createOnly: true,
    maxBytes: 10,
    sizeEnforcement: "completion-check",
  });
  expect(link.headers).toEqual({ "content-type": "image/png", "if-none-match": "*" });
  expect(new URL(link.url).searchParams.get("X-Amz-SignedHeaders")).toContain("content-type");
  expect(new URL(link.url).searchParams.get("X-Amz-SignedHeaders")).toContain("if-none-match");
  const download = await f.storage.signDownload({
    key: "signed",
    ifMatch: '"etag"',
    expiresIn: 60,
  });
  expect(download.headers).toEqual({ "if-match": '"etag"' });
  expect(new URL(download.url).searchParams.get("X-Amz-SignedHeaders")).toContain("if-match");
  await expect(f.storage.signDownload({ key: "signed", expiresIn: 604801 })).rejects.toMatchObject({
    code: "invalid-input",
  });
  await expect(f.storage.head("forbidden")).rejects.toMatchObject({
    code: "forbidden",
    message: "S3 storage operation failed",
  });
});

test("cancellation after a part is uploaded aborts multipart and cancels blocked source", async () => {
  const f = await fixture();
  let pulls = 0;
  let cancelled = false;
  const source = new ReadableStream<Uint8Array>({
    pull(controller) {
      if (pulls++ === 0) controller.enqueue(new Uint8Array(5 * 1024 * 1024 + 1));
      else return new Promise(() => {});
    },
    cancel() {
      cancelled = true;
    },
  });
  const controller = new AbortController();
  const pending = f.storage.put({ key: "large", body: source, signal: controller.signal });
  const rejection = pending.catch((cause: unknown) => cause);
  for (let attempt = 0; !f.parts.has(1) && attempt < 100; attempt++) await Bun.sleep(5);
  expect(f.parts.has(1)).toBe(true);
  controller.abort();
  expect(await rejection).toMatchObject({ code: "aborted" });
  expect(cancelled).toBe(true);
  expect(f.parts.size).toBe(0);
  expect(f.requests.some((r) => r.method === "DELETE" && r.url.searchParams.has("uploadId"))).toBe(
    true,
  );
});

test("owned client registers cleanup immediately and stopping cancels active uploads", async () => {
  let destroyed = 0;
  const plugin = createS3StoragePlugin({
    id: "owned",
    bucket: "bucket",
    clientConfig: {
      region: "us-east-1",
      credentials: { accessKeyId: "test", secretAccessKey: "test" },
      requestHandler: {
        async handle() {
          throw new Error("unexpected request");
        },
        destroy() {
          destroyed++;
        },
      },
    },
  });
  const app = await startApp({ plugins: [plugin] });
  let cancelled = false;
  const pending = app.get(plugin).put({
    key: "blocked",
    body: new ReadableStream({
      pull: () => new Promise(() => {}),
      cancel() {
        cancelled = true;
      },
    }),
  });
  const rejection = pending.catch((cause: unknown) => cause);
  await app.stop();
  expect(await rejection).toMatchObject({ code: "aborted" });
  expect(cancelled).toBe(true);
  expect(destroyed).toBe(1);
});

test("R2 S3 endpoints require single PUT; signing never fixes a checksum for an unknown body", async () => {
  const client = new S3Client({
    endpoint: "https://account.r2.cloudflarestorage.com",
    region: "auto",
    credentials: { accessKeyId: "test", secretAccessKey: "test" },
  });
  cleanups.push(() => client.destroy());
  const plugin = createS3StoragePlugin({ id: "r2-s3", bucket: "bucket", client });
  const app = await startApp({ plugins: [plugin] });
  cleanups.push(() => app.stop());
  const storage = app.get(plugin);
  expect(storage.capabilities.uploadRequiresSize).toBe(true);
  await expect(storage.put({ key: "file", body: body("abc") })).rejects.toMatchObject({
    code: "invalid-input",
  });
  await expect(storage.put({ key: "file", body: body("abc"), size: 3 })).rejects.toMatchObject({
    code: "unsupported",
  });
  const link = await storage.signUpload({ key: "file", contentType: "text/plain", expiresIn: 60 });
  expect([...new URL(link.url).searchParams.keys()].some((key) => key.includes("checksum"))).toBe(
    false,
  );
  expect(await client.config.requestChecksumCalculation()).toBe("WHEN_SUPPORTED");
});
