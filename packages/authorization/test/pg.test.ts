import { SQL } from "bun";
import { expect, test } from "bun:test";
import { drizzle } from "drizzle-orm/bun-sql";
import { readFile } from "node:fs/promises";
import { postgresRoleStore } from "../src/drizzle/pg";

const url = process.env.AUTHORIZATION_TEST_PG_URL;
const authorized = process.env.AUTHORIZATION_TEST_PG_OWNED === "1";

(url && authorized ? test : test.skip)(
  "PostgreSQL task-owned fixture: migration, graph JSON, namespaces, revocation and concurrent CAS",
  async () => {
    const parsed = new URL(url!);
    if (
      !["127.0.0.1", "localhost", "[::1]"].includes(parsed.hostname) ||
      parsed.pathname !== "/authorization_fixture"
    )
      throw new Error("Use only a task-owned local authorization_fixture database");
    const client = new SQL(url!);
    try {
      const migration = await readFile(
        new URL("../migrations/0001-role-graphs-pg.sql", import.meta.url),
        "utf8",
      );
      await client.unsafe(migration);
      const db = drizzle(client);
      const scope = { type: "personal", id: "home" };
      const principal = { realmId: "app", subjectId: "alice", kind: "user" };
      const graph = {
        roles: [
          {
            id: "reader",
            scope,
            permissions: [{ action: "read" as const, resourceType: "note", scope }],
          },
        ],
        bindings: [{ id: "binding", scope, principal, roleId: "reader" }],
      };
      const namespace = crypto.randomUUID();
      const store = postgresRoleStore(db, namespace, ["read"]);
      const other = postgresRoleStore(db, `${namespace}-other`, ["read"]);
      const evaluation = { now: 0, signal: new AbortController().signal };
      await store.initialize({ revision: "r1", graph });
      await other.initialize({ revision: "other", graph: { roles: [], bindings: [] } });
      const initial = await store.read(evaluation);
      expect(initial.graph).toEqual(graph);
      expect(Object.isFrozen(initial.graph.roles)).toBe(true);
      const next = { ...graph, bindings: [] };
      const results = await Promise.all([
        store.compareAndSwap("r1", { revision: "r2", graph: next }, evaluation),
        store.compareAndSwap("r1", { revision: "r3", graph: next }, evaluation),
      ]);
      expect(results.filter(Boolean)).toHaveLength(1);
      expect((await store.read(evaluation)).graph.bindings).toHaveLength(0);
      expect((await other.read(evaluation)).revision).toBe("other");
      await client`UPDATE authorization_role_graphs SET graph = '{"roles":[],"bindings":[{"id":"broken"}]}'::jsonb WHERE namespace=${namespace}`;
      await expect(store.read(evaluation)).rejects.toThrow("Invalid role graph");
    } finally {
      await client.close();
    }
  },
  30_000,
);
