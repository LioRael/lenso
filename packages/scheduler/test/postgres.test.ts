import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { SQL } from "bun";
import { drizzle, type BunSQLDatabase } from "drizzle-orm/bun-sql";
import { audience, createAuth, defineSource, realm, type Actor } from "@lenso/auth";
import { createTaskQueue, defineTask, type TaskQueue } from "@lenso/tasks";
import { definePlugin, startApp } from "@lenso/core";
import { z } from "zod";
import { createPostgresTaskProvider, migratePostgresTaskQueue } from "@lenso/tasks/postgres";
import {
  createScheduler,
  type Occurrence,
  type Schedule,
  type ScheduleStore,
  type SchedulerOptions,
} from "../src/index";
import { createPostgresScheduleStore } from "../src/postgres";
import { createSchedulerPlugin } from "../src/plugin";
import { fixtureTask } from "./fixtures/task";

const url = process.env.SCHEDULER_TEST_DATABASE_URL;
const integration = url ? describe : describe.skip;
let client: SQL;
let db: BunSQLDatabase;
let store: ScheduleStore;
type FixtureActor = Actor<"scheduler-fixture", string, "scheduler-fixture">;
let actor: FixtureActor;
let bob: FixtureActor;
const auth = createAuth(
  realm(
    "scheduler-fixture",
    defineSource({
      verify: async (subject: string) => ({ status: "verified", subjectId: subject }),
    }),
  ),
);
const queues: TaskQueue[] = [];

async function waitForJob(queue: TaskQueue, id: string, state: string) {
  const until = Date.now() + 15_000;
  while (Date.now() < until) {
    const status = await queue.get(id);
    if (status?.state === state) return status;
    await Bun.sleep(25);
  }
  throw new Error("Timed out waiting for real Tasks worker");
}

async function setup(extra: Partial<SchedulerOptions<FixtureActor>> = {}) {
  const scope = { namespace: `fixture-${crypto.randomUUID()}`, tenantId: "tenant-a" };
  const queueName = `scheduler_${crypto.randomUUID().replaceAll("-", "")}`;
  const providerOptions = {
    connectionString: url!,
    queueName,
    pollIntervalMs: 25,
    heartbeatSeconds: 10,
  };
  await migratePostgresTaskQueue(providerOptions);
  const queue = createTaskQueue({
    provider: await createPostgresTaskProvider(providerOptions),
    tasks: [fixtureTask],
  });
  queues.push(queue);
  let now = Date.parse("2025-01-01T00:00:00Z");
  const options: SchedulerOptions<FixtureActor> = {
    store,
    scope,
    queue,
    tasks: [fixtureTask],
    clock: () => now,
    dispatchLeaseMs: 100,
    authorize: (caller, _action, _scope, schedule) =>
      caller.audience === "scheduler-fixture" &&
      (!schedule ||
        !("initiator" in schedule) ||
        schedule.initiator.subjectId === caller.subjectId),
    authorizeExecution: (initiator, actualScope) =>
      initiator.realmId === actor.realmId &&
      initiator.subjectId === actor.subjectId &&
      actualScope.tenantId === "tenant-a",
    ...extra,
  };
  const scheduler = createScheduler(options);
  const input = {
    task: fixtureTask.name,
    input: { value: 42 },
    rule: { kind: "once" as const, at: now },
    misfire: "coalesce" as const,
    graceMs: 0,
  };
  return {
    scope,
    queueName,
    queue,
    scheduler,
    options,
    input,
    get now() {
      return now;
    },
    setNow(value: number) {
      now = value;
    },
  };
}

function entry(schedule: Schedule): Occurrence {
  return {
    id: crypto.randomUUID(),
    scheduleId: schedule.id,
    revision: schedule.revision,
    scheduledAt: schedule.nextAt!,
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

integration("real PostgreSQL scheduler and existing Tasks", () => {
  beforeAll(async () => {
    client = new SQL(url!);
    db = drizzle({ client });
    const [row] = await client`SELECT to_regclass('public.lenso_schedule') AS installed`;
    if (!row.installed) {
      const migration = await Bun.file(
        new URL("../migrations/0001_scheduler.sql", import.meta.url),
      ).text();
      await client.unsafe(migration).simple();
    }
    const [binding] =
      await client`SELECT to_regclass('public.lenso_schedule_queue_binding') AS installed`;
    if (!binding.installed) {
      await client
        .unsafe(
          await Bun.file(new URL("../migrations/0002_queue_binding.sql", import.meta.url)).text(),
        )
        .simple();
    }
    store = await createPostgresScheduleStore(db);
    actor = await auth.for(audience("scheduler-fixture")).required("alice");
    bob = await auth.for(audience("scheduler-fixture")).required("bob");
  });

  afterAll(async () => {
    await Promise.all(queues.map((queue) => queue.close()));
    await auth.close();
    await client.close();
  });

  test("unmigrated store setup fails without DDL or closing its borrowed client", async () => {
    const borrowed = new SQL({ url: url!, max: 1 });
    const schema = `missing_${crypto.randomUUID().replaceAll("-", "")}`;
    try {
      await borrowed.unsafe(`SET search_path TO ${schema}`);
      await expect(
        createPostgresScheduleStore(drizzle({ client: borrowed })),
      ).rejects.toBeDefined();
      expect((await borrowed`SELECT 1 AS alive`)[0].alive).toBe(1);
      expect(await borrowed`SELECT 1 FROM pg_namespace WHERE nspname = ${schema}`).toHaveLength(0);
    } finally {
      await borrowed.close();
    }
  });

  test("concurrent multi-instance ticks enqueue one occurrence and real worker executes/retries", async () => {
    const f = await setup();
    const created = await f.scheduler.create(
      { ...f.input, input: { value: 42, failUntil: 1 } },
      actor,
    );
    const second = createScheduler(f.options);
    const outcomes = await Promise.all([f.scheduler.tick(), second.tick(), f.scheduler.tick()]);
    expect(outcomes.reduce((sum, value) => sum + value.advanced, 0)).toBe(1);
    expect(outcomes.reduce((sum, value) => sum + value.enqueued, 0)).toBe(1);
    const [linked] = await f.scheduler.occurrences(created.id, actor);
    expect(linked.occurrence.state).toBe("enqueued");
    expect(linked.occurrence.initiator).toEqual({
      realmId: actor.realmId,
      subjectId: actor.subjectId,
    });
    expect(linked.job?.state).toBe("pending");
    const worker = await f.queue.startWorker();
    const status = await waitForJob(f.queue, linked.occurrence.jobId!, "succeeded");
    expect(status.attempt).toBe(2);
    expect(status.result).toEqual({ value: 42, attempt: 2 });
    await worker.stop();
    expect((await f.scheduler.get(created.id, actor)).state).toBe("completed");
    expect((await f.scheduler.occurrences(created.id, actor))[0].job?.state).toBe("succeeded");
    expect(await f.scheduler.tick()).toEqual({ advanced: 0, enqueued: 0, denied: 0, failed: 0 });
  }, 20_000);

  test("schedule completion does not hide final task failure", async () => {
    const f = await setup();
    const created = await f.scheduler.create(
      { ...f.input, input: { value: 1, failUntil: 100 } },
      actor,
    );
    await f.scheduler.tick();
    const [linked] = await f.scheduler.occurrences(created.id, actor);
    const worker = await f.queue.startWorker();
    await waitForJob(f.queue, linked.occurrence.jobId!, "failed");
    await worker.stop();
    expect((await f.scheduler.get(created.id, actor)).state).toBe("completed");
    expect((await f.scheduler.occurrences(created.id, actor))[0].job?.error).toBe("handler-failed");
  }, 20_000);

  test("lease claim is exclusive and stale acknowledgement cannot overwrite a new owner", async () => {
    const f = await setup();
    const created = await f.scheduler.create(f.input, actor);
    const snapshot = (await store.get(f.scope, created.id))!;
    expect(await store.advance(f.scope, snapshot, null, entry(snapshot))).toBe(true);
    const claims = await Promise.all(
      Array.from({ length: 8 }, () => store.claim(f.scope, f.now, 100)),
    );
    expect(claims.filter(Boolean)).toHaveLength(1);
    const first = claims.find(Boolean)!;
    const second = (await store.claim(f.scope, f.now + 101, 100))!;
    expect(second.id).toBe(first.id);
    expect(second.leaseToken).not.toBe(first.leaseToken);
    expect(
      await store.settle(f.scope, first.id, first.leaseToken!, { jobId: crypto.randomUUID() }),
    ).toBe(false);
    expect(
      await store.settle(f.scope, second.id, second.leaseToken!, { jobId: crypto.randomUUID() }),
    ).toBe(true);
  });

  test("outbox insertion failure rolls back schedule advancement", async () => {
    const f = await setup();
    const created = await f.scheduler.create(f.input, actor);
    const snapshot = (await store.get(f.scope, created.id))!;
    await expect(
      store.advance(f.scope, snapshot, null, {
        ...entry(snapshot),
        scheduleId: crypto.randomUUID(),
      }),
    ).rejects.toBeDefined();
    expect(await store.get(f.scope, created.id)).toEqual(snapshot);
    expect(await store.occurrences(f.scope, created.id, 100)).toHaveLength(0);
    await f.scheduler.tick();
    expect(await store.occurrences(f.scope, created.id, 100)).toHaveLength(1);
  });

  test("JSON null remains JSONB null through create/update/trigger/advance and executes on Tasks", async () => {
    const nullTask = defineTask({
      name: "schedulerNull",
      input: z.null(),
      async handler(input) {
        return input;
      },
      result: () => null,
    });
    const f = await setup({ tasks: [nullTask] });
    const nullQueue = createTaskQueue({
      provider: await createPostgresTaskProvider({
        connectionString: url!,
        queueName: f.queueName,
        pollIntervalMs: 25,
      }),
      tasks: [nullTask],
    });
    queues.push(nullQueue);
    const scheduler = createScheduler({ ...f.options, tasks: [nullTask], queue: nullQueue });
    const input = { ...f.input, task: nullTask.name, input: null };
    const created = await scheduler.create(input, actor);
    const updated = await scheduler.update(created.id, created.revision, input, actor);
    await scheduler.trigger(created.id, "null-manual", actor);
    expect((await scheduler.tick()).enqueued).toBe(2);
    expect((await store.get(f.scope, updated.id))!.input).toBeNull();
    const rows = await store.occurrences(f.scope, updated.id, 10);
    expect(rows).toHaveLength(2);
    expect(rows.every((row) => row.input === null)).toBe(true);
    const worker = await nullQueue.startWorker();
    for (const row of rows)
      expect((await waitForJob(nullQueue, row.jobId!, "succeeded")).result).toBeNull();
    await worker.stop();
  }, 20_000);

  test("a slow authorization cannot enqueue after a newer claimant denies execution", async () => {
    const entered = Promise.withResolvers<void>();
    const release = Promise.withResolvers<void>();
    const f = await setup({
      authorizeExecution: async () => {
        entered.resolve();
        await release.promise;
        return true;
      },
    });
    const created = await f.scheduler.create(f.input, actor);
    const first = f.scheduler.tick();
    await entered.promise;
    f.setNow(f.now + 101);
    const other = createScheduler({ ...f.options, authorizeExecution: () => false });
    expect((await other.tick()).denied).toBe(1);
    release.resolve();
    expect((await first).enqueued).toBe(0);
    const [status] = await f.scheduler.occurrences(created.id, actor);
    expect(status.occurrence.state).toBe("blocked");
    expect(status.acceptance).toBe("unknown");
    const mappings =
      await client`SELECT job_id FROM pgboss.lenso_task_relation WHERE queue_name = ${f.queueName}`;
    expect(mappings).toHaveLength(0);
  });

  test("the real Auth enforce boundary rejects forged actors, and plugin borrows exact dependencies", async () => {
    const access = auth.for(audience("scheduler-fixture"));
    const f = await setup({
      authorize: async (caller, _action, _scope, resource) => {
        await access.enforce(
          caller,
          resource,
          ({ principal, resource: value }) =>
            !value || !("initiator" in value) || value.initiator.subjectId === principal.subjectId,
        );
        return true;
      },
    });
    const database = definePlugin({ id: "borrowed-db", setup: () => db });
    const queue = definePlugin({ id: "borrowed-queue", setup: () => f.queue });
    const plugin = createSchedulerPlugin({
      id: "scheduler-fixture",
      database,
      queue,
      connect: (resource) => createPostgresScheduleStore(resource),
      options: { ...f.options },
    });
    const app = await startApp({ plugins: [database, queue, plugin] });
    try {
      const scheduler = app.get(plugin);
      const created = await scheduler.create(f.input, actor);
      await expect(scheduler.get(created.id, { ...actor } as FixtureActor)).rejects.toMatchObject({
        code: "UNAUTHORIZED",
      });
      expect((await scheduler.get(created.id, actor)).id).toBe(created.id);
      expect(plugin.requires).toEqual([database, queue]);
    } finally {
      await app.stop();
    }
    expect((await client`SELECT 1 AS alive`)[0].alive).toBe(1);
    expect(await f.queue.get(crypto.randomUUID())).toBeNull();
  });

  test("pause/resume uses misfire policy; skip drops late occurrences, coalesce emits one", async () => {
    const f = await setup({ maxSchedulesPerTick: 1, maxDispatchesPerTick: 1 });
    const cron = { kind: "cron" as const, expression: "* * * * * *", timezone: "UTC" };
    const created = await f.scheduler.create({ ...f.input, rule: cron }, actor);
    const paused = await f.scheduler.pause(created.id, created.revision, actor);
    f.setNow(f.now + 365 * 86_400_000);
    expect((await f.scheduler.tick()).advanced).toBe(0);
    await f.scheduler.resume(created.id, paused.revision, actor);
    expect((await f.scheduler.tick()).enqueued).toBe(1);
    expect((await f.scheduler.get(created.id, actor)).nextAt).toBe(f.now + 1000);
    expect((await f.scheduler.occurrences(created.id, actor))[0].occurrence.scheduledAt).toBe(
      f.input.rule.at + 1000,
    );
    const skipped = await f.scheduler.create({ ...f.input, misfire: "skip" }, actor);
    expect((await f.scheduler.tick()).advanced).toBe(1);
    expect(await f.scheduler.occurrences(skipped.id, actor)).toHaveLength(0);
  });

  test("update/cancel races use revision CAS; committed occurrences retain old payload and survive cancellation", async () => {
    const f = await setup();
    const created = await f.scheduler.create(f.input, actor);
    const snapshot = (await store.get(f.scope, created.id))!;
    const updated = await f.scheduler.update(
      created.id,
      created.revision,
      { ...f.input, input: { value: 99 } },
      actor,
    );
    expect(await store.advance(f.scope, snapshot, null, entry(snapshot))).toBe(false);
    await expect(f.scheduler.cancel(created.id, created.revision, actor)).rejects.toMatchObject({
      code: "conflict",
    });
    const newer = (await store.get(f.scope, created.id))!;
    const reserved = entry(newer);
    expect(await store.advance(f.scope, newer, null, reserved)).toBe(true);
    await expect(
      f.scheduler.update(created.id, updated.revision, f.input, actor),
    ).rejects.toMatchObject({ code: "conflict" });
    const current = await f.scheduler.get(created.id, actor);
    await f.scheduler.cancel(created.id, current.revision, actor);
    expect((await f.scheduler.tick()).enqueued).toBe(1);
    expect((await store.occurrences(f.scope, created.id, 10))[0].input).toEqual({ value: 99 });
    await expect(f.scheduler.trigger(created.id, "after-cancel", actor)).rejects.toMatchObject({
      code: "cancelled",
    });
    const cancelled = await f.scheduler.create(f.input, actor);
    const beforeCancel = (await store.get(f.scope, cancelled.id))!;
    await f.scheduler.cancel(cancelled.id, cancelled.revision, actor);
    expect(await store.advance(f.scope, beforeCancel, null, entry(beforeCancel))).toBe(false);
    expect(await store.trigger(f.scope, beforeCancel, entry(beforeCancel))).toBeNull();
  });

  test("manual trigger keys are idempotent, carry the triggering actor, and execution permission is independent", async () => {
    const f = await setup({ authorize: (caller) => caller.audience === "scheduler-fixture" });
    const created = await f.scheduler.create(
      { ...f.input, rule: { kind: "once", at: f.now + 100_000 } },
      actor,
    );
    const triggers = await Promise.all([
      f.scheduler.trigger(created.id, "manual-1", bob),
      f.scheduler.trigger(created.id, "manual-1", bob),
    ]);
    expect(triggers[0]).toEqual(triggers[1]);
    expect((await f.scheduler.tick()).denied).toBe(1);
    const [status] = await f.scheduler.occurrences(created.id, actor);
    expect(status.occurrence.initiator.subjectId).toBe("bob");
    expect(status.occurrence.state).toBe("blocked");
    expect(status.occurrence.error).toBe("execution-denied");
    expect(status.job).toBeNull();
  });

  test("overlapping update and cancellation cannot both commit", async () => {
    const f = await setup();
    const created = await f.scheduler.create(f.input, actor);
    const results = await Promise.allSettled([
      f.scheduler.update(created.id, created.revision, { ...f.input, input: { value: 99 } }, actor),
      f.scheduler.cancel(created.id, created.revision, actor),
    ]);
    expect(results.filter((result) => result.status === "fulfilled")).toHaveLength(1);
    const current = await f.scheduler.get(created.id, actor);
    expect(current.revision).toBe(2);
    await f.scheduler.tick();
    const entries = await store.occurrences(f.scope, current.id, 10);
    if (current.state === "cancelled") expect(entries).toHaveLength(0);
    else {
      expect(entries).toHaveLength(1);
      expect(entries[0].input).toEqual({ value: 99 });
    }
  });

  test("tenant/namespace isolation and safe read output; invalid payload does not create a plan", async () => {
    const f = await setup();
    const created = await f.scheduler.create(f.input, actor);
    await expect(f.scheduler.get(created.id, bob)).rejects.toMatchObject({ code: "forbidden" });
    for (const scope of [
      { ...f.scope, tenantId: "tenant-b" },
      { ...f.scope, namespace: "other-instance" },
    ]) {
      expect(await store.get(scope, created.id)).toBeNull();
      expect(await store.due(scope, f.now, 100)).toHaveLength(0);
      expect(await store.claim(scope, f.now, 100)).toBeNull();
    }
    const malformed: number[] = [];
    malformed.length = 1;
    Object.assign(malformed, { fake: 1 });
    for (const input of [{ value: NaN }, { value: new Date() }, { value: 1, nested: malformed }])
      await expect(
        f.scheduler.create({ ...f.input, input: input as never }, actor),
      ).rejects.toMatchObject({ code: "invalid-input" });
    expect(await f.scheduler.list(actor)).toHaveLength(1);
    expect(await f.scheduler.get(created.id, actor)).not.toHaveProperty("input");
    expect(() =>
      createScheduler({ ...f.options, store: { ...store, kind: "memory" } as never }),
    ).toThrow("unsupported-storage");
  });

  test("durable scope rejects a different queue identity or backend", async () => {
    const f = await setup();
    await f.scheduler.create(f.input, actor);
    const other = await setup();
    const changed = createScheduler({ ...f.options, queue: other.queue });
    await expect(changed.tick()).rejects.toMatchObject({ code: "queue-mismatch" });
    const crossBackend = createScheduler({ ...f.options, store: { ...store, kind: "d1" } });
    await expect(crossBackend.tick()).rejects.toMatchObject({ code: "unsupported-storage" });
    expect(
      (
        await client`SELECT job_id FROM pgboss.lenso_task_relation WHERE queue_name = ${other.queueName}`
      ).length,
    ).toBe(0);
  });

  for (const scenario of ["before", "after", "expired", "revoked"] as const) {
    test(`SIGKILL recovery: ${scenario}`, async () => {
      const f = await setup();
      const created = await f.scheduler.create(f.input, actor);
      const afterEnqueue = scenario !== "before";
      const child = Bun.spawn(
        [process.execPath, new URL("./fixtures/crash-tick.ts", import.meta.url).pathname],
        {
          env: {
            ...process.env,
            SCHEDULER_TEST_DATABASE_URL: url!,
            SCHEDULER_FIXTURE_QUEUE: f.queueName,
            SCHEDULER_FIXTURE_NAMESPACE: f.scope.namespace,
            SCHEDULER_FIXTURE_NOW: String(f.now),
            SCHEDULER_CRASH_AFTER_ENQUEUE: afterEnqueue ? "1" : "0",
          },
          stdout: "pipe",
          stderr: "pipe",
        },
      );
      const exit = await child.exited;
      const stderr = await new Response(child.stderr).text();
      expect(stderr).toBe("");
      expect(exit).not.toBe(0);
      const [pending] = await store.occurrences(f.scope, created.id, 100);
      expect(pending.state).toBe("pending");
      expect(pending.leaseToken).not.toBeNull();
      const before =
        await client`SELECT job_id FROM pgboss.lenso_task_relation WHERE queue_name = ${f.queueName}`;
      expect(before).toHaveLength(afterEnqueue ? 1 : 0);
      f.setNow(f.now + 101);
      if (scenario === "expired") {
        // Reproduce retention's missing-job/tombstone combination using only our test queue.
        await client`DELETE FROM pgboss.job WHERE name = ${f.queueName} AND id = ${before[0].job_id}`;
        expect((await f.scheduler.tick()).enqueued).toBe(1);
        const [status] = await f.scheduler.occurrences(created.id, actor);
        expect(status.occurrence.state).toBe("enqueued");
        expect(status.occurrence.jobId).toBe(before[0].job_id);
        expect(status.acceptance).toBe("confirmed");
        expect(status.job).toBeNull();
        expect((await f.scheduler.tick()).failed).toBe(0);
        return;
      }
      if (scenario === "revoked") {
        const revoked = createScheduler({ ...f.options, authorizeExecution: () => false });
        expect((await revoked.tick()).enqueued).toBe(1);
        const [status] = await f.scheduler.occurrences(created.id, actor);
        expect(status.acceptance).toBe("confirmed");
        expect(status.occurrence.jobId).toBe(before[0].job_id);
        expect(status.job?.state).toBe("pending");
        const worker = await f.queue.startWorker();
        await waitForJob(f.queue, before[0].job_id, "succeeded");
        await worker.stop();
        return;
      }
      expect((await f.scheduler.tick()).enqueued).toBe(1);
      const [linked] = await f.scheduler.occurrences(created.id, actor);
      if (afterEnqueue) expect(linked.occurrence.jobId).toBe(before[0].job_id);
      const persisted =
        await client`SELECT job_id FROM pgboss.lenso_task_relation WHERE queue_name = ${f.queueName}`;
      expect(persisted).toHaveLength(1);
      const worker = await f.queue.startWorker();
      await waitForJob(f.queue, linked.occurrence.jobId!, "succeeded");
      await worker.stop();
    }, 20_000);
  }
});
