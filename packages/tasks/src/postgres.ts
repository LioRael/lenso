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
import { taskQueueIdentitySchema, taskQueueSchema } from "./schema";
import { traceMetadata } from "./telemetry";
import { createTaskWorker, type WorkerBackend } from "./worker";
import { jobPage, normalizeJobQuery } from "./query";

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
  const client = await pool.connect().catch((cause) => {
    throw new TaskQueueError("provider-unavailable", { cause });
  });
  try {
    await client.query("BEGIN");
    const result = await run(client, adapter(client));
    await client.query("COMMIT");
    return result;
  } catch (error) {
    const failure =
      error instanceof TaskQueueError
        ? error
        : new TaskQueueError("provider-unavailable", { cause: error });
    try {
      await client.query("ROLLBACK");
    } catch (cleanup) {
      throw new AggregateError([failure, cleanup], "Task transaction and rollback failed");
    }
    throw failure;
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
    // The portable queue boundary owns spans, propagation and bounded metric labels.
    openTelemetry: { enabled: false },
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
  } catch (cause) {
    failures.push(new TaskQueueError("provider-unavailable", { cause }));
  }
  if (owned) {
    try {
      await pool.end();
    } catch (cause) {
      failures.push(new TaskQueueError("provider-unavailable", { cause }));
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
  let failure: unknown;
  try {
    await boss.start();
    const existing = await boss.getQueue(options.queueName);
    if (existing?.partition || (existing && existing.policy !== "standard")) {
      throw new Error("Task queue requires an ordinary nonpartitioned standard queue");
    }
    if (!existing)
      await boss.createQueue(options.queueName, { partition: false, policy: "standard" });
    const migrationSql = await readFile(
      new URL("../migrations/0001_task_relation.sql", import.meta.url),
      "utf8",
    );
    await transaction(pool, async (client) => {
      await client.query(migrationSql.replaceAll("__LENSO_SCHEMA__", `"${config.schema}"`));
      const metadataSql = await readFile(
        new URL("../migrations/0002_trace_metadata.sql", import.meta.url),
        "utf8",
      );
      await client.query(metadataSql.replaceAll("__LENSO_SCHEMA__", `"${config.schema}"`));
      const identitySql = await readFile(
        new URL("../migrations/0003_queue_identity.sql", import.meta.url),
        "utf8",
      );
      await client.query(identitySql.replaceAll("__LENSO_SCHEMA__", `"${config.schema}"`));
      await drizzle(client)
        .insert(taskQueueIdentitySchema(config.schema))
        .values({ queueName: options.queueName, queueId: randomUUID() })
        .onConflictDoNothing();
    });
  } catch (error) {
    failure =
      error instanceof TaskQueueError || error instanceof AggregateError
        ? error
        : new TaskQueueError("provider-unavailable", { cause: error });
  }
  try {
    await release(boss, pool, !options.pool);
  } catch (cleanup) {
    if (failure !== undefined)
      throw new AggregateError([failure, cleanup], "Task migration and cleanup failed");
    throw cleanup;
  }
  if (failure !== undefined) throw failure;
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
  const identityTable = taskQueueIdentitySchema(config.schema);
  let queueId: string;
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
    const [identity] = await drizzle(pool)
      .select()
      .from(identityTable)
      .where(eq(identityTable.queueName, options.queueName));
    if (!identity || !/^[0-9a-f]{8}(-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(identity.queueId))
      throw new Error("Task queue identity is not migrated");
    queueId = identity.queueId;
  } catch (error) {
    const failure =
      error instanceof TaskQueueError || error instanceof AggregateError
        ? error
        : new TaskQueueError("provider-unavailable", { cause: error });
    try {
      await release(boss, pool, !options.pool);
    } catch (cleanupError) {
      throw new AggregateError([failure, cleanupError], "Task queue startup and cleanup failed");
    }
    throw failure;
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
          traceMetadata: traceMetadata(own.traceMetadata),
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
  const provider: TaskProvider = {
    async identity() {
      assertOpen();
      return { kind: "postgres", id: queueId };
    },
    async lookupDeduplicationKey(key) {
      assertOpen();
      const [accepted] = await drizzle(pool)
        .select({ jobId: relation.jobId })
        .from(relation)
        .where(and(eq(relation.queueName, options.queueName), eq(relation.deduplicationKey, key)));
      if (!accepted) return null;
      // Pruning between these reads preserves acceptance and yields a null status.
      return { jobId: accepted.jobId, status: await provider.get(accepted.jobId) };
    },
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
            traceMetadata: traceMetadata(job.traceMetadata),
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
    async list(query) {
      assertOpen();
      const { tasks, limit, after } = normalizeJobQuery(query);
      if (!tasks.length) return { items: [], nextCursor: null };
      return transaction(pool, async (client) => {
        const response = await client.query<{
          id: string;
          task: string;
          state: string;
          retry_count: number;
          retry_limit: number;
          started_on: Date | null;
          cancel_requested: boolean;
        }>(
          `SELECT j.id, r.task, j.state, j.retry_count, j.retry_limit,
            j.started_on, r.cancel_requested
          FROM ${jobTable} j
          JOIN "${config.schema}".lenso_task_relation r
            ON r.queue_name = j.name AND r.job_id = j.id
          WHERE j.name = $1 AND r.task = ANY($2::text[])
            AND ($3::uuid IS NULL OR j.id > $3::uuid)
          ORDER BY j.id ASC LIMIT $4`,
          [options.queueName, tasks, after ?? null, limit + 1],
        );
        return jobPage(
          response.rows.map((job) => {
            const state = states[job.state];
            if (!state) throw new TaskQueueError("provider-unavailable");
            return {
              jobId: job.id,
              task: job.task,
              state,
              attempt: job.started_on ? job.retry_count + 1 : 0,
              maxAttempts: job.retry_limit + 1,
              cancelRequested: job.cancel_requested,
            };
          }),
          limit,
        );
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
    async startWorker(execute, workerOptions = {}) {
      assertOpen();
      const worker = createTaskWorker(backend, execute, workerOptions, config.pollIntervalMs);
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
  return provider;
}
