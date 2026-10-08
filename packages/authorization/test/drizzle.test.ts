import { Database } from "bun:sqlite";
import { expect, test } from "bun:test";
import { drizzle } from "drizzle-orm/bun-sqlite";
import { readFile } from "node:fs/promises";
import type { RoleSnapshot } from "../src/types";
import { sqliteRoleStore } from "../src/drizzle/sqlite";
import { authorizationRoleGraphs } from "../src/drizzle/schema-sqlite";

const migration = await readFile(
  new URL("../migrations/0001-role-graphs-sqlite.sql", import.meta.url),
  "utf8",
);

const snapshot = (revision: string): RoleSnapshot<"read"> => ({
  revision,
  graph: { roles: [], bindings: [] },
});
const evaluation = { now: 0, signal: new AbortController().signal };

test("SQLite role store initializes explicitly, isolates namespaces, and enforces CAS", async () => {
  const client = new Database(":memory:");
  try {
    client.exec(migration);
    const db = drizzle(client, { schema: { authorizationRoleGraphs } });
    const alpha = sqliteRoleStore(db, "alpha", ["read"]);
    const beta = sqliteRoleStore(db, "beta", ["read"]);
    await alpha.initialize(snapshot("r1"));
    await beta.initialize(snapshot("other"));

    expect((await alpha.read(evaluation)).revision).toBe("r1");
    expect((await beta.read(evaluation)).revision).toBe("other");
    expect(await alpha.compareAndSwap("stale", snapshot("r2"), evaluation)).toBe(false);
    expect(await alpha.compareAndSwap("r1", snapshot("r2"), evaluation)).toBe(true);
    expect(await alpha.compareAndSwap("r1", snapshot("r3"), evaluation)).toBe(false);
    expect((await alpha.read(evaluation)).revision).toBe("r2");
    expect((await beta.read(evaluation)).revision).toBe("other");
  } finally {
    client.close();
  }
});

test("SQLite role store rejects malformed persisted documents", async () => {
  const client = new Database(":memory:");
  try {
    client.exec(migration);
    const db = drizzle(client, { schema: { authorizationRoleGraphs } });
    const store = sqliteRoleStore(db, "broken", ["read"]);
    await store.initialize(snapshot("r1"));
    client
      .query("UPDATE authorization_role_graphs SET graph = ? WHERE namespace = ?")
      .run("{broken", "broken");
    await expect(store.read(evaluation)).rejects.toThrow("Invalid role graph");
  } finally {
    client.close();
  }
});

test("SQLite role store rejects malformed roles, missing namespace, cancelled writes and revision reuse", async () => {
  const client = new Database(":memory:");
  try {
    client.exec(migration);
    const store = sqliteRoleStore(drizzle(client), "app", ["read"]);
    await expect(store.read(evaluation)).rejects.toThrow("Invalid role graph");
    await store.initialize(snapshot("r1"));
    await store.initialize(snapshot("ignored"));
    expect((await store.read(evaluation)).revision).toBe("r1");
    const before = await store.read(evaluation);
    expect(Object.isFrozen(before.graph.roles)).toBe(true);
    await expect(store.compareAndSwap("r1", before, evaluation)).rejects.toThrow(
      "Invalid role graph",
    );
    await expect(
      store.compareAndSwap("r1", snapshot("r2"), {
        ...evaluation,
        signal: AbortSignal.abort(),
      }),
    ).rejects.toThrow("Invalid role graph");
    expect((await store.read(evaluation)).revision).toBe("r1");
    const results = await Promise.all([
      store.compareAndSwap("r1", snapshot("r2"), evaluation),
      store.compareAndSwap("r1", snapshot("r3"), evaluation),
    ]);
    expect(results.filter(Boolean)).toHaveLength(1);
    client
      .query("UPDATE authorization_role_graphs SET graph = ? WHERE namespace = ?")
      .run('{"roles":[],"bindings":[{"id":"bad"}]}', "app");
    await expect(store.read(evaluation)).rejects.toThrow("Invalid role graph");
  } finally {
    client.close();
  }
});
