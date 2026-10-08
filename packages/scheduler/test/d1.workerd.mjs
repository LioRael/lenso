import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";
import { drizzle } from "drizzle-orm/d1";
import { audience, createAuth, defineSource, realm } from "@lenso/auth";
import { createTaskQueue, defineTask } from "@lenso/tasks";
import { createD1TaskProvider, provisionD1TaskQueue } from "@lenso/tasks/d1";
import { createScheduler } from "@lenso/scheduler";
import { createD1ScheduleStore } from "@lenso/scheduler/d1";
import { createD1FixtureTask } from "./fixtures/d1-task.ts";

async function applyMigration(database, path) {
  const script = await readFile(new URL(path, import.meta.url), "utf8");
  // Reviewed DDL only; no procedural SQL or semicolons inside string literals.
  for (const sql of script
    .replace(/--[^\n]*/g, "")
    .split(";")
    .map((part) => part.trim())
    .filter(Boolean))
    await database.prepare(sql).run();
}

async function runtime(t) {
  const script = await readFile(new URL("../.lenso/workerd/d1-worker.js", import.meta.url), "utf8");
  const mf = new Miniflare({
    ...convertV4MiniflareOptions({
      modules: true,
      script,
      compatibilityDate: "2026-10-06",
      compatibilityFlags: ["nodejs_compat"],
      d1Databases: { DB: crypto.randomUUID() },
      d1Persist: false,
    }),
    host: "127.0.0.1",
    port: 0,
    telemetry: { enabled: false },
  });
  t.after(() => mf.dispose());
  const database = await mf.getD1Database("DB");
  await applyMigration(database, "../../tasks/migrations/d1/0001_tasks.sql");
  await applyMigration(database, "../migrations/d1/0001_scheduler.sql");
  await database
    .prepare(
      "CREATE TABLE fixture_permission(subject_id TEXT PRIMARY KEY, allowed INTEGER NOT NULL)",
    )
    .run();
  await database.prepare("INSERT INTO fixture_permission VALUES ('alice', 1), ('bob', 0)").run();
  await database
    .prepare("CREATE TABLE fixture_effect(job_id TEXT PRIMARY KEY, value INTEGER NOT NULL)")
    .run();
  return { mf, database };
}

async function setup(t, options = {}) {
  const { mf, database } = await runtime(t);
  const queueName = options.queueName ?? `queue-${crypto.randomUUID()}`;
  await provisionD1TaskQueue(database, queueName);
  const time = { now: Date.parse("2025-01-01T00:00:00Z") };
  const providerOptions = {
    database,
    queueName,
    clock: () => time.now,
    leaseMs: 100,
    pollIntervalMs: 10,
  };
  const task = createD1FixtureTask(database);
  const provider = await createD1TaskProvider(providerOptions);
  const queue = createTaskQueue({ provider, tasks: [task] });
  t.after(() => queue.close());
  const store = await createD1ScheduleStore(drizzle(database));
  const scope = {
    namespace: options.namespace ?? `scope-${crypto.randomUUID()}`,
    tenantId: "tenant-a",
  };
  const auth = createAuth(
    realm(
      "d1-fixture",
      defineSource({
        async verify(subject) {
          return ["alice", "bob"].includes(subject)
            ? { status: "verified", subjectId: subject }
            : { status: "rejected" };
        },
      }),
    ),
  );
  t.after(() => auth.close());
  const access = auth.for(audience("schedules"));
  const actor = await access.required("alice");
  const schedulerOptions = {
    store,
    scope,
    queue,
    tasks: [task],
    clock: () => time.now,
    dispatchLeaseMs: 100,
    async authorize(caller, _action, actualScope, resource) {
      await access.enforce(
        caller,
        resource,
        ({ principal, resource: value }) =>
          actualScope.tenantId === "tenant-a" &&
          (!value || !("initiator" in value) || value.initiator.subjectId === principal.subjectId),
      );
      return true;
    },
    async authorizeExecution(initiator, actualScope) {
      return (
        actualScope.tenantId === "tenant-a" &&
        initiator.realmId === "d1-fixture" &&
        !!(
          await database
            .prepare("SELECT allowed FROM fixture_permission WHERE subject_id = ?")
            .bind(initiator.subjectId)
            .first()
        )?.allowed
      );
    },
  };
  const scheduler = createScheduler(schedulerOptions);
  const input = {
    task: task.name,
    input: { value: 42 },
    rule: { kind: "once", at: time.now },
    misfire: "coalesce",
    graceMs: 0,
  };
  return {
    mf,
    database,
    queueName,
    time,
    providerOptions,
    provider,
    task,
    queue,
    store,
    scope,
    actor,
    access,
    schedulerOptions,
    scheduler,
    input,
  };
}

function occurrence(schedule) {
  return {
    id: crypto.randomUUID(),
    scheduleId: schedule.id,
    revision: schedule.revision,
    scheduledAt: schedule.nextAt,
    source: "timer",
    task: schedule.task,
    input: schedule.input,
    initiator: schedule.initiator,
    state: "pending",
    jobId: null,
    error: null,
    leaseToken: null,
    leaseUntil: null,
  };
}

const settings = { timeout: 30_000 };

test(
  "actual workerd scheduled invocations share Tasks execution and duplicate events remain idempotent",
  settings,
  async (t) => {
    const f = await setup(t, { queueName: "workerd-fixture", namespace: "workerd-fixture" });
    const plan = await f.scheduler.create(f.input, f.actor);
    const worker = await f.mf.getWorker();
    const validation = await f.mf.dispatchFetch("http://localhost/");
    assert.deepEqual(await validation.json(), { value: { value: 42, failUntil: 0 } });
    await Promise.all([
      worker.scheduled({ scheduledTime: f.time.now, cron: "* * * * *" }),
      worker.scheduled({ scheduledTime: f.time.now, cron: "* * * * *" }),
    ]);
    const [linked] = await f.scheduler.occurrences(plan.id, f.actor);
    assert.equal(linked.acceptance, "confirmed", JSON.stringify(linked));
    assert.equal(linked.job.state, "succeeded");
    assert.deepEqual(linked.job.result, { value: 42, attempt: 1 });
    assert.equal(
      (await f.database.prepare("SELECT COUNT(*) AS count FROM fixture_effect").first()).count,
      1,
    );
    await worker.scheduled({ scheduledTime: f.time.now, cron: "* * * * *" });
    assert.equal((await f.scheduler.occurrences(plan.id, f.actor)).length, 1);
  },
);

test(
  "D1 durable identity and canonical input dedup survive provider recreation; lookup writes nothing",
  settings,
  async (t) => {
    const f = await setup(t);
    const before = await f.queue.identity();
    await provisionD1TaskQueue(f.database, f.queueName);
    const other = await createD1TaskProvider(f.providerOptions);
    t.after(() => other.close());
    assert.deepEqual(await other.identity(), before);
    assert.equal(await f.queue.lookupDeduplicationKey("missing"), null);
    const jobs = await Promise.all(
      Array.from({ length: 8 }, (_, index) =>
        f.queue.enqueue(
          f.task,
          index % 2 ? { value: 7, failUntil: 0 } : { failUntil: 0, value: 7 },
          { deduplicationKey: "same" },
        ),
      ),
    );
    assert.equal(new Set(jobs).size, 1);
    assert.equal((await other.lookupDeduplicationKey("same")).jobId, jobs[0]);
    await assert.rejects(
      f.queue.enqueue(f.task, { value: 8, failUntil: 0 }, { deduplicationKey: "same" }),
      { code: "deduplication-conflict" },
    );
    assert.equal(
      (await f.database.prepare("SELECT COUNT(*) AS count FROM lenso_d1_task_job").first()).count,
      1,
    );
  },
);

test("actual workerd cron tick preserves the pinned DST next-instant rule", settings, async (t) => {
  const f = await setup(t, { queueName: "workerd-fixture", namespace: "workerd-fixture" });
  f.time.now = Date.parse("2024-03-09T08:00:00Z");
  const plan = await f.scheduler.create(
    {
      ...f.input,
      rule: { kind: "cron", expression: "30 2 * * *", timezone: "America/New_York" },
    },
    f.actor,
  );
  assert.equal(plan.nextAt, Date.parse("2024-03-10T07:30:00Z"));
  const worker = await f.mf.getWorker();
  await worker.scheduled({ scheduledTime: plan.nextAt, cron: "* * * * *" });
  assert.equal(
    (await f.scheduler.get(plan.id, f.actor)).nextAt,
    Date.parse("2024-03-11T06:30:00Z"),
  );
  assert.equal((await f.scheduler.occurrences(plan.id, f.actor))[0].job.state, "succeeded");
  await worker.scheduled({ scheduledTime: plan.nextAt, cron: "* * * * *" });
  assert.equal((await f.scheduler.occurrences(plan.id, f.actor)).length, 1);
});

test(
  "D1 atomic advance rejects stale CAS and rolls back the cursor on insert failure",
  settings,
  async (t) => {
    const f = await setup(t);
    const plan = await f.scheduler.create(f.input, f.actor);
    const snapshot = await f.store.get(f.scope, plan.id);
    await assert.rejects(
      f.store.advance(f.scope, snapshot, null, {
        ...occurrence(snapshot),
        scheduleId: crypto.randomUUID(),
      }),
    );
    assert.deepEqual(await f.store.get(f.scope, plan.id), snapshot);
    const outcomes = await Promise.all(
      Array.from({ length: 8 }, () =>
        f.store.advance(f.scope, snapshot, null, occurrence(snapshot)),
      ),
    );
    assert.equal(outcomes.filter(Boolean).length, 1);
    assert.equal((await f.store.occurrences(f.scope, plan.id, 10)).length, 1);
  },
);

test(
  "D1 lease claims and renew/settle are fenced across instances and tenant scopes",
  settings,
  async (t) => {
    const f = await setup(t);
    const plan = await f.scheduler.create(f.input, f.actor);
    const snapshot = await f.store.get(f.scope, plan.id);
    await f.store.advance(f.scope, snapshot, null, occurrence(snapshot));
    const claims = await Promise.all(
      Array.from({ length: 8 }, () => f.store.claim(f.scope, f.time.now, 100)),
    );
    assert.equal(claims.filter(Boolean).length, 1);
    const first = claims.find(Boolean);
    assert.equal(
      await f.store.claim({ ...f.scope, tenantId: "tenant-b" }, f.time.now + 101, 100),
      null,
    );
    const second = await f.store.claim(f.scope, f.time.now + 101, 100);
    assert.notEqual(first.leaseToken, second.leaseToken);
    assert.equal(
      await f.store.renew(f.scope, first.id, first.leaseToken, f.time.now + 101, 100),
      false,
    );
    assert.equal(
      await f.store.settle(f.scope, first.id, first.leaseToken, { jobId: crypto.randomUUID() }),
      false,
    );
    assert.equal(
      await f.store.settle(f.scope, second.id, second.leaseToken, { error: "execution-denied" }),
      true,
    );
  },
);

test(
  "D1 stale update/trigger/cancel and durable queue replacement cannot silently succeed",
  settings,
  async (t) => {
    const f = await setup(t);
    const plan = await f.scheduler.create(f.input, f.actor);
    const snapshot = await f.store.get(f.scope, plan.id);
    const paused = await f.scheduler.pause(plan.id, plan.revision, f.actor);
    assert.equal(await f.store.advance(f.scope, snapshot, null, occurrence(snapshot)), false);
    assert.equal(await f.store.trigger(f.scope, snapshot, occurrence(snapshot)), null);
    const triggers = await Promise.all([
      f.scheduler.trigger(plan.id, "manual", f.actor),
      f.scheduler.trigger(plan.id, "manual", f.actor),
    ]);
    assert.deepEqual(triggers[0], triggers[1]);
    await f.scheduler.cancel(plan.id, paused.revision, f.actor);
    assert.equal((await f.scheduler.tick()).enqueued, 1);
    await provisionD1TaskQueue(f.database, "other-queue");
    const replacement = createTaskQueue({
      provider: await createD1TaskProvider({ ...f.providerOptions, queueName: "other-queue" }),
      tasks: [f.task],
    });
    t.after(() => replacement.close());
    await assert.rejects(createScheduler({ ...f.schedulerOptions, queue: replacement }).tick(), {
      code: "queue-mismatch",
    });
    await assert.rejects(f.scheduler.get(plan.id, { ...f.actor }), { code: "UNAUTHORIZED" });
  },
);

test(
  "D1 accepted enqueue with lost acknowledgement recovers after revocation without enqueueing again",
  settings,
  async (t) => {
    const f = await setup(t);
    const plan = await f.scheduler.create(f.input, f.actor);
    let calls = 0;
    const unreliable = createScheduler({
      ...f.schedulerOptions,
      queue: {
        ...f.queue,
        async enqueue(...args) {
          calls++;
          await f.queue.enqueue(...args);
          throw new Error("fixture lost acknowledgement");
        },
      },
    });
    assert.equal((await unreliable.tick()).failed, 1);
    await f.database
      .prepare("UPDATE fixture_permission SET allowed = 0 WHERE subject_id = 'alice'")
      .run();
    f.time.now += 101;
    const recovered = createScheduler({
      ...f.schedulerOptions,
      queue: {
        ...f.queue,
        async enqueue() {
          throw new Error("must not enqueue accepted work");
        },
      },
    });
    assert.equal((await recovered.tick()).enqueued, 1);
    const [linked] = await recovered.occurrences(plan.id, f.actor);
    assert.equal(linked.acceptance, "confirmed");
    assert.equal(linked.job.state, "pending");
    assert.equal(calls, 1);
    await f.queue.runBatch({ maxJobs: 10 });
    f.time.now += 1001;
    await f.queue.runBatch({ maxJobs: 10 });
    assert.equal((await recovered.occurrences(plan.id, f.actor))[0].job.state, "failed");
    assert.equal(
      (await f.database.prepare("SELECT COUNT(*) AS count FROM fixture_effect").first()).count,
      0,
    );
  },
);

test(
  "D1 before-enqueue failure respects revoked execution permission on recovery",
  settings,
  async (t) => {
    const f = await setup(t);
    const plan = await f.scheduler.create(f.input, f.actor);
    const unavailable = createScheduler({
      ...f.schedulerOptions,
      queue: {
        ...f.queue,
        async enqueue() {
          throw new Error("fixture unavailable");
        },
      },
    });
    assert.equal((await unavailable.tick()).failed, 1);
    await f.database
      .prepare("UPDATE fixture_permission SET allowed = 0 WHERE subject_id = 'alice'")
      .run();
    f.time.now += 101;
    assert.equal((await f.scheduler.tick()).denied, 1);
    const [status] = await f.scheduler.occurrences(plan.id, f.actor);
    assert.equal(status.occurrence.state, "blocked");
    assert.equal(status.acceptance, "unknown");
    assert.equal(status.job, null);
  },
);

test(
  "D1 read-only lookup reveals an enqueue that completes after another dispatcher denied it",
  settings,
  async (t) => {
    const f = await setup(t);
    const plan = await f.scheduler.create(f.input, f.actor);
    const entered = Promise.withResolvers();
    const release = Promise.withResolvers();
    const slow = createScheduler({
      ...f.schedulerOptions,
      queue: {
        ...f.queue,
        async enqueue(...args) {
          entered.resolve();
          await release.promise;
          return f.queue.enqueue(...args);
        },
      },
    });
    const first = slow.tick();
    try {
      await entered.promise;
      f.time.now += 101;
      await f.database
        .prepare("UPDATE fixture_permission SET allowed = 0 WHERE subject_id = 'alice'")
        .run();
      assert.equal((await f.scheduler.tick()).denied, 1);
    } finally {
      release.resolve();
      await first;
    }
    const [linked] = await f.scheduler.occurrences(plan.id, f.actor);
    assert.equal(linked.occurrence.state, "blocked");
    assert.equal(linked.acceptance, "confirmed");
    assert.equal(linked.job.state, "pending");
    await f.queue.runBatch({ maxJobs: 10 });
    f.time.now += 1001;
    await f.queue.runBatch({ maxJobs: 10 });
    assert.equal((await f.scheduler.occurrences(plan.id, f.actor))[0].job.state, "failed");
  },
);

test(
  "D1 finite workers delay/retry, cancel and explicit retry without resetting job identity",
  settings,
  async (t) => {
    const f = await setup(t);
    const id = await f.queue.enqueue(
      f.task,
      { value: 1, failUntil: 2 },
      { runAt: new Date(f.time.now + 1000), deduplicationKey: "retry" },
    );
    await f.queue.runBatch({ maxJobs: 5 });
    assert.equal((await f.queue.get(id)).attempt, 0);
    f.time.now += 1001;
    await f.queue.runBatch({ maxJobs: 5 });
    assert.equal((await f.queue.get(id)).attempt, 1);
    assert.equal((await f.queue.get(id)).state, "pending");
    f.time.now += 1001;
    await f.queue.runBatch({ maxJobs: 5 });
    assert.equal((await f.queue.get(id)).state, "failed");
    assert.equal(await f.queue.retry(id), true);
    await f.queue.runBatch({ maxJobs: 5 });
    assert.equal((await f.queue.get(id)).state, "succeeded");
    assert.equal((await f.queue.get(id)).attempt, 3);
    assert.equal((await f.queue.lookupDeduplicationKey("retry")).jobId, id);
    const cancelled = await f.queue.enqueue(f.task, { value: 2 });
    assert.equal(await f.queue.cancel(cancelled), "cancelled");
    await f.queue.runBatch({ maxJobs: 5 });
    assert.equal((await f.queue.get(cancelled)).attempt, 0);
    await f.queue.close();
    assert.equal((await f.database.prepare("SELECT 1 AS alive").first()).alive, 1);
  },
);

test(
  "D1 expired worker attempts recover and stale settlement cannot overwrite the new attempt",
  settings,
  async (t) => {
    const f = await setup(t);
    const id = await f.provider.enqueue({ task: "direct", input: null, maxAttempts: 2 });
    const entered = Promise.withResolvers();
    const release = Promise.withResolvers();
    const first = await f.provider.startWorker(
      async () => {
        entered.resolve();
        await release.promise;
        return { ok: true, result: { owner: "stale" } };
      },
      { maxJobs: 1, stopWhenIdle: true },
    );
    await entered.promise;
    f.time.now += 101;
    const other = await createD1TaskProvider(f.providerOptions);
    t.after(() => other.close());
    const replacement = await other.startWorker(
      async () => ({ ok: true, result: { owner: "new" } }),
      { maxJobs: 1, stopWhenIdle: true },
    );
    await replacement.done;
    release.resolve();
    await first.done;
    assert.equal((await f.queue.get(id)).attempt, 2);
    assert.equal((await f.queue.get(id)).state, "succeeded");
    assert.deepEqual((await f.queue.get(id)).result, { owner: "new" });
  },
);

test(
  "D1 running cancellation stays requested until the actual handler settles",
  settings,
  async (t) => {
    const f = await setup(t);
    const id = await f.provider.enqueue({ task: "cooperative", input: null, maxAttempts: 2 });
    const entered = Promise.withResolvers();
    const release = Promise.withResolvers();
    const worker = await f.provider.startWorker(
      async () => {
        entered.resolve();
        await release.promise;
        return { ok: true, result: null };
      },
      { maxJobs: 1, stopWhenIdle: true },
    );
    try {
      await entered.promise;
      assert.equal(await f.queue.cancel(id), "requested");
      assert.equal((await f.queue.get(id)).state, "running");
      assert.equal((await f.queue.get(id)).cancelRequested, true);
    } finally {
      release.resolve();
      await worker.done;
    }
    assert.equal((await f.queue.get(id)).state, "cancelled");
  },
);

test(
  "D1 backoff without an explicit delay waits after failure and expired-claim recovery",
  settings,
  async (t) => {
    const f = await setup(t);
    const task = defineTask({
      name: "backoffOnly",
      input: f.task.input,
      maxAttempts: 2,
      retry: { backoff: true },
      async handler() {
        throw new Error("fixture failure");
      },
    });
    const queue = createTaskQueue({
      provider: await createD1TaskProvider(f.providerOptions),
      tasks: [task],
    });
    t.after(() => queue.close());
    const id = await queue.enqueue(task, { value: 1 });
    await queue.runBatch({ maxJobs: 10 });
    assert.equal((await queue.get(id)).attempt, 1);
    assert.equal((await queue.get(id)).state, "pending");
    assert.equal(
      (
        await f.database
          .prepare("SELECT run_at FROM lenso_d1_task_job WHERE id = ?")
          .bind(id)
          .first()
      ).run_at,
      f.time.now + 1000,
    );
    const abandoned = await f.provider.enqueue({
      task: "recoveryBackoff",
      input: null,
      maxAttempts: 2,
      retry: { backoff: true },
    });
    const entered = Promise.withResolvers();
    const release = Promise.withResolvers();
    const worker = await f.provider.startWorker(
      async () => {
        entered.resolve();
        await release.promise;
        return { ok: true, result: null };
      },
      { maxJobs: 1, stopWhenIdle: true },
    );
    try {
      await entered.promise;
      f.time.now += 101;
      await queue.runBatch({ maxJobs: 1 });
      assert.equal((await f.queue.get(abandoned)).state, "pending");
      assert.equal(
        (
          await f.database
            .prepare("SELECT run_at FROM lenso_d1_task_job WHERE id = ?")
            .bind(abandoned)
            .first()
        ).run_at,
        f.time.now + 1000,
      );
    } finally {
      release.resolve();
      await worker.done;
    }
  },
);

test(
  "D1 JSON null executes, session-backed resources are rejected, and setup never creates tables",
  settings,
  async (t) => {
    const f = await setup(t);
    const task = defineTask({
      name: "null",
      input: {
        "~standard": {
          version: 1,
          vendor: "fixture",
          validate: (value) =>
            value === null ? { value } : { issues: [{ message: "null only" }] },
        },
      },
      async handler() {
        return null;
      },
      result: () => null,
    });
    const queue = createTaskQueue({
      provider: await createD1TaskProvider(f.providerOptions),
      tasks: [task],
    });
    t.after(() => queue.close());
    const scheduler = createScheduler({ ...f.schedulerOptions, tasks: [task], queue });
    const plan = await scheduler.create({ ...f.input, task: task.name, input: null }, f.actor);
    await scheduler.tick();
    await queue.runBatch({ maxJobs: 5 });
    assert.equal((await scheduler.occurrences(plan.id, f.actor))[0].job.state, "succeeded");
    assert.equal((await f.store.get(f.scope, plan.id)).input, null);
    await assert.rejects(
      createD1ScheduleStore(drizzle(f.database.withSession("first-primary"))),
      /plain|primary|binding/i,
    );
    await assert.rejects(
      createD1TaskProvider({
        ...f.providerOptions,
        database: f.database.withSession("first-primary"),
      }),
    );
    await assert.rejects(
      createD1TaskProvider({ ...f.providerOptions, queueName: "not-provisioned" }),
    );
    assert.equal(
      await f.database
        .prepare("SELECT queue_id FROM lenso_d1_task_queue WHERE queue_name='not-provisioned'")
        .first(),
      null,
    );
  },
);
