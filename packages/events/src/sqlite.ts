import type { Database } from "bun:sqlite";
import {
  duration,
  name,
  type Delivery,
  type DeliveryStatus,
  type EventStore,
  type Json,
} from "./contracts";

/** Explicit setup/migration step. Never called by publish, dispatch, or module import. */
export function initializeEvents(db: Database): void {
  db.transaction(() => {
    db.run(`CREATE TABLE IF NOT EXISTS lenso_event_subscriptions (
      id TEXT PRIMARY KEY, topic TEXT NOT NULL, active INTEGER NOT NULL CHECK(active IN (0,1))
    )`);
    db.run(`CREATE TABLE IF NOT EXISTS lenso_events (
      id TEXT PRIMARY KEY, topic TEXT NOT NULL, payload TEXT NOT NULL
    )`);
    db.run(`CREATE TABLE IF NOT EXISTS lenso_event_deliveries (
      event_id TEXT NOT NULL, subscription_id TEXT NOT NULL,
      state TEXT NOT NULL CHECK(state IN ('pending','processing','delivered','cancelled')),
      attempts INTEGER NOT NULL DEFAULT 0, available_at INTEGER NOT NULL,
      lease_until INTEGER, token TEXT,
      PRIMARY KEY(event_id, subscription_id)
    )`);
    db.run(`CREATE INDEX IF NOT EXISTS lenso_event_ready
      ON lenso_event_deliveries(subscription_id, state, available_at, lease_until)`);
  }).immediate();
}

// Canonical JSON makes retry identity insensitive to object key order. Reject lossy
// JSON values (NaN, undefined, sparse arrays, class instances), rather than changing an event.
function encode(value: Json, ancestors = new Set<object>()): string {
  if (value === null || typeof value === "boolean" || typeof value === "string")
    return JSON.stringify(value);
  if (typeof value === "number" && Number.isFinite(value)) return JSON.stringify(value);
  if (typeof value !== "object" || ancestors.has(value))
    throw new Error("Invalid JSON event payload");
  ancestors.add(value);
  try {
    if (Array.isArray(value)) {
      if (Object.keys(value).length !== value.length) throw new Error("Invalid JSON event array");
      return `[${Array.from(value, (item) => encode(item, ancestors)).join(",")}]`;
    }
    if (Object.getPrototypeOf(value) !== Object.prototype && Object.getPrototypeOf(value) !== null)
      throw new Error("Event payload must use plain JSON objects");
    if (Object.getOwnPropertySymbols(value).length) throw new Error("Invalid JSON event payload");
    return `{${Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${encode(value[key], ancestors)}`)
      .join(",")}}`;
  } finally {
    ancestors.delete(value);
  }
}

interface Row {
  event_id: string;
  subscription_id: string;
  topic: string;
  payload: string;
  attempts: number;
}

/** Bun SQLite adapter. Borrowed connection; caller owns initialization and close. */
export function createSqliteEventStore(
  db: Database,
  options: { now?: () => number } = {},
): EventStore {
  const now = () => {
    const value = (options.now ?? Date.now)();
    if (!Number.isSafeInteger(value) || value < 0 || value > Number.MAX_SAFE_INTEGER - 86_400_000)
      throw new Error("Invalid event store clock");
    return value;
  };
  return {
    register(id, topic) {
      name(id);
      name(topic);
      db.transaction(() => {
        const row = db
          .query<{ topic: string; active: number }, [string]>(
            "SELECT topic, active FROM lenso_event_subscriptions WHERE id = ?",
          )
          .get(id);
        if (row && (row.topic !== topic || !row.active))
          throw new Error("Subscription ID cannot be reused");
        if (!row) db.run("INSERT INTO lenso_event_subscriptions VALUES (?, ?, 1)", [id, topic]);
      }).immediate();
    },
    cancel(id) {
      name(id);
      db.transaction(() => {
        db.run("UPDATE lenso_event_subscriptions SET active = 0 WHERE id = ?", [id]);
        db.run(
          `UPDATE lenso_event_deliveries SET state = 'cancelled', token = NULL, lease_until = NULL
          WHERE subscription_id = ? AND state IN ('pending','processing')`,
          [id],
        );
      }).immediate();
    },
    publish(event) {
      name(event.id);
      name(event.topic);
      const payload = encode(event.payload);
      if (new TextEncoder().encode(payload).byteLength > 1_048_576)
        throw new Error("Event payload exceeds 1 MiB");
      return db
        .transaction(() => {
          const existing = db
            .query<{ topic: string; payload: string }, [string]>(
              "SELECT topic, payload FROM lenso_events WHERE id = ?",
            )
            .get(event.id);
          if (existing) {
            if (existing.topic !== event.topic || existing.payload !== payload)
              throw new Error("Event ID conflicts with an existing event");
            return { created: false };
          }
          db.run("INSERT INTO lenso_events VALUES (?, ?, ?)", [event.id, event.topic, payload]);
          db.run(
            `INSERT INTO lenso_event_deliveries(event_id, subscription_id, state, available_at)
          SELECT ?, id, 'pending', ? FROM lenso_event_subscriptions WHERE topic = ? AND active = 1`,
            [event.id, now(), event.topic],
          );
          return { created: true };
        })
        .immediate();
    },
    claim(ids, leaseMs) {
      duration(leaseMs);
      ids.forEach(name);
      if (!ids.length) return null;
      return db
        .transaction(() => {
          const time = now();
          const row = db
            .query<Row, [string, number, number]>(`SELECT d.event_id, d.subscription_id,
          e.topic, e.payload, d.attempts FROM lenso_event_deliveries d
          JOIN lenso_events e ON e.id = d.event_id
          JOIN lenso_event_subscriptions s ON s.id = d.subscription_id
          WHERE s.active = 1 AND d.subscription_id IN (SELECT value FROM json_each(?))
          AND ((d.state = 'pending' AND d.available_at <= ?)
            OR (d.state = 'processing' AND d.lease_until <= ?))
          ORDER BY d.available_at, d.event_id, d.subscription_id LIMIT 1`)
            .get(JSON.stringify(ids), time, time);
          if (!row) return null;
          const token = crypto.randomUUID();
          db.run(
            `UPDATE lenso_event_deliveries SET state = 'processing', attempts = attempts + 1,
          lease_until = ?, token = ? WHERE event_id = ? AND subscription_id = ?`,
            [time + leaseMs, token, row.event_id, row.subscription_id],
          );
          return {
            event: Object.freeze({
              id: row.event_id,
              topic: row.topic,
              payload: JSON.parse(row.payload) as Json,
            }),
            subscriptionId: row.subscription_id,
            idempotencyKey: JSON.stringify([row.event_id, row.subscription_id]),
            attempt: row.attempts + 1,
            token,
          };
        })
        .immediate();
    },
    acknowledge(delivery: Delivery) {
      return (
        db.run(
          `UPDATE lenso_event_deliveries SET state = 'delivered', token = NULL, lease_until = NULL
        WHERE event_id = ? AND subscription_id = ? AND token = ? AND state = 'processing' AND lease_until > ?`,
          [delivery.event.id, delivery.subscriptionId, delivery.token, now()],
        ).changes === 1
      );
    },
    retry(delivery: Delivery, delayMs) {
      duration(delayMs);
      const time = now();
      return (
        db.run(
          `UPDATE lenso_event_deliveries SET state = 'pending', token = NULL,
        lease_until = NULL, available_at = ?
        WHERE event_id = ? AND subscription_id = ? AND token = ? AND state = 'processing' AND lease_until > ?`,
          [time + delayMs, delivery.event.id, delivery.subscriptionId, delivery.token, time],
        ).changes === 1
      );
    },
    status(id) {
      name(id);
      return db
        .query<DeliveryStatus, [string]>(`SELECT subscription_id AS subscriptionId,
        state, attempts, available_at AS availableAt, lease_until AS leaseUntil
        FROM lenso_event_deliveries WHERE event_id = ? ORDER BY subscription_id`)
        .all(id);
    },
  };
}
