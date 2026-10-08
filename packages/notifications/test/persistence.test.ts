import { afterEach, describe, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { drizzle } from "drizzle-orm/bun-sqlite";
import { drizzle as d1Drizzle } from "drizzle-orm/d1";
import type { D1Database } from "@cloudflare/workers-types";
import type { DeliveryAttempt, NotificationRecord, NotificationStore } from "../src/contracts";
import { createSqliteNotificationStore } from "../src/sqlite";
import { createD1NotificationStore } from "../src/d1";

const migration = await Bun.file(
  new URL("../migrations/sqlite/0001_notifications.sql", import.meta.url),
).text();
const clients: Database[] = [];
afterEach(() => {
  for (const client of clients.splice(0)) client.close();
});

function record(overrides: Partial<NotificationRecord> = {}): NotificationRecord {
  return {
    id: crypto.randomUUID(),
    tenantId: "tenant",
    scope: "invoice",
    idempotencyKey: "key",
    fingerprint: "fingerprint",
    businessId: "invoice-1",
    recipientId: "recipient",
    templateId: "invoice",
    templateVersion: "1",
    category: "billing",
    necessity: "required",
    channelId: "email",
    message: {
      from: "from@test",
      to: "to@test",
      subject: "Private",
      text: "Secret",
      html: "<p>Secret</p>",
    },
    state: "pending",
    revision: 0,
    attemptCount: 0,
    firstRequestAt: null,
    leaseUntil: null,
    taskJobId: null,
    providerMessageId: null,
    error: null,
    retryable: false,
    createdAt: 1,
    updatedAt: 1,
    ...overrides,
  };
}

function attempt(
  notification: NotificationRecord,
  overrides: Partial<DeliveryAttempt> = {},
): DeliveryAttempt {
  return {
    id: crypto.randomUUID(),
    notificationId: notification.id,
    number: notification.attemptCount,
    state: "sending",
    startedAt: 10,
    finishedAt: null,
    providerMessageId: null,
    error: null,
    ...overrides,
  };
}

function database() {
  const client = new Database(":memory:");
  clients.push(client);
  client.exec("PRAGMA foreign_keys = ON");
  client.exec(migration);
  return client;
}

// Exercises Drizzle's actual D1 prepared-query/batch API over SQLite, not a real D1 backend.
function fixtureBinding(client: Database): D1Database {
  class Statement {
    constructor(
      readonly query: string,
      readonly parameters: unknown[] = [],
    ) {}
    bind(...parameters: unknown[]) {
      return new Statement(this.query, parameters);
    }
    async all() {
      const results = client.query(this.query).all(...(this.parameters as never[]));
      return { success: true, results, meta: {} };
    }
    async raw() {
      return client.query(this.query).values(...(this.parameters as never[]));
    }
    async run() {
      const result = client.query(this.query).run(...(this.parameters as never[]));
      return { success: true, results: [], meta: { changes: result.changes } };
    }
  }
  return {
    prepare(query: string) {
      return new Statement(query);
    },
    async batch(statements: Statement[]) {
      // Match D1 batch atomicity, including rollback on a later attempt constraint error.
      return client.transaction(() =>
        statements.map((statement) => {
          const results = client.query(statement.query).all(...(statement.parameters as never[]));
          return { success: true, results, meta: {} };
        }),
      )();
    },
  } as unknown as D1Database;
}

for (const backend of ["Bun SQLite", "D1 batch fixture (not real D1)"] as const) {
  describe(backend, () => {
    function setup(): { client: Database; store: NotificationStore } {
      const client = database();
      return {
        client,
        store:
          backend === "Bun SQLite"
            ? createSqliteNotificationStore(drizzle(client))
            : createD1NotificationStore(d1Drizzle(fixtureBinding(client))),
      };
    }

    test("explicit migration, concurrent insertOrGet, and private immutable snapshot", async () => {
      const { store, client } = setup();
      const input = record();
      const winners = await Promise.all(
        Array.from({ length: 20 }, () => store.insertOrGet({ ...input, id: crypto.randomUUID() })),
      );
      expect(new Set(winners.map((winner) => winner.id)).size).toBe(1);
      expect(client.query("SELECT count(*) AS n FROM lenso_notifications").get()).toEqual({ n: 1 });
      const winner = winners[0]!;
      winner.message = { ...winner.message, text: "mutated outside store" };
      const original = (await store.get(winner.id))!;
      expect(original.message.text).toBe("Secret");
      expect(
        await store.save(
          { ...original, revision: 1, message: { ...original.message, text: "rewrite" } },
          0,
        ),
      ).toBe(true);
      expect((await store.get(original.id))!.message.text).toBe("Secret");
      expect(await store.findKey("other-tenant", input.scope, input.idempotencyKey)).toBeNull();
      expect(client.query("SELECT 1 AS open").get()).toEqual({ open: 1 });
    });

    test("competing CAS saves commit one attempt and stale writes cannot insert attempts", async () => {
      const { store } = setup();
      const original = await store.insertOrGet(record());
      const next = { ...original, revision: 1, attemptCount: 1, state: "sending" as const };
      const attempts = Array.from({ length: 10 }, () => attempt(next));
      const results = await Promise.all(attempts.map((a) => store.save(next, 0, a)));
      expect(results.filter(Boolean)).toHaveLength(1);
      expect(await store.attempts(next.id)).toHaveLength(1);
      expect((await store.get(next.id))!.revision).toBe(1);
      expect((await store.get(next.id))!.attemptCount).toBe(1);
      const current = (await store.attempts(next.id))[0]!;
      expect(
        await store.save({ ...next, revision: 2, state: "accepted" }, 1, {
          ...current,
          state: "accepted",
          finishedAt: 20,
        }),
      ).toBe(true);
      expect((await store.attempts(next.id))[0]!.finishedAt).toBe(20);
      expect(await store.save({ ...next, revision: 2 }, 1, attempt(next))).toBe(false);
      expect(await store.attempts(next.id)).toHaveLength(1);
    });

    test("attempt constraint failure rolls back revision and state", async () => {
      const { store } = setup();
      const original = await store.insertOrGet(record());
      const next = { ...original, revision: 1, attemptCount: 1 };
      const a = attempt(next);
      expect(await store.save(next, 0, a)).toBe(true);
      await expect(
        store.save({ ...next, revision: 2, state: "accepted" }, 1, attempt(next)),
      ).rejects.toThrow();
      expect((await store.get(next.id))!.revision).toBe(1);
      expect((await store.get(next.id))!.state).toBe("pending");
      expect(await store.attempts(next.id)).toEqual([a]);
      const other = await store.insertOrGet(record({ idempotencyKey: "other" }));
      const otherNext = { ...other, revision: 1, attemptCount: 1 };
      await expect(store.save(otherNext, 0, attempt(otherNext, { id: a.id }))).rejects.toThrow();
      expect((await store.get(other.id))!.revision).toBe(0);
      expect(await store.attempts(other.id)).toEqual([]);
    });

    test("revision, attempt count and initial send time cannot drift", async () => {
      const { store } = setup();
      const original = await store.insertOrGet(record());
      await expect(store.save({ ...original, revision: 2 }, 0)).rejects.toThrow();
      const next = { ...original, revision: 1, attemptCount: 1, firstRequestAt: 10 };
      expect(await store.save(next, 0)).toBe(false);
      expect(
        await store.save({ ...next, attemptCount: 3 }, 0, attempt({ ...next, attemptCount: 3 })),
      ).toBe(false);
      await expect(
        store.save(next, 0, attempt(next, { notificationId: "foreign" })),
      ).rejects.toThrow();
      const a = attempt(next);
      expect(await store.save(next, 0, a)).toBe(true);
      expect(await store.save({ ...next, revision: 2, firstRequestAt: 99 }, 1, a)).toBe(true);
      expect((await store.get(next.id))!.firstRequestAt).toBe(10);
      expect(await store.save({ ...next, revision: 3, attemptCount: 0 }, 2)).toBe(false);
      expect((await store.get(next.id))!.revision).toBe(2);
      expect(await store.attempts(next.id)).toHaveLength(1);
    });

    test("tenant/recipient filters, limits and unique preferences", async () => {
      const { store } = setup();
      await store.insertOrGet(record());
      await store.insertOrGet(record({ tenantId: "other" }));
      await store.insertOrGet(record({ recipientId: "second", idempotencyKey: "second" }));
      expect(
        await store.list({ tenantId: "tenant", recipientId: "recipient", limit: 10 }),
      ).toHaveLength(1);
      expect(await store.list({ tenantId: "tenant", limit: 1 })).toHaveLength(1);
      for (const limit of [0, -1, 101, 1.5, NaN]) {
        await expect(store.list({ tenantId: "tenant", limit })).rejects.toThrow();
        await expect(store.recoverable(100, limit)).rejects.toThrow();
      }
      const key = {
        tenantId: "tenant",
        recipientId: "recipient",
        category: "billing",
        channelId: "email",
      };
      expect(await store.getPreference(key)).toBeNull();
      await store.setPreference({ ...key, enabled: false });
      await store.setPreference({ ...key, enabled: true });
      expect(await store.getPreference(key)).toEqual({ ...key, enabled: true });
      expect(await store.getPreference({ ...key, tenantId: "other" })).toBeNull();
    });

    test("durable handoff is set-once and does not fence or get overwritten by delivery CAS", async () => {
      const { store } = setup();
      const original = await store.insertOrGet(record({ retryable: true }));
      expect(await store.markEnqueued(original.id, "task-job")).toBe(true);
      expect(await store.markEnqueued(original.id, "task-job")).toBe(true);
      expect(await store.markEnqueued(original.id, "other-job")).toBe(false);
      expect((await store.get(original.id))!.revision).toBe(0);
      expect(await store.recoverable(100, 100)).toEqual([]);
      const next = { ...original, revision: 1, state: "sending" as const, attemptCount: 1 };
      expect(await store.save(next, 0, attempt(next))).toBe(true);
      expect((await store.get(original.id))!.taskJobId).toBe("task-job");
      expect(await store.attempts(original.id)).toHaveLength(1);
      const later = await store.insertOrGet(record({ idempotencyKey: "later", updatedAt: 2 }));
      expect((await store.recoverable(100, 1)).map((row) => row.id)).toEqual([later.id]);
    });

    test("bounded recovery excludes terminal, unretryable unknown, and unexpired leases", async () => {
      const { store } = setup();
      const cases: Partial<NotificationRecord>[] = [
        { state: "pending" },
        { state: "unknown", retryable: true },
        { state: "failed", retryable: true },
        { state: "sending", leaseUntil: 50 },
        { state: "sending", leaseUntil: 101 },
        { state: "unknown", retryable: false },
        { state: "failed", retryable: false },
        { state: "accepted" },
        { state: "delivered" },
      ];
      const records = await Promise.all(
        cases.map((value, i) => store.insertOrGet(record({ ...value, idempotencyKey: `${i}` }))),
      );
      expect(new Set((await store.recoverable(100, 100)).map((r) => r.id))).toEqual(
        new Set(records.slice(0, 4).map((r) => r.id)),
      );
      expect(await store.recoverable(100, 2)).toHaveLength(2);
    });
  });
}
