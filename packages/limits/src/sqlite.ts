import { and, eq, sql } from "drizzle-orm";
import { check, integer, primaryKey, sqliteTable, text } from "drizzle-orm/sqlite-core";
import type { BunSQLiteDatabase } from "drizzle-orm/bun-sqlite";
import {
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

export const counters = sqliteTable(
  "lenso_limit_counters",
  {
    scope: text("scope").notNull(),
    kind: text("kind").$type<CounterKind>().notNull(),
    capacity: integer("capacity").notNull(),
    periodMs: integer("period_ms").notNull(),
    windowStart: integer("window_start").notNull(),
    used: integer("used").notNull(),
    lastNow: integer("last_now").notNull(),
  },
  (table) => [
    primaryKey({ columns: [table.scope, table.kind], name: "lenso_limit_counters_pk" }),
    check("lenso_limit_counter_kind", sql`${table.kind} IN ('rate', 'quota')`),
    check("lenso_limit_counter_capacity", sql`${table.capacity} BETWEEN 1 AND 2147483647`),
    check("lenso_limit_counter_period", sql`${table.periodMs} BETWEEN 1 AND 31622400000`),
    check("lenso_limit_counter_used", sql`${table.used} BETWEEN 0 AND ${table.capacity}`),
    check("lenso_limit_counter_clock", sql`${table.windowStart} >= 0 AND ${table.lastNow} >= 0`),
  ],
);

export const concurrencyBuckets = sqliteTable(
  "lenso_limit_concurrency",
  {
    scope: text("scope").primaryKey().notNull(),
    capacity: integer("capacity").notNull(),
    lastNow: integer("last_now").notNull(),
    leases: text("leases", { mode: "json" }).$type<Lease[]>().notNull(),
  },
  (table) => [
    check("lenso_limit_lease_capacity", sql`${table.capacity} BETWEEN 1 AND 2147483647`),
    check("lenso_limit_lease_clock", sql`${table.lastNow} >= 0`),
  ],
);

export const limitSchema = { counters, concurrencyBuckets };
const sqliteNow = sql<number>`CAST(round((julianday('now') - 2440587.5) * 86400000) AS INTEGER)`;

/** Bun SQLite only. The caller owns the database, busy handling, and explicit migrations. */
export function createSqliteLimitStore<TSchema extends Record<string, unknown>>(
  db: BunSQLiteDatabase<TSchema>,
): LimitStore {
  return {
    async consume(kind, input) {
      validateKind(kind);
      validateConsume(input);
      const key = scopeKey(input.scope);
      // Drizzle invokes this synchronous callback after BEGIN IMMEDIATE obtains the writer lock.
      return db.transaction(
        (tx) => {
          const now = tx.values<[number]>(sql`SELECT ${sqliteNow}`)[0]![0];
          const previous = tx
            .select()
            .from(counters)
            .where(and(eq(counters.scope, key), eq(counters.kind, kind)))
            .get();
          const { state, result } = consumeState(previous, input, now);
          tx.insert(counters)
            .values({ scope: key, kind, ...state })
            .onConflictDoUpdate({ target: [counters.scope, counters.kind], set: state })
            .run();
          return result;
        },
        { behavior: "immediate" },
      );
    },
    async acquire(input, token) {
      validateAcquire(input);
      validateToken(token);
      const key = scopeKey(input.scope);
      return db.transaction(
        (tx) => {
          const now = tx.values<[number]>(sql`SELECT ${sqliteNow}`)[0]![0];
          const previous = tx
            .select()
            .from(concurrencyBuckets)
            .where(eq(concurrencyBuckets.scope, key))
            .get();
          const state = liveState(previous, input.capacity, now);
          const result = acquireState(state, input, token);
          tx.insert(concurrencyBuckets)
            .values({ scope: key, ...state })
            .onConflictDoUpdate({ target: concurrencyBuckets.scope, set: state })
            .run();
          return result;
        },
        { behavior: "immediate" },
      );
    },
    async renew(scope, token, ttlMs) {
      const key = scopeKey(scope);
      validateToken(token);
      positiveInteger(ttlMs, maxDurationMs);
      return db.transaction(
        (tx) => {
          const now = tx.values<[number]>(sql`SELECT ${sqliteNow}`)[0]![0];
          const previous = tx
            .select()
            .from(concurrencyBuckets)
            .where(eq(concurrencyBuckets.scope, key))
            .get();
          if (!previous) return null;
          const state = liveState(previous, previous.capacity, now);
          const lease = renewState(state, scope, token, ttlMs);
          tx.update(concurrencyBuckets).set(state).where(eq(concurrencyBuckets.scope, key)).run();
          return lease;
        },
        { behavior: "immediate" },
      );
    },
    async release(scope, token) {
      const key = scopeKey(scope);
      validateToken(token);
      db.transaction(
        (tx) => {
          const now = tx.values<[number]>(sql`SELECT ${sqliteNow}`)[0]![0];
          const previous = tx
            .select()
            .from(concurrencyBuckets)
            .where(eq(concurrencyBuckets.scope, key))
            .get();
          if (!previous) return;
          const state = liveState(previous, previous.capacity, now);
          state.leases = state.leases.filter((lease) => lease.token !== token);
          tx.update(concurrencyBuckets).set(state).where(eq(concurrencyBuckets.scope, key)).run();
        },
        { behavior: "immediate" },
      );
    },
  };
}
