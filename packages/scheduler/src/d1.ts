import { and, asc, desc, eq, exists, gt, isNull, lte, ne, or, sql } from "drizzle-orm";
import type { DrizzleD1Database } from "drizzle-orm/d1";
import type { JsonValue } from "@lenso/tasks";
import type { Occurrence, Schedule, ScheduleScope, ScheduleStore } from "./contracts";
import {
  d1Schedules as schedules,
  d1ScheduleOccurrences as scheduleOccurrences,
  d1QueueBindings as queueBindings,
} from "./schema-d1";

export {
  d1Schedules,
  d1ScheduleOccurrences,
  d1QueueBindings,
  schedulerD1Schema,
} from "./schema-d1";

const scopeWhere = (
  table: typeof schedules | typeof scheduleOccurrences | typeof queueBindings,
  scope: ScheduleScope,
) => and(eq(table.namespace, scope.namespace), eq(table.tenantId, scope.tenantId));

const boundedLimit = (limit: number) =>
  Number.isFinite(limit) ? Math.max(0, Math.min(10_000, Math.floor(limit))) : 0;

function invalidRow(): never {
  throw new TypeError("Invalid D1 scheduler row");
}

function finiteJson(value: unknown): value is JsonValue {
  let nodes = 0;
  const seen = new Set<object>();
  function check(item: unknown, depth: number): boolean {
    if (++nodes > 65_536 || depth > 32) return false;
    if (item === null || typeof item === "string" || typeof item === "boolean") return true;
    if (typeof item === "number") return Number.isFinite(item);
    if (typeof item !== "object" || seen.has(item)) return false;
    if (!Array.isArray(item) && ![Object.prototype, null].includes(Object.getPrototypeOf(item)))
      return false;
    seen.add(item);
    const keys = Reflect.ownKeys(item).filter((key) => !Array.isArray(item) || key !== "length");
    if (Array.isArray(item) && keys.length !== item.length) return false;
    for (const key of keys) {
      const descriptor = Object.getOwnPropertyDescriptor(item, key)!;
      if (
        typeof key !== "string" ||
        !descriptor.enumerable ||
        !("value" in descriptor) ||
        (Array.isArray(item) && (!/^(0|[1-9]\d*)$/.test(key) || Number(key) >= item.length)) ||
        !check(descriptor.value, depth + 1)
      )
        return false;
    }
    seen.delete(item);
    return true;
  }
  return check(value, 0);
}

function encode(value: unknown): string {
  if (!finiteJson(value)) throw new TypeError("D1 scheduler values must be finite JSON");
  return JSON.stringify(value);
}

function decode(value: string): JsonValue {
  if (typeof value !== "string") return invalidRow();
  const parsed: unknown = JSON.parse(value);
  if (!finiteJson(parsed)) return invalidRow();
  return parsed;
}

function subject(value: string): Schedule["initiator"] {
  const parsed = decode(value);
  if (
    parsed === null ||
    typeof parsed !== "object" ||
    Array.isArray(parsed) ||
    typeof parsed.realmId !== "string" ||
    typeof parsed.subjectId !== "string"
  )
    return invalidRow();
  return { realmId: parsed.realmId, subjectId: parsed.subjectId };
}

function rule(value: string): Schedule["rule"] {
  const parsed = decode(value);
  if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) return invalidRow();
  if (parsed.kind === "once" && typeof parsed.at === "number" && Number.isSafeInteger(parsed.at))
    return { kind: "once", at: parsed.at };
  if (
    parsed.kind === "cron" &&
    typeof parsed.expression === "string" &&
    typeof parsed.timezone === "string"
  )
    return { kind: "cron", expression: parsed.expression, timezone: parsed.timezone };
  return invalidRow();
}

const integer = (value: number) => Number.isSafeInteger(value);
const nullableInteger = (value: number | null) => value === null || integer(value);
const nullableString = (value: string | null) => value === null || typeof value === "string";

const scheduleValues = (scope: ScheduleScope, s: Schedule) => ({
  ...scope,
  id: s.id,
  revision: s.revision,
  state: s.state,
  task: s.task,
  input: encode(s.input),
  rule: encode(s.rule),
  misfire: s.misfire,
  graceMs: s.graceMs,
  nextAt: s.nextAt,
  initiator: encode(s.initiator),
});

function asSchedule(row: typeof schedules.$inferSelect): Schedule {
  if (
    typeof row.id !== "string" ||
    typeof row.task !== "string" ||
    !integer(row.revision) ||
    row.revision < 0 ||
    !integer(row.graceMs) ||
    row.graceMs < 0 ||
    !nullableInteger(row.nextAt) ||
    !["active", "paused", "cancelled", "completed"].includes(row.state) ||
    !["skip", "coalesce"].includes(row.misfire)
  )
    return invalidRow();
  return {
    id: row.id,
    revision: row.revision,
    state: row.state,
    task: row.task,
    input: decode(row.input),
    rule: rule(row.rule),
    misfire: row.misfire,
    graceMs: row.graceMs,
    nextAt: row.nextAt,
    initiator: subject(row.initiator),
  };
}

function asOccurrence(row: typeof scheduleOccurrences.$inferSelect): Occurrence {
  if (
    typeof row.id !== "string" ||
    typeof row.scheduleId !== "string" ||
    typeof row.task !== "string" ||
    !integer(row.revision) ||
    row.revision < 0 ||
    !integer(row.scheduledAt) ||
    !nullableInteger(row.leaseUntil) ||
    !nullableString(row.jobId) ||
    !nullableString(row.leaseToken) ||
    !["timer", "manual"].includes(row.source) ||
    !["pending", "enqueued", "blocked"].includes(row.state) ||
    (row.error !== null &&
      !["dispatch-failed", "execution-denied", "job-expired", "dispatch-invalid"].includes(
        row.error,
      ))
  )
    return invalidRow();
  const validLifecycle =
    row.state === "pending"
      ? row.jobId === null && (row.error === null || row.error === "dispatch-failed")
      : row.state === "enqueued"
        ? row.jobId !== null &&
          row.error === null &&
          row.leaseToken === null &&
          row.leaseUntil === null
        : row.jobId === null &&
          row.error !== null &&
          row.error !== "dispatch-failed" &&
          row.leaseToken === null &&
          row.leaseUntil === null;
  if (!validLifecycle) return invalidRow();
  return {
    id: row.id,
    scheduleId: row.scheduleId,
    revision: row.revision,
    scheduledAt: row.scheduledAt,
    source: row.source,
    task: row.task,
    input: decode(row.input),
    initiator: subject(row.initiator),
    state: row.state,
    jobId: row.jobId,
    error: row.error,
    leaseToken: row.leaseToken,
    leaseUntil: row.leaseUntil,
  };
}

// A zero-row CAS does not abort a D1 batch. Gate the insert on this batch's
// unique token so a lost CAS cannot produce an outbox entry.
function gatedOccurrence(scope: ScheduleScope, scheduleId: string, token: string, o: Occurrence) {
  return sql`
    SELECT ${scope.namespace}, ${scope.tenantId}, ${o.id}, ${o.scheduleId},
      ${o.revision}, ${o.scheduledAt}, ${o.source}, ${o.task}, ${encode(o.input)},
      ${encode(o.initiator)}, ${o.state}, ${o.jobId}, ${o.error}, ${o.leaseToken}, ${o.leaseUntil}
    FROM ${schedules}
    WHERE ${scopeWhere(schedules, scope)}
      AND ${schedules.id} = ${scheduleId} AND ${schedules.writeToken} = ${token}
  `;
}

/** Borrow a plain D1 binding; never open a session, acquire resources, or run DDL. */
export async function createD1ScheduleStore<T extends Record<string, unknown>>(
  db: DrizzleD1Database<T>,
): Promise<ScheduleStore> {
  const client = (
    db as {
      $client?: {
        prepare?: unknown;
        batch?: unknown;
        withSession?: unknown;
        getBookmark?: unknown;
      };
    } | null
  )?.$client;
  if (
    !client ||
    typeof client.prepare !== "function" ||
    typeof client.batch !== "function" ||
    typeof client.withSession !== "function" ||
    typeof client.getBookmark === "function" ||
    typeof db.batch !== "function"
  )
    throw new TypeError("createD1ScheduleStore requires a plain D1 Drizzle database binding");
  // Bare D1 binding reads stay on primary, including these migration shape probes.
  await db.select().from(schedules).limit(0);
  await db.select().from(scheduleOccurrences).limit(0);
  await db.select().from(queueBindings).limit(0);

  return {
    kind: "d1",
    async bind(scope, identity) {
      const [, rows] = await db.batch([
        db
          .insert(queueBindings)
          .values({
            ...scope,
            queueKind: identity.kind,
            queueId: identity.id,
          })
          .onConflictDoNothing(),
        db.select().from(queueBindings).where(scopeWhere(queueBindings, scope)).limit(1),
      ]);
      return rows[0]?.queueKind === identity.kind && rows[0].queueId === identity.id;
    },
    async create(scope, schedule) {
      await db.insert(schedules).values(scheduleValues(scope, schedule));
    },
    async get(scope, id) {
      const [row] = await db
        .select()
        .from(schedules)
        .where(and(scopeWhere(schedules, scope), eq(schedules.id, id)))
        .limit(1);
      return row ? asSchedule(row) : null;
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
      const token = crypto.randomUUID();
      const update = db
        .update(schedules)
        .set({
          revision: expected.revision + 1,
          nextAt,
          state: nextAt === null ? "completed" : expected.state,
          writeToken: token,
        })
        .where(
          and(
            scopeWhere(schedules, scope),
            eq(schedules.id, expected.id),
            eq(schedules.revision, expected.revision),
            eq(schedules.state, "active"),
            expected.nextAt === null
              ? isNull(schedules.nextAt)
              : eq(schedules.nextAt, expected.nextAt),
          ),
        )
        .returning({ id: schedules.id });
      const [rows] = occurrence
        ? await db.batch([
            update,
            db
              .insert(scheduleOccurrences)
              .select(gatedOccurrence(scope, expected.id, token, occurrence)),
          ])
        : await db.batch([update]);
      return rows.length === 1;
    },
    async trigger(scope, expected, occurrence) {
      const token = crypto.randomUUID();
      const [, , rows] = await db.batch([
        db
          .update(schedules)
          .set({ writeToken: token })
          .where(
            and(
              scopeWhere(schedules, scope),
              eq(schedules.id, expected.id),
              eq(schedules.revision, expected.revision),
              ne(schedules.state, "cancelled"),
            ),
          )
          .returning({ id: schedules.id }),
        db
          .insert(scheduleOccurrences)
          .select(gatedOccurrence(scope, expected.id, token, occurrence))
          .onConflictDoNothing(),
        db
          .select()
          .from(scheduleOccurrences)
          .where(
            and(
              scopeWhere(scheduleOccurrences, scope),
              eq(scheduleOccurrences.id, occurrence.id),
              exists(
                db
                  .select({ id: schedules.id })
                  .from(schedules)
                  .where(
                    and(
                      scopeWhere(schedules, scope),
                      eq(schedules.id, expected.id),
                      eq(schedules.writeToken, token),
                    ),
                  ),
              ),
            ),
          )
          .limit(1),
      ]);
      return rows[0] ? asOccurrence(rows[0]) : null;
    },
    async claim(scope, now, leaseMs) {
      const claimable = and(
        scopeWhere(scheduleOccurrences, scope),
        eq(scheduleOccurrences.state, "pending"),
        lte(scheduleOccurrences.scheduledAt, now),
        or(isNull(scheduleOccurrences.leaseUntil), lte(scheduleOccurrences.leaseUntil, now)),
      );
      const candidate = db
        .select({ id: scheduleOccurrences.id })
        .from(scheduleOccurrences)
        .where(claimable)
        .orderBy(asc(scheduleOccurrences.scheduledAt), asc(scheduleOccurrences.id))
        .limit(1);
      const rows = await db
        .update(scheduleOccurrences)
        .set({
          leaseToken: crypto.randomUUID(),
          leaseUntil: now + leaseMs,
        })
        .where(and(claimable, eq(scheduleOccurrences.id, candidate)))
        .returning();
      return rows[0] ? asOccurrence(rows[0]) : null;
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
