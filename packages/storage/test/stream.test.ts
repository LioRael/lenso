import { expect, test } from "bun:test";
import { checkedUpload } from "../src/stream";

test("counting is pull-based, does not consume ahead, and verifies actual bytes", async () => {
  let pulls = 0;
  const source = new ReadableStream<Uint8Array>(
    {
      pull(controller) {
        pulls++;
        if (pulls <= 3) controller.enqueue(new Uint8Array(1024));
        else controller.close();
      },
    },
    { highWaterMark: 0 },
  );
  const upload = checkedUpload({ key: "a", body: source, size: 3072, maxBytes: 3072 });
  expect(pulls).toBe(0);
  const reader = upload.body.getReader();
  expect((await reader.read()).value?.length).toBe(1024);
  expect(pulls).toBe(1);
  await reader.read();
  await reader.read();
  expect((await reader.read()).done).toBe(true);
  expect(upload.size()).toBe(3072);
});

test("over-limit and wrong declared size cancel rather than buffering the rest", async () => {
  let cancelled = false;
  const upload = checkedUpload({
    key: "a",
    maxBytes: 1,
    body: new ReadableStream({
      pull(controller) {
        controller.enqueue(new Uint8Array(2));
      },
      cancel() {
        cancelled = true;
      },
    }),
  });
  await expect(upload.body.getReader().read()).rejects.toMatchObject({ code: "too-large" });
  expect(cancelled).toBe(true);
  const wrong = checkedUpload({
    key: "a",
    size: 1,
    body: new ReadableStream({
      start(controller) {
        controller.close();
      },
    }),
  });
  await expect(wrong.body.getReader().read()).rejects.toMatchObject({ code: "invalid-input" });
});

test("aborting a stalled input cancels the source and rejects its consumer", async () => {
  const signal = new AbortController();
  let cancelled = false;
  const upload = checkedUpload({
    key: "a",
    signal: signal.signal,
    body: new ReadableStream({
      cancel() {
        cancelled = true;
      },
    }),
  });
  const pending = upload.body.getReader().read();
  signal.abort();
  await expect(pending).rejects.toMatchObject({ code: "aborted" });
  expect(cancelled).toBe(true);
});

test("cleanup can recover a cancellation failure after the primary stream error", async () => {
  const cleanupFailure = new Error("Source cleanup failed");
  const upload = checkedUpload({
    key: "a",
    maxBytes: 1,
    body: new ReadableStream({
      pull(controller) {
        controller.enqueue(new Uint8Array(2));
      },
      cancel() {
        throw cleanupFailure;
      },
    }),
  });
  await expect(upload.body.getReader().read()).rejects.toMatchObject({ code: "too-large" });
  await expect(upload.cancel()).rejects.toBe(cleanupFailure);
  await expect(upload.cancel()).rejects.toBe(cleanupFailure);
});
