import { expect, test } from "bun:test";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";
import { drizzle } from "drizzle-orm/d1";
import { audience, createAuth, defineSource, realm } from "@lenso/auth";
import { definePlugin, startApp } from "@lenso/core";
import { createManageAdapter } from "@lenso/manage";
import { createTaskPlugin, defineTask } from "../../tasks/src/index";
import { createD1TaskProvider, provisionD1TaskQueue } from "../../tasks/src/d1";
import { z } from "zod";
import { createScheduler } from "../src/index";
import { createD1ScheduleStore } from "../src/d1";
import { createSchedulerManage } from "../src/manage";

test("native D1 Scheduler Manage rechecks actors and revisions and executes only registered task input", async () => {
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
      "scheduler-manage-test",
      defineSource<string>({
        async verify(evidence) {
          return evidence === "alice" || evidence === "bob"
            ? { status: "verified", subjectId: evidence, kind: "user" }
            : { status: "rejected" };
        },
      }),
    ),
  );
  const access = auth.for(audience("scheduler-manage"));
  try {
    const database = await worker.getD1Database("DB");
    for (const path of [
      new URL("../../tasks/migrations/d1/0001_tasks.sql", import.meta.url),
      new URL("../migrations/d1/0001_scheduler.sql", import.meta.url),
    ]) {
      const sql = await Bun.file(path).text();
      await database.batch(
        sql
          .split(";")
          .map((statement) => statement.trim())
          .filter(Boolean)
          .map((statement) => database.prepare(statement)),
      );
    }
    await provisionD1TaskQueue(database, "scheduler-manage");
    let handled = 0;
    const task = defineTask({
      name: "owned-task",
      input: z.object({ secret: z.string(), mode: z.string().default("private-example") }).strict(),
      async handler(input) {
        handled++;
        return input;
      },
      result: (input) => input,
    });
    const queue = createTaskPlugin({
      id: "queue",
      tasks: [task],
      connect: () =>
        createD1TaskProvider({ database, queueName: "scheduler-manage", pollIntervalMs: 1 }),
    });
    const alice = await access.required("alice");
    const bob = await access.required("bob");
    let revoked = false;
    const scheduler = definePlugin({
      id: "scheduler",
      requires: [queue],
      async setup(context) {
        return createScheduler({
          store: await createD1ScheduleStore(drizzle(database)),
          queue: context.get(queue),
          tasks: [task],
          scope: { namespace: "manage", tenantId: "tenant-a" },
          authorize: async (actor: typeof alice, _action, scope, schedule) => {
            try {
              await access.enforce(
                actor,
                { scope, schedule },
                ({ principal, resource }) =>
                  !revoked &&
                  principal.subjectId === "alice" &&
                  resource.scope.tenantId === "tenant-a" &&
                  (!resource.schedule || resource.schedule.task === task.name) &&
                  (!resource.schedule ||
                    !("initiator" in resource.schedule) ||
                    resource.schedule.initiator.subjectId === principal.subjectId),
              );
              return true;
            } catch {
              return false;
            }
          },
          authorizeExecution: async (initiator) => !revoked && initiator.subjectId === "alice",
        });
      },
    });
    const companion = createSchedulerManage({
      id: "scheduler.manage",
      scheduler,
      tasks: [task],
      authorizeCatalog: async (actor, signal) => {
        signal.throwIfAborted();
        if (revoked || actor.subjectId !== "alice") throw new Error("catalog forbidden");
      },
    });
    const unauthenticatedCompanion = createSchedulerManage({
      id: "scheduler.manage.unprotected",
      scheduler,
      tasks: [task],
    });
    const running = await startApp({
      plugins: [queue, scheduler, companion.plugin, unauthenticatedCompanion.plugin],
    });
    let actor = alice;
    let signal = new AbortController().signal;
    const adapter = createManageAdapter({
      running,
      plugins: [companion.plugin, unauthenticatedCompanion.plugin],
      operations: [...companion.operations, ...unauthenticatedCompanion.operations],
      canList: () => true,
      binding: () => ({ context: { actor, signal } }),
    });
    const invoke = (method: string, input: unknown) =>
      adapter.invoke(companion.plugin.id, method, input);
    try {
      const catalog = (await invoke("catalog", {})) as {
        tasks: {
          name: string;
          schemaAvailability: string;
          inputSchema: Record<string, unknown> | null;
        }[];
      };
      expect(catalog.tasks.map(({ name }) => name)).toEqual(["owned-task"]);
      expect(catalog.tasks[0]?.schemaAvailability).toBe("available");
      expect(JSON.stringify(catalog)).not.toContain("private-example");
      expect(JSON.stringify(catalog)).not.toContain("examples");
      const closedCatalog = (await adapter.invoke(
        unauthenticatedCompanion.plugin.id,
        "catalog",
        {},
      )) as { tasks: unknown[] };
      expect(closedCatalog.tasks).toEqual([]);
      const definition = {
        task: task.name,
        input: { secret: "PRIVATE-PAYLOAD" },
        rule: { kind: "once" as const, at: Date.now() + 60_000 },
        misfire: "skip" as const,
        graceMs: 0,
      };
      for (const invalid of [
        { ...definition, actor: { subjectId: "alice" } },
        { ...definition, tenantId: "tenant-b" },
        { ...definition, task: "arbitrary-code" },
        { ...definition, input: { secret: 42 } },
      ]) {
        await expect(invoke("create", invalid)).rejects.toMatchObject({
          diagnostic: { code: "invalid-input" },
        });
      }
      const created = (await invoke("create", definition)) as { id: string; revision: number };
      expect(JSON.stringify(created)).not.toContain("PRIVATE");
      expect(handled).toBe(0);
      const paused = (await invoke("pause", { id: created.id, revision: created.revision })) as {
        revision: number;
      };
      expect(paused.revision).toBe(2);
      await expect(invoke("resume", { id: created.id, revision: 1 })).rejects.toMatchObject({
        diagnostic: { code: "conflict" },
      });
      await invoke("resume", { id: created.id, revision: 2 });
      await invoke("trigger", { id: created.id, key: "manual-once" });
      await invoke("trigger", { id: created.id, key: "manual-once" });
      expect(handled).toBe(0);
      await running.get(scheduler).tick();
      await running.get(queue).runBatch();
      expect(handled).toBe(1);
      const occurrences = (await invoke("occurrences", { id: created.id })) as unknown[];
      expect(occurrences).toHaveLength(1);
      expect(JSON.stringify(occurrences)).not.toContain("PRIVATE");
      expect(JSON.stringify(occurrences)).not.toContain('"result"');
      actor = bob;
      await expect(invoke("create", definition)).rejects.toMatchObject({
        diagnostic: { code: "forbidden" },
      });
      await expect(invoke("list", {})).rejects.toMatchObject({ diagnostic: { code: "forbidden" } });
      for (const method of ["get", "occurrences", "trigger", "pause", "resume", "cancel"]) {
        await expect(
          invoke(method, {
            id: created.id,
            ...(method === "trigger"
              ? { key: "denied" }
              : ["pause", "resume", "cancel"].includes(method)
                ? { revision: 3 }
                : {}),
          }),
        ).rejects.toMatchObject({ diagnostic: { code: "forbidden" } });
      }
      actor = alice;
      revoked = true;
      await expect(invoke("list", {})).rejects.toMatchObject({ diagnostic: { code: "forbidden" } });
      await expect(invoke("catalog", {})).rejects.toBeDefined();
      revoked = false;
      const controller = new AbortController();
      controller.abort();
      signal = controller.signal;
      await expect(invoke("trigger", { id: created.id, key: "aborted" })).rejects.toBeDefined();
      signal = new AbortController().signal;
      await invoke("cancel", { id: created.id, revision: 3 });
      expect(handled).toBe(1);
    } finally {
      await running.stop();
    }
  } finally {
    await auth.close();
    await worker.dispose();
  }
}, 20_000);
