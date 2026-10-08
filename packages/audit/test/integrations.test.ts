import { expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { definePlugin, startApp } from "@lenso/core";
import { drizzle } from "drizzle-orm/bun-sqlite";
import { createAuditPlugin } from "../src/plugin";
import { createSqliteAuditRepository } from "../src/sqlite";
import { createAuditReconciliationTask } from "../src/tasks";
import { createAuditService, type AuditAuthority, type AuditRepository } from "../src/index";

const migration = await Bun.file(
  new URL("../migrations/sqlite/0000_audit.sql", import.meta.url),
).text();
const system = Object.freeze({});
const scope = { tenantId: null, scopeId: "maintenance" };
const authority: AuditAuthority<object> = {
  async resolve(principal, requested) {
    if (principal !== system || requested.tenantId !== null || requested.scopeId !== scope.scopeId)
      throw new Error("not authorized");
    return { kind: "system", systemId: "maintenance" };
  },
};

test("exact plugin dependencies, config preflight and owned cleanup on stop/rollback", async () => {
  let acquired = 0;
  let closed = 0;
  const repository = definePlugin<AuditRepository>({
    id: "audit-repository",
    setup(context) {
      const client = new Database(":memory:");
      acquired++;
      context.onCleanup(() => {
        client.close();
        closed++;
      });
      client.exec(migration);
      return createSqliteAuditRepository(drizzle(client));
    },
  });
  const permissions = definePlugin({ id: "audit-authority", setup: () => authority });
  const plugin = createAuditPlugin({
    id: "audit",
    repository,
    authority: permissions,
    config: { maxPageSize: 2 },
  });
  expect(plugin.requires).toEqual([repository, permissions]);
  const app = await startApp({ plugins: [repository, permissions, plugin] });
  await expect(app.get(plugin).query({ scope, limit: 3 }, system)).rejects.toMatchObject({
    code: "invalid-input",
  });
  await app.stop();
  await app.stop();
  expect({ acquired, closed }).toEqual({ acquired: 1, closed: 1 });

  const invalid = createAuditPlugin({
    id: "invalid-audit",
    repository,
    authority: permissions,
    config: { maxPageSize: 0 },
  });
  await expect(startApp({ plugins: [repository, permissions, invalid] })).rejects.toBeDefined();
  expect({ acquired, closed }).toEqual({ acquired: 1, closed: 1 });
  const rollback = createAuditPlugin({
    id: "rollback-audit",
    repository,
    authority: permissions,
    summaryPolicy: { action: { secret: { type: "boolean" } } },
  });
  await expect(startApp({ plugins: [repository, permissions, rollback] })).rejects.toBeDefined();
  expect({ acquired, closed }).toEqual({ acquired: 2, closed: 2 });
  const lookalike = definePlugin({ id: permissions.id, setup: () => authority });
  await expect(startApp({ plugins: [repository, lookalike, plugin] })).rejects.toBeDefined();
  expect(acquired).toBe(2);
});

test("Tasks reconciliation carries only locators, reauthorizes, appends stable outcomes without repeating effects", async () => {
  const client = new Database(":memory:");
  try {
    client.exec(migration);
    const audit = createAuditService({
      repository: createSqliteAuditRepository(drizzle(client)),
      authority,
    });
    const intentId = crypto.randomUUID();
    const outcomeId = crypto.randomUUID();
    await audit.append(
      {
        id: intentId,
        occurredAt: 1,
        scope,
        action: "maintenance.run",
        target: { type: "resource", id: "resource-1" },
        result: "intent",
        reasonCode: "requested",
      },
      system,
    );
    let currentPrincipal = system;
    let reconciliations = 0;
    const task = createAuditReconciliationTask({
      name: "audit.reconcile",
      audit,
      async principal() {
        return currentPrincipal;
      },
      async reconcile() {
        reconciliations++;
        // A fixture for reading effect state, not a durable queue/backend test.
        return { id: outcomeId, occurredAt: 2, result: "success", reasonCode: "verified" };
      },
    });
    const payload = { scope, intentId };
    expect(JSON.stringify(payload)).not.toMatch(/actor|credential|subjectId/);
    const context = {
      jobId: crypto.randomUUID(),
      attempt: 1,
      signal: new AbortController().signal,
    };
    expect(await task.handler(payload, context)).toEqual({ eventId: outcomeId });
    expect(await task.handler(payload, { ...context, attempt: 2 })).toEqual({ eventId: outcomeId });
    expect((await audit.query({ scope }, system)).events).toHaveLength(2);
    currentPrincipal = {};
    await expect(task.handler(payload, context)).rejects.toMatchObject({ code: "unauthorized" });
    expect(reconciliations).toBe(2);
    const invalid = await task.input["~standard"].validate({ ...payload, actor: "fake" });
    expect(invalid.issues).toBeDefined();
  } finally {
    client.close();
  }
});

test("standalone root bundles without optional framework/provider packages", async () => {
  const result = await Bun.build({
    entrypoints: [new URL("../src/index.ts", import.meta.url).pathname],
    target: "browser",
    packages: "bundle",
  });
  expect(result.success).toBe(true);
  const code = await result.outputs[0].text();
  expect(code).not.toMatch(/(?:from|import)\s*["'](?:@lenso|drizzle-orm|zod|@opentelemetry)/);
});
