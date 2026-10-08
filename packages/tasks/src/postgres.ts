import { randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
import { and, eq, sql } from "drizzle-orm";
import { drizzle } from "drizzle-orm/node-postgres";
import { Pool, type PoolClient } from "pg";
import { PgBoss, type Db } from "pg-boss";
import type {
  ErrorCode,
  ExecutionResult,
  JobState,
  JobStatus,
  TaskProvider,
  TaskWorker,
} from "./contracts";
import { TaskQueueError } from "./errors";
import { copyJson, RESULT_LIMIT_BYTES } from "./json";
import { taskQueueSchema } from "./schema";
import { createTaskWorker, type WorkerBackend } from "./worker";

export type PostgresTaskProviderOptions = {
  readonly queueName: string;
  readonly schema?: string;
  readonly pollIntervalMs?: number;
  readonly heartbeatSeconds?: number;
  readonly expireInSeconds?: number;
  readonly retentionSeconds?: number;
  readonly superviseIntervalSeconds?: number;
  /** Operational notifications contain a fixed safe error, never driver error text. */
  readonly onError?: (error: TaskQueueError) => void;
} & (
  | { readonly connectionString: string; readonly pool?: never }
  /** Borrowed pools remain open after provider close and migration. */
  | { readonly pool: Pool; readonly connectionString?: never }
);

function configuration(options: PostgresTaskProviderOptions) {
  const schema = options.schema ?? "pgboss";
  if (!/^[a-z_][a-z0-9_]*$/.test(schema) || schema.length > 63 || schema === "public") {
    throw new TaskQueueError("invalid-options");
  }
  if (
    !options.queueName ||
    (!options.pool && !options.connectionString) ||
    (options.pool && options.connectionString)
  ) {
    throw new TaskQueueError("invalid-options");
  }
  const values = {
    pollIntervalMs: options.pollIntervalMs ?? 1000,
    heartbeatSeconds: options.heartbeatSeconds ?? 30,
    expireInSeconds: options.expireInSeconds ?? 900,
    retentionSeconds: options.retentionSeconds ?? 604800,
    superviseIntervalSeconds: options.superviseIntervalSeconds ?? 30,
  };
  for (const value of Object.values(values)) {
    if (!Number.isSafeInteger(value) || value < 1) throw new TaskQueueError("invalid-options");
  }
  if (values.heartbeatSeconds < 10 || values.pollIntervalMs > 2_147_483_647) {
    throw new TaskQueueError("invalid-options");
  }
  return { schema, ...values };
}

function adapter(client: Pool | PoolClient): Db {
  return { executeSql: (text, values) => client.query(text, values) };
}

async function transaction<T>(
  pool: Pool,
  run: (client: PoolClient, db: Db) => Promise<T>,
): Promise<T> {
  const client = await pool.connect().catch(() => {
    throw new TaskQueueError("provider-unavailable");
  });
  try {
    await client.query("BEGIN");
    const result = await run(client, adapter(client));
    await client.query("COMMIT");
    return result;
  } catch (error) {
    await client.query("ROLLBACK").catch(() => {});
    throw error instanceof TaskQueueError ? error : new TaskQueueError("provider-unavailable");
  } finally {
    client.release();
  }
}

// pg-boss 12.37's declaration omits the runtime response fields.
function affected(response: unknown): number {
  if (
    typeof response !== "object" ||
    response === null ||
    !("affected" in response) ||
    typeof response.affected !== "number"
  )
    throw new Error("Invalid task queue command response");
  return response.affected;
}

function bossClient(pool: Pool, options: PostgresTaskProviderOptions, migrate: boolean) {
  const config = configuration(options);
  const boss = new PgBoss({
    db: adapter(pool),
    schema: config.schema,
    migrate,
    supervise: !migrate,
    schedule: false,
    reindex: false,
    superviseIntervalSeconds: config.superviseIntervalSeconds,
    monitorIntervalSeconds: config.superviseIntervalSeconds,
  });
  // Supervision reports errors through this event, not through a rejected worker promise.
  // The worker independently verifies every claim before renewing or settling it.
  boss.on("error", () => options.onError?.(new TaskQueueError("provider-unavailable")));
  return { boss, config };
}

async function release(boss: PgBoss, pool: Pool, owned: boolean): Promise<void> {
  const failures: unknown[] = [];
  try {
    await boss.stop();
  } catch {
    failures.push(new TaskQueueError("provider-unavailable"));
  }
  if (owned) {
    try {
      await pool.end();
    } catch {
      failures.push(new TaskQueueError("provider-unavailable"));
    }
  }
  if (failures.length) throw new AggregateError(failures, "Task queue cleanup failed");
}

export async function migratePostgresTaskQueue(
  options: PostgresTaskProviderOptions,
): Promise<void> {
  configuration(options);
  const pool = options.pool ?? new Pool({ connectionString: options.connectionString });
  if (!options.pool)
    pool.on("error", () => options.onError?.(new TaskQueueError("provider-unavailable")));
  const { boss, config } = bossClient(pool, options, true);
  try {
    await boss.start();
    const existing = await boss.getQueue(options.queueName);
    if (existing?.partition || (existing && existing.policy !== "standard")) {
      throw new Error("Task queue requires an ordinary nonpartitioned standard queue");
    }
    if (!existing)
      await boss.createQueue(options.queueName, { partition: false, policy: "standard" });
    const sql = await readFile(
      new URL("../migrations/0001_task_relation.sql", import.meta.url),
      "utf8",
    );
    await transaction(pool, async (client) => {
      await client.query(sql.replaceAll("__LENSO_SCHEMA__", `"${config.schema}"`));
    });
  } catch (error) {
    throw error instanceof TaskQueueError ? error : new TaskQueueError("provider-unavailable");
  } finally {
    await release(boss, pool, !options.pool);
  }
}

type StoredJob = {
  id: string;
  state: string;
  retry_count: number;
  retry_limit: number;
  started_on: Date | null;
  output: unknown;
};

const states: Record<string, JobState> = {
  created: "pending",
  retry: "pending",
  active: "running",
  completed: "succeeded",
  failed: "failed",
  cancelled: "cancelled",
};
const errorCodes = new Set<ErrorCode>([
  "handler-failed",
  "invalid-input",
  "invalid-result",
  "aborted",
]);

function safeOutput(output: unknown): ExecutionResult | null {
  if (typeof output !== "object" || output === null) return null;
  const candidate = output as Record<string, unknown>;
  if (candidate.ok === false && errorCodes.has(candidate.error as ErrorCode)) {
    return { ok: false, error: candidate.error as ErrorCode };
  }
  if (candidate.ok === true) {
    try {
      return { ok: true, result: copyJson(candidate.result, RESULT_LIMIT_BYTES, "invalid-result") };
    } catch {
      return null;
    }
  }
  return null;
}

export async function createPostgresTaskProvider(
  options: PostgresTaskProviderOptions,
): Promise<TaskProvider> {
  options = { ...options };
  configuration(options);
  const pool = options.pool ?? new Pool({ connectionString: options.connectionString });
  if (!options.pool)
    pool.on("error", () => options.onError?.(new TaskQueueError("provider-unavailable")));
  const { boss, config } = bossClient(pool, options, false);
  const relation = taskQueueSchema(config.schema);
  const jobTable = `"${config.schema}".job`;
  const workers = new Set<TaskWorker>();
  let closing = false;
  let closePromise: Promise<void> | undefined;
  try {
    await boss.start();
    const queue = await boss.getQueue(options.queueName);
    if (!queue || queue.partition || queue.policy !== "standard") {
      throw new Error("Task queue is not migrated as an ordinary nonpartitioned standard queue");
    }
    // Read the declared relation shape, not only its name. No startup DDL is permitted.
    await drizzle(pool).select().from(relation).limit(0);
  } catch (error) {
    try {
      await release(boss, pool, !options.pool);
    } catch (cleanupError) {
      throw new AggregateError(
        [new TaskQueueError("provider-unavailable"), cleanupError],
        "Task queue startup and cleanup failed",
      );
    }
    throw error instanceof TaskQueueError ? error : new TaskQueueError("provider-unavailable");
  }

  function assertOpen() {
    if (closing) throw new TaskQueueError("closed");
  }
  const where = (jobId: string) =>
    and(eq(relation.queueName, options.queueName), eq(relation.jobId, jobId));
  async function lockedJob(client: PoolClient, jobId: string): Promise<StoredJob | undefined> {
    for (let attempt = 0; attempt < 3; attempt++) {
      const result = await client.query<StoredJob>(
        `SELECT id, state, retry_count, retry_limit, started_on, output FROM ${jobTable} WHERE name = $1 AND id = $2 FOR UPDATE`,
        [options.queueName, jobId],
      );
      if (result.rows[0]) return result.rows[0];
      // pg-boss failure moves a job via DELETE/INSERT. A waiting FOR UPDATE may miss
      // the replacement tuple under READ COMMITTED; inspect a fresh statement snapshot.
      const retained = await client.query(`SELECT 1 FROM ${jobTable} WHERE name = $1 AND id = $2`, [
        options.queueName,
        jobId,
      ]);
      if (!retained.rows.length) return undefined;
    }
    throw new TaskQueueError("provider-unavailable");
  }
  async function ownedRelation(client: PoolClient, jobId: string) {
    return (await drizzle(client).select().from(relation).where(where(jobId)))[0];
  }
  const backend: WorkerBackend = {
    fetch: () =>
      transaction(pool, async (client, db) => {
        const [job] = await boss.fetch(options.queueName, {
          batchSize: 1,
          includeMetadata: true,
          db,
        });
        if (!job) return null;
        const own = await ownedRelation(client, job.id);
        if (!own) {
          await boss.fail(
            options.queueName,
            { id: job.id, retryCount: job.retryCount },
            { ok: false, error: "invalid-input" },
            { db },
          );
          return null;
        }
        return {
          jobId: job.id,
          retryCount: job.retryCount,
          attempt: job.retryCount + 1,
          task: own.task,
          input: own.input,
          heartbeatMs: Math.max(
            1,
            Math.min(
              config.pollIntervalMs,
              (job.heartbeatSeconds ?? config.heartbeatSeconds) * 500,
            ),
          ),
        };
      }),
    pulse: (claim) =>
      transaction(pool, async (client, db) => {
        const owned =
          affected(
            await boss.touch(
              options.queueName,
              { id: claim.jobId, retryCount: claim.retryCount },
              { db },
            ),
          ) === 1;
        const own = await ownedRelation(client, claim.jobId);
        return { owned: owned && !!own, cancelRequested: own?.cancelRequested ?? false };
      }),
    settle: (claim, result) =>
      transaction(pool, async (client, db) => {
        const job = await lockedJob(client, claim.jobId);
        if (!job || job.state !== "active" || job.retry_count !== claim.retryCount) return;
        const own = await ownedRelation(client, claim.jobId);
        if (!own) return;
        const attempt = { id: claim.jobId, retryCount: claim.retryCount };
        if (own.cancelRequested) await boss.cancel(options.queueName, attempt, { db });
        else {
          const output =
            safeOutput(result) ??
            ({ ok: false, error: "invalid-result" } satisfies ExecutionResult);
          if (output.ok) await boss.complete(options.queueName, attempt, output, { db });
          else await boss.fail(options.queueName, attempt, output, { db });
        }
      }),
  };
  return {
    async enqueue(job) {
      assertOpen();
      return transaction(pool, async (client, db) => {
        const orm = drizzle(client);
        const jobId = randomUUID();
        const [inserted] = await orm
          .insert(relation)
          .values({
            queueName: options.queueName,
            jobId,
            task: job.task,
            input: sql`${JSON.stringify(job.input)}::jsonb`,
            deduplicationKey: job.deduplicationKey,
          })
          .onConflictDoNothing()
          .returning();
        if (!inserted) {
          const [existing] = await orm
            .select()
            .from(relation)
            .where(
              and(
                eq(relation.queueName, options.queueName),
                eq(relation.deduplicationKey, job.deduplicationKey!),
              ),
            );
          if (!existing || existing.task !== job.task)
            throw new TaskQueueError("deduplication-conflict");
          const comparison = await client.query<{ same: boolean }>(
            "SELECT $1::jsonb = $2::jsonb AS same",
            [JSON.stringify(existing.input), JSON.stringify(job.input)],
          );
          if (!comparison.rows[0]?.same) throw new TaskQueueError("deduplication-conflict");
          const retained = await client.query(
            `SELECT 1 FROM ${jobTable} WHERE name = $1 AND id = $2`,
            [options.queueName, existing.jobId],
          );
          if (!retained.rows.length) throw new TaskQueueError("job-expired");
          return existing.jobId;
        }
        const sent = await boss.send(
          options.queueName,
          { task: job.task, input: job.input },
          {
            id: jobId,
            db,
            startAfter: job.runAt,
            retryLimit: job.maxAttempts - 1,
            retryDelay: job.retry?.delaySeconds ?? 0,
            retryBackoff: job.retry?.backoff ?? false,
            ...(job.retry?.maxDelaySeconds === undefined
              ? {}
              : { retryDelayMax: job.retry.maxDelaySeconds }),
            heartbeatSeconds: config.heartbeatSeconds,
            expireInSeconds: config.expireInSeconds,
            retentionSeconds: config.retentionSeconds,
            deleteAfterSeconds: config.retentionSeconds,
          },
        );
        if (sent !== jobId) throw new Error("Task queue could not enqueue the job");
        return jobId;
      });
    },
    async get(jobId): Promise<JobStatus | null> {
      assertOpen();
      return transaction(pool, async (client) => {
        const job = await lockedJob(client, jobId);
        const own = await ownedRelation(client, jobId);
        if (!job || !own) return null;
        const state = states[job.state];
        if (!state) throw new Error("Invalid task queue state");
        const output = safeOutput(job.output);
        return {
          jobId: job.id,
          task: own.task,
          state,
          attempt: job.started_on ? job.retry_count + 1 : 0,
          maxAttempts: job.retry_limit + 1,
          cancelRequested: own.cancelRequested,
          result: state === "succeeded" && output?.ok ? output.result : null,
          error:
            state === "failed"
              ? output && !output.ok
                ? output.error
                : "handler-failed"
              : state === "cancelled"
                ? "aborted"
                : null,
        };
      });
    },
    async cancel(jobId) {
      assertOpen();
      return transaction(pool, async (client) => {
        const job = await lockedJob(client, jobId);
        if (!job || !(await ownedRelation(client, jobId))) return "missing";
        if (["completed", "failed", "cancelled"].includes(job.state)) return "terminal";
        await drizzle(client).update(relation).set({ cancelRequested: true }).where(where(jobId));
        if (job.state === "active") return "requested";
        // Administrative pending cancellation: the row lock excludes fetch.
        // Attempt fencing applies to active claims only; never use this path for one.
        await boss.cancel(options.queueName, jobId, { db: adapter(client) });
        return "cancelled";
      });
    },
    async retry(jobId) {
      assertOpen();
      return transaction(pool, async (client, db) => {
        const job = await lockedJob(client, jobId);
        if (!job || job.state !== "failed" || !(await ownedRelation(client, jobId))) return false;
        // Official retry adds one attempt and preserves started_on/retry_count, so the next claim advances the fence.
        const retried = affected(await boss.retry(options.queueName, jobId, { db })) === 1;
        if (retried)
          await drizzle(client)
            .update(relation)
            .set({ cancelRequested: false })
            .where(where(jobId));
        return retried;
      });
    },
    async startWorker(execute, options = {}) {
      assertOpen();
      const worker = createTaskWorker(backend, execute, options, config.pollIntervalMs);
      workers.add(worker);
      return worker;
    },
    close() {
      closing = true;
      closePromise ??= (async () => {
        const outcomes = await Promise.allSettled([...workers].map((worker) => worker.stop()));
        const failures = outcomes.flatMap((outcome) =>
          outcome.status === "rejected" ? [outcome.reason] : [],
        );
        try {
          await release(boss, pool, !options.pool);
        } catch (error) {
          failures.push(error);
        }
        if (failures.length) throw new AggregateError(failures, "Task queue close failed");
      })();
      return closePromise;
    },
  };
}
