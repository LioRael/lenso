import { Database } from "bun:sqlite";
import { drizzle } from "drizzle-orm/bun-sqlite";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, test } from "bun:test";
import { sqlitePaymentsStore } from "../src/drizzle/sqlite";
import type { PaymentRecord } from "../src/contracts";

const migration = readFileSync(new URL("../migrations/sqlite.sql", import.meta.url), "utf8");
const opened = new Set<Database>();
const directories: string[] = [];
afterEach(() => {
  for (const db of opened) db.close();
  opened.clear();
  for (const directory of directories.splice(0))
    rmSync(directory, { recursive: true, force: true });
});
function record(paymentId: string, orderKey: string, reconcileAt = 10): PaymentRecord {
  return {
    paymentId,
    tenantId: "tenant",
    orderId: "order",
    accountId: "acct",
    live: true,
    amount: 500,
    currency: "usd",
    key: `k-${paymentId}`,
    orderKey,
    status: "unknown",
    providerId: null,
    revision: 0,
    attemptedAt: null,
    lease: null,
    refunds: [],
    results: [],
    createdAt: 1,
    updatedAt: 1,
    reconcileAt,
  };
}

describe("SQLite PaymentsStore", () => {
  test("durable aggregate, atomic CAS across connections, uniqueness, inbox and scoped ordered scan", async () => {
    const directory = mkdtempSync(join(tmpdir(), "payments-store-"));
    directories.push(directory);
    const path = join(directory, "payments.sqlite");
    const first = new Database(path);
    opened.add(first);
    first.exec(migration);
    const db1 = drizzle(first);
    const second = new Database(path);
    opened.add(second);
    const db2 = drizzle(second);
    const store = sqlitePaymentsStore(db1);
    const other = sqlitePaymentsStore(db2);
    const a = record("p-a", "o-a");
    expect(await store.insert(a)).toBe(true);
    expect(await store.insert(record("p-b", "o-a"))).toBe(false);
    expect(await store.insert(record("p-c", "o-c", 5))).toBe(true);
    const changed = {
      ...a,
      revision: 1,
      results: [
        {
          paymentId: a.paymentId,
          tenantId: a.tenantId,
          orderId: a.orderId,
          accountId: a.accountId,
          live: a.live,
          amount: a.amount,
          currency: a.currency,
          resultId: "r1",
          kind: "payment.succeeded" as const,
        },
      ],
    };
    const writes = await Promise.all([
      store.compareAndSet(changed, 0),
      other.compareAndSet({ ...changed, updatedAt: 2 }, 0),
    ]);
    expect(writes.filter(Boolean)).toHaveLength(1);
    expect((await other.get("p-a"))?.results).toHaveLength(1);
    expect(await other.due("acct", true, 20, 1)).toHaveLength(1);
    expect(await other.due("another-account", true, 20, 10)).toEqual([]);
    expect(await other.due("acct", false, 20, 10)).toEqual([]);
    const event = {
      eventKey: "evt-key",
      eventId: "evt",
      paymentId: "p-a",
      objectId: "pi-a",
      refundId: null,
      accountId: "acct",
      live: true,
      createdAt: 2,
      reconcileAt: 3,
      done: false,
    };
    expect(await store.receive(event)).toBe(true);
    expect(await other.receive(event)).toBe(false);
    expect((await other.inbox("acct", true, 10, 5))[0]?.eventKey).toBe("evt-key");
    await other.deferEvent("evt-key", 20);
    expect(await store.inbox("acct", true, 10, 5)).toHaveLength(0);
    await store.finishEvent("evt-key");
    expect(await other.inbox("acct", true, 30, 5)).toHaveLength(0);
    first.close();
    opened.delete(first);
    second.close();
    opened.delete(second);
    const restarted = new Database(path);
    opened.add(restarted);
    const persisted = await sqlitePaymentsStore(drizzle(restarted)).get("p-a");
    expect(persisted?.revision).toBe(1);
    expect(persisted?.results).toHaveLength(1);
  });
});
