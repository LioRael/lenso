import { and, eq, lte, sql } from "drizzle-orm";
import { check, index, integer, primaryKey, sqliteTable, text } from "drizzle-orm/sqlite-core";
import type { DrizzleD1Database } from "drizzle-orm/d1";
import {
  LimitError,
  maxDurationMs,
  positiveInteger,
  scopeKey,
  validateAcquire,
  validateConsume,
  validateKind,
  validateToken,
  type CounterKind,
  type Lease,
  type LimitScope,
  type LimitStore,
} from "./contracts";
import { counterDecision, leaseDecision } from "./state";

export const d1Counters = sqliteTable(
  "lenso_d1_limit_counters",
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
    primaryKey({ columns: [table.scope, table.kind] }),
    check("lenso_d1_counter_kind", sql`${table.kind} IN ('rate', 'quota')`),
    check("lenso_d1_counter_capacity", sql`${table.capacity} BETWEEN 1 AND 2147483647`),
    check("lenso_d1_counter_period", sql`${table.periodMs} BETWEEN 1 AND 31622400000`),
    check("lenso_d1_counter_used", sql`${table.used} BETWEEN 0 AND ${table.capacity}`),
    check("lenso_d1_counter_clock", sql`${table.windowStart} >= 0 AND ${table.lastNow} >= 0`),
  ],
);

export const d1Concurrency = sqliteTable(
  "lenso_d1_limit_concurrency",
  {
    scope: text("scope").primaryKey().notNull(),
    capacity: integer("capacity").notNull(),
    lastNow: integer("last_now").notNull(),
  },
  (table) => [
    check("lenso_d1_lease_capacity", sql`${table.capacity} BETWEEN 1 AND 2147483647`),
    check("lenso_d1_lease_clock", sql`${table.lastNow} >= 0`),
  ],
);

export const d1Leases = sqliteTable(
  "lenso_d1_limit_leases",
  {
    scope: text("scope")
      .notNull()
      .references(() => d1Concurrency.scope),
    token: text("token").notNull(),
    quantity: integer("quantity").notNull(),
    expiresAt: integer("expires_at").notNull(),
  },
  (table) => [
    primaryKey({ columns: [table.scope, table.token] }),
    index("lenso_d1_lease_expiry").on(table.scope, table.expiresAt),
    check("lenso_d1_lease_quantity", sql`${table.quantity} BETWEEN 1 AND 2147483647`),
    check("lenso_d1_lease_expiry_check", sql`${table.expiresAt} >= 0`),
  ],
);

export const d1LimitSchema = { d1Counters, d1Concurrency, d1Leases };
const databaseNow = sql<number>`CAST(round((julianday('now') - 2440587.5) * 86400000) AS INTEGER)`;

function presentLease(
  scope: LimitScope,
  row: Pick<Lease, "token" | "quantity" | "expiresAt">,
): Lease {
  return Object.freeze({ ...row, scope: Object.freeze({ ...scope }) });
}

/** Native D1 batch transactions, never Drizzle's interactive transaction API or replica reads. */
export function createD1LimitStore<TSchema extends Record<string, unknown>>(
  db: DrizzleD1Database<TSchema>,
): LimitStore {
  const bucketTime = (key: string, capacity?: number) => sql<number>`(
    SELECT ${d1Concurrency.lastNow} FROM ${d1Concurrency}
    WHERE ${and(eq(d1Concurrency.scope, key), capacity === undefined ? undefined : eq(d1Concurrency.capacity, capacity))}
  )`;
  const prune = (key: string, capacity?: number) =>
    db
      .delete(d1Leases)
      .where(and(eq(d1Leases.scope, key), lte(d1Leases.expiresAt, bucketTime(key, capacity))));
  const advanceClock = (key: string) =>
    db
      .update(d1Concurrency)
      .set({ lastNow: sql`max(${d1Concurrency.lastNow}, ${databaseNow})` })
      .where(eq(d1Concurrency.scope, key));

  return {
    async consume(kind, input) {
      validateKind(kind);
      validateConsume(input);
      const key = scopeKey(input.scope);
      const samePolicy = and(
        eq(d1Counters.capacity, input.capacity),
        eq(d1Counters.periodMs, input.periodMs),
      );
      const scopePredicate = and(eq(d1Counters.scope, key), eq(d1Counters.kind, kind));
      const effectiveNow = sql<number>`max(${d1Counters.lastNow}, ${databaseNow})`;
      const nextWindow = sql<number>`(${effectiveNow} / ${d1Counters.periodMs}) * ${d1Counters.periodMs}`;
      const [, admitted, snapshot] = await db.batch([
        db
          .insert(d1Counters)
          .values({
            scope: key,
            kind,
            capacity: input.capacity,
            periodMs: input.periodMs,
            // D1 binds JS numbers as REAL; force integer division for epoch-aligned windows.
            windowStart: sql`(${databaseNow} / CAST(${input.periodMs} AS INTEGER)) * CAST(${input.periodMs} AS INTEGER)`,
            used: 0,
            lastNow: databaseNow,
          })
          .onConflictDoUpdate({
            target: [d1Counters.scope, d1Counters.kind],
            set: {
              lastNow: effectiveNow,
              windowStart: nextWindow,
              used: sql`CASE WHEN ${d1Counters.windowStart} = ${nextWindow} THEN ${d1Counters.used} ELSE 0 END`,
            },
            setWhere: samePolicy,
          }),
        db
          .update(d1Counters)
          .set({ used: sql`${d1Counters.used} + ${input.quantity}` })
          .where(
            and(
              scopePredicate,
              samePolicy,
              sql`${input.quantity} <= ${d1Counters.capacity} - ${d1Counters.used}`,
            ),
          )
          .returning(),
        db.select().from(d1Counters).where(scopePredicate),
      ]);
      const state = snapshot[0];
      if (!state) throw new LimitError("backend-failure");
      if (state.capacity !== input.capacity || state.periodMs !== input.periodMs)
        throw new LimitError("policy-conflict");
      return counterDecision(state, input.quantity, admitted.length === 1);
    },
    async acquire(input, token) {
      validateAcquire(input);
      validateToken(token);
      const key = scopeKey(input.scope);
      const used = sql<number>`(
        SELECT coalesce(sum(${d1Leases.quantity}), 0) FROM ${d1Leases}
        WHERE ${d1Leases.scope} = ${key}
      )`;
      // Admission is SQL INSERT ... SELECT under the same primary transaction as pruning.
      const [, , admitted, headers, holders] = await db.batch([
        db
          .insert(d1Concurrency)
          .values({ scope: key, capacity: input.capacity, lastNow: databaseNow })
          .onConflictDoUpdate({
            target: d1Concurrency.scope,
            set: { lastNow: sql`max(${d1Concurrency.lastNow}, ${databaseNow})` },
            setWhere: eq(d1Concurrency.capacity, input.capacity),
          }),
        prune(key, input.capacity),
        db
          .insert(d1Leases)
          .select(
            db
              .select({
                scope: sql<string>`${key}`.as("scope"),
                token: sql<string>`${token}`.as("token"),
                quantity: sql<number>`${input.quantity}`.as("quantity"),
                expiresAt: sql<number>`${d1Concurrency.lastNow} + ${input.ttlMs}`.as("expires_at"),
              })
              .from(d1Concurrency)
              .where(
                and(
                  eq(d1Concurrency.scope, key),
                  eq(d1Concurrency.capacity, input.capacity),
                  sql`${input.quantity} <= ${d1Concurrency.capacity} - ${used}`,
                ),
              ),
          )
          .returning(),
        db.select().from(d1Concurrency).where(eq(d1Concurrency.scope, key)),
        db.select().from(d1Leases).where(eq(d1Leases.scope, key)),
      ]);
      const header = headers[0];
      if (!header) throw new LimitError("backend-failure");
      if (header.capacity !== input.capacity) throw new LimitError("policy-conflict");
      const leases = holders.map((row) => presentLease(input.scope, row));
      return {
        ...leaseDecision({ ...header, leases }, input.quantity, admitted.length === 1),
        lease: admitted[0] ? presentLease(input.scope, admitted[0]) : null,
      };
    },
    async renew(scope, token, ttlMs) {
      const key = scopeKey(scope);
      validateToken(token);
      positiveInteger(ttlMs, maxDurationMs);
      const [, , rows] = await db.batch([
        advanceClock(key),
        prune(key),
        db
          .update(d1Leases)
          .set({ expiresAt: sql`max(${d1Leases.expiresAt}, ${bucketTime(key)} + ${ttlMs})` })
          .where(and(eq(d1Leases.scope, key), eq(d1Leases.token, token)))
          .returning(),
      ]);
      return rows[0] ? presentLease(scope, rows[0]) : null;
    },
    async release(scope, token) {
      const key = scopeKey(scope);
      validateToken(token);
      await db.batch([
        advanceClock(key),
        prune(key),
        db.delete(d1Leases).where(and(eq(d1Leases.scope, key), eq(d1Leases.token, token))),
      ]);
    },
  };
}
