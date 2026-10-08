import { expect, test } from "bun:test";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";
import { d1PaymentsStore } from "../src/drizzle/d1";
import type { PaymentRecord } from "../src/contracts";

test("local D1 executes migrations, unique reservations, aggregate CAS and durable inbox", async () => {
  const worker = new Miniflare(
    convertV4MiniflareOptions({
      modules: true,
      script: "export default { fetch() { return new Response('fixture'); } };",
      compatibilityDate: "2026-10-08",
      d1Databases: ["DB"],
    }),
  );
  try {
    const binding = await worker.getD1Database("DB");
    const migration = await Bun.file(new URL("../migrations/sqlite.sql", import.meta.url)).text();
    await binding.batch(
      migration
        .split(";")
        .map((sql) => sql.trim())
        .filter(Boolean)
        .map((sql) => binding.prepare(sql)),
    );
    const store = d1PaymentsStore(binding);
    const other = d1PaymentsStore(binding);
    const record: PaymentRecord = {
      paymentId: "d1-fixture",
      tenantId: "tenant",
      orderId: "order",
      orderKey: "d1-order",
      key: "d1-key",
      accountId: "acct_fixture",
      live: false,
      amount: 1000,
      currency: "usd",
      status: "unknown",
      providerId: null,
      attemptedAt: null,
      lease: null,
      revision: 0,
      refunds: [],
      results: [],
      createdAt: 1,
      updatedAt: 1,
      reconcileAt: 1,
    };
    expect(await store.insert(record)).toBe(true);
    expect(await other.insert({ ...record, paymentId: "same-order" })).toBe(false);
    expect(await other.get(record.paymentId)).toEqual(record);
    const next = { ...record, revision: 1, reconcileAt: 20 };
    const writes = await Promise.all([store.compareAndSet(next, 0), other.compareAndSet(next, 0)]);
    expect(writes.filter(Boolean)).toHaveLength(1);
    expect(await other.due(record.accountId, false, 10, 10)).toEqual([]);
    const event = {
      eventKey: "d1-event",
      eventId: "evt",
      paymentId: record.paymentId,
      objectId: "pi_fixture",
      refundId: null,
      accountId: record.accountId,
      live: false,
      createdAt: 2,
      reconcileAt: 2,
      done: false,
    };
    expect(await store.receive(event)).toBe(true);
    expect(await other.receive(event)).toBe(false);
    expect(await other.inbox(record.accountId, false, 10, 10)).toEqual([event]);
    await other.finishEvent(event.eventKey);
    expect(await store.inbox(record.accountId, false, 10, 10)).toEqual([]);
    expect(() =>
      d1PaymentsStore(binding.withSession("first-primary") as unknown as typeof binding),
    ).toThrow("Payments: invalid-input");
  } finally {
    await worker.dispose();
  }
});
