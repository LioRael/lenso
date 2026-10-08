import { Database } from "bun:sqlite";
import { expect, test } from "bun:test";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { drizzle } from "drizzle-orm/bun-sqlite";
import { auditEvents, auditSqliteSchema, createSqliteAuditRepository } from "../src/sqlite";
import type { AuditEvent } from "../src/contracts";

const migration = await readFile(
  new URL("../migrations/sqlite/0000_audit.sql", import.meta.url),
  "utf8",
);
const uuid = (n: number) => `00000000-0000-0000-0000-${n.toString().padStart(12, "0")}`;
function event(
  id: number,
  scopeId = "scope",
  tenantId: string | null = null,
  recordedAt = 10,
): AuditEvent {
  return {
    id: uuid(id),
    occurredAt: 1,
    recordedAt,
    scope: { tenantId, scopeId },
    actor: { kind: "system", systemId: "worker" },
    action: "update",
    target: { type: "document", id: `doc-${id}` },
    result: "success",
    reasonCode: "completed",
    correlationId: "trace",
    summary: {},
  };
}

test("SQLite audit repository scopes ids, detects duplicates/conflicts, pages and persists", async () => {
  const root = await mkdtemp(
    join(process.env.DELTA_SCRATCH_DIR ?? import.meta.dir, ".audit-sqlite-"),
  );
  const path = join(root, "audit.sqlite");
  let db = new Database(path);
  try {
    db.exec(migration);
    const repo = createSqliteAuditRepository(drizzle(db, { schema: auditSqliteSchema }));
    expect(repo.durableIntents).toBe(false);
    const first = event(1);
    expect(await repo.insert(first)).toBe("inserted");
    expect(await repo.insert({ ...first, recordedAt: 20 })).toBe("duplicate");
    expect(await repo.insert({ ...first, action: "delete" })).toBe("conflict");
    expect(await repo.get({ tenantId: null, scopeId: "other" }, first.id)).toBeNull();
    expect(await repo.insert(event(1, "other"))).toBe("inserted");
    expect(await repo.insert(event(1, "scope", "tenant"))).toBe("inserted");
    expect(await repo.get({ tenantId: "tenant", scopeId: "scope" }, first.id)).toEqual(
      event(1, "scope", "tenant"),
    );
    const tiedA = event(2, "scope", null, 30);
    const tiedB = event(3, "scope", null, 30);
    await repo.insert(tiedA);
    await repo.insert(tiedB);
    expect((await repo.list({ scope: first.scope, limit: 2 })).map((item) => item.id)).toEqual([
      tiedB.id,
      tiedA.id,
    ]);
    expect(
      (
        await repo.list({ scope: first.scope, limit: 2, cursor: { recordedAt: 30, id: tiedA.id } })
      ).map((item) => item.id),
    ).toEqual([first.id]);
    expect(
      await repo.list({
        scope: first.scope,
        limit: 5,
        action: "update",
        target: tiedA.target,
        result: "success",
        correlationId: "trace",
        recordedFrom: 30,
        recordedTo: 30,
      }),
    ).toEqual([tiedA]);
    db.close();
    db = new Database(path);
    expect(
      await createSqliteAuditRepository(drizzle(db, { schema: auditSqliteSchema })).get(
        first.scope,
        first.id,
      ),
    ).toEqual(first);
    expect(db.query("SELECT 1").get()).toEqual({ "1": 1 });
  } finally {
    db.close();
    await rm(root, { recursive: true, force: true });
  }
});

test("SQLite repository does not own its borrowed database", async () => {
  const db = new Database(":memory:");
  db.exec(migration);
  const repo = createSqliteAuditRepository(drizzle(db, { schema: { auditEvents } }), {
    durableIntents: true,
  });
  expect(repo.durableIntents).toBe(true);
  expect(await repo.insert(event(9))).toBe("inserted");
  expect(db.query("SELECT COUNT(*) AS count FROM lenso_audit_events").get()).toEqual({ count: 1 });
  db.close();
});
