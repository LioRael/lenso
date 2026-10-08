import { and, asc, desc, eq, isNull, lte, or, sql } from "drizzle-orm";
import type { BunSQLiteDatabase } from "drizzle-orm/bun-sqlite";
import type { DrizzleD1Database } from "drizzle-orm/d1";
import type { NotificationStore } from "./contracts";
import {
  notifications as n,
  notificationAttempts as a,
  notificationPreferences as p,
} from "./schema-sqlite";
import {
  boundedLimit,
  decodeRecord,
  insertValues,
  mutableValues,
  validateSave,
} from "./persistence";

export { notificationSchema } from "./schema-sqlite";

export function sqliteReads<T extends Record<string, unknown>>(
  db: BunSQLiteDatabase<T> | DrizzleD1Database<T>,
): Omit<NotificationStore, "save"> {
  return {
    async insertOrGet(record) {
      await db
        .insert(n)
        .values(insertValues(record))
        .onConflictDoNothing({
          target: [n.tenantId, n.scope, n.idempotencyKey],
        })
        .run();
      const found = await this.findKey(record.tenantId, record.scope, record.idempotencyKey);
      if (!found) throw new Error("Inserted notification is missing");
      return found;
    },
    async findKey(tenantId, scope, key) {
      const [row] = await db
        .select()
        .from(n)
        .where(and(eq(n.tenantId, tenantId), eq(n.scope, scope), eq(n.idempotencyKey, key)))
        .limit(1)
        .all();
      return row ? decodeRecord(row) : null;
    },
    async get(id) {
      const [row] = await db.select().from(n).where(eq(n.id, id)).limit(1).all();
      return row ? decodeRecord(row) : null;
    },
    async list(filter) {
      const rows = await db
        .select()
        .from(n)
        .where(
          and(
            eq(n.tenantId, filter.tenantId),
            filter.recipientId === undefined ? undefined : eq(n.recipientId, filter.recipientId),
          ),
        )
        .orderBy(desc(n.createdAt), asc(n.id))
        .limit(boundedLimit(filter.limit))
        .all();
      return rows.map(decodeRecord);
    },
    async attempts(id) {
      return await db.select().from(a).where(eq(a.notificationId, id)).orderBy(asc(a.number)).all();
    },
    async getPreference(key) {
      const [row] = await db
        .select()
        .from(p)
        .where(
          and(
            eq(p.tenantId, key.tenantId),
            eq(p.recipientId, key.recipientId),
            eq(p.category, key.category),
            eq(p.channelId, key.channelId),
          ),
        )
        .limit(1)
        .all();
      return row ?? null;
    },
    async setPreference(preference) {
      await db
        .insert(p)
        .values(preference)
        .onConflictDoUpdate({
          target: [p.tenantId, p.recipientId, p.category, p.channelId],
          set: { enabled: preference.enabled },
        })
        .run();
    },
    async markEnqueued(id, taskJobId) {
      const rows = await db
        .update(n)
        .set({ taskJobId })
        .where(and(eq(n.id, id), or(isNull(n.taskJobId), eq(n.taskJobId, taskJobId))))
        .returning()
        .all();
      return rows.length === 1;
    },
    async recoverable(now, limit) {
      const rows = await db
        .select()
        .from(n)
        .where(
          and(
            isNull(n.taskJobId),
            or(
              eq(n.state, "pending"),
              and(eq(n.state, "unknown"), eq(n.retryable, true)),
              and(eq(n.state, "failed"), eq(n.retryable, true)),
              and(eq(n.state, "sending"), lte(n.leaseUntil, now)),
            ),
          ),
        )
        .orderBy(asc(n.updatedAt), asc(n.id))
        .limit(boundedLimit(limit))
        .all();
      return rows.map(decodeRecord);
    },
  };
}

export function createSqliteNotificationStore<T extends Record<string, unknown>>(
  db: BunSQLiteDatabase<T>,
): NotificationStore {
  return {
    ...sqliteReads(db),
    async save(record, expected, attempt) {
      validateSave(record, expected, attempt);
      return db.transaction((tx) => {
        const rows = tx
          .update(n)
          .set({
            ...mutableValues(record),
            firstRequestAt: sql`coalesce(${n.firstRequestAt}, ${record.firstRequestAt})`,
          })
          .where(
            and(
              eq(n.id, record.id),
              eq(n.tenantId, record.tenantId),
              eq(n.revision, expected),
              attempt
                ? or(
                    eq(n.attemptCount, record.attemptCount),
                    eq(n.attemptCount, record.attemptCount - 1),
                  )
                : eq(n.attemptCount, record.attemptCount),
            ),
          )
          .returning({ id: n.id })
          .all();
        if (!rows.length) return false;
        if (attempt)
          tx.insert(a)
            .values(attempt)
            .onConflictDoUpdate({
              target: a.id,
              set: {
                state: attempt.state,
                finishedAt: attempt.finishedAt,
                providerMessageId: attempt.providerMessageId,
                error: attempt.error,
                // A foreign attempt ID collision violates NOT NULL and rolls back the CAS.
                startedAt: sql`CASE WHEN ${a.notificationId} = ${attempt.notificationId}
              AND ${a.number} = ${attempt.number} THEN ${a.startedAt} ELSE NULL END`,
              },
            })
            .run();
        return true;
      });
    },
  };
}
