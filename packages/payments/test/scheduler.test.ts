import { expect, test } from "bun:test";
import { pathToFileURL } from "node:url";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";
import { drizzle } from "drizzle-orm/d1";
import { audience, createAuth, defineSource, realm } from "@lenso/auth";
import { createTaskQueue, type TaskQueue } from "@lenso/tasks";
import { createD1TaskProvider, provisionD1TaskQueue } from "@lenso/tasks/d1";
import { createScheduler } from "@lenso/scheduler";
import { createD1ScheduleStore } from "@lenso/scheduler/d1";
import { createPayments, PaymentsError, type PaymentsProvider, type ProviderPayment } from "../src";
import { d1PaymentsStore } from "../src/drizzle/d1";
import { createPaymentsReconciliationTask } from "../src/tasks";
import { createPaymentsRecoverySchedule } from "../src/scheduler";

test("native D1 Scheduler and Tasks recover a lost wake, enforce Auth, and block revoked dispatch", async () => {
  const worker = new Miniflare(
    convertV4MiniflareOptions({
      modules: true,
      script: "export default { fetch() { return new Response('fixture'); } };",
      compatibilityDate: "2026-10-08",
      d1Databases: ["DB"],
    }),
  );
  const auth = createAuth(
    realm(
      "payments-maintenance-fixture",
      defineSource<string>({
        async verify(evidence) {
          return evidence === "fixture-service-proof"
            ? { status: "verified", subjectId: "maintenance", kind: "service" }
            : { status: "rejected" };
        },
      }),
    ),
  );
  let queue: TaskQueue | undefined;
  try {
    const binding = await worker.getD1Database("DB");
    const schedulerEntry = pathToFileURL(Bun.resolveSync("@lenso/scheduler", import.meta.dir));
    const tasksEntry = pathToFileURL(Bun.resolveSync("@lenso/tasks", import.meta.dir));
    for (const file of [
      new URL("../migrations/sqlite.sql", import.meta.url),
      new URL("../migrations/d1/0001_scheduler.sql", schedulerEntry),
      new URL("../migrations/d1/0001_tasks.sql", tasksEntry),
    ]) {
      const sql = await Bun.file(file).text();
      await binding.batch(
        sql
          .split(";")
          .map((statement) => statement.trim())
          .filter(Boolean)
          .map((statement) => binding.prepare(statement)),
      );
    }
    await binding
      .prepare(
        "CREATE TABLE payment_recovery_permission (subject_id TEXT PRIMARY KEY, allowed INTEGER NOT NULL)",
      )
      .run();
    await binding
      .prepare("INSERT INTO payment_recovery_permission VALUES ('maintenance', 1)")
      .run();
    let offset = 0;
    const clock = () => Date.now() + offset;
    let posts = 0;
    let remote: ProviderPayment | null = null;
    const unsupported = async (): Promise<never> => {
      throw new Error("unused fixture operation");
    };
    // Only the external provider is a fixture. All schedule/job/payment storage below is real D1.
    const provider: PaymentsProvider = {
      accountId: "acct_fixture",
      live: false,
      replayWindowMs: 23 * 3_600_000,
      validateAmount(amount, currency) {
        if (!Number.isSafeInteger(amount) || amount < 1 || currency !== "usd")
          throw new PaymentsError("invalid-input");
      },
      validateRefundAmount: () => {},
      async createPayment(record, _key, beforeWrite) {
        await beforeWrite?.();
        posts++;
        remote = { ...record, id: "pi_fixture", status: "succeeded", received: record.amount };
        throw new Error("fixture provider committed; response lost");
      },
      async findPayment() {
        return remote ? structuredClone(remote) : null;
      },
      async getPayment() {
        if (!remote) throw new Error("fixture object missing");
        return structuredClone(remote);
      },
      clientSecret: async () => null,
      createRefund: unsupported,
      getRefund: unsupported,
      findRefund: unsupported,
      verifyWebhook: unsupported,
    };
    const store = d1PaymentsStore(binding);
    const runtime = createPayments({
      store,
      provider,
      clock,
      authorize: async (tenant: string, _action, record) => {
        if (tenant !== record.tenantId) throw new PaymentsError("forbidden");
      },
    });
    const payment = await runtime.payments.create(
      {
        tenantId: "tenant-a",
        orderId: "order-a",
        key: "lost-wake",
        amount: 1000,
        currency: "usd",
      },
      "tenant-a",
    );
    expect(payment.status).toBe("unknown");
    expect(posts).toBe(1);
    const task = createPaymentsReconciliationTask({
      name: "payments.acct-fixture.test.reconcile",
      runtime: () => runtime.reconciliation,
      continuation: false,
      queue: () => {
        if (!queue) throw new Error("fixture queue not ready");
        return queue;
      },
    });
    await provisionD1TaskQueue(binding, "payments-recovery-fixture");
    const taskProvider = await createD1TaskProvider({
      database: binding,
      queueName: "payments-recovery-fixture",
      clock,
      pollIntervalMs: 1,
    });
    try {
      queue = createTaskQueue({ provider: taskProvider, tasks: [task] });
      const access = auth.for(audience("payments-maintenance"));
      const actor = await access.required("fixture-service-proof");
      const scope = { namespace: "payments:acct_fixture:test", tenantId: "operator-scope" };
      const scheduleStore = await createD1ScheduleStore(drizzle(binding));
      const assemble = () =>
        createScheduler({
          store: scheduleStore,
          queue: queue!,
          tasks: [task],
          scope,
          clock,
          authorize: async (principal: typeof actor, _action, targetScope, definition) => {
            await access.enforce(
              principal,
              { targetScope, definition },
              (context) =>
                context.principal.kind === "service" &&
                context.principal.subjectId === "maintenance" &&
                context.resource.targetScope.namespace === scope.namespace &&
                context.resource.targetScope.tenantId === scope.tenantId &&
                (!context.resource.definition || context.resource.definition.task === task.name),
            );
            return true;
          },
          authorizeExecution: async (initiator, targetScope, occurrence) =>
            initiator.realmId === actor.realmId &&
            initiator.subjectId === actor.subjectId &&
            targetScope.namespace === scope.namespace &&
            targetScope.tenantId === scope.tenantId &&
            occurrence.task === task.name &&
            !!(
              await binding
                .prepare("SELECT allowed FROM payment_recovery_permission WHERE subject_id = ?")
                .bind(initiator.subjectId)
                .first<{ allowed: number }>()
            )?.allowed,
        });
      const scheduler = assemble();
      const options = {
        scheduler,
        task,
        rule: { kind: "cron" as const, expression: "* * * * *", timezone: "UTC" },
      };
      await expect(
        createPaymentsRecoverySchedule(options, { ...actor } as typeof actor),
      ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
      await expect(
        createPaymentsRecoverySchedule({ ...options, limit: 201 }, actor),
      ).rejects.toMatchObject({ code: "invalid-input" });
      const plan = await createPaymentsRecoverySchedule(options, actor);
      expect((await scheduleStore.get(scope, plan.id))?.input).toEqual({ limit: 50 });
      expect((await scheduleStore.get(scope, plan.id))?.misfire).toBe("coalesce");
      expect(await scheduler.list(actor)).toHaveLength(1);
      const future = (await store.get(payment.paymentId))!;
      expect(
        await store.compareAndSet(
          {
            ...future,
            revision: future.revision + 1,
            reconcileAt: clock() + 86_400_000,
          },
          future.revision,
        ),
      ).toBe(true);
      await queue.enqueue(task, { limit: 50 });
      await queue.runBatch({ maxJobs: 10 });
      expect((await runtime.payments.get(payment, "tenant-a")).status).toBe("unknown");
      const earlyJobs = await binding
        .prepare("SELECT state FROM lenso_d1_task_job")
        .all<{ state: string }>();
      expect(earlyJobs.results.map((job) => job.state)).toEqual(["succeeded"]);
      const deferred = (await store.get(payment.paymentId))!;
      expect(
        await store.compareAndSet(
          {
            ...deferred,
            revision: deferred.revision + 1,
            reconcileAt: clock(),
          },
          deferred.revision,
        ),
      ).toBe(true);
      // Reassemble from persistent state with no wake or queued continuation left to recover it.
      const restarted = assemble();
      offset =
        Math.max(plan.nextAt!, (await store.get(payment.paymentId))!.reconcileAt) - Date.now() + 1;
      expect((await restarted.tick()).enqueued).toBe(1);
      const occurrences = await restarted.occurrences(plan.id, actor);
      expect(occurrences).toHaveLength(1);
      expect(occurrences[0].acceptance).toBe("confirmed");
      expect(occurrences[0].job?.state).toBe("pending");
      await queue.runBatch({ maxJobs: 10 });
      expect((await runtime.payments.get(payment, "tenant-a")).status).toBe("succeeded");
      expect(await runtime.payments.results(payment, "tenant-a")).toHaveLength(1);
      expect((await restarted.occurrences(plan.id, actor))[0].job?.state).toBe("succeeded");
      expect(posts).toBe(1);
      expect((await restarted.tick()).enqueued).toBe(0);
      const payloads = await binding
        .prepare("SELECT input FROM lenso_d1_task_job")
        .all<{ input: string }>();
      expect(payloads.results.map((row) => JSON.parse(row.input))).toEqual([
        { limit: 50 },
        { limit: 50 },
      ]);
      const nextAt = (await restarted.get(plan.id, actor)).nextAt!;
      await binding
        .prepare(
          "UPDATE payment_recovery_permission SET allowed = 0 WHERE subject_id = 'maintenance'",
        )
        .run();
      offset = nextAt - Date.now() + 1;
      expect(await restarted.tick()).toMatchObject({ enqueued: 0, denied: 1 });
      await queue.runBatch({ maxJobs: 10 });
      expect(posts).toBe(1);
      expect(
        (await scheduleStore.occurrences(scope, plan.id, 10)).find(
          (entry) => entry.state === "blocked",
        )?.error,
      ).toBe("execution-denied");
      expect(await runtime.payments.results(payment, "tenant-a")).toHaveLength(1);
    } finally {
      if (queue) await queue.close();
      else await taskProvider.close();
    }
    // Closing the task owner must not close the borrowed D1 binding.
    expect(await binding.prepare("SELECT 1 AS usable").first<{ usable: number }>()).toEqual({
      usable: 1,
    });
  } finally {
    try {
      await auth.close();
    } finally {
      await worker.dispose();
    }
  }
}, 30_000);
