import { expect, test } from "bun:test";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";
import { drizzle } from "drizzle-orm/d1";
import { readFile } from "node:fs/promises";
import { d1RoleStore } from "../src/drizzle/d1";

test("D1 local Miniflare: real driver migration, immutable read, conditional UPDATE RETURNING and malformed graph", async () => {
  const fixture = new Miniflare(
    convertV4MiniflareOptions({
      modules: true,
      script: "export default { fetch() { return new Response('local fixture'); } }",
      compatibilityDate: "2026-10-01",
      d1Databases: { DB: "authorization-local-fixture" },
    }),
  );
  try {
    const binding = await fixture.getD1Database("DB");
    const migration = await readFile(
      new URL("../migrations/0001-role-graphs-sqlite.sql", import.meta.url),
      "utf8",
    );
    await binding.exec(migration.replaceAll("\n", " "));
    const db = drizzle(binding);
    const store = d1RoleStore(db, "app", ["read"]);
    const evaluation = { now: 0, signal: new AbortController().signal };
    await store.initialize({ revision: "r1", graph: { roles: [], bindings: [] } });
    const loaded = await store.read(evaluation);
    expect(loaded.revision).toBe("r1");
    expect(Object.isFrozen(loaded.graph)).toBe(true);
    const results = await Promise.all([
      store.compareAndSwap("r1", { revision: "r2", graph: loaded.graph }, evaluation),
      store.compareAndSwap("r1", { revision: "r3", graph: loaded.graph }, evaluation),
    ]);
    expect(results.filter(Boolean)).toHaveLength(1);
    expect(
      await store.compareAndSwap("r1", { revision: "r4", graph: loaded.graph }, evaluation),
    ).toBe(false);
    await binding
      .prepare("UPDATE authorization_role_graphs SET graph = ? WHERE namespace = ?")
      .bind('{"roles":[{"id":"invalid"}],"bindings":[]}', "app")
      .run();
    await expect(store.read(evaluation)).rejects.toThrow("Invalid role graph");
  } finally {
    await fixture.dispose();
  }
}, 30_000);
