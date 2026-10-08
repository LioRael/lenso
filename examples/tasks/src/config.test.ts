import { expect, test } from "bun:test";
import { startApp } from "@lenso/core";
import { resolveConfig } from "@lenso/core/config";
import { tasksConfig, tasksEnvSource } from "./config";
import { createTasksPlugin } from "./plugin";

test("Tasks environment source is deferred and queue default belongs to the schema", async () => {
  const reads: string[] = [];
  const source = tasksEnvSource((name) => {
    reads.push(name);
    return name === "DATABASE_URL" ? "postgres://localhost/tasks" : undefined;
  });
  expect(reads).toEqual([]);
  const snapshot = await resolveConfig("tasks", { contract: tasksConfig, sources: [source] });
  expect(snapshot.value).toEqual({
    connectionString: "postgres://localhost/tasks",
    queueName: "reports",
  });
  expect(reads).toEqual(["DATABASE_URL", "TASK_QUEUE_NAME"]);
});

test("invalid default Tasks configuration fails before connecting Auth or resources", async () => {
  const originalDatabase = process.env.DATABASE_URL;
  const originalQueue = process.env.TASK_QUEUE_NAME;
  try {
    for (const [database, queue] of [
      [undefined, undefined],
      ["", undefined],
      ["postgres://localhost/tasks", ""],
    ]) {
      if (database === undefined) delete process.env.DATABASE_URL;
      else process.env.DATABASE_URL = database;
      if (queue === undefined) delete process.env.TASK_QUEUE_NAME;
      else process.env.TASK_QUEUE_NAME = queue;
      let authStarts = 0;
      const plugin = createTasksPlugin({
        evidence: () => null,
        async connectAuth() {
          authStarts++;
          throw new Error("must not connect Auth");
        },
      });
      await expect(startApp({ plugins: [plugin] })).rejects.toBeDefined();
      expect(authStarts).toBe(0);
    }
  } finally {
    if (originalDatabase === undefined) delete process.env.DATABASE_URL;
    else process.env.DATABASE_URL = originalDatabase;
    if (originalQueue === undefined) delete process.env.TASK_QUEUE_NAME;
    else process.env.TASK_QUEUE_NAME = originalQueue;
  }
});
