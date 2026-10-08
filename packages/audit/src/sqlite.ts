import { and, desc, eq, lt, or, gte, lte, type SQL } from "drizzle-orm";
import { index, integer, primaryKey, sqliteTable, text } from "drizzle-orm/sqlite-core";
import type { BunSQLiteDatabase } from "drizzle-orm/bun-sqlite";
import type { DrizzleD1Database } from "drizzle-orm/d1";
import type { AuditQuery, AuditRepository, AuditScope } from "./contracts";
import { decodeEvent, insertionResult, tenantKey } from "./repository-helpers";

export const auditEvents = sqliteTable(
  "lenso_audit_events",
  {
    tenantKey: text("tenant_key").notNull(),
    scopeId: text("scope_id").notNull(),
    id: text("id").notNull(),
    recordedAt: integer("recorded_at").notNull(),
    action: text("action").notNull(),
    targetType: text("target_type").notNull(),
    targetId: text("target_id").notNull(),
    result: text("result").notNull(),
    correlationId: text("correlation_id"),
    eventJson: text("event_json").notNull(),
  },
  (table) => [
    primaryKey({ columns: [table.tenantKey, table.scopeId, table.id] }),
    index("lenso_audit_events_scope_order").on(
      table.tenantKey,
      table.scopeId,
      desc(table.recordedAt),
      desc(table.id),
    ),
  ],
);
export const auditSqliteSchema = { auditEvents };

export function createSqliteAuditRepository<TSchema extends Record<string, unknown>>(
  db: BunSQLiteDatabase<TSchema> | DrizzleD1Database<TSchema>,
  options: { durableIntents?: boolean } = {},
): AuditRepository {
  return {
    durableIntents: options.durableIntents ?? false,
    async insert(event) {
      const rows = await db
        .insert(auditEvents)
        .values({
          tenantKey: tenantKey(event.scope),
          scopeId: event.scope.scopeId,
          id: event.id,
          recordedAt: event.recordedAt,
          action: event.action,
          targetType: event.target.type,
          targetId: event.target.id,
          result: event.result,
          correlationId: event.correlationId ?? null,
          eventJson: JSON.stringify(event),
        })
        .onConflictDoNothing({
          target: [auditEvents.tenantKey, auditEvents.scopeId, auditEvents.id],
        })
        .returning();
      if (rows.length) return "inserted";
      return insertionResult(event, (await this.get(event.scope, event.id)) ?? undefined);
    },
    async get(scope: AuditScope, id: string) {
      const rows = await db
        .select()
        .from(auditEvents)
        .where(
          and(
            eq(auditEvents.tenantKey, tenantKey(scope)),
            eq(auditEvents.scopeId, scope.scopeId),
            eq(auditEvents.id, id),
          ),
        )
        .limit(1);
      return rows[0] ? decodeEvent(rows[0].eventJson) : null;
    },
    async list(query: AuditQuery & { limit: number }) {
      const terms: SQL[] = [
        eq(auditEvents.tenantKey, tenantKey(query.scope)),
        eq(auditEvents.scopeId, query.scope.scopeId),
      ];
      if (query.action !== undefined) terms.push(eq(auditEvents.action, query.action));
      if (query.target)
        terms.push(
          eq(auditEvents.targetType, query.target.type),
          eq(auditEvents.targetId, query.target.id),
        );
      if (query.result !== undefined) terms.push(eq(auditEvents.result, query.result));
      if (query.correlationId !== undefined)
        terms.push(eq(auditEvents.correlationId, query.correlationId));
      if (query.recordedFrom !== undefined)
        terms.push(gte(auditEvents.recordedAt, query.recordedFrom));
      if (query.recordedTo !== undefined) terms.push(lte(auditEvents.recordedAt, query.recordedTo));
      if (query.cursor)
        terms.push(
          or(
            lt(auditEvents.recordedAt, query.cursor.recordedAt),
            and(
              eq(auditEvents.recordedAt, query.cursor.recordedAt),
              lt(auditEvents.id, query.cursor.id),
            ),
          )!,
        );
      const rows = await db
        .select()
        .from(auditEvents)
        .where(and(...terms))
        .orderBy(desc(auditEvents.recordedAt), desc(auditEvents.id))
        .limit(query.limit);
      return rows.map((row) => decodeEvent(row.eventJson));
    },
  };
}
