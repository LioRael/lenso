import { describe, expect, test } from "bun:test";
import { Pool } from "pg";
import { PgBoss, type Db } from "pg-boss";
import { z } from "zod";
import { createTaskQueue, defineTask } from "../src/index";
import { createPostgresTaskProvider, migratePostgresTaskQueue } from "../src/postgres";

const connectionString = process.env.TASK_TEST_DATABASE_URL;
const integration = connectionString ? describe : describe.skip;
const fixture = new URL("./fixtures/crash-worker.ts", import.meta.url).pathname;

async function waitFor<T>(
  read: () => Promise<T>,
  condition: (value: T) => boolean,
  timeoutMs = 15_000,
): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const value = await read();
    if (condition(value)) return value;
    await Bun.sleep(30);
  }
  throw new Error("Timed out waiting for PostgreSQL task state");
}

async function setup() {
  const queueName = `test_${crypto.randomUUID().replaceAll("-", "")}`;
  const options = {
    connectionString: connectionString!,
    queueName,
    pollIntervalMs: 30,
    heartbeatSeconds: 10,
    expireInSeconds: 120,
    superviseIntervalSeconds: 1,
  };
  await migratePostgresTaskQueue(options);
  return { queueName, options };
}

integration("real PostgreSQL durable tasks", () => {
  test("unmigrated startup does not create a schema; borrowed pool stays open", async () => {
    const pool = new Pool({ connectionString });
    const schema = `uninstalled_${crypto.randomUUID().replaceAll("-", "")}`;
    try {
      await expect(
        createPostgresTaskProvider({ pool, schema, queueName: "missing" }),
      ).rejects.toBeDefined();
      const result = await pool.query("SELECT 1 FROM pg_namespace WHERE nspname = $1", [schema]);
      expect(result.rows).toHaveLength(0);
      expect((await pool.query("SELECT 1 AS alive")).rows[0].alive).toBe(1);
      const { options } = await setup();
      const provider = await createPostgresTaskProvider({
        ...options,
        pool,
        connectionString: undefined,
      });
      const second = await setup();
      const other = await createPostgresTaskProvider(second.options);
      try {
        const jobId = await provider.enqueue({ task: "isolated", input: null, maxAttempts: 1 });
        expect((await provider.get(jobId))?.attempt).toBe(0);
        expect(await other.get(jobId)).toBeNull();
        expect(await other.cancel(jobId)).toBe("missing");
        expect(await other.retry(jobId)).toBe(false);
      } finally {
        await provider.close();
        await other.close();
      }
      expect((await pool.query("SELECT 1 AS alive")).rows[0].alive).toBe(1);
    } finally {
      await pool.end();
    }
  }, 20_000);

  test("delayed jobs, persistent dedup, limited failures and explicit retry retain jobId", async () => {
    const { options } = await setup();
    const task = defineTask({
      name: "report",
      input: z.object({ reportId: z.string(), failUntil: z.number() }),
      maxAttempts: 3,
      retry: { delaySeconds: 1, backoff: false },
      async handler(input, context) {
        if (context.attempt <= input.failUntil) throw new Error("Bearer secret-must-not-leak");
        return { reportId: input.reportId };
      },
      result: (report) => report,
    });
    let queue = createTaskQueue({
      provider: await createPostgresTaskProvider(options),
      tasks: [task],
    });
    const input = { reportId: "idempotent-report", failUntil: 3 };
    const jobId = await queue.enqueue(task, input, {
      runAt: new Date(Date.now() + 500),
      deduplicationKey: "report-key",
    });
    await queue.close();
    queue = createTaskQueue({ provider: await createPostgresTaskProvider(options), tasks: [task] });
    try {
      expect(await queue.enqueue(task, input, { deduplicationKey: "report-key" })).toBe(jobId);
      await expect(
        queue.enqueue(
          task,
          { ...input, reportId: "different" },
          { deduplicationKey: "report-key" },
        ),
      ).rejects.toBeDefined();
      await queue.startWorker();
      expect((await queue.get(jobId))?.state).toBe("pending");
      const failed = await waitFor(
        () => queue.get(jobId),
        (job) => job?.state === "failed",
      );
      expect(failed).toMatchObject({ jobId, attempt: 3, maxAttempts: 3, error: "handler-failed" });
      expect(JSON.stringify(failed)).not.toContain("secret");
      expect(await queue.retry(jobId)).toBe(true);
      const completed = await waitFor(
        () => queue.get(jobId),
        (job) => job?.state === "succeeded",
      );
      expect(completed).toMatchObject({
        jobId,
        attempt: 4,
        maxAttempts: 4,
        result: { reportId: input.reportId },
      });
      expect(await queue.retry(jobId)).toBe(false);
      expect(await queue.cancel(jobId)).toBe("terminal");
    } finally {
      await queue.close();
    }
  }, 20_000);

  test("pending cancellation and cooperative running cancellation are actual terminal states", async () => {
    const { options } = await setup();
    let entered = false;
    const task = defineTask({
      name: "cancellable",
      input: z.object({}),
      async handler(_input, { signal }) {
        entered = true;
        await new Promise<void>((resolve) => {
          signal.addEventListener("abort", () => resolve(), { once: true });
          if (signal.aborted) resolve();
        });
        return null;
      },
    });
    const queue = createTaskQueue({
      provider: await createPostgresTaskProvider(options),
      tasks: [task],
    });
    try {
      const pending = await queue.enqueue(task, {});
      expect(await queue.cancel(pending)).toBe("cancelled");
      expect((await queue.get(pending))?.state).toBe("cancelled");
      await queue.startWorker();
      const running = await queue.enqueue(task, {});
      await waitFor(async () => entered, Boolean);
      expect(await queue.cancel(running)).toBe("requested");
      await waitFor(
        () => queue.get(running),
        (job) => job?.state === "cancelled",
      );
      expect(await queue.cancel(running)).toBe("terminal");
    } finally {
      await queue.close();
    }
  }, 20_000);

  test("cancel/timeout do not free an ignoring handler's slot; stop waits and stops claiming", async () => {
    const { options } = await setup();
    const gate = Promise.withResolvers<void>();
    let entered = 0;
    let signal: AbortSignal | undefined;
    const task = defineTask({
      name: "ignoresAbort",
      input: z.object({}),
      maxAttempts: 1,
      async handler(_input, context) {
        entered++;
        signal = context.signal;
        await gate.promise;
      },
    });
    const queue = createTaskQueue({
      provider: await createPostgresTaskProvider(options),
      tasks: [task],
    });
    try {
      const first = await queue.enqueue(task, {});
      const second = await queue.enqueue(task, {});
      const worker = await queue.startWorker({ concurrency: 1, timeoutMs: 50 });
      await waitFor(async () => signal?.aborted, Boolean);
      expect(entered).toBe(1);
      expect((await queue.get(first))?.state).toBe("running");
      expect((await queue.get(second))?.state).toBe("pending");
      expect(await queue.cancel(first)).toBe("requested");
      await Bun.sleep(100);
      expect(entered).toBe(1);
      expect((await queue.get(first))?.state).toBe("running");
      let stopped = false;
      const stop = worker.stop({ abort: true }).then(() => {
        stopped = true;
      });
      await Bun.sleep(100);
      expect(stopped).toBe(false);
      gate.resolve();
      await stop;
      expect(stopped).toBe(true);
      expect(entered).toBe(1);
      expect((await queue.get(first))?.state).toBe("cancelled");
      expect((await queue.get(second))?.state).toBe("pending");
    } finally {
      gate.resolve();
      await queue.close();
    }
  }, 20_000);

  test("cancel survives pg-boss failure row replacement; stale acknowledgement is fenced", async () => {
    const { options, queueName } = await setup();
    const pool = new Pool({ connectionString, max: 1 });
    const control = new Pool({ connectionString });
    const provider = await createPostgresTaskProvider({
      ...options,
      pool,
      connectionString: undefined,
    });
    const boss = new PgBoss({
      db: { executeSql: (query, values) => control.query(query, values) },
      migrate: false,
      supervise: false,
      schedule: false,
      reindex: false,
    });
    const task = defineTask({
      name: "fenced",
      input: z.object({}),
      maxAttempts: 3,
      async handler() {},
    });
    const queue = createTaskQueue({ provider, tasks: [task] });
    const tx = await control.connect();
    try {
      await boss.start();
      const jobId = await queue.enqueue(task, {});
      const [old] = await boss.fetch(queueName, { includeMetadata: true });
      await tx.query("BEGIN");
      const db: Db = { executeSql: (query, values) => tx.query(query, values) };
      await boss.fail(queueName, old, { ok: false, error: "handler-failed" }, { db });
      const pid = (await pool.query("SELECT pg_backend_pid() AS pid")).rows[0].pid;
      const cancellation = queue.cancel(jobId);
      await waitFor(
        async () =>
          (
            await control.query("SELECT wait_event_type FROM pg_stat_activity WHERE pid = $1", [
              pid,
            ])
          ).rows[0]?.wait_event_type,
        (event) => event === "Lock",
      );
      await tx.query("COMMIT");
      expect(await cancellation).toBe("cancelled");
      expect((await queue.get(jobId))?.state).toBe("cancelled");

      const nextId = await queue.enqueue(task, {});
      const [attemptA] = await boss.fetch(queueName, { includeMetadata: true });
      await boss.fail(queueName, attemptA, { ok: false, error: "handler-failed" });
      const [attemptB] = await boss.fetch(queueName, { includeMetadata: true });
      expect(attemptB.retryCount).toBe(attemptA.retryCount + 1);
      const stale = await boss.complete(queueName, attemptA, { ok: true, result: "stale" });
      expect((stale as { affected: number }).affected).toBe(0);
      expect((await queue.get(nextId))?.state).toBe("running");
      await boss.complete(queueName, attemptB, { ok: true, result: "current" });
      expect((await queue.get(nextId))?.result).toBe("current");
    } finally {
      await tx.query("ROLLBACK");
      tx.release();
      await queue.close();
      await boss.stop();
      await pool.end();
      await control.end();
    }
  }, 20_000);

  test("separate producer exits; SIGKILL worker is recovered by another process", async () => {
    const { options, queueName } = await setup();
    const env = {
      ...process.env,
      TASK_TEST_DATABASE_URL: connectionString!,
      TASK_TEST_QUEUE: queueName,
    };
    const producer = Bun.spawn([process.execPath, fixture, "enqueue"], {
      env,
      stdout: "pipe",
      stderr: "pipe",
    });
    const jobId = (await new Response(producer.stdout).text()).trim();
    expect(await producer.exited).toBe(0);
    expect(jobId).toMatch(/^[a-f0-9-]{36}$/);
    const provider = await createPostgresTaskProvider(options);
    let child = Bun.spawn([process.execPath, fixture], { env, stdout: "pipe", stderr: "pipe" });
    try {
      expect((await provider.get(jobId))?.state).toBe("pending");
      const reader = child.stdout.getReader();
      const start = await reader.read();
      expect(new TextDecoder().decode(start.value)).toContain('"attempt":1');
      child.kill("SIGKILL");
      await child.exited;
      child = Bun.spawn([process.execPath, fixture], { env, stdout: "pipe", stderr: "pipe" });
      // Recover by missed heartbeat, well before the independent 120-second expiry.
      const recovered = await waitFor(
        () => provider.get(jobId),
        (job) => job?.state === "succeeded",
        20_000,
      );
      expect(recovered).toMatchObject({
        jobId,
        attempt: 2,
        result: { key: "stable-business-key" },
      });
      child.kill("SIGTERM");
      expect(await child.exited).toBe(0);
    } finally {
      child.kill("SIGKILL");
      await child.exited;
      await provider.close();
    }
  }, 30_000);
});
