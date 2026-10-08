import { expect, test } from "bun:test";
import { createTaskWorker, type WorkerBackend, type WorkerClaim } from "../src/worker";

test("a fetch returning after stop is settled without entering business code", async () => {
  const fetch = Promise.withResolvers<WorkerClaim | null>();
  let entered = false;
  let settled = false;
  const backend: WorkerBackend = {
    fetch: () => fetch.promise,
    async pulse() {
      return { owned: true, cancelRequested: false };
    },
    async settle(_claim, result) {
      expect(result).toEqual({ ok: false, error: "aborted" });
      settled = true;
    },
  };
  const worker = createTaskWorker(
    backend,
    async () => {
      entered = true;
      return { ok: true, result: null };
    },
    {},
    10,
  );
  const stop = worker.stop();
  fetch.resolve({
    jobId: crypto.randomUUID(),
    retryCount: 0,
    attempt: 1,
    task: "task",
    input: null,
    heartbeatMs: 10,
  });
  await stop;
  await worker.done;
  expect(entered).toBe(false);
  expect(settled).toBe(true);
});

test("worker failure is observable and stop shares the drained completion", async () => {
  const backend: WorkerBackend = {
    async fetch() {
      throw new Error("postgres://private-credential");
    },
    async pulse() {
      return { owned: true, cancelRequested: false };
    },
    async settle() {},
  };
  const worker = createTaskWorker(backend, async () => ({ ok: true, result: null }), {}, 10);
  await expect(worker.done).rejects.toMatchObject({
    code: "provider-unavailable",
    message: "Task queue operation failed",
  });
  expect(worker.stop()).toBe(worker.done);
  await expect(worker.stop()).rejects.toMatchObject({ code: "provider-unavailable" });
});
