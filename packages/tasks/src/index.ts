import type { StandardSchemaV1 } from "@standard-schema/spec";
import { definePlugin, type Plugin, type PluginContext } from "lenso/plugin";
import type {
  ClaimedJob,
  EnqueueOptions,
  ExecutionResult,
  Task,
  TaskProvider,
  WorkerOptions,
} from "./contracts";
import { TaskQueueError } from "./errors";
import { copyJson, INPUT_LIMIT_BYTES, RESULT_LIMIT_BYTES } from "./json";

export type * from "./contracts";
export { TaskQueueError } from "./errors";
export { INPUT_LIMIT_BYTES, RESULT_LIMIT_BYTES } from "./json";

// Heterogeneous registrations retain each task's own schema at the enqueue call.
type RegisteredTask = Task<any, any>;

export function defineTask<S extends StandardSchemaV1, R>(definition: Task<S, R>): Task<S, R> {
  if (
    !/^[a-zA-Z][a-zA-Z0-9_.-]{0,127}$/.test(definition.name) ||
    typeof definition.input?.["~standard"]?.validate !== "function" ||
    typeof definition.handler !== "function" ||
    (definition.result !== undefined && typeof definition.result !== "function") ||
    !Number.isInteger(definition.maxAttempts ?? 3) ||
    (definition.maxAttempts ?? 3) < 1 ||
    (definition.maxAttempts ?? 3) > 100
  )
    throw new TaskQueueError("invalid-task");
  const retry = definition.retry;
  if (
    retry &&
    ((retry.maxDelaySeconds !== undefined && retry.backoff !== true) ||
      (retry.backoff !== undefined && typeof retry.backoff !== "boolean") ||
      [retry.delaySeconds, retry.maxDelaySeconds].some(
        (seconds) =>
          seconds !== undefined && (!Number.isInteger(seconds) || seconds < 1 || seconds > 604800),
      ))
  )
    throw new TaskQueueError("invalid-options");
  return Object.freeze({
    ...definition,
    ...(retry ? { retry: Object.freeze({ ...retry }) } : {}),
  });
}

export function createTaskQueue(options: {
  readonly provider: TaskProvider;
  readonly tasks: readonly RegisteredTask[];
}) {
  const tasks = new Map<string, RegisteredTask>();
  for (const task of options.tasks) {
    defineTask(task);
    if (tasks.has(task.name)) throw new TaskQueueError("invalid-task");
    tasks.set(task.name, task);
  }
  let closed = false;
  let closePromise: Promise<void> | undefined;
  function assertOpen(): void {
    if (closed) throw new TaskQueueError("closed");
  }
  async function operation<T>(call: () => Promise<T>): Promise<T> {
    assertOpen();
    try {
      return await call();
    } catch (error) {
      if (error instanceof TaskQueueError) throw error;
      throw new TaskQueueError("provider-unavailable");
    }
  }
  function checkJobId(jobId: string): void {
    if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(jobId))
      throw new TaskQueueError("invalid-options");
  }
  async function execute(job: ClaimedJob): Promise<ExecutionResult> {
    const task = tasks.get(job.task);
    if (!task) return { ok: false, error: "invalid-input" };
    let input: unknown;
    try {
      const raw = copyJson(job.input, INPUT_LIMIT_BYTES, "invalid-input");
      const validation = await task.input["~standard"].validate(raw);
      if (validation.issues) return { ok: false, error: "invalid-input" };
      input = validation.value;
      copyJson(input, INPUT_LIMIT_BYTES, "invalid-input");
    } catch {
      return { ok: false, error: "invalid-input" };
    }
    let value: unknown;
    try {
      job.signal.throwIfAborted();
      value = await task.handler(input, {
        jobId: job.jobId,
        attempt: job.attempt,
        signal: job.signal,
      });
      job.signal.throwIfAborted();
    } catch {
      return { ok: false, error: job.signal.aborted ? "aborted" : "handler-failed" };
    }
    try {
      return {
        ok: true,
        result: task.result
          ? copyJson(task.result(value), RESULT_LIMIT_BYTES, "invalid-result")
          : null,
      };
    } catch {
      return { ok: false, error: "invalid-result" };
    }
  }
  return {
    async enqueue<S extends StandardSchemaV1, R>(
      task: Task<S, R>,
      input: StandardSchemaV1.InferInput<S>,
      enqueueOptions: EnqueueOptions = {},
    ): Promise<string> {
      assertOpen();
      if (tasks.get(task.name) !== task) throw new TaskQueueError("invalid-task");
      if (
        (enqueueOptions.runAt !== undefined &&
          (!(enqueueOptions.runAt instanceof Date) ||
            !Number.isFinite(enqueueOptions.runAt.getTime()))) ||
        (enqueueOptions.deduplicationKey !== undefined &&
          (typeof enqueueOptions.deduplicationKey !== "string" ||
            !enqueueOptions.deduplicationKey.length ||
            new TextEncoder().encode(enqueueOptions.deduplicationKey).byteLength > 256))
      )
        throw new TaskQueueError("invalid-options");
      const raw = copyJson(input, INPUT_LIMIT_BYTES, "invalid-input");
      try {
        const validation = await task.input["~standard"].validate(
          copyJson(raw, INPUT_LIMIT_BYTES, "invalid-input"),
        );
        if (validation.issues) throw new TaskQueueError("invalid-input");
        copyJson(validation.value, INPUT_LIMIT_BYTES, "invalid-input");
      } catch {
        throw new TaskQueueError("invalid-input");
      }
      // Persist the raw JSON, not transformed output: the same schema runs once on each boundary.
      return operation(() =>
        options.provider.enqueue({
          task: task.name,
          input: raw,
          maxAttempts: task.maxAttempts ?? 3,
          retry: task.retry,
          runAt: enqueueOptions.runAt ? new Date(enqueueOptions.runAt.getTime()) : undefined,
          deduplicationKey: enqueueOptions.deduplicationKey,
        }),
      );
    },
    get(jobId: string) {
      checkJobId(jobId);
      return operation(() => options.provider.get(jobId));
    },
    cancel(jobId: string) {
      checkJobId(jobId);
      return operation(() => options.provider.cancel(jobId));
    },
    retry(jobId: string) {
      checkJobId(jobId);
      return operation(() => options.provider.retry(jobId));
    },
    startWorker(workerOptions: WorkerOptions = {}) {
      if (
        !Number.isInteger(workerOptions.concurrency ?? 1) ||
        (workerOptions.concurrency ?? 1) < 1 ||
        (workerOptions.concurrency ?? 1) > 100 ||
        (workerOptions.timeoutMs !== undefined &&
          (!Number.isInteger(workerOptions.timeoutMs) ||
            workerOptions.timeoutMs < 1 ||
            workerOptions.timeoutMs > 2_147_483_647))
      )
        throw new TaskQueueError("invalid-options");
      return operation(() => options.provider.startWorker(execute, workerOptions));
    },
    close(): Promise<void> {
      if (!closePromise) {
        closed = true;
        closePromise = Promise.resolve().then(() => options.provider.close());
      }
      return closePromise;
    },
  };
}

export type TaskQueue = ReturnType<typeof createTaskQueue>;

export function createTaskPlugin(options: {
  readonly id: string;
  readonly requires?: readonly Plugin<unknown>[];
  readonly tasks: readonly RegisteredTask[];
  readonly connect: (context: PluginContext) => TaskProvider | Promise<TaskProvider>;
  /** No worker starts unless explicitly enabled. */
  readonly worker?: WorkerOptions;
}): Plugin<TaskQueue> {
  return definePlugin({
    id: options.id,
    requires: options.requires,
    async setup(context) {
      const provider = await options.connect(context);
      let queue: TaskQueue | undefined;
      const release = context.onCleanup(() => (queue ? queue.close() : provider.close()));
      queue = createTaskQueue({ provider, tasks: options.tasks });
      if (options.worker !== undefined) await queue.startWorker(options.worker);
      return { ...queue, close: release };
    },
  });
}
