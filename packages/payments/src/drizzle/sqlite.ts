import { and, eq, lte } from "drizzle-orm";
import type { BunSQLiteDatabase } from "drizzle-orm/bun-sqlite";
import type { DrizzleD1Database } from "drizzle-orm/d1";
import { index, integer, sqliteTable, text } from "drizzle-orm/sqlite-core";
import type { PaymentRecord, PaymentsStore } from "../contracts";

const payments = sqliteTable(
  "lenso_payments",
  {
    paymentId: text("payment_id").primaryKey(),
    orderKey: text("order_key").notNull().unique(),
    accountId: text("account_id").notNull(),
    live: integer("live").notNull(),
    revision: integer("revision").notNull(),
    reconcileAt: integer("reconcile_at").notNull(),
    data: text("data", { mode: "json" }).$type<PaymentRecord>().notNull(),
  },
  (t) => [index("lenso_payments_due_idx").on(t.accountId, t.live, t.reconcileAt, t.paymentId)],
);
const inboxTable = sqliteTable(
  "lenso_payment_events",
  {
    eventKey: text("event_key").primaryKey(),
    eventId: text("event_id").notNull(),
    paymentId: text("payment_id").notNull(),
    objectId: text("object_id").notNull(),
    refundId: text("refund_id"),
    accountId: text("account_id").notNull(),
    live: integer("live").notNull(),
    createdAt: integer("created_at").notNull(),
    reconcileAt: integer("reconcile_at").notNull(),
    done: integer("done").notNull(),
  },
  (t) => [
    index("lenso_payment_events_due_idx").on(
      t.accountId,
      t.live,
      t.done,
      t.reconcileAt,
      t.eventKey,
    ),
  ],
);
type SqliteDb<T extends Record<string, unknown>> = BunSQLiteDatabase<T> | DrizzleD1Database<T>;
const copy = <T>(value: T): T => structuredClone(value);

export function sqlitePaymentsStore<T extends Record<string, unknown> = Record<string, unknown>>(
  db: SqliteDb<T>,
): PaymentsStore {
  return {
    async insert(r) {
      return (
        (
          await db
            .insert(payments)
            .values({
              paymentId: r.paymentId,
              orderKey: r.orderKey,
              accountId: r.accountId,
              live: +r.live,
              revision: r.revision,
              reconcileAt: r.reconcileAt,
              data: copy(r),
            })
            .onConflictDoNothing()
            .returning()
            .all()
        ).length > 0
      );
    },
    async get(id) {
      const [r] = await db.select().from(payments).where(eq(payments.paymentId, id)).limit(1).all();
      return r ? copy(r.data) : null;
    },
    async getByOrder(key) {
      const [r] = await db.select().from(payments).where(eq(payments.orderKey, key)).limit(1).all();
      return r ? copy(r.data) : null;
    },
    async compareAndSet(r, revision) {
      if (r.revision !== revision + 1) return false;
      return (
        (
          await db
            .update(payments)
            .set({
              revision: r.revision,
              reconcileAt: r.reconcileAt,
              data: copy(r),
            })
            .where(
              and(
                eq(payments.paymentId, r.paymentId),
                eq(payments.revision, revision),
                eq(payments.orderKey, r.orderKey),
                eq(payments.accountId, r.accountId),
                eq(payments.live, +r.live),
              ),
            )
            .returning()
            .all()
        ).length > 0
      );
    },
    async due(accountId, live, now, limit) {
      return (
        await db
          .select()
          .from(payments)
          .where(
            and(
              eq(payments.accountId, accountId),
              eq(payments.live, +live),
              lte(payments.reconcileAt, now),
            ),
          )
          .orderBy(payments.reconcileAt, payments.paymentId)
          .limit(Math.max(0, limit))
          .all()
      ).map((r) => copy(r.data));
    },
    async receive(e) {
      return (
        (
          await db
            .insert(inboxTable)
            .values({ ...copy(e), live: +e.live, done: +e.done })
            .onConflictDoNothing()
            .returning()
            .all()
        ).length > 0
      );
    },
    async inbox(accountId, live, now, limit) {
      return (
        await db
          .select()
          .from(inboxTable)
          .where(
            and(
              eq(inboxTable.accountId, accountId),
              eq(inboxTable.live, +live),
              eq(inboxTable.done, 0),
              lte(inboxTable.reconcileAt, now),
            ),
          )
          .orderBy(inboxTable.reconcileAt, inboxTable.eventKey)
          .limit(Math.max(0, limit))
          .all()
      ).map((r) => ({ ...r, live: !!r.live, done: !!r.done }));
    },
    async finishEvent(eventKey) {
      await db.update(inboxTable).set({ done: 1 }).where(eq(inboxTable.eventKey, eventKey));
    },
    async deferEvent(eventKey, reconcileAt) {
      await db.update(inboxTable).set({ reconcileAt }).where(eq(inboxTable.eventKey, eventKey));
    },
  };
}
