import { expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { drizzle } from "drizzle-orm/bun-sqlite";
import { createPostgresScheduleStore } from "../src/postgres";

test("the PostgreSQL factory rejects SQLite and leaves the borrowed resource usable", async () => {
  const client = new Database(":memory:");
  try {
    await expect(createPostgresScheduleStore(drizzle(client) as never)).rejects.toThrow(
      "PostgreSQL",
    );
    expect(client.query("SELECT 1 AS alive").get()).toEqual({ alive: 1 });
    expect(client.query("SELECT name FROM sqlite_master WHERE type = 'table'").all()).toEqual([]);
  } finally {
    client.close();
  }
});
