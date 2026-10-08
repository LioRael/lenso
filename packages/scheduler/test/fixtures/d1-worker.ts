import type { D1Database, ScheduledController } from "@cloudflare/workers-types";
import { drizzle } from "drizzle-orm/d1";
import { createTaskQueue } from "@lenso/tasks";
import { createD1TaskProvider } from "@lenso/tasks/d1";
import { createScheduler } from "@lenso/scheduler";
import { createD1ScheduleStore } from "@lenso/scheduler/d1";
import { createD1FixtureTask } from "./d1-task";

export default {
  async scheduled(controller: ScheduledController, env: { DB: D1Database }) {
    const task = createD1FixtureTask(env.DB);
    const queue = createTaskQueue({
      provider: await createD1TaskProvider({
        database: env.DB,
        queueName: "workerd-fixture",
        clock: () => controller.scheduledTime,
      }),
      tasks: [task],
    });
    try {
      const scheduler = createScheduler({
        store: await createD1ScheduleStore(drizzle(env.DB)),
        scope: { namespace: "workerd-fixture", tenantId: "tenant-a" },
        queue,
        tasks: [task],
        clock: () => controller.scheduledTime,
        authorize: () => false,
        authorizeExecution: async (initiator, scope) => {
          if (
            initiator.realmId !== "d1-fixture" ||
            initiator.subjectId !== "alice" ||
            scope.tenantId !== "tenant-a"
          )
            return false;
          return !!(
            await env.DB.prepare("SELECT allowed FROM fixture_permission WHERE subject_id = ?")
              .bind(initiator.subjectId)
              .first<{ allowed: number }>()
          )?.allowed;
        },
      });
      await scheduler.tick();
      await queue.runBatch({ maxJobs: 10, concurrency: 2 });
    } finally {
      await queue.close();
    }
  },
  async fetch(_request: Request, env: { DB: D1Database }) {
    const task = createD1FixtureTask(env.DB);
    return Response.json(await task.input["~standard"].validate({ value: 42 }));
  },
};
