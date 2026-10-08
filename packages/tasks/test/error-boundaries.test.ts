import { expect, test } from "bun:test";
import { resolve } from "node:path";

async function isolated(script: string) {
  const child = Bun.spawn([process.execPath, "-e", script], {
    cwd: resolve(import.meta.dir, ".."),
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
  });
  const [exit, stdout, stderr] = await Promise.all([
    child.exited,
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
  ]);
  expect({ exit, stdout, stderr }).toEqual({ exit: 0, stdout: "", stderr: "" });
}

test("task provider startup, migration and rollback retain both internal failures", async () => {
  await isolated(`
    import { mock } from "bun:test";
    import assert from "node:assert/strict";
    const primary = new Error("PRIVATE-primary");
    const cleanup = new Error("PRIVATE-cleanup");
    let phase = "startup", released = 0, stopped = 0;
    mock.module("pg-boss", () => ({
      PgBoss: class {
        on() {}
        async start() { if (phase === "startup") throw primary; }
        async stop() { stopped++; if (phase === "startup") throw cleanup; }
        async getQueue() { return { policy: "standard", partition: false }; }
      },
    }));
    const client = {
      async query(sql) {
        if (sql === "BEGIN") return {};
        if (sql === "ROLLBACK") throw cleanup;
        throw primary;
      },
      release() { released++; },
    };
    const pool = { async connect() { return client; } };
    const { createPostgresTaskProvider, migratePostgresTaskQueue } =
      await import("./src/postgres.ts");
    for (const action of [createPostgresTaskProvider, migratePostgresTaskQueue]) {
      const error = await action({ queueName: "fixture", pool }).catch(error => error);
      assert(error instanceof AggregateError);
      assert.equal(error.errors[0].code, "provider-unavailable");
      assert.equal(error.errors[0].cause, primary);
      assert(error.errors[1] instanceof AggregateError);
      assert.equal(error.errors[1].errors[0].cause, cleanup);
    }
    assert.equal(stopped, 2);
    phase = "rollback";
    const error = await migratePostgresTaskQueue({ queueName: "fixture", pool })
      .catch(error => error);
    assert(error instanceof AggregateError);
    assert.equal(error.errors[0].cause, primary);
    assert.equal(error.errors[1], cleanup);
    assert.equal(released, 1);
    assert.equal(stopped, 3);
  `);
});

test("task telemetry failure after side effects cannot replace result or cause", async () => {
  await isolated(`
    import { spyOn } from "bun:test";
    import assert from "node:assert/strict";
    import { metrics, trace } from "@opentelemetry/api";
    import { createTaskQueue, defineTask } from "./src/index.ts";
    import { z } from "zod";
    const broken = () => { throw new Error("PRIVATE-telemetry"); };
    spyOn(metrics, "getMeter").mockImplementation(broken);
    const span = { setAttribute: broken, setStatus: broken, end: broken };
    const tracer = spyOn(trace, "getTracer").mockReturnValue({
      startActiveSpan(...args) { return args.at(-1)(span); },
    });
    const cause = new Error("PRIVATE-provider");
    let worker, enqueues = 0, fail = false;
    const jobId = crypto.randomUUID();
    const task = defineTask({
      name: "fixture", input: z.object({}),
      async handler() { return { done: true }; }, result: value => value,
    });
    const provider = {
      async enqueue() { enqueues++; if (fail) throw cause; return jobId; },
      async startWorker(execute) { worker = execute; return { done: Promise.resolve(), async stop() {} }; },
      async close() {},
    };
    const queue = createTaskQueue({ provider, tasks: [task] });
    assert.equal(await queue.enqueue(task, {}), jobId);
    assert.equal(enqueues, 1);
    await queue.startWorker();
    assert.deepEqual(await worker({
      task: task.name, input: {}, jobId, attempt: 1,
      signal: new AbortController().signal,
    }), { ok: true, result: { done: true } });
    fail = true;
    const failure = await queue.enqueue(task, {}).catch(error => error);
    assert.equal(failure.code, "provider-unavailable");
    assert.equal(failure.cause, cause);
    assert.equal(enqueues, 2);
    fail = false;
    tracer.mockImplementation(broken);
    assert.equal(await queue.enqueue(task, {}), jobId);
    assert.equal(enqueues, 3);
    await queue.close();
  `);
});
