import type {
  ClaimedJob,
  ErrorCode,
  ExecutionResult,
  JobStatus,
  JsonValue,
  ProviderJob,
  TaskProvider,
  TaskWorker,
  WorkerOptions,
} from "./contracts";
import { TaskQueueError } from "./errors";
import { copyJson, INPUT_LIMIT_BYTES, RESULT_LIMIT_BYTES } from "./json";
import { traceMetadata } from "./telemetry";
import { createTaskWorker, type WorkerBackend } from "./worker";
import { jobPage, normalizeJobQuery } from "./query";

export { taskD1Schema } from "./schema-d1";

/** Structural binding types keep this entry usable without ambient Workers globals. */
export interface D1PreparedStatement {
  bind(...values: unknown[]): D1PreparedStatement;
  all<T = Record<string, unknown>>(): Promise<D1Result<T>>;
}

export interface D1Result<T = Record<string, unknown>> {
  readonly success: boolean;
  readonly results: T[];
}

export interface D1Database {
  prepare(query: string): D1PreparedStatement;
  batch<T = Record<string, unknown>>(statements: D1PreparedStatement[]): Promise<D1Result<T>[]>;
  withSession(constraintOrBookmark?: string): unknown;
}

export interface D1TaskProviderOptions {
  readonly database: D1Database;
  readonly queueName: string;
  readonly clock?: () => number;
  readonly leaseMs?: number;
  readonly expireInMs?: number;
  readonly pollIntervalMs?: number;
}

type StatusRow = {
  id: string;
  task: string;
  state: JobStatus["state"];
  attempt: number;
  max_attempts: number;
  cancel_requested: number;
  result: string | null;
  error: string | null;
};
type ClaimRow = {
  id: string;
  task: string;
  input: string;
  trace_metadata: string | null;
  attempt: number;
};

const statusColumns = "id, task, state, attempt, max_attempts, cancel_requested, result, error";
const errors = new Set<ErrorCode>(["handler-failed", "invalid-input", "invalid-result", "aborted"]);

function validateBinding(database: D1Database, queueName: string): void {
  if (
    !database ||
    typeof database.prepare !== "function" ||
    typeof database.batch !== "function" ||
    typeof database.withSession !== "function" ||
    typeof queueName !== "string" ||
    !queueName.length
  )
    throw new TaskQueueError("invalid-options");
}

async function safely<T>(run: () => Promise<T>): Promise<T> {
  try {
    return await run();
  } catch (error) {
    throw error instanceof TaskQueueError ? error : new TaskQueueError("provider-unavailable");
  }
}

function rows<T>(result: D1Result<T> | undefined): T[] {
  if (!result?.success || !Array.isArray(result.results)) {
    throw new TaskQueueError("provider-unavailable");
  }
  return result.results;
}

function canonicalJson(value: JsonValue): string {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  return `{${Object.keys(value)
    .sort()
    .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key]!)}`)
    .join(",")}}`;
}

function status(row: StatusRow): JobStatus {
  let result: JsonValue | null = null;
  if (row.state === "succeeded" && row.result !== null) {
    try {
      result = copyJson(JSON.parse(row.result), RESULT_LIMIT_BYTES, "invalid-result");
    } catch {
      // An operator-corrupted projection must not bypass the public JSON boundary.
    }
  }
  return {
    jobId: row.id,
    task: row.task,
    state: row.state,
    attempt: row.attempt,
    maxAttempts: row.max_attempts,
    cancelRequested: row.cancel_requested === 1,
    result,
    error:
      row.state === "cancelled"
        ? "aborted"
        : row.state === "failed"
          ? errors.has(row.error as ErrorCode)
            ? (row.error as ErrorCode)
            : "handler-failed"
          : null,
  };
}

export async function provisionD1TaskQueue(database: D1Database, queueName: string): Promise<void> {
  validateBinding(database, queueName);
  await safely(async () => {
    rows(
      await database
        .prepare(
          "INSERT INTO lenso_d1_task_queue (queue_name, queue_id) VALUES (?, ?) ON CONFLICT(queue_name) DO NOTHING RETURNING queue_id",
        )
        .bind(queueName, crypto.randomUUID())
        .all(),
    );
  });
}

function validateJob(job: ProviderJob): void {
  if (
    typeof job.task !== "string" ||
    !job.task.length ||
    !Number.isSafeInteger(job.maxAttempts) ||
    job.maxAttempts < 1 ||
    (job.runAt !== undefined &&
      (!(job.runAt instanceof Date) || !Number.isFinite(job.runAt.getTime()))) ||
    (job.deduplicationKey !== undefined &&
      (typeof job.deduplicationKey !== "string" ||
        !job.deduplicationKey.length ||
        new TextEncoder().encode(job.deduplicationKey).byteLength > 256)) ||
    [job.retry?.delaySeconds, job.retry?.maxDelaySeconds].some(
      (value) => value !== undefined && (!Number.isSafeInteger(value) || value < 0),
    ) ||
    (job.retry?.backoff !== undefined && typeof job.retry.backoff !== "boolean")
  )
    throw new TaskQueueError("invalid-options");
}

// attempt=1 uses the base delay. SQLite shifts avoid a dependency on optional math functions.
// Saturate only beyond JS's representable millisecond range, not at a polling/timer limit.
const retryDelay = `MIN(
  COALESCE(retry_max_delay_seconds, 9007199254740),
  CASE WHEN retry_backoff = 1
    THEN MAX(retry_delay_seconds, 1) * (1 << MIN(attempt - 1, 52))
    ELSE retry_delay_seconds END,
  9007199254740
) * 1000`;

export async function createD1TaskProvider(options: D1TaskProviderOptions): Promise<TaskProvider> {
  const { database, queueName } = options;
  validateBinding(database, queueName);
  const leaseMs = options.leaseMs ?? 30_000;
  const expireInMs = options.expireInMs ?? 900_000;
  const pollIntervalMs = options.pollIntervalMs ?? 1000;
  if (
    [leaseMs, expireInMs, pollIntervalMs].some(
      (value) => !Number.isSafeInteger(value) || value < 1 || value > 2_147_483_647,
    ) ||
    (options.clock !== undefined && typeof options.clock !== "function")
  )
    throw new TaskQueueError("invalid-options");
  const clock = options.clock ?? Date.now;
  function now(): number {
    const value = clock();
    if (
      !Number.isSafeInteger(value) ||
      value < 0 ||
      !Number.isSafeInteger(value + expireInMs + leaseMs)
    ) {
      throw new TaskQueueError("invalid-options");
    }
    return value;
  }
  const statement = (query: string, ...values: unknown[]) =>
    database.prepare(query).bind(...values);
  const queueId = await safely(async () => {
    const [queue] = rows(
      await statement(
        "SELECT queue_id FROM lenso_d1_task_queue WHERE queue_name = ?",
        queueName,
      ).all<{ queue_id: string }>(),
    );
    if (!queue) throw new TaskQueueError("provider-unavailable");
    rows(
      await statement(`SELECT ${statusColumns}, input, trace_metadata, dedup_key, run_at,
      lease_until, expires_at, retry_delay_seconds, retry_backoff, retry_max_delay_seconds
      FROM lenso_d1_task_job LIMIT 0`).all(),
    );
    return queue.queue_id;
  });
  const workers = new Set<TaskWorker>();
  let closing = false;
  let closePromise: Promise<void> | undefined;
  function assertOpen(): void {
    if (closing) throw new TaskQueueError("closed");
  }
  const fence =
    "queue_name = ? AND id = ? AND attempt = ? AND state = 'running' AND lease_until > ? AND expires_at > ?";
  const backend: WorkerBackend = {
    fetch: () =>
      safely(async () => {
        const time = now();
        // Recovery and claim share one atomic batch. No subsequent write depends on a zero-row CAS.
        const responses = await database.batch([
          statement(
            `UPDATE lenso_d1_task_job SET
          state = CASE WHEN cancel_requested = 1 THEN 'cancelled'
            WHEN attempt >= max_attempts THEN 'failed' ELSE 'pending' END,
          run_at = CASE WHEN cancel_requested = 0 AND attempt < max_attempts
            THEN ? + ${retryDelay} ELSE run_at END,
          lease_until = NULL, expires_at = NULL, result = NULL,
          error = CASE WHEN cancel_requested = 1 OR attempt >= max_attempts THEN 'aborted' ELSE NULL END
          WHERE queue_name = ? AND state = 'running' AND (lease_until <= ? OR expires_at <= ?)
          RETURNING id`,
            time,
            queueName,
            time,
            time,
          ),
          statement(
            `UPDATE lenso_d1_task_job SET state = 'running', attempt = attempt + 1,
          lease_until = ?, expires_at = ?, result = NULL, error = NULL
          WHERE queue_name = ? AND id = (
            SELECT id FROM lenso_d1_task_job WHERE queue_name = ? AND state = 'pending'
              AND cancel_requested = 0 AND run_at <= ? AND attempt < max_attempts
            ORDER BY run_at, id LIMIT 1
          ) AND state = 'pending' AND cancel_requested = 0 AND attempt < max_attempts
          RETURNING id, task, input, trace_metadata, attempt`,
            time + Math.min(leaseMs, expireInMs),
            time + expireInMs,
            queueName,
            queueName,
            time,
          ),
        ]);
        rows(responses[0]);
        const [job] = rows(responses[1]) as unknown as ClaimRow[];
        if (!job) return null;
        return {
          jobId: job.id,
          task: job.task,
          input: copyJson(JSON.parse(job.input), INPUT_LIMIT_BYTES, "invalid-input"),
          traceMetadata:
            job.trace_metadata === null ? undefined : traceMetadata(JSON.parse(job.trace_metadata)),
          attempt: job.attempt,
          retryCount: job.attempt - 1,
          heartbeatMs: Math.max(
            1,
            Math.min(pollIntervalMs, Math.floor(leaseMs / 2), Math.floor(expireInMs / 2)),
          ),
        };
      }),
    pulse: (claim) =>
      safely(async () => {
        const time = now();
        const [job] = rows(
          await statement(
            `UPDATE lenso_d1_task_job SET lease_until = MIN(?, expires_at)
          WHERE ${fence} RETURNING cancel_requested`,
            time + leaseMs,
            queueName,
            claim.jobId,
            claim.attempt,
            time,
            time,
          ).all<{ cancel_requested: number }>(),
        );
        return { owned: !!job, cancelRequested: job?.cancel_requested === 1 };
      }),
    settle: (claim, outcome) =>
      safely(async () => {
        const time = now();
        let output: ExecutionResult;
        try {
          output = outcome.ok
            ? { ok: true, result: copyJson(outcome.result, RESULT_LIMIT_BYTES, "invalid-result") }
            : { ok: false, error: errors.has(outcome.error) ? outcome.error : "handler-failed" };
        } catch {
          output = { ok: false, error: "invalid-result" };
        }
        const success = output.ok ? 1 : 0;
        rows(
          await statement(
            `UPDATE lenso_d1_task_job SET
        state = CASE WHEN cancel_requested = 1 THEN 'cancelled' WHEN ? = 1 THEN 'succeeded'
          WHEN attempt < max_attempts THEN 'pending' ELSE 'failed' END,
        run_at = CASE WHEN cancel_requested = 0 AND ? = 0 AND attempt < max_attempts
          THEN ? + ${retryDelay} ELSE run_at END,
        result = CASE WHEN cancel_requested = 0 AND ? = 1 THEN ? ELSE NULL END,
        error = CASE WHEN cancel_requested = 1 THEN 'aborted'
          WHEN ? = 0 AND attempt >= max_attempts THEN ? ELSE NULL END,
        lease_until = NULL, expires_at = NULL
        WHERE ${fence} RETURNING id`,
            success,
            success,
            time,
            success,
            output.ok ? JSON.stringify(output.result) : null,
            success,
            output.ok ? null : output.error,
            queueName,
            claim.jobId,
            claim.attempt,
            time,
            time,
          ).all(),
        );
      }),
  };
  const provider = {
    async identity(): Promise<{ kind: "d1"; id: string }> {
      assertOpen();
      return { kind: "d1", id: queueId };
    },
    async enqueue(job: ProviderJob) {
      assertOpen();
      validateJob(job);
      const input = canonicalJson(copyJson(job.input, INPUT_LIMIT_BYTES, "invalid-input"));
      const metadata = traceMetadata(job.traceMetadata);
      const id = crypto.randomUUID();
      return safely(async () => {
        const responses = await database.batch([
          statement(
            `INSERT INTO lenso_d1_task_job
            (queue_name, id, task, input, trace_metadata, dedup_key, max_attempts, run_at,
              retry_delay_seconds, retry_backoff, retry_max_delay_seconds)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT DO NOTHING RETURNING id`,
            queueName,
            id,
            job.task,
            input,
            metadata ? JSON.stringify(metadata) : null,
            job.deduplicationKey ?? null,
            job.maxAttempts,
            job.runAt?.getTime() ?? now(),
            job.retry?.delaySeconds ?? 0,
            job.retry?.backoff ? 1 : 0,
            job.retry?.maxDelaySeconds ?? null,
          ),
          statement(
            `SELECT id, task, input FROM lenso_d1_task_job
            WHERE queue_name = ? AND (id = ? OR dedup_key = ?)`,
            queueName,
            id,
            job.deduplicationKey ?? null,
          ),
        ]);
        rows(responses[0]);
        const [stored] = rows(responses[1]) as unknown as {
          id: string;
          task: string;
          input: string;
        }[];
        if (!stored) throw new TaskQueueError("provider-unavailable");
        if (stored.task !== job.task || canonicalJson(JSON.parse(stored.input)) !== input) {
          throw new TaskQueueError("deduplication-conflict");
        }
        return stored.id;
      });
    },
    async get(jobId: string) {
      assertOpen();
      return safely(async () => {
        const [job] = rows(
          await statement(
            `SELECT ${statusColumns} FROM lenso_d1_task_job WHERE queue_name = ? AND id = ?`,
            queueName,
            jobId,
          ).all<StatusRow>(),
        );
        return job ? status(job) : null;
      });
    },
    async list(query: import("./contracts").JobQuery) {
      assertOpen();
      const { tasks, limit, after } = normalizeJobQuery(query);
      if (!tasks.length) return { items: [], nextCursor: null };
      return safely(async () => {
        const jobs = rows(
          await statement(
            `SELECT id, task, state, attempt, max_attempts, cancel_requested
            FROM lenso_d1_task_job WHERE queue_name = ?
            AND task IN (${tasks.map(() => "?").join(",")})
            ${after === undefined ? "" : "AND id > ?"}
            ORDER BY id ASC LIMIT ?`,
            queueName,
            ...tasks,
            ...(after === undefined ? [] : [after]),
            limit + 1,
          ).all<Omit<StatusRow, "result" | "error">>(),
        );
        return jobPage(
          jobs.map((job) => ({
            jobId: job.id,
            task: job.task,
            state: job.state,
            attempt: job.attempt,
            maxAttempts: job.max_attempts,
            cancelRequested: job.cancel_requested === 1,
          })),
          limit,
        );
      });
    },
    async lookupDeduplicationKey(key: string) {
      assertOpen();
      return safely(async () => {
        const [job] = rows(
          await statement(
            `SELECT ${statusColumns} FROM lenso_d1_task_job WHERE queue_name = ? AND dedup_key = ?`,
            queueName,
            key,
          ).all<StatusRow>(),
        );
        return job ? { jobId: job.id, status: status(job) } : null;
      });
    },
    async cancel(jobId: string): ReturnType<TaskProvider["cancel"]> {
      assertOpen();
      return safely(async () => {
        const responses = await database.batch([
          statement(
            `UPDATE lenso_d1_task_job SET cancel_requested = 1,
            state = CASE WHEN state = 'pending' THEN 'cancelled' ELSE state END,
            error = CASE WHEN state = 'pending' THEN 'aborted' ELSE error END,
            lease_until = CASE WHEN state = 'pending' THEN NULL ELSE lease_until END,
            expires_at = CASE WHEN state = 'pending' THEN NULL ELSE expires_at END
            WHERE queue_name = ? AND id = ? AND state IN ('pending', 'running') RETURNING state`,
            queueName,
            jobId,
          ),
          statement(
            "SELECT state FROM lenso_d1_task_job WHERE queue_name = ? AND id = ?",
            queueName,
            jobId,
          ),
        ]);
        const [changed] = rows(responses[0]) as unknown as { state: string }[];
        const [existing] = rows(responses[1]);
        if (changed) return changed.state === "running" ? "requested" : "cancelled";
        return existing ? "terminal" : "missing";
      });
    },
    async retry(jobId: string) {
      assertOpen();
      return safely(
        async () =>
          rows(
            await statement(
              `UPDATE lenso_d1_task_job SET state = 'pending', max_attempts = max_attempts + 1,
          cancel_requested = 0, run_at = ?, lease_until = NULL, expires_at = NULL, result = NULL, error = NULL
          WHERE queue_name = ? AND id = ? AND state = 'failed' RETURNING id`,
              now(),
              queueName,
              jobId,
            ).all(),
          ).length === 1,
      );
    },
    async startWorker(
      execute: (job: ClaimedJob) => Promise<ExecutionResult>,
      workerOptions: WorkerOptions = {},
    ) {
      assertOpen();
      const worker = createTaskWorker(backend, execute, workerOptions, pollIntervalMs);
      workers.add(worker);
      void worker.done.then(
        () => workers.delete(worker),
        () => {},
      );
      return worker;
    },
    close() {
      closing = true;
      closePromise ??= (async () => {
        const outcomes = await Promise.allSettled([...workers].map((worker) => worker.stop()));
        if (outcomes.some((outcome) => outcome.status === "rejected")) {
          throw new TaskQueueError("provider-unavailable");
        }
      })();
      return closePromise;
    },
  };
  return provider;
}
