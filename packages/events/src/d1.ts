import {
  duration,
  name,
  type Delivery,
  type DeliveryStatus,
  type EventStore,
  type Json,
} from "./contracts";

/** The existing Store contract, with asynchronous D1 results. */
export type D1EventStore = {
  [K in keyof EventStore]: (
    ...args: Parameters<EventStore[K]>
  ) => Promise<Awaited<ReturnType<EventStore[K]>>>;
};

/** Structural binding types keep Cloudflare globals out of portable consumers. */
export interface D1EventResult<T = unknown> {
  success: boolean;
  results: T[];
  meta: { changes: number };
}
export interface D1EventStatement {
  bind(...values: unknown[]): D1EventStatement;
  all<T = Record<string, unknown>>(): Promise<D1EventResult<T>>;
  run<T = Record<string, unknown>>(): Promise<D1EventResult<T>>;
}
export interface D1EventDatabase {
  prepare(query: string): D1EventStatement;
  batch<T = unknown>(statements: D1EventStatement[]): Promise<D1EventResult<T>[]>;
}

/** Explicit setup only; never called by store construction or request operations. */
export async function initializeD1Events(db: D1EventDatabase): Promise<void> {
  await db.batch([
    db.prepare(`CREATE TABLE IF NOT EXISTS lenso_event_subscriptions (
      id TEXT PRIMARY KEY, topic TEXT NOT NULL, active INTEGER NOT NULL CHECK(active IN (0,1))
    )`),
    db.prepare(`CREATE TABLE IF NOT EXISTS lenso_events (
      id TEXT PRIMARY KEY, topic TEXT NOT NULL, payload TEXT NOT NULL
    )`),
    db.prepare(`CREATE TABLE IF NOT EXISTS lenso_event_deliveries (
      event_id TEXT NOT NULL, subscription_id TEXT NOT NULL,
      state TEXT NOT NULL CHECK(state IN ('pending','processing','delivered','cancelled')),
      attempts INTEGER NOT NULL DEFAULT 0, available_at INTEGER NOT NULL,
      lease_until INTEGER, token TEXT,
      PRIMARY KEY(event_id, subscription_id)
    )`),
    db.prepare(`CREATE INDEX IF NOT EXISTS lenso_event_ready
      ON lenso_event_deliveries(subscription_id, state, available_at, lease_until)`),
  ]);
}

// Same canonical JSON identity as the SQLite store, without importing its native entry.
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

// Evaluate the production clock inside SQL, after any database queueing. Worker
// clocks (or a timestamp sampled before awaiting D1) cannot enforce lease expiry.
const clock =
  "COALESCE(?, CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4, 3) AS INTEGER))";

/** Borrow a D1 binding. Every multi-statement transition is one atomic D1 batch. */
export function createD1EventStore(
  db: D1EventDatabase,
  options: { now?: () => number } = {},
): D1EventStore {
  const now = () => {
    if (!options.now) return null;
    const value = options.now();
    if (!Number.isSafeInteger(value) || value < 0 || value > Number.MAX_SAFE_INTEGER - 86_400_000)
      throw new Error("Invalid event store clock");
    return value;
  };
  return {
    async register(id, topic) {
      name(id);
      name(topic);
      const [, result] = await db.batch<{ topic: string; active: number }>([
        db
          .prepare(`INSERT INTO lenso_event_subscriptions VALUES (?, ?, 1)
          ON CONFLICT(id) DO NOTHING`)
          .bind(id, topic),
        db.prepare("SELECT topic, active FROM lenso_event_subscriptions WHERE id = ?").bind(id),
      ]);
      const row = result.results[0];
      if (row.topic !== topic || !row.active) throw new Error("Subscription ID cannot be reused");
    },
    async cancel(id) {
      name(id);
      await db.batch([
        db.prepare("UPDATE lenso_event_subscriptions SET active = 0 WHERE id = ?").bind(id),
        db
          .prepare(`UPDATE lenso_event_deliveries SET state = 'cancelled', token = NULL, lease_until = NULL
          WHERE subscription_id = ? AND state IN ('pending','processing')`)
          .bind(id),
      ]);
    },
    async publish(event) {
      name(event.id);
      name(event.topic);
      const payload = encode(event.payload);
      if (new TextEncoder().encode(payload).byteLength > 1_048_576)
        throw new Error("Event payload exceeds 1 MiB");
      const time = now();
      const [inserted, , existing] = await db.batch<{ topic: string; payload: string }>([
        db
          .prepare(`INSERT INTO lenso_events VALUES (?, ?, ?) ON CONFLICT(id) DO NOTHING`)
          .bind(event.id, event.topic, payload),
        // changes() refers to the immediately preceding INSERT in this transaction.
        // A producer retry must not fan out to subscriptions registered afterwards.
        db
          .prepare(`INSERT INTO lenso_event_deliveries(event_id, subscription_id, state, available_at)
          SELECT ?, id, 'pending', ${clock} FROM lenso_event_subscriptions
          WHERE topic = ? AND active = 1 AND changes() = 1`)
          .bind(event.id, time, event.topic),
        db.prepare("SELECT topic, payload FROM lenso_events WHERE id = ?").bind(event.id),
      ]);
      const row = existing.results[0];
      if (row.topic !== event.topic || row.payload !== payload)
        throw new Error("Event ID conflicts with an existing event");
      return { created: inserted.meta.changes === 1 };
    },
    async claim(ids, leaseMs) {
      duration(leaseMs);
      ids.forEach(name);
      if (!ids.length) return null;
      const time = now();
      const token = crypto.randomUUID();
      const selected = await db
        .prepare(`UPDATE lenso_event_deliveries SET state = 'processing', attempts = attempts + 1,
          lease_until = ${clock} + ?, token = ? WHERE (event_id, subscription_id) = (
            SELECT d.event_id, d.subscription_id FROM lenso_event_deliveries d
            JOIN lenso_event_subscriptions s ON s.id = d.subscription_id
            WHERE s.active = 1 AND d.subscription_id IN (SELECT value FROM json_each(?))
            AND ((d.state = 'pending' AND d.available_at <= ${clock})
              OR (d.state = 'processing' AND d.lease_until <= ${clock}))
            ORDER BY d.available_at, d.event_id, d.subscription_id LIMIT 1
          ) RETURNING event_id, subscription_id, attempts,
            (SELECT topic FROM lenso_events WHERE id = event_id) AS topic,
            (SELECT payload FROM lenso_events WHERE id = event_id) AS payload`)
        .bind(time, leaseMs, token, JSON.stringify(ids), time, time)
        .all<Row>();
      const row = selected.results[0];
      if (!row) return null;
      return {
        event: Object.freeze({
          id: row.event_id,
          topic: row.topic,
          payload: JSON.parse(row.payload) as Json,
        }),
        subscriptionId: row.subscription_id,
        idempotencyKey: JSON.stringify([row.event_id, row.subscription_id]),
        attempt: row.attempts,
        token,
      };
    },
    async acknowledge(delivery: Delivery) {
      const result = await db
        .prepare(`UPDATE lenso_event_deliveries
        SET state = 'delivered', token = NULL, lease_until = NULL
        WHERE event_id = ? AND subscription_id = ? AND token = ?
          AND state = 'processing' AND lease_until > ${clock}`)
        .bind(delivery.event.id, delivery.subscriptionId, delivery.token, now())
        .run();
      return result.meta.changes === 1;
    },
    async retry(delivery: Delivery, delayMs) {
      duration(delayMs);
      const time = now();
      const result = await db
        .prepare(`UPDATE lenso_event_deliveries SET state = 'pending', token = NULL,
        lease_until = NULL, available_at = ${clock} + ?
        WHERE event_id = ? AND subscription_id = ? AND token = ? AND state = 'processing' AND lease_until > ${clock}`)
        .bind(time, delayMs, delivery.event.id, delivery.subscriptionId, delivery.token, time)
        .run();
      return result.meta.changes === 1;
    },
    async status(id) {
      name(id);
      const result = await db
        .prepare(`SELECT subscription_id AS subscriptionId,
        state, attempts, available_at AS availableAt, lease_until AS leaseUntil
        FROM lenso_event_deliveries WHERE event_id = ? ORDER BY subscription_id`)
        .bind(id)
        .all<DeliveryStatus>();
      return result.results;
    },
  };
}
