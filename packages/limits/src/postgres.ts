import { and, eq, sql } from "drizzle-orm";
import type { PgDatabase, PgQueryResultHKT } from "drizzle-orm/pg-core";
import { check, jsonb, pgTable, primaryKey, text, bigint } from "drizzle-orm/pg-core";
import {
  LimitError,
  maxDurationMs,
  scopeKey,
  validateAcquire,
  validateConsume,
  validateKind,
  validateToken,
  positiveInteger,
  type CounterKind,
  type LimitStore,
  type Lease,
} from "./contracts";
import { acquireState, consumeState, liveState, renewState } from "./state";

const counters = pgTable(
  "lenso_limit_counters",
  {
    scope: text("scope").notNull(),
    kind: text("kind").$type<CounterKind>().notNull(),
    capacity: bigint("capacity", { mode: "number" }).notNull(),
    periodMs: bigint("period_ms", { mode: "number" }).notNull(),
    windowStart: bigint("window_start", { mode: "number" }).notNull(),
    used: bigint("used", { mode: "number" }).notNull(),
    lastNow: bigint("last_now", { mode: "number" }).notNull(),
  },
  (t) => [
    primaryKey({ columns: [t.scope, t.kind], name: "lenso_limit_counters_pk" }),
    check("lenso_limit_counter_kind", sql`${t.kind} IN ('rate', 'quota')`),
    check("lenso_limit_counter_capacity", sql`${t.capacity} BETWEEN 1 AND 2147483647`),
    check("lenso_limit_counter_period", sql`${t.periodMs} BETWEEN 1 AND 31622400000`),
    check("lenso_limit_counter_used", sql`${t.used} BETWEEN 0 AND ${t.capacity}`),
    check("lenso_limit_counter_clock", sql`${t.windowStart} >= 0 AND ${t.lastNow} >= 0`),
  ],
);

const concurrencyBuckets = pgTable(
  "lenso_limit_concurrency",
  {
    scope: text("scope").primaryKey().notNull(),
    capacity: bigint("capacity", { mode: "number" }).notNull(),
    lastNow: bigint("last_now", { mode: "number" }).notNull(),
    leases: jsonb("leases").$type<Lease[]>().notNull(),
  },
  (t) => [
    check("lenso_limit_lease_capacity", sql`${t.capacity} BETWEEN 1 AND 2147483647`),
    check("lenso_limit_lease_clock", sql`${t.lastNow} >= 0`),
  ],
);

export const postgresLimitSchema = { counters, concurrencyBuckets };
const dbNow = sql<number>`floor(extract(epoch from clock_timestamp()) * 1000)::bigint`;
const serializedJson = (value: Lease[]) => sql`${JSON.stringify(value)}::text::jsonb`;

/** Borrows the database; schema creation and connection lifetime belong to the caller. */
export function createPostgresLimitStore<
  TResult extends PgQueryResultHKT = PgQueryResultHKT,
  TSchema extends Record<string, unknown> = Record<string, unknown>,
>(db: PgDatabase<TResult, TSchema>): LimitStore {
  async function now(tx: PgDatabase<TResult, TSchema>) {
    const [row] = await tx.select({ now: dbNow }).from(sql`(SELECT 1) AS clock_source`);
    return Number(row!.now);
  }
  return {
    async consume(kind, input) {
      validateKind(kind);
      validateConsume(input);
      const key = scopeKey(input.scope);
      return db.transaction(async (tx) => {
        await tx
          .insert(counters)
          .values({
            scope: key,
            kind,
            capacity: input.capacity,
            periodMs: input.periodMs,
            windowStart: 0,
            used: 0,
            lastNow: 0,
          })
          .onConflictDoNothing();
        const [previous] = await tx
          .select()
          .from(counters)
          .where(and(eq(counters.scope, key), eq(counters.kind, kind)))
          .for("update");
        if (!previous) throw new LimitError("backend-failure");
        // A new statement after the row lock excludes time spent waiting for another holder.
        const clock = await now(tx);
        const { state, result } = consumeState(previous, input, clock);
        await tx
          .update(counters)
          .set(state)
          .where(and(eq(counters.scope, key), eq(counters.kind, kind)));
        return result;
      });
    },
    async acquire(input, token) {
      validateAcquire(input);
      validateToken(token);
      const key = scopeKey(input.scope);
      return db.transaction(async (tx) => {
        await tx
          .insert(concurrencyBuckets)
          .values({
            scope: key,
            capacity: input.capacity,
            lastNow: 0,
            leases: serializedJson([]),
          })
          .onConflictDoNothing();
        const [previous] = await tx
          .select()
          .from(concurrencyBuckets)
          .where(eq(concurrencyBuckets.scope, key))
          .for("update");
        if (!previous) throw new LimitError("backend-failure");
        const clock = await now(tx);
        const state = liveState(previous, input.capacity, clock);
        const result = acquireState(state, input, token);
        await tx
          .update(concurrencyBuckets)
          .set({ ...state, leases: serializedJson(state.leases) })
          .where(eq(concurrencyBuckets.scope, key));
        return result;
      });
    },
    async renew(scope, token, ttlMs) {
      const key = scopeKey(scope);
      validateToken(token);
      positiveInteger(ttlMs, maxDurationMs);
      return db.transaction(async (tx) => {
        const [previous] = await tx
          .select()
          .from(concurrencyBuckets)
          .where(eq(concurrencyBuckets.scope, key))
          .for("update");
        if (!previous) return null;
        const state = liveState(previous, previous.capacity, await now(tx));
        const lease = renewState(state, scope, token, ttlMs);
        await tx
          .update(concurrencyBuckets)
          .set({ ...state, leases: serializedJson(state.leases) })
          .where(eq(concurrencyBuckets.scope, key));
        return lease;
      });
    },
    async release(scope, token) {
      const key = scopeKey(scope);
      validateToken(token);
      await db.transaction(async (tx) => {
        const [previous] = await tx
          .select()
          .from(concurrencyBuckets)
          .where(eq(concurrencyBuckets.scope, key))
          .for("update");
        if (!previous) return;
        const state = liveState(previous, previous.capacity, await now(tx));
        state.leases = state.leases.filter((lease) => lease.token !== token);
        await tx
          .update(concurrencyBuckets)
          .set({ ...state, leases: serializedJson(state.leases) })
          .where(eq(concurrencyBuckets.scope, key));
      });
    },
  };
}
