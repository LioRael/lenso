import { z } from "zod";
import { createTaskQueue, defineTask } from "../../src/index";
import { createPostgresTaskProvider } from "../../src/postgres";

const connectionString = process.env.TASK_TEST_DATABASE_URL!;
const queueName = process.env.TASK_TEST_QUEUE!;
const crashTask = defineTask({
  name: "crash",
  input: z.object({ key: z.string() }),
  maxAttempts: 3,
  retry: { delaySeconds: 1, backoff: false },
  async handler(input, context) {
    console.log(JSON.stringify({ entered: context.jobId, attempt: context.attempt }));
    if (context.attempt === 1) await new Promise(() => {});
    return input.key;
  },
  result: (key) => ({ key }),
});
const provider = await createPostgresTaskProvider({
  connectionString,
  queueName,
  pollIntervalMs: 50,
  heartbeatSeconds: 10,
  expireInSeconds: 120,
  superviseIntervalSeconds: 1,
});
const queue = createTaskQueue({ provider, tasks: [crashTask] });
if (process.argv[2] === "enqueue") {
  console.log(await queue.enqueue(crashTask, { key: "stable-business-key" }));
  await queue.close();
} else {
  await queue.startWorker();
  let stopping = false;
  const stop = () => {
    if (stopping) return;
    stopping = true;
    void queue.close().catch(() => {
      process.exitCode = 1;
    });
  };
  process.on("SIGTERM", stop);
  process.on("SIGINT", stop);
}
