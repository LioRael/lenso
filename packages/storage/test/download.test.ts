import { expect, test } from "bun:test";
import { S3Client } from "@aws-sdk/client-s3";
import { createS3StoragePlugin } from "../src/s3";
import { createR2StoragePlugin, type R2StorageBinding } from "../src/r2";
import { StorageError, type PutObjectInput } from "../src/index";
import { checkedDownload, validateUpload } from "../src/stream";

const context = {
  instanceId: "download-test",
  onCleanup() {
    return async () => {};
  },
  get(): never {
    throw new Error("unused");
  },
};

for (const provider of ["s3", "r2"] as const) {
  async function storage(body: ReadableStream<Uint8Array>, beforeReturn = () => {}) {
    if (provider === "r2") {
      const binding = {
        async get() {
          beforeReturn();
          return { key: "file", size: 1, body, httpEtag: '"etag"' };
        },
      } as unknown as R2StorageBinding;
      return createR2StoragePlugin({ id: "r2", binding }).setup(context);
    }
    const client = new S3Client({ region: "us-east-1" });
    client.send = (async () => {
      beforeReturn();
      return { ContentLength: 1, Body: { transformToWebStream: () => body } };
    }) as typeof client.send;
    return createS3StoragePlugin({ id: "s3", bucket: "test", client }).setup(context);
  }

  test(`${provider}: cancelling after get interrupts a pending download and releases its source`, async () => {
    const abort = new AbortController();
    let cancelled = 0;
    let pulls = 0;
    const source = new ReadableStream<Uint8Array>(
      {
        pull() {
          pulls++;
        },
        cancel() {
          cancelled++;
        },
      },
      { highWaterMark: 0 },
    );
    const service = await storage(source);
    const object = await service.get("file", { signal: abort.signal });
    expect(pulls).toBe(0);
    const pending = object.body.getReader().read();
    abort.abort("client disconnected");
    await expect(pending).rejects.toMatchObject({ code: "aborted" });
    await Promise.resolve();
    expect(cancelled).toBe(1);
    expect(source.locked).toBe(false);
  });

  test(`${provider}: abort while awaiting provider cancels the returned body`, async () => {
    const abort = new AbortController();
    let cancelled = false;
    const source = new ReadableStream<Uint8Array>({
      cancel() {
        cancelled = true;
      },
    });
    const service = await storage(source, () => abort.abort());
    const object = await service.get("file", { signal: abort.signal });
    await expect(object.body.getReader().read()).rejects.toMatchObject({ code: "aborted" });
    expect(cancelled).toBe(true);
  });

  test(`${provider}: provider failures during body reads retain causes as StorageError`, async () => {
    const cause = new Error("private provider detail");
    const source = new ReadableStream<Uint8Array>({
      pull(controller) {
        controller.error(cause);
      },
    });
    const service = await storage(source);
    const object = await service.get("file");
    await expect(object.body.getReader().read()).rejects.toMatchObject({
      code: "provider",
      cause,
    });
    expect(source.locked).toBe(false);
  });

  test(`${provider}: invalid metadata fails before calling the upload driver`, async () => {
    const service = await storage(new ReadableStream());
    const body = new ReadableStream<Uint8Array>();
    await expect(
      service.put({
        key: "file",
        body,
        size: 1,
        customMetadata: { owner: 42 } as unknown as Record<string, string>,
      }),
    ).rejects.toMatchObject({ code: "invalid-input" });
    expect(body.locked).toBe(false);
  });
}

test("downloads keep large generated sources pull-based and propagate consumer cancellation", async () => {
  let pulls = 0;
  let reason: unknown;
  const source = new ReadableStream<Uint8Array>(
    {
      pull(controller) {
        pulls++;
        controller.enqueue(new Uint8Array(64 * 1024));
      },
      cancel(value) {
        reason = value;
      },
    },
    { highWaterMark: 0 },
  );
  const body = checkedDownload(
    source,
    undefined,
    (error) => new StorageError("provider", "Failed", { cause: error }),
  );
  const reader = body.getReader();
  expect(pulls).toBe(0);
  for (let i = 0; i < 1024; i++) {
    expect((await reader.read()).value?.byteLength).toBe(64 * 1024);
    expect(pulls).toBe(i + 1);
  }
  await reader.cancel("enough");
  expect(reason).toBe("enough");
  expect(source.locked).toBe(false);
});

test("all upload adapters reject malformed metadata before consuming a stream", () => {
  for (const customMetadata of [null, [], "invalid", { owner: 1 }]) {
    const input = {
      key: "file",
      body: new ReadableStream<Uint8Array>(),
      customMetadata,
    } as unknown as PutObjectInput;
    expect(() => validateUpload(input)).toThrow(StorageError);
    expect(input.body.locked).toBe(false);
  }
  expect(() =>
    validateUpload({
      key: "file",
      body: new ReadableStream(),
      contentType: 42 as unknown as string,
    }),
  ).toThrow(StorageError);
  validateUpload({ key: "file", body: new ReadableStream(), customMetadata: { owner: "alice" } });
});
