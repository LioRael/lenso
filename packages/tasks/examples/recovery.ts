import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Database, type SQLQueryBindings } from "bun:sqlite";
import { z } from "zod";
import { createTaskQueue, defineTask } from "../src/index";
import {
  createD1TaskProvider,
  provisionD1TaskQueue,
  type D1Database,
  type D1PreparedStatement,
  type D1Result,
} from "../src/d1";

// Native Bun worker and actual SQL, with a simulated D1 binding. This is not workerd or hosted D1.
async function open(path: string, initialize = false) {
  const sqlite = new Database(path);
  sqlite.exec("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000");
  if (initialize) {
    sqlite.exec(
      await readFile(new URL("../migrations/d1/0001_tasks.sql", import.meta.url), "utf8"),
    );
    sqlite.exec("CREATE TABLE receipt (business_key TEXT PRIMARY KEY, amount INTEGER NOT NULL)");
  }
  const runners = new WeakMap<D1PreparedStatement, () => D1Result<unknown>>();
  const queries = new WeakMap<D1PreparedStatement, string>();
  let beforeFetch: (() => Promise<void>) | undefined;
  const database: D1Database = {
    prepare(query) {
      let values: SQLQueryBindings[] = [];
      const run = () => ({ success: true, results: sqlite.query(query).all(...values) });
      const statement: D1PreparedStatement = {
        bind(...bindings) {
          values = bindings as SQLQueryBindings[];
          return statement;
        },
        async all<T>() {
          return run() as D1Result<T>;
        },
      };
      runners.set(statement, run);
      queries.set(statement, query);
      return statement;
    },
    async batch<T>(statements: D1PreparedStatement[]) {
      if (queries.get(statements[0]!)?.includes("AND state = 'running'")) await beforeFetch?.();
      // Execute synchronously inside one SQLite transaction, like D1's atomic batch.
      return sqlite.transaction(() =>
        statements.map((statement) => runners.get(statement)!()),
      )() as D1Result<T>[];
    },
    withSession() {
      return database;
    },
  };
  if (initialize) await provisionD1TaskQueue(database, "receipts");
  const provider = await createD1TaskProvider({
    database,
    queueName: "receipts",
    leaseMs: 200,
    pollIntervalMs: 10,
  });
  return {
    sqlite,
    provider,
    interceptFetch(callback?: () => Promise<void>) {
      beforeFetch = callback;
    },
  };
}

function receiptTask(
  sqlite: Database,
  mode: "normal" | "crash" | "fault",
  entered?: () => void,
  cleaned?: () => void,
) {
  return defineTask({
    name: "recordReceipt",
    input: z.object({ businessKey: z.string(), amount: z.number().int() }),
    maxAttempts: 2,
    async handler(input, context) {
      sqlite
        .query("INSERT OR IGNORE INTO receipt VALUES (?, ?)")
        .run(input.businessKey, input.amount);
      if (mode === "crash") {
        console.log(JSON.stringify({ ready: context.jobId }));
        await new Promise<void>(() => {});
      }
      if (mode === "fault" && context.attempt === 1) {
        const aborted = new Promise<void>((resolve) =>
          context.signal.addEventListener("abort", () => resolve(), { once: true }),
        );
        entered?.();
        try {
          await aborted;
        } finally {
          cleaned?.();
        }
      }
      return { businessKey: input.businessKey };
    },
    result: (value) => value,
  });
}

async function child(path: string) {
  const { sqlite, provider } = await open(path);
  const task = receiptTask(sqlite, "crash");
  const queue = createTaskQueue({ provider, tasks: [task] });
  await (
    await queue.startWorker({ concurrency: 2, maxJobs: 2 })
  ).done;
}

async function demonstrate() {
  const directory = await mkdtemp(join(tmpdir(), "lenso-task-recovery-"));
  const path = join(directory, "isolated.sqlite");
  const { sqlite, provider, interceptFetch } = await open(path, true);
  const task = receiptTask(sqlite, "normal");
  const queue = createTaskQueue({ provider, tasks: [task] });
  try {
    const input = { businessKey: "invoice-42", amount: 1200 };
    const jobId = await queue.enqueue(task, input, { deduplicationKey: input.businessKey });
    assert.equal(await queue.enqueue(task, input, { deduplicationKey: input.businessKey }), jobId);
    const finalId = await provider.enqueue({
      task: task.name,
      input: { businessKey: "invoice-final", amount: 500 },
      maxAttempts: 1,
    });
    console.log(JSON.stringify({ stage: "submitted", job: await queue.get(jobId) }));
    const childProcess = Bun.spawn([process.execPath, import.meta.path, "--crash", path], {
      stdout: "pipe",
      stderr: "pipe",
    });
    const timeout = setTimeout(() => childProcess.kill("SIGKILL"), 5000);
    try {
      const reader = childProcess.stdout.getReader();
      let output = "";
      while (output.trim().split("\n").filter(Boolean).length < 2) {
        const { value, done } = await reader.read();
        if (done) throw new Error("Crash fixture exited before both business writes");
        output += new TextDecoder().decode(value);
      }
      assert.equal((await queue.get(jobId))?.state, "running");
    } finally {
      clearTimeout(timeout);
      childProcess.kill("SIGKILL");
      await childProcess.exited;
      assert.equal(childProcess.signalCode, "SIGKILL");
      console.log(
        JSON.stringify({
          stage: "interrupted",
          signal: childProcess.signalCode,
          job: await queue.get(jobId),
        }),
      );
    }
    await Bun.sleep(250);
    await queue.runBatch();
    assert.equal((await queue.get(jobId))?.state, "succeeded");
    assert.equal((await queue.get(jobId))?.attempt, 2);
    assert.equal((await queue.get(finalId))?.state, "failed");
    assert.equal((await queue.get(finalId))?.error, "aborted");
    console.log(JSON.stringify({ stage: "attempts-exhausted", job: await queue.get(finalId) }));
    assert.equal(await queue.retry(finalId), true);
    await queue.runBatch();
    assert.equal((await queue.get(finalId))?.state, "succeeded");
    console.log(
      JSON.stringify({
        stage: "recovered-after-SIGKILL",
        job: await queue.get(jobId),
        explicitlyRetried: await queue.get(finalId),
        receipts: sqlite.query("SELECT * FROM receipt ORDER BY business_key").all(),
      }),
    );

    const entered = Promise.withResolvers<void>();
    let cleaned = false;
    const faultTask = receiptTask(
      sqlite,
      "fault",
      () => entered.resolve(),
      () => {
        cleaned = true;
      },
    );
    const faultQueue = createTaskQueue({ provider, tasks: [faultTask] });
    const faultId = await faultQueue.enqueue(faultTask, {
      businessKey: "invoice-fault",
      amount: 300,
    });
    let fetches = 0;
    interceptFetch(async () => {
      if (++fetches === 2) {
        await entered.promise;
        throw new Error("simulated provider outage");
      }
    });
    const worker = await faultQueue.startWorker({ concurrency: 2, maxJobs: 2 });
    await assert.rejects(worker.done, { code: "provider-unavailable" });
    assert.equal(cleaned, true);
    assert.equal((await faultQueue.get(faultId))?.state, "pending");
    console.log(
      JSON.stringify({
        stage: "provider-failure-drained",
        cleaned,
        job: await faultQueue.get(faultId),
      }),
    );
    interceptFetch();
    await faultQueue.runBatch();
    assert.equal((await faultQueue.get(faultId))?.state, "succeeded");
    assert.equal((await faultQueue.get(faultId))?.attempt, 2);

    const cancelEntered = Promise.withResolvers<void>();
    let cancelCleaned = false;
    const cancellable = defineTask({
      name: "waitForCancellation",
      input: z.object({}),
      async handler(_input, context) {
        const abort = new Promise<void>((resolve) =>
          context.signal.addEventListener("abort", () => resolve(), { once: true }),
        );
        cancelEntered.resolve();
        try {
          await abort;
        } finally {
          cancelCleaned = true;
        }
      },
    });
    const cancelQueue = createTaskQueue({ provider, tasks: [cancellable] });
    const cancelId = await cancelQueue.enqueue(cancellable, {});
    const cancelWorker = await cancelQueue.startWorker({ maxJobs: 1 });
    await cancelEntered.promise;
    assert.equal(await cancelQueue.cancel(cancelId), "requested");
    await cancelWorker.done;
    assert.equal(cancelCleaned, true);
    assert.equal((await cancelQueue.get(cancelId))?.state, "cancelled");
    const receipts = sqlite.query("SELECT * FROM receipt ORDER BY business_key").all();
    assert.equal(receipts.length, 3);
    console.log(
      JSON.stringify({
        stage: "complete",
        runtime: "native-bun",
        persistence: "simulated-D1-on-SQLite",
        workerPlatform: "not-verified-here",
        recovered: await faultQueue.get(faultId),
        cancelled: await cancelQueue.get(cancelId),
        cancelCleaned,
        receipts,
      }),
    );
  } finally {
    // A failed worker remains observable by provider.close; all handlers are already drained.
    await queue.close().catch((error: unknown) => {
      if (!(error instanceof Error) || !("code" in error) || error.code !== "provider-unavailable")
        throw error;
    });
    sqlite.close();
    await rm(directory, { recursive: true, force: true });
  }
}

if (process.argv[2] === "--crash") await child(process.argv[3]!);
else await demonstrate();
