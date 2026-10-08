import { and, asc, desc, eq, gt, isNull, lte, or, sql } from "drizzle-orm";
import type { BunSQLDatabase } from "drizzle-orm/bun-sql";
import { schedules, scheduleOccurrences, scheduleQueueBindings } from "./schema";
import type { Occurrence, Schedule, ScheduleScope, ScheduleStore } from "./contracts";

export { schedules, scheduleOccurrences, scheduleQueueBindings, schedulerSchema } from "./schema";

const scopeWhere = (table: typeof schedules | typeof scheduleOccurrences, scope: ScheduleScope) =>
  and(eq(table.namespace, scope.namespace), eq(table.tenantId, scope.tenantId));

const boundedLimit = (limit: number) =>
  Number.isFinite(limit) ? Math.max(0, Math.min(10_000, Math.floor(limit))) : 0;

const scheduleValues = (scope: ScheduleScope, s: Schedule) => ({
  namespace: scope.namespace,
  tenantId: scope.tenantId,
  id: s.id,
  revision: s.revision,
  state: s.state,
  task: s.task,
  input: sql`${JSON.stringify(s.input)}::jsonb`,
  rule: s.rule,
  misfire: s.misfire,
  graceMs: s.graceMs,
  nextAt: s.nextAt,
  initiator: s.initiator,
});
const occurrenceValues = (scope: ScheduleScope, o: Occurrence) => ({
  namespace: scope.namespace,
  tenantId: scope.tenantId,
  id: o.id,
  scheduleId: o.scheduleId,
  revision: o.revision,
  scheduledAt: o.scheduledAt,
  source: o.source,
  task: o.task,
  input: sql`${JSON.stringify(o.input)}::jsonb`,
  initiator: o.initiator,
  state: o.state,
  jobId: o.jobId,
  error: o.error,
  leaseToken: o.leaseToken,
  leaseUntil: o.leaseUntil,
});
const asSchedule = (row: typeof schedules.$inferSelect): Schedule => ({
  id: row.id,
  revision: row.revision,
  state: row.state,
  task: row.task,
  input: row.input,
  rule: row.rule,
  misfire: row.misfire,
  graceMs: row.graceMs,
  nextAt: row.nextAt,
  initiator: row.initiator,
});
const asOccurrence = (row: typeof scheduleOccurrences.$inferSelect): Occurrence => ({
  id: row.id,
  scheduleId: row.scheduleId,
  revision: row.revision,
  scheduledAt: row.scheduledAt,
  source: row.source,
  task: row.task,
  input: row.input,
  initiator: row.initiator,
  state: row.state,
  jobId: row.jobId,
  error: row.error,
  leaseToken: row.leaseToken,
  leaseUntil: row.leaseUntil,
});

/**
 * Create a store over a caller-owned Bun SQL Drizzle connection. The probe is
 * intentionally PostgreSQL-specific; this factory never creates or closes a connection.
 */
export async function createPostgresScheduleStore<TSchema extends Record<string, unknown>>(
  db: BunSQLDatabase<TSchema>,
): Promise<ScheduleStore> {
  try {
    await db.execute(sql`SELECT 1::integer LIMIT 0`);
  } catch (cause) {
    throw new TypeError(
      "createPostgresScheduleStore requires a Bun SQL PostgreSQL Drizzle database",
      { cause },
    );
  }
  // Inspect the declared shape without DDL; missing migrations must fail at setup.
  await db.select().from(schedules).limit(0);
  await db.select().from(scheduleOccurrences).limit(0);
  await db.select().from(scheduleQueueBindings).limit(0);
  return {
    kind: "postgres",
    async bind(scope, identity) {
      return db.transaction(async (tx) => {
        await tx
          .insert(scheduleQueueBindings)
          .values({
            ...scope,
            queueKind: identity.kind,
            queueId: identity.id,
          })
          .onConflictDoNothing();
        const [row] = await tx
          .select()
          .from(scheduleQueueBindings)
          .where(
            and(
              eq(scheduleQueueBindings.namespace, scope.namespace),
              eq(scheduleQueueBindings.tenantId, scope.tenantId),
            ),
          );
        return row?.queueKind === identity.kind && row.queueId === identity.id;
      });
    },
    async create(scope, schedule) {
      await db.insert(schedules).values(scheduleValues(scope, schedule));
    },
    async get(scope, id) {
      const rows = await db
        .select()
        .from(schedules)
        .where(and(scopeWhere(schedules, scope), eq(schedules.id, id)))
        .limit(1);
      return rows[0] ? asSchedule(rows[0]) : null;
    },
    async list(scope, limit) {
      const rows = await db
        .select()
        .from(schedules)
        .where(scopeWhere(schedules, scope))
        .orderBy(asc(schedules.id))
        .limit(boundedLimit(limit));
      return rows.map(asSchedule);
    },
    async replace(scope, expectedRevision, schedule) {
      const rows = await db
        .update(schedules)
        .set(scheduleValues(scope, schedule))
        .where(
          and(
            scopeWhere(schedules, scope),
            eq(schedules.id, schedule.id),
            eq(schedules.revision, expectedRevision),
          ),
        )
        .returning({ id: schedules.id });
      return rows.length === 1;
    },
    async due(scope, now, limit) {
      const rows = await db
        .select()
        .from(schedules)
        .where(
          and(
            scopeWhere(schedules, scope),
            eq(schedules.state, "active"),
            lte(schedules.nextAt, now),
          ),
        )
        .orderBy(asc(schedules.nextAt), asc(schedules.id))
        .limit(boundedLimit(limit));
      return rows.map(asSchedule);
    },
    async advance(scope, expected, nextAt, occurrence) {
      return db.transaction(async (tx) => {
        const rows = await tx
          .update(schedules)
          .set({
            revision: expected.revision + 1,
            nextAt,
            state: nextAt === null ? "completed" : expected.state,
          })
          .where(
            and(
              scopeWhere(schedules, scope),
              eq(schedules.id, expected.id),
              eq(schedules.revision, expected.revision),
              expected.nextAt === null
                ? isNull(schedules.nextAt)
                : eq(schedules.nextAt, expected.nextAt),
              eq(schedules.state, "active"),
            ),
          )
          .returning({ id: schedules.id });
        if (rows.length !== 1) return false;
        if (occurrence)
          await tx.insert(scheduleOccurrences).values(occurrenceValues(scope, occurrence));
        return true;
      });
    },
    async trigger(scope, expected, occurrence) {
      return db.transaction(async (tx) => {
        const rows = await tx
          .select()
          .from(schedules)
          .where(and(scopeWhere(schedules, scope), eq(schedules.id, expected.id)))
          .for("update");
        const row = rows[0];
        if (!row || row.revision !== expected.revision || row.state === "cancelled") return null;
        await tx
          .insert(scheduleOccurrences)
          .values(occurrenceValues(scope, occurrence))
          .onConflictDoNothing();
        const inserted = await tx
          .select()
          .from(scheduleOccurrences)
          .where(
            and(scopeWhere(scheduleOccurrences, scope), eq(scheduleOccurrences.id, occurrence.id)),
          )
          .limit(1);
        return inserted[0] ? asOccurrence(inserted[0]) : null;
      });
    },
    async claim(scope, now, leaseMs) {
      return db.transaction(async (tx) => {
        const rows = await tx
          .select()
          .from(scheduleOccurrences)
          .where(
            and(
              scopeWhere(scheduleOccurrences, scope),
              eq(scheduleOccurrences.state, "pending"),
              lte(scheduleOccurrences.scheduledAt, now),
              or(
                sql`${scheduleOccurrences.leaseUntil} IS NULL`,
                lte(scheduleOccurrences.leaseUntil, now),
              ),
            ),
          )
          .orderBy(asc(scheduleOccurrences.scheduledAt), asc(scheduleOccurrences.id))
          .limit(1)
          .for("update", { skipLocked: true });
        const row = rows[0];
        if (!row) return null;
        const leaseToken = crypto.randomUUID();
        const updated = await tx
          .update(scheduleOccurrences)
          .set({
            leaseToken,
            leaseUntil: now + leaseMs,
          })
          .where(
            and(
              scopeWhere(scheduleOccurrences, scope),
              eq(scheduleOccurrences.id, row.id),
              eq(scheduleOccurrences.state, "pending"),
              lte(scheduleOccurrences.scheduledAt, now),
              or(
                sql`${scheduleOccurrences.leaseUntil} IS NULL`,
                lte(scheduleOccurrences.leaseUntil, now),
              ),
            ),
          )
          .returning();
        return updated[0] ? asOccurrence(updated[0]) : null;
      });
    },
    async renew(scope, id, leaseToken, now, leaseMs) {
      const rows = await db
        .update(scheduleOccurrences)
        .set({ leaseUntil: now + leaseMs })
        .where(
          and(
            scopeWhere(scheduleOccurrences, scope),
            eq(scheduleOccurrences.id, id),
            eq(scheduleOccurrences.state, "pending"),
            eq(scheduleOccurrences.leaseToken, leaseToken),
            gt(scheduleOccurrences.leaseUntil, now),
          ),
        )
        .returning({ id: scheduleOccurrences.id });
      return rows.length === 1;
    },
    async settle(scope, id, leaseToken, result) {
      const dispatchFailed = "error" in result && result.error === "dispatch-failed";
      const rows = await db
        .update(scheduleOccurrences)
        .set(
          "jobId" in result
            ? {
                state: "enqueued",
                jobId: result.jobId,
                error: null,
                leaseToken: null,
                leaseUntil: null,
              }
            : dispatchFailed
              ? { state: "pending", error: "dispatch-failed", leaseToken: null }
              : { state: "blocked", error: result.error, leaseToken: null, leaseUntil: null },
        )
        .where(
          and(
            scopeWhere(scheduleOccurrences, scope),
            eq(scheduleOccurrences.id, id),
            eq(scheduleOccurrences.leaseToken, leaseToken),
            eq(scheduleOccurrences.state, "pending"),
          ),
        )
        .returning({ id: scheduleOccurrences.id });
      return rows.length === 1;
    },
    async occurrences(scope, scheduleId, limit) {
      const rows = await db
        .select()
        .from(scheduleOccurrences)
        .where(
          and(
            scopeWhere(scheduleOccurrences, scope),
            eq(scheduleOccurrences.scheduleId, scheduleId),
          ),
        )
        .orderBy(desc(scheduleOccurrences.scheduledAt), asc(scheduleOccurrences.id))
        .limit(boundedLimit(limit));
      return rows.map(asOccurrence);
    },
  };
}
