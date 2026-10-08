import { expect, test } from "bun:test";
import { createTaskWorker, type WorkerBackend, type WorkerClaim } from "../src/worker";

function claim(): WorkerClaim {
  return {
    jobId: crypto.randomUUID(),
    task: "task",
    input: null,
    attempt: 1,
    retryCount: 0,
    heartbeatMs: 1,
  };
}

test("finite fetch budget is global across concurrent lanes and drains handlers and pulses", async () => {
  const gate = Promise.withResolvers<void>();
  let fetched = 0;
  let entered = 0;
  let settled = 0;
  let pulses = 0;
  const backend: WorkerBackend = {
    async fetch() {
      fetched++;
      return claim();
    },
    async pulse() {
      pulses++;
      return { owned: true, cancelRequested: false };
    },
    async settle(_claim, result) {
      expect(result).toEqual({ ok: true, result: null });
      settled++;
    },
  };
  const worker = createTaskWorker(
    backend,
    async (job) => {
      entered++;
      await gate.promise;
      expect(job.signal.aborted).toBe(false);
      return { ok: true, result: null };
    },
    { concurrency: 5, maxJobs: 3 },
    10000,
  );
  let done = false;
  void worker.done.then(() => {
    done = true;
  });
  await Bun.sleep(10);
  expect(fetched).toBe(3);
  expect(entered).toBe(3);
  expect(pulses).toBeGreaterThan(3);
  expect(done).toBe(false);
  gate.resolve();
  await worker.done;
  expect(settled).toBe(3);
  const drainedPulses = pulses;
  await Bun.sleep(5);
  expect(pulses).toBe(drainedPulses);
});

test("empty fetches consume budget without a final polling delay", async () => {
  let fetched = 0;
  const backend: WorkerBackend = {
    async fetch() {
      fetched++;
      return null;
    },
    async pulse() {
      throw new Error("unexpected pulse");
    },
    async settle() {
      throw new Error("unexpected settle");
    },
  };
  const worker = createTaskWorker(
    backend,
    async () => ({ ok: true, result: null }),
    { concurrency: 5, maxJobs: 3 },
    10000,
  );
  await worker.done;
  expect(fetched).toBe(3);
}, 1000);

test("lanes refill within concurrency without multiplying the fetch budget", async () => {
  let fetched = 0;
  let active = 0;
  let peak = 0;
  const backend: WorkerBackend = {
    async fetch() {
      fetched++;
      return claim();
    },
    async pulse() {
      return { owned: true, cancelRequested: false };
    },
    async settle() {},
  };
  const worker = createTaskWorker(
    backend,
    async () => {
      peak = Math.max(peak, ++active);
      await Bun.sleep(1);
      active--;
      return { ok: true, result: null };
    },
    { concurrency: 2, maxJobs: 5, stopWhenIdle: true },
    10000,
  );
  await worker.done;
  expect(fetched).toBe(5);
  expect(peak).toBe(2);
  expect(active).toBe(0);
});

test("worker rejects invalid finite options before fetching", () => {
  const backend: WorkerBackend = {
    async fetch() {
      throw new Error("unexpected fetch");
    },
    async pulse() {
      throw new Error("unexpected pulse");
    },
    async settle() {},
  };
  for (const maxJobs of [0, -1, 1.5, 1001, Infinity]) {
    expect(() =>
      createTaskWorker(backend, async () => ({ ok: true, result: null }), { maxJobs }, 1),
    ).toThrow();
  }
  expect(() =>
    createTaskWorker(
      backend,
      async () => ({ ok: true, result: null }),
      { stopWhenIdle: "yes" as unknown as boolean },
      1,
    ),
  ).toThrow();
});

test("idle exit wakes polling lanes and drains claims racing with empty fetch", async () => {
  const late = Promise.withResolvers<WorkerClaim | null>();
  const gate = Promise.withResolvers<void>();
  let fetched = 0;
  let entered = 0;
  let settled = 0;
  const backend: WorkerBackend = {
    fetch() {
      return ++fetched === 1 ? Promise.resolve(null) : late.promise;
    },
    async pulse() {
      return { owned: true, cancelRequested: false };
    },
    async settle(_claim, result) {
      expect(result.ok).toBe(true);
      settled++;
    },
  };
  const worker = createTaskWorker(
    backend,
    async (job) => {
      entered++;
      await gate.promise;
      expect(job.signal.aborted).toBe(false);
      return { ok: true, result: null };
    },
    { concurrency: 2, stopWhenIdle: true },
    10000,
  );
  await Bun.sleep(0);
  late.resolve(claim());
  await Bun.sleep(5);
  expect(entered).toBe(1);
  gate.resolve();
  await worker.done;
  expect(fetched).toBe(2);
  expect(settled).toBe(1);
}, 1000);

test("idle worker exits promptly while default worker keeps polling until stopped", async () => {
  let fetched = 0;
  const backend: WorkerBackend = {
    async fetch() {
      fetched++;
      return null;
    },
    async pulse() {
      throw new Error("unexpected pulse");
    },
    async settle() {},
  };
  await createTaskWorker(
    backend,
    async () => ({ ok: true, result: null }),
    { stopWhenIdle: true },
    10000,
  ).done;
  expect(fetched).toBe(1);
  const worker = createTaskWorker(backend, async () => ({ ok: true, result: null }), {}, 1);
  await Bun.sleep(10);
  expect(fetched).toBeGreaterThan(2);
  await worker.stop();
}, 1000);

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
