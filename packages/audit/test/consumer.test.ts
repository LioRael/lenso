import { expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createBunSqlitePlugin } from "@lenso/db/bun-sqlite";
import { sqliteSessionStore } from "@lenso/auth/drizzle/sqlite";
import { definePlugin, startApp } from "@lenso/core";
import { createManageAdapter } from "@lenso/manage";
import { eq } from "drizzle-orm";
import { createNotesAuthPlugin } from "../../../examples/notes/src/auth";
import { createNotesOperations } from "../../../examples/notes/src/operations";
import { createNotesPlugin, notesAudiences } from "../../../examples/notes/src/notes";
import { createSqliteNotesQueries } from "../../../examples/notes/src/queries-sqlite";
import * as notesSchema from "../../../examples/notes/src/schema-sqlite";
import { createAuthAuditAuthority } from "@lenso/audit/auth";
import { auditSqliteSchema, createSqliteAuditRepository } from "@lenso/audit/sqlite";
import { createAuditPlugin } from "@lenso/audit/plugin";
import { createAuditManage } from "@lenso/audit/manage";
import { createAuditReporter } from "@lenso/audit/diagnostics";

const schema = { ...notesSchema, ...auditSqliteSchema };

function loginKey() {
  return Array.from(crypto.getRandomValues(new Uint8Array(32)), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
}

test("Notes remove records a scoped best-effort event and exposes only authorized audit query", async () => {
  const principals = [
    { subjectId: "alice", key: loginKey() },
    { subjectId: "bob", key: loginKey() },
  ];
  const directory = await mkdtemp(
    join(process.env.DELTA_SCRATCH_DIR ?? import.meta.dir, ".audit-notes-consumer-"),
  );
  const filename = join(directory, "consumer.sqlite");
  const client = new Database(filename);
  const database = createBunSqlitePlugin({ id: "consumer-db", client, schema });
  const authentication = createNotesAuthPlugin({
    database,
    store: sqliteSessionStore,
    principals,
  });
  const notes = createNotesPlugin({
    id: "consumer-notes",
    database,
    authentication,
    queries: createSqliteNotesQueries,
  });
  const authorityPlugin = definePlugin({
    id: "consumer-audit-authority",
    requires: [authentication],
    setup(context) {
      return createAuthAuditAuthority(
        context.get(authentication).for(notesAudiences.remove),
        ({ principal, scope }) =>
          principal.kind === "user" &&
          scope.tenantId === null &&
          scope.scopeId === `owner:${principal.subjectId}`,
      );
    },
  });
  const repository = definePlugin({
    id: "consumer-audit-repository",
    requires: [database],
    setup(context) {
      return createSqliteAuditRepository(context.get(database), { durableIntents: false });
    },
  });
  const diagnostics = definePlugin({
    id: "consumer-audit-diagnostics",
    setup: () => createAuditReporter({ logger: { warn: (fields) => warnings.push(fields) } }),
  });
  const warnings: Record<string, unknown>[] = [];
  const audit = createAuditPlugin({
    id: "consumer-audit",
    repository,
    authority: authorityPlugin,
    diagnostics,
    summaryPolicy: { "notes.remove": { removed: { type: "boolean" } } },
  });
  const auditManage = createAuditManage({ id: "consumer-audit-manage", audit });
  const operations = createNotesOperations({
    notes,
    authentication,
    audit,
  });
  const plugins = [
    operations.plugin,
    auditManage.plugin,
    audit,
    diagnostics,
    repository,
    authorityPlugin,
    notes,
    authentication,
    database,
  ];
  let app: Awaited<ReturnType<typeof startApp>> | undefined;
  try {
    client.exec(`
      CREATE TABLE notes (
        id TEXT PRIMARY KEY NOT NULL, owner_id TEXT NOT NULL, title TEXT NOT NULL,
        body TEXT NOT NULL, created_at INTEGER NOT NULL
      );
    `);
    const authMigration = await Bun.file(
      fileURLToPath(
        new URL("../../auth/migrations/sqlite/0000_auth_sessions.sql", import.meta.url),
      ),
    ).text();
    client.exec(authMigration);
    const migration = await Bun.file(
      fileURLToPath(new URL("../migrations/sqlite/0000_audit.sql", import.meta.url)),
    ).text();
    client.exec(migration);
    app = await startApp({ plugins });
    const auth = app.get(authentication);
    const aliceEvidence = (await auth.issue(principals[0]!.key)).credential;
    const bobEvidence = (await auth.issue(principals[1]!.key)).credential;
    const notesService = app.get(notes);
    const alice = await auth.for(notesAudiences.create).required(aliceEvidence);
    const bob = await auth.for(notesAudiences.create).required(bobEvidence);
    const owned = await notesService.create(alice, {
      title: "private title",
      body: "private body",
    });
    const foreign = await notesService.create(bob, { title: "bob title", body: "bob body" });
    const remove = operations.operations.find((operation) => operation.method === "remove")!;
    const adapter = createManageAdapter({
      running: app,
      plugins,
      operations: [remove],
      binding: () => ({ context: { evidence: aliceEvidence } }),
      canList: async () =>
        (await auth.for(notesAudiences.remove).required(aliceEvidence)).kind === "user",
    });
    await expect(
      adapter.invoke(operations.plugin.id, "remove", {
        id: owned.id,
        actor: { subjectId: "bob" },
      }),
    ).rejects.toBeDefined();
    expect(await adapter.invoke(operations.plugin.id, "remove", { id: owned.id })).toEqual({
      removed: true,
    });
    expect(
      await notesService.read(
        await auth.for(notesAudiences.read).required(aliceEvidence),
        owned.id,
      ),
    ).toBeNull();
    await expect(
      adapter.invoke(operations.plugin.id, "remove", { id: foreign.id }),
    ).rejects.toBeDefined();
    expect(
      await notesService.read(
        await auth.for(notesAudiences.read).required(bobEvidence),
        foreign.id,
      ),
    ).toMatchObject({ title: "bob title" });

    const queryOperation = auditManage.operation;
    const queryAdapter = createManageAdapter({
      running: app,
      plugins,
      operations: [queryOperation],
      binding: async () => ({
        context: {
          principal: await auth.for(notesAudiences.remove).required(aliceEvidence),
        },
      }),
      canList: async () =>
        (await auth.for(notesAudiences.remove).required(aliceEvidence)).kind === "user",
    });
    expect((await queryAdapter.catalog()).map((entry) => entry.method)).toEqual(["query"]);
    await expect(
      queryAdapter.invoke(auditManage.plugin.id, "query", {
        scope: { tenantId: null, scopeId: "owner:alice" },
        principal: { kind: "user", subjectId: "alice" },
      }),
    ).rejects.toBeDefined();
    const page = (await queryAdapter.invoke(auditManage.plugin.id, "query", {
      scope: { tenantId: null, scopeId: "owner:alice" },
      result: "success",
    })) as { events: Array<{ reasonCode: string; summary: Record<string, unknown> }> };
    expect(page.events).toHaveLength(1);
    expect(page.events[0]).toMatchObject({
      action: "notes.remove",
      target: { type: "note", id: owned.id },
      result: "success",
      reasonCode: "removed",
      summary: { removed: true },
      scope: { tenantId: null, scopeId: "owner:alice" },
    });
    expect(Object.keys(page.events[0]!.summary)).toEqual(["removed"]);
    const serialized = JSON.stringify(page.events);
    for (const secret of [
      "private title",
      "private body",
      aliceEvidence,
      bobEvidence,
      principals[0]!.key,
      principals[1]!.key,
    ])
      expect(serialized).not.toContain(secret);
    await expect(
      queryAdapter.invoke(auditManage.plugin.id, "query", {
        scope: { tenantId: null, scopeId: "owner:bob" },
      }),
    ).rejects.toBeDefined();
    expect(
      await adapter.invoke(operations.plugin.id, "remove", { id: crypto.randomUUID() }),
    ).toEqual({
      removed: false,
    });
    const allEvents = (await queryAdapter.invoke(auditManage.plugin.id, "query", {
      scope: { tenantId: null, scopeId: "owner:alice" },
    })) as { events: Array<{ reasonCode: string; summary: Record<string, unknown> }> };
    expect(allEvents.events.map((event) => event.reasonCode).sort()).toEqual([
      "FORBIDDEN",
      "missing",
      "removed",
    ]);
    expect(Object.keys(allEvents).sort()).toEqual(["events", "nextCursor"]);
    expect(warnings).toEqual([]);
    expect(
      await app
        .get(database)
        .select()
        .from(notesSchema.notes)
        .where(eq(notesSchema.notes.id, foreign.id))
        .all(),
    ).toHaveLength(1);
    const longTarget = { type: "note", id: "x".repeat(129) };
    await app.get(audit).append(
      {
        id: crypto.randomUUID(),
        occurredAt: Date.now(),
        scope: { tenantId: null, scopeId: "owner:alice" },
        action: "notes.remove",
        target: longTarget,
        result: "success",
        reasonCode: "fixture",
        summary: { removed: false },
      },
      await auth.for(notesAudiences.remove).required(aliceEvidence),
    );
    const filtered = (await queryAdapter.invoke(auditManage.plugin.id, "query", {
      scope: { tenantId: null, scopeId: "owner:alice" },
      target: longTarget,
    })) as { events: Array<{ target: { id: string } }> };
    expect(filtered.events).toHaveLength(1);
    expect(filtered.events[0].target).toEqual(longTarget);
  } finally {
    await app?.stop();
    expect(client.query("SELECT 1").get()).toEqual({ "1": 1 });
    client.close();
    await rm(directory, { recursive: true, force: true });
  }
});
