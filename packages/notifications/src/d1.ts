import { and, eq, or, sql } from "drizzle-orm";
import type { DrizzleD1Database } from "drizzle-orm/d1";
import type { NotificationStore } from "./contracts";
import { notifications as n, notificationAttempts as a } from "./schema-sqlite";
import { sqliteReads } from "./sqlite";
import { mutableValues, validateSave } from "./persistence";

export { notificationSchema } from "./schema-sqlite";

export function createD1NotificationStore<T extends Record<string, unknown>>(
  db: DrizzleD1Database<T>,
): NotificationStore {
  return {
    ...sqliteReads(db),
    async save(record, expected, attempt) {
      validateSave(record, expected, attempt);
      const token = crypto.randomUUID();
      const update = db
        .update(n)
        .set({
          ...mutableValues(record),
          mutationToken: token,
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
        .returning({ id: n.id });
      if (!attempt) {
        const [rows] = await db.batch([update]);
        return rows.length === 1;
      }
      // D1 batches are transactional; a stale CAS cannot borrow another writer's token.
      const upsert = db
        .insert(a)
        .select(
          db
            .select({
              id: sql<string>`${attempt.id}`.as("id"),
              notificationId: sql<string>`${attempt.notificationId}`.as("notification_id"),
              number: sql<number>`${attempt.number}`.as("number"),
              state: sql<typeof attempt.state>`${attempt.state}`.as("state"),
              startedAt: sql<number>`${attempt.startedAt}`.as("started_at"),
              finishedAt: sql<number | null>`${attempt.finishedAt}`.as("finished_at"),
              providerMessageId: sql<string | null>`${attempt.providerMessageId}`.as(
                "provider_message_id",
              ),
              error: sql<typeof attempt.error>`${attempt.error}`.as("error"),
            })
            .from(n)
            .where(and(eq(n.id, record.id), eq(n.mutationToken, token))),
        )
        .onConflictDoUpdate({
          target: a.id,
          set: {
            state: attempt.state,
            finishedAt: attempt.finishedAt,
            providerMessageId: attempt.providerMessageId,
            error: attempt.error,
            startedAt: sql`CASE WHEN ${a.notificationId} = ${attempt.notificationId}
              AND ${a.number} = ${attempt.number} THEN ${a.startedAt} ELSE NULL END`,
          },
        });
      const [rows] = await db.batch([update, upsert]);
      return rows.length === 1;
    },
  };
}
