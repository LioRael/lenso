import { describe, expect, test } from "bun:test";
import { z } from "zod";
import { lifecycleFailure, startApp } from "@lenso/core";
import {
  createTaskPlugin,
  createTaskQueue,
  defineTask,
  INPUT_LIMIT_BYTES,
  TaskQueueError,
  taskErrorDiagnostic,
} from "../src/index";
import type {
  ClaimedJob,
  ExecutionResult,
  ProviderJob,
  TaskProvider,
  WorkerOptions,
} from "../src/contracts";

function recordingProvider() {
  let sent: ProviderJob | undefined;
  let execute: ((job: ClaimedJob) => Promise<ExecutionResult>) | undefined;
  let closed = 0;
  const jobId = crypto.randomUUID();
  const provider: TaskProvider = {
    async identity() {
      return { kind: "postgres", id: jobId };
    },
    async lookupDeduplicationKey() {
      return sent ? { jobId, status: null } : null;
    },
    async enqueue(job) {
      sent = job;
      return jobId;
    },
    async get() {
      throw new Error("postgres://user:private-password@host");
    },
    async cancel() {
      return "missing";
    },
    async retry() {
      return false;
    },
    async startWorker(handler) {
      execute = handler;
      return { done: Promise.resolve(), async stop() {} };
    },
    async close() {
      closed++;
    },
  };
  return {
    provider,
    jobId,
    sent: () => sent!,
    closed: () => closed,
    run(signal = new AbortController().signal, input = sent!.input) {
      return execute!({ ...sent!, input, jobId, attempt: 1, signal });
    },
  };
}

test("TaskQueue runtime codes normalize safely and keep internal provider cause", async () => {
  const marker = "PRIVATE-provider-error";
  const cause = new Error(marker);
  const invalid = new TaskQueueError(marker as "closed", { cause });
  expect(invalid.code).toBe("provider-unavailable");
  expect(invalid.cause).toBe(cause);
  expect(taskErrorDiagnostic(invalid)).toEqual({
    code: "provider-unavailable",
    phase: "invoke",
    message: "Task queue operation failed",
  });
  expect(taskErrorDiagnostic({ code: "closed", message: marker })).toBeUndefined();
  const objectCode = { private: marker, toString: () => "closed" };
  const malformed = new TaskQueueError(objectCode as unknown as "closed");
  expect(malformed.code).toBe("provider-unavailable");
  Object.assign(malformed, { code: objectCode });
  expect(taskErrorDiagnostic(malformed)?.code).toBe("provider-unavailable");
  const record = recordingProvider();
  record.provider.get = async () => {
    throw cause;
  };
  const queue = createTaskQueue({ tasks: [], provider: record.provider });
  const failure = await queue.get(record.jobId).catch((error) => error);
  expect(failure).toBeInstanceOf(TaskQueueError);
  expect(failure.cause).toBe(cause);
  expect(JSON.stringify(taskErrorDiagnostic(failure))).not.toContain(marker);
  await queue.close();
});

describe("task contract boundary (not persistence tests)", () => {
  test("list requires a bounded registered allowlist and fails honestly for legacy providers", async () => {
    const record = recordingProvider();
    const task = defineTask({ name: "listed", input: z.object({}), async handler() {} });
    const queue = createTaskQueue({ tasks: [task], provider: record.provider });
    await expect(queue.list({ tasks: [task.name] })).rejects.toMatchObject({ code: "unsupported" });
    expect(() => queue.list({ tasks: ["unregistered"] })).toThrow(TaskQueueError);
    for (const limit of [0, 101, 1.5, NaN]) {
      expect(() => queue.list({ tasks: [task.name], limit })).toThrow(TaskQueueError);
    }
    expect(() => queue.list({ tasks: [task.name], after: "arbitrary" })).toThrow(TaskQueueError);
    const item = {
      jobId: record.jobId,
      task: task.name,
      state: "pending" as const,
      attempt: 0,
      maxAttempts: 3,
      cancelRequested: false,
      result: { secret: "private" },
      error: "private",
    };
    record.provider.list = async () => ({ items: [item], nextCursor: null });
    expect(await queue.list({ tasks: [task.name] })).toEqual({
      items: [
        {
          jobId: record.jobId,
          task: task.name,
          state: "pending",
          attempt: 0,
          maxAttempts: 3,
          cancelRequested: false,
        },
      ],
      nextCursor: null,
    });
    record.provider.list = async () => ({ items: [item, item], nextCursor: null });
    await expect(queue.list({ tasks: [task.name] })).rejects.toMatchObject({
      code: "provider-unavailable",
    });
    await queue.close();
    await expect(queue.list({ tasks: [task.name] })).rejects.toMatchObject({ code: "closed" });
  });

  test("identity and lookup use the open/error boundary and retain tombstones", async () => {
    const record = recordingProvider();
    const task = defineTask({ name: "accepted", input: z.object({}), async handler() {} });
    const queue = createTaskQueue({ tasks: [task], provider: record.provider });
    expect(await queue.identity()).toEqual({ kind: "postgres", id: record.jobId });
    expect(await queue.lookupDeduplicationKey("key")).toBeNull();
    await queue.enqueue(task, {}, { deduplicationKey: "key" });
    expect(await queue.lookupDeduplicationKey("key")).toEqual({
      jobId: record.jobId,
      status: null,
    });
    expect(await queue.lookupDeduplicationKey("é".repeat(128))).toBeDefined();
    for (const key of ["", "é".repeat(129)]) {
      expect(() => queue.lookupDeduplicationKey(key)).toThrow(TaskQueueError);
      await expect(queue.enqueue(task, {}, { deduplicationKey: key })).rejects.toMatchObject({
        code: "invalid-options",
      });
    }
    record.provider.identity = async () => {
      throw new Error("private connection");
    };
    record.provider.lookupDeduplicationKey = async () => {
      throw new Error("private input");
    };
    await expect(queue.identity()).rejects.toMatchObject({ code: "provider-unavailable" });
    await expect(queue.lookupDeduplicationKey("key")).rejects.toMatchObject({
      code: "provider-unavailable",
    });
    await queue.close();
    await expect(queue.identity()).rejects.toMatchObject({ code: "closed" });
    await expect(queue.lookupDeduplicationKey("key")).rejects.toMatchObject({ code: "closed" });
  });

  test("runBatch uses the existing executor, defaults a finite budget, and awaits drain", async () => {
    const record = recordingProvider();
    const task = defineTask({ name: "batch", input: z.object({}), async handler() {} });
    const queue = createTaskQueue({ tasks: [task], provider: record.provider });
    const drain = Promise.withResolvers<void>();
    let options: WorkerOptions | undefined;
    record.provider.startWorker = async (execute, workerOptions) => {
      options = workerOptions;
      expect(
        await execute({
          jobId: record.jobId,
          task: task.name,
          input: {},
          attempt: 1,
          signal: new AbortController().signal,
        }),
      ).toEqual({ ok: true, result: null });
      return { done: drain.promise, stop: () => drain.promise };
    };
    let finished = false;
    const batch = queue.runBatch().then(() => {
      finished = true;
    });
    await Bun.sleep(0);
    expect(options).toEqual({ maxJobs: 100, stopWhenIdle: true });
    expect(finished).toBe(false);
    drain.resolve();
    await batch;
    await queue.runBatch({ maxJobs: 3, concurrency: 2, timeoutMs: 100 });
    expect(options).toEqual({ maxJobs: 3, concurrency: 2, timeoutMs: 100, stopWhenIdle: true });
    for (const maxJobs of [0, 1001, 1.5, NaN]) {
      expect(() => queue.startWorker({ maxJobs })).toThrow(TaskQueueError);
      await expect(queue.runBatch({ maxJobs })).rejects.toMatchObject({ code: "invalid-options" });
    }
    expect(() => queue.startWorker({ stopWhenIdle: "yes" as unknown as boolean })).toThrow(
      TaskQueueError,
    );
    record.provider.startWorker = async () => ({
      done: Promise.reject(new Error("private failure")),
      async stop() {},
    });
    await expect(queue.runBatch()).rejects.toMatchObject({ code: "provider-unavailable" });
    await queue.close();
    await expect(queue.runBatch()).rejects.toMatchObject({ code: "closed" });
  });

  test("same schema at both boundaries; transforms are not applied twice to payload", async () => {
    const record = recordingProvider();
    const task = defineTask({
      name: "report",
      input: z.object({ amount: z.number().transform((value) => value + 1) }),
      async handler(input, context) {
        expect(context.jobId).toBe(record.jobId);
        return input.amount;
      },
      result: (value) => ({ amount: value }),
    });
    const queue = createTaskQueue({ tasks: [task], provider: record.provider });
    await queue.enqueue(task, { amount: 1 });
    expect(record.sent().input).toEqual({ amount: 1 });
    expect(record.sent().traceMetadata).toBeUndefined();
    await queue.startWorker();
    expect(await record.run()).toEqual({ ok: true, result: { amount: 2 } });
    expect(await record.run(undefined, { amount: "invalid" })).toEqual({
      ok: false,
      error: "invalid-input",
    });
    await queue.close();
    expect(record.closed()).toBe(1);
  });

  test("rejects unsupported JSON, oversized input and accessors without executing them", async () => {
    const record = recordingProvider();
    const task = defineTask({ name: "json", input: z.unknown(), async handler() {} });
    const queue = createTaskQueue({ tasks: [task], provider: record.provider });
    const cyclic: Record<string, unknown> = {};
    cyclic.self = cyclic;
    let accessed = false;
    const accessor = {
      get secret() {
        accessed = true;
        return "secret";
      },
    };
    for (const input of [
      undefined,
      { value: undefined },
      new Date(),
      () => {},
      1n,
      NaN,
      new Map(),
      cyclic,
      accessor,
      Array(3),
      { [Symbol("key")]: "value" },
      new Proxy({ value: 1 }, { get: () => Infinity }),
      "x".repeat(INPUT_LIMIT_BYTES + 1),
    ])
      await expect(queue.enqueue(task, input)).rejects.toMatchObject({ code: "invalid-input" });
    expect(accessed).toBe(false);
    expect(record.sent()).toBeUndefined();
  });

  test("default results are discarded, thrown secrets never become error summaries", async () => {
    const record = recordingProvider();
    let fail = false;
    const task = defineTask({
      name: "private",
      input: z.object({ id: z.string() }),
      async handler() {
        if (fail) throw new Error("Bearer private-token");
        return { password: "secret" };
      },
    });
    const queue = createTaskQueue({ provider: record.provider, tasks: [task] });
    await queue.enqueue(task, { id: "business-key" });
    await queue.startWorker();
    expect(await record.run()).toEqual({ ok: true, result: null });
    fail = true;
    expect(await record.run()).toEqual({ ok: false, error: "handler-failed" });
    await expect(queue.get(record.jobId)).rejects.toMatchObject({
      code: "provider-unavailable",
      message: "Task queue operation failed",
    });
  });

  test("projects only bounded JSON and respects abort after actual handler settlement", async () => {
    const record = recordingProvider();
    const controller = new AbortController();
    const task = defineTask({
      name: "abort",
      input: z.object({}),
      async handler() {
        controller.abort();
        return "finished but did not undo side effects";
      },
      result: (result) => result,
    });
    const queue = createTaskQueue({ provider: record.provider, tasks: [task] });
    await queue.enqueue(task, {});
    await queue.startWorker();
    expect(await record.run(controller.signal)).toEqual({ ok: false, error: "aborted" });
    const second = recordingProvider();
    const tooLarge = defineTask({
      name: "largeResult",
      input: z.object({}),
      async handler() {
        return "x".repeat(20_000);
      },
      result: (value) => value,
    });
    const other = createTaskQueue({ provider: second.provider, tasks: [tooLarge] });
    await other.enqueue(tooLarge, {});
    await other.startWorker();
    expect(await second.run()).toEqual({ ok: false, error: "invalid-result" });
  });

  test("requires exact task reference and refuses invalid worker/retry options", async () => {
    const record = recordingProvider();
    const definition = { name: "task", input: z.object({}), async handler() {} };
    const task = defineTask(definition);
    const queue = createTaskQueue({ provider: record.provider, tasks: [task] });
    await expect(queue.enqueue(defineTask(definition), {})).rejects.toBeInstanceOf(TaskQueueError);
    expect(() => queue.startWorker({ concurrency: 0 })).toThrow(TaskQueueError);
    expect(() => defineTask({ ...definition, maxAttempts: Infinity })).toThrow(TaskQueueError);
    expect(() => defineTask({ ...definition, retry: { delaySeconds: NaN } })).toThrow(
      TaskQueueError,
    );
    expect(() => createTaskQueue({ provider: record.provider, tasks: [task, task] })).toThrow(
      TaskQueueError,
    );
    await queue.close();
    await queue.close();
    expect(record.closed()).toBe(1);
    await expect(queue.enqueue(task, {})).rejects.toMatchObject({ code: "closed" });
  });

  test("plugin registers rollback cleanup and does not start a worker implicitly", async () => {
    const record = recordingProvider();
    const task = defineTask({ name: "task", input: z.object({}), async handler() {} });
    let workers = 0;
    record.provider.startWorker = async () => {
      workers++;
      throw new Error("startup failed");
    };
    const producer = createTaskPlugin({
      id: "producer",
      tasks: [task],
      connect: () => record.provider,
    });
    const app = await startApp({ plugins: [producer] });
    expect(workers).toBe(0);
    const released = app.get(producer).close();
    expect(app.get(producer).close()).toBe(released);
    await released;
    await app.stop();
    expect(record.closed()).toBe(1);
    const worker = createTaskPlugin({
      id: "worker",
      tasks: [task],
      connect: () => record.provider,
      worker: { concurrency: 1 },
    });
    await expect(startApp({ plugins: [worker] })).rejects.toMatchObject({
      code: "provider-unavailable",
    });
    expect(record.closed()).toBe(2);
  });

  test("early plugin close shares SDK cleanup attribution and failure with app stop", async () => {
    const record = recordingProvider();
    const failure = new Error("cleanup failed");
    let closes = 0;
    record.provider.close = async () => {
      closes++;
      throw failure;
    };
    const plugin = createTaskPlugin({
      id: "early-close",
      tasks: [],
      connect: () => record.provider,
    });
    const app = await startApp({ plugins: [plugin] });
    await expect(app.get(plugin).close()).rejects.toBe(failure);
    expect(lifecycleFailure(failure)).toMatchObject({ phase: "cleanup", pluginId: "early-close" });
    await expect(app.stop()).rejects.toBeInstanceOf(AggregateError);
    expect(closes).toBe(1);
  });
});

function typeProof(queue: ReturnType<typeof createTaskQueue>) {
  const task = defineTask({
    name: "typed",
    input: z.object({ rows: z.array(z.number()) }),
    async handler(input) {
      return input.rows.reduce((sum, row) => sum + row, 0);
    },
    result: (sum) => ({ sum }),
  });
  // @ts-expect-error Inputs are inferred from the task's schema, not erased by queue registration.
  void queue.enqueue(task, { rows: ["not a number"] });
}
void typeProof;
