import type { StandardSchemaV1 } from "@standard-schema/spec";
import { definePlugin, type Logger, type Plugin, type PluginContext } from "@lenso/core/plugin";
import type {
  ClaimedJob,
  EnqueueOptions,
  ExecutionResult,
  JobQuery,
  Task,
  TaskProvider,
  WorkerOptions,
} from "./contracts";
import { TaskQueueError } from "./errors";
import { copyJson, INPUT_LIMIT_BYTES, RESULT_LIMIT_BYTES } from "./json";
import { metrics, ROOT_CONTEXT, SpanKind, SpanStatusCode } from "@opentelemetry/api";
import { observe, producerLinks, producerMetadata, taskSpan } from "./telemetry";
import { jobSummary, normalizeJobQuery } from "./query";

export type * from "./contracts";
export { TaskQueueError, taskErrorDiagnostic } from "./errors";
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

function checkJobId(jobId: string): void {
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(jobId))
    throw new TaskQueueError("invalid-options");
}

function checkDeduplicationKey(key: string): void {
  if (typeof key !== "string" || !key.length || new TextEncoder().encode(key).byteLength > 256)
    throw new TaskQueueError("invalid-options");
}

function checkWorkerOptions(options: WorkerOptions): void {
  if (
    !Number.isInteger(options.concurrency ?? 1) ||
    (options.concurrency ?? 1) < 1 ||
    (options.concurrency ?? 1) > 100 ||
    (options.timeoutMs !== undefined &&
      (!Number.isInteger(options.timeoutMs) ||
        options.timeoutMs < 1 ||
        options.timeoutMs > 2_147_483_647)) ||
    (options.maxJobs !== undefined &&
      (!Number.isInteger(options.maxJobs) || options.maxJobs < 1 || options.maxJobs > 1000)) ||
    (options.stopWhenIdle !== undefined && typeof options.stopWhenIdle !== "boolean")
  )
    throw new TaskQueueError("invalid-options");
}

export function createTaskQueue(options: {
  readonly provider: TaskProvider;
  readonly tasks: readonly RegisteredTask[];
  readonly instanceId?: string;
  readonly pluginId?: string;
  readonly logger?: Logger;
}) {
  const instanceId = options.instanceId ?? crypto.randomUUID();
  const pluginId = options.pluginId ?? "tasks";
  const scope = {
    "lenso.instance.id": instanceId,
    "lenso.plugin.id": pluginId,
  };
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
      throw new TaskQueueError("provider-unavailable", { cause: error });
    }
  }
  async function execute(job: ClaimedJob): Promise<ExecutionResult> {
    const started = performance.now();
    return taskSpan(
      "lenso.task.attempt",
      {
        kind: SpanKind.CONSUMER,
        attributes: {
          ...scope,
          "messaging.message.id": job.jobId,
          "lenso.task.name": job.task,
          "messaging.operation.type": "process",
          "lenso.task.attempt": job.attempt,
        },
        links: producerLinks(job.traceMetadata),
      },
      async (span) => {
        const fields = {
          instanceId,
          pluginId,
          operation: job.task,
          jobId: job.jobId,
          attempt: job.attempt,
        };
        let logger: Logger | undefined;
        try {
          logger = options.logger?.child(fields);
        } catch {}
        try {
          try {
            logger?.debug({}, "Task attempt started");
          } catch {}
          const result = await executeAttempt(job, logger);
          observe(() => {
            if (!result.ok) span?.setStatus({ code: SpanStatusCode.ERROR });
          });
          const labels = { outcome: result.ok ? "success" : "failure" };
          observe(() => {
            const meter = metrics.getMeter("@lenso/tasks");
            meter.createCounter("lenso.task.attempts").add(1, labels);
            if (!result.ok) meter.createCounter("lenso.task.errors").add(1);
            meter
              .createHistogram("lenso.task.duration", { unit: "ms" })
              .record(performance.now() - started, labels);
          });
          try {
            if (result.ok) logger?.info({ outcome: "success" }, "Task attempt completed");
            else
              logger?.warn({ outcome: "failure", errorCode: result.error }, "Task attempt failed");
          } catch {}
          return result;
        } finally {
          observe(() => span?.end());
        }
      },
      ROOT_CONTEXT,
    );
  }
  async function executeAttempt(job: ClaimedJob, logger?: Logger): Promise<ExecutionResult> {
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
        instanceId,
        pluginId,
        ...(logger ? { logger } : {}),
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
        enqueueOptions.runAt !== undefined &&
        (!(enqueueOptions.runAt instanceof Date) ||
          !Number.isFinite(enqueueOptions.runAt.getTime()))
      )
        throw new TaskQueueError("invalid-options");
      if (enqueueOptions.deduplicationKey !== undefined)
        checkDeduplicationKey(enqueueOptions.deduplicationKey);
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
      return taskSpan(
        "lenso.task.enqueue",
        {
          kind: SpanKind.PRODUCER,
          attributes: {
            ...scope,
            "messaging.operation.type": "send",
            "lenso.task.name": task.name,
          },
        },
        async (span) => {
          const started = performance.now();
          let outcome = "success";
          try {
            const jobId = await operation(() =>
              options.provider.enqueue({
                traceMetadata: producerMetadata(),
                task: task.name,
                input: raw,
                maxAttempts: task.maxAttempts ?? 3,
                retry: task.retry,
                runAt: enqueueOptions.runAt ? new Date(enqueueOptions.runAt.getTime()) : undefined,
                deduplicationKey: enqueueOptions.deduplicationKey,
              }),
            );
            observe(() => span?.setAttribute("messaging.message.id", jobId));
            try {
              options.logger?.info(
                { instanceId, pluginId, operation: task.name, jobId },
                "Task enqueued",
              );
            } catch {}
            return jobId;
          } catch (error) {
            outcome = "failure";
            observe(() => span?.setStatus({ code: SpanStatusCode.ERROR }));
            throw error;
          } finally {
            observe(() => {
              const meter = metrics.getMeter("@lenso/tasks");
              meter.createCounter("lenso.task.enqueues").add(1, { outcome });
              meter
                .createHistogram("lenso.task.enqueue.duration", { unit: "ms" })
                .record(performance.now() - started, { outcome });
            });
            observe(() => span?.end());
          }
        },
      );
    },
    identity() {
      return operation(() => options.provider.identity());
    },
    lookupDeduplicationKey(key: string) {
      checkDeduplicationKey(key);
      return operation(() => options.provider.lookupDeduplicationKey(key));
    },
    get(jobId: string) {
      checkJobId(jobId);
      return operation(() => options.provider.get(jobId));
    },
    list(query: JobQuery) {
      const normalized = normalizeJobQuery(query);
      for (const name of normalized.tasks) {
        if (!tasks.has(name)) throw new TaskQueueError("invalid-task");
      }
      return operation(async () => {
        if (!options.provider.list) throw new TaskQueueError("unsupported");
        const page = await options.provider.list(normalized);
        if (
          page.items.length > normalized.limit ||
          page.items.some(
            (job, index) =>
              !normalized.tasks.includes(job.task) ||
              (normalized.after !== undefined && job.jobId <= normalized.after) ||
              (index > 0 && job.jobId <= page.items[index - 1]!.jobId),
          ) ||
          (page.nextCursor !== null && page.nextCursor !== page.items[page.items.length - 1]?.jobId)
        )
          throw new TaskQueueError("provider-unavailable");
        return { items: page.items.map(jobSummary), nextCursor: page.nextCursor };
      });
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
      checkWorkerOptions(workerOptions);
      return operation(() => options.provider.startWorker(execute, workerOptions));
    },
    async runBatch(
      batchOptions: { maxJobs?: number; concurrency?: number; timeoutMs?: number } = {},
    ): Promise<void> {
      const workerOptions = {
        ...batchOptions,
        maxJobs: batchOptions.maxJobs ?? 100,
        stopWhenIdle: true,
      };
      checkWorkerOptions(workerOptions);
      return operation(async () => {
        const worker = await options.provider.startWorker(execute, workerOptions);
        await worker.done;
      });
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
      queue = createTaskQueue({
        provider,
        tasks: options.tasks,
        instanceId: context.instanceId,
        pluginId: options.id,
        logger: context.logger,
      });
      if (options.worker !== undefined) await queue.startWorker(options.worker);
      return { ...queue, close: release };
    },
  });
}
