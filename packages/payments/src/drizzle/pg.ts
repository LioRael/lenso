import { and, eq, lte, sql } from "drizzle-orm";
import type { PgDatabase, PgQueryResultHKT } from "drizzle-orm/pg-core";
import { bigint, index, integer, jsonb, pgTable, text } from "drizzle-orm/pg-core";
import type { PaymentRecord, PaymentsStore } from "../contracts";

const payments = pgTable(
  "lenso_payments",
  {
    paymentId: text("payment_id").primaryKey(),
    orderKey: text("order_key").notNull().unique(),
    accountId: text("account_id").notNull(),
    live: integer("live").notNull(),
    revision: integer("revision").notNull(),
    reconcileAt: bigint("reconcile_at", { mode: "number" }).notNull(),
    data: jsonb("data").$type<PaymentRecord>().notNull(),
  },
  (t) => [index("lenso_payments_due_idx").on(t.accountId, t.live, t.reconcileAt, t.paymentId)],
);
const inboxTable = pgTable(
  "lenso_payment_events",
  {
    eventKey: text("event_key").primaryKey(),
    eventId: text("event_id").notNull(),
    paymentId: text("payment_id").notNull(),
    objectId: text("object_id").notNull(),
    refundId: text("refund_id"),
    accountId: text("account_id").notNull(),
    live: integer("live").notNull(),
    createdAt: bigint("created_at", { mode: "number" }).notNull(),
    reconcileAt: bigint("reconcile_at", { mode: "number" }).notNull(),
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
const copy = <T>(value: T): T => structuredClone(value);

export function postgresPaymentsStore<
  TSchema extends Record<string, unknown> = Record<string, unknown>,
  TResult extends PgQueryResultHKT = PgQueryResultHKT,
>(db: PgDatabase<TResult, TSchema>): PaymentsStore {
  return {
    async insert(record) {
      const rows = await db
        .insert(payments)
        .values({
          paymentId: record.paymentId,
          orderKey: record.orderKey,
          accountId: record.accountId,
          live: Number(record.live),
          revision: record.revision,
          reconcileAt: record.reconcileAt,
          data: sql`${JSON.stringify(record)}::text::jsonb`,
        })
        .onConflictDoNothing()
        .returning();
      return rows.length > 0;
    },
    async get(paymentId) {
      const [row] = await db
        .select({ data: payments.data })
        .from(payments)
        .where(eq(payments.paymentId, paymentId))
        .limit(1);
      return row ? copy(row.data) : null;
    },
    async getByOrder(orderKey) {
      const [row] = await db
        .select({ data: payments.data })
        .from(payments)
        .where(eq(payments.orderKey, orderKey))
        .limit(1);
      return row ? copy(row.data) : null;
    },
    async compareAndSet(record, revision) {
      if (record.revision !== revision + 1) return false;
      const rows = await db
        .update(payments)
        .set({
          revision: record.revision,
          reconcileAt: record.reconcileAt,
          data: sql`${JSON.stringify(record)}::text::jsonb`,
        })
        .where(
          and(
            eq(payments.paymentId, record.paymentId),
            eq(payments.revision, revision),
            eq(payments.orderKey, record.orderKey),
            eq(payments.accountId, record.accountId),
            eq(payments.live, Number(record.live)),
          ),
        )
        .returning();
      return rows.length > 0;
    },
    async due(accountId, live, now, limit) {
      const rows = await db
        .select({ data: payments.data })
        .from(payments)
        .where(
          and(
            eq(payments.accountId, accountId),
            eq(payments.live, Number(live)),
            lte(payments.reconcileAt, now),
          ),
        )
        .orderBy(payments.reconcileAt, payments.paymentId)
        .limit(Math.max(0, limit));
      return rows.map(({ data }) => copy(data));
    },
    async receive(event) {
      const rows = await db
        .insert(inboxTable)
        .values({ ...copy(event), live: Number(event.live), done: Number(event.done) })
        .onConflictDoNothing()
        .returning();
      return rows.length > 0;
    },
    async inbox(accountId, live, now, limit) {
      return (
        await db
          .select()
          .from(inboxTable)
          .where(
            and(
              eq(inboxTable.accountId, accountId),
              eq(inboxTable.live, Number(live)),
              eq(inboxTable.done, 0),
              lte(inboxTable.reconcileAt, now),
            ),
          )
          .orderBy(inboxTable.reconcileAt, inboxTable.eventKey)
          .limit(Math.max(0, limit))
      ).map((r) => ({ ...r, live: Boolean(r.live), done: Boolean(r.done) }));
    },
    async finishEvent(eventKey) {
      await db.update(inboxTable).set({ done: 1 }).where(eq(inboxTable.eventKey, eventKey));
    },
    async deferEvent(eventKey, reconcileAt) {
      await db.update(inboxTable).set({ reconcileAt }).where(eq(inboxTable.eventKey, eventKey));
    },
  };
}
