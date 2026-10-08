import { expect, test } from "bun:test";
import { readFile } from "node:fs/promises";
import { drizzle } from "drizzle-orm/d1";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";
import { auditSqliteSchema, createSqliteAuditRepository } from "../src/sqlite";
import type { AuditEvent } from "../src/contracts";

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
    summary: { count: id },
  };
}

test("local workerd D1 audit repository inserts, detects duplicates/conflicts, scopes and pages", async () => {
  const migration = await readFile(
    new URL("../migrations/sqlite/0000_audit.sql", import.meta.url),
    "utf8",
  );
  const mf = new Miniflare(
    convertV4MiniflareOptions({
      modules: true,
      script: "export default { fetch() { return new Response('ready'); } };",
      compatibilityDate: "2026-10-08",
      d1Databases: ["AUDIT_DB"],
    }),
  );
  try {
    const binding = await mf.getD1Database("AUDIT_DB");
    for (const statement of migration
      .split(";")
      .map((part) => part.trim())
      .filter(Boolean)) {
      await binding.prepare(statement).run();
    }
    const repo = createSqliteAuditRepository(drizzle(binding, { schema: auditSqliteSchema }));
    const first = event(1);
    expect(await repo.insert(first)).toBe("inserted");
    expect(await repo.insert(first)).toBe("duplicate");
    expect(await repo.insert({ ...first, action: "delete" })).toBe("conflict");
    expect(await repo.get(first.scope, first.id)).toEqual(first);
    expect(await repo.get({ tenantId: null, scopeId: "other" }, first.id)).toBeNull();
    expect(await repo.insert(event(1, "other"))).toBe("inserted");
    expect(await repo.insert(event(1, "scope", "tenant"))).toBe("inserted");
    expect(await repo.get({ tenantId: "tenant", scopeId: "scope" }, first.id)).toEqual(
      event(1, "scope", "tenant"),
    );
    const tiedA = event(2, "scope", null, 30);
    const tiedB = event(3, "scope", null, 30);
    expect(await repo.insert(tiedA)).toBe("inserted");
    expect(await repo.insert(tiedB)).toBe("inserted");
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
  } finally {
    await mf.dispose();
  }
}, 30_000);
