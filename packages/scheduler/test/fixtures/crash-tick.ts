import { SQL } from "bun";
import { drizzle } from "drizzle-orm/bun-sql";
import { createScheduler } from "@lenso/scheduler";
import { createPostgresScheduleStore } from "@lenso/scheduler/postgres";
import { createTaskQueue } from "@lenso/tasks";
import { createPostgresTaskProvider } from "@lenso/tasks/postgres";
import { fixtureTask } from "./task";

const client = new SQL(process.env.SCHEDULER_TEST_DATABASE_URL!);
const queue = createTaskQueue({
  provider: await createPostgresTaskProvider({
    connectionString: process.env.SCHEDULER_TEST_DATABASE_URL!,
    queueName: process.env.SCHEDULER_FIXTURE_QUEUE!,
  }),
  tasks: [fixtureTask],
});
const scheduler = createScheduler({
  store: await createPostgresScheduleStore(drizzle({ client })),
  scope: { namespace: process.env.SCHEDULER_FIXTURE_NAMESPACE!, tenantId: "tenant-a" },
  queue: {
    get: queue.get,
    identity: queue.identity,
    lookupDeduplicationKey: queue.lookupDeduplicationKey,
    async enqueue(task, input, options) {
      if (process.env.SCHEDULER_CRASH_AFTER_ENQUEUE === "1")
        await queue.enqueue(task, input, options);
      process.kill(process.pid, "SIGKILL");
      return new Promise<string>(() => {});
    },
  },
  tasks: [fixtureTask],
  clock: () => Number(process.env.SCHEDULER_FIXTURE_NOW),
  dispatchLeaseMs: 100,
  authorize: () => false,
  authorizeExecution: () => true,
});
await scheduler.tick();
throw new Error("Crash fixture unexpectedly completed");
