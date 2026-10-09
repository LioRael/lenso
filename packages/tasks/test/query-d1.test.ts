import { Database, type SQLQueryBindings } from "bun:sqlite";
import { expect, test } from "bun:test";
import { readFile } from "node:fs/promises";
import { z } from "zod";
import { startApp } from "@lenso/core";
import { createTaskPlugin, defineTask } from "../src/index";
import {
  createD1TaskProvider,
  provisionD1TaskQueue,
  type D1Database,
  type D1PreparedStatement,
} from "../src/d1";

test("D1 SQL list through an SDK plugin is queue/task scoped, redacted and keyset paginated", async () => {
  const sqlite = new Database(":memory:");
  sqlite.exec(await readFile(new URL("../migrations/d1/0001_tasks.sql", import.meta.url), "utf8"));
  const database: D1Database = {
    prepare(query) {
      let values: SQLQueryBindings[] = [];
      const statement: D1PreparedStatement = {
        bind(...bindings) {
          values = bindings as SQLQueryBindings[];
          return statement;
        },
        async all<T>() {
          return { success: true, results: sqlite.query(query).all(...values) as T[] };
        },
      };
      return statement;
    },
    async batch<T>(statements: D1PreparedStatement[]) {
      sqlite.exec("BEGIN");
      try {
        const result = [];
        for (const statement of statements) result.push(await statement.all<T>());
        sqlite.exec("COMMIT");
        return result;
      } catch (error) {
        sqlite.exec("ROLLBACK");
        throw error;
      }
    },
    withSession() {
      return database;
    },
  };
  const task = defineTask({
    name: "visible",
    input: z.object({ secret: z.string() }),
    async handler(input) {
      return input;
    },
    result: (input) => input,
  });
  const hidden = defineTask({ ...task, name: "hidden" });
  await provisionD1TaskQueue(database, "owned");
  await provisionD1TaskQueue(database, "other");
  const plugin = createTaskPlugin({
    id: "tasks-query",
    tasks: [task, hidden],
    connect: () => createD1TaskProvider({ database, queueName: "owned" }),
  });
  const other = await createD1TaskProvider({ database, queueName: "other" });
  const app = await startApp({ plugins: [plugin] });
  try {
    const queue = app.get(plugin);
    const ids = [];
    for (let index = 0; index < 4; index++) {
      ids.push(await queue.enqueue(task, { secret: "INPUT-SECRET" }));
    }
    await queue.enqueue(hidden, { secret: "HIDDEN-SECRET" });
    await other.enqueue({ task: task.name, input: "OTHER-SECRET", maxAttempts: 1 });
    await queue.runBatch();
    ids.sort();
    const first = await queue.list({ tasks: [task.name], limit: 2 });
    expect(first.items.map((item) => item.jobId)).toEqual(ids.slice(0, 2));
    expect(first.nextCursor).toBe(ids[1]!);
    const second = await queue.list({ tasks: [task.name], limit: 2, after: first.nextCursor! });
    expect(second.items.map((item) => item.jobId)).toEqual(ids.slice(2));
    expect(second.nextCursor).toBeNull();
    expect(first.items.every((item) => item.state === "succeeded")).toBe(true);
    for (const item of [...first.items, ...second.items]) {
      expect(Object.keys(item).sort()).toEqual([
        "attempt",
        "cancelRequested",
        "jobId",
        "maxAttempts",
        "state",
        "task",
      ]);
    }
    expect(JSON.stringify(first)).not.toContain("SECRET");
    expect(await queue.list({ tasks: [] })).toEqual({ items: [], nextCursor: null });
  } finally {
    await app.stop();
    await other.close();
    sqlite.close();
  }
});
