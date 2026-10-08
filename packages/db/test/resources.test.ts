import { expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { sql } from "drizzle-orm";
import { definePlugin, startApp } from "@lenso/core";
import { createBunSqlitePlugin } from "../src/bun-sqlite";

test("distinct database references stay isolated; owned clients close at stop", async () => {
  const first = createBunSqlitePlugin({ id: "first-db", filename: ":memory:", schema: {} });
  const second = createBunSqlitePlugin({ id: "second-db", filename: ":memory:", schema: {} });
  const consumer = definePlugin({
    id: "consumer",
    requires: [first, second],
    setup(context) {
      return [context.get(first), context.get(second)];
    },
  });
  const app = await startApp({ plugins: [consumer, first, second] });
  const [a, b] = app.get(consumer);
  try {
    expect(() => a.run(sql`select * from notes`)).toThrow(); // setup never migrates
    a.run(sql`create table marker (value text not null)`);
    expect(() => b.run(sql`select * from marker`)).toThrow();
  } finally {
    await app.stop();
  }
  expect(() => a.run(sql`select 1`)).toThrow();
  expect(() => b.run(sql`select 1`)).toThrow();
  await app.stop();
});

test("borrowed clients survive stop; rollback releases already acquired owned clients", async () => {
  const client = new Database(":memory:");
  try {
    const borrowed = createBunSqlitePlugin({ id: "borrowed", client, schema: {} });
    const app = await startApp({ plugins: [borrowed] });
    await app.stop();
    expect(client.query("select 1 as value").get()).toEqual({ value: 1 });
  } finally {
    client.close();
  }
  const owned = createBunSqlitePlugin({ id: "owned", filename: ":memory:", schema: {} });
  let acquired: ReturnType<typeof owned.setup> | undefined;
  const failing = definePlugin({
    id: "failing",
    requires: [owned],
    setup(context): never {
      acquired = context.get(owned);
      throw new Error("later setup failed");
    },
  });
  await expect(startApp({ plugins: [owned, failing] })).rejects.toThrow("later setup failed");
  expect(acquired).toBeDefined();
  const database = await acquired;
  expect(() => database?.run(sql`select 1`)).toThrow();
});
