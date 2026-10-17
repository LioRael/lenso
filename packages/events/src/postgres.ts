import type { SQL } from "bun";
import { readFile } from "node:fs/promises";
import {
  duration,
  name,
  type AsyncEventStore,
  type Delivery,
  type DeliveryStatus,
  type Json,
} from "./contracts";

/** Explicit migration only. The caller owns the PostgreSQL pool and its search_path. */
export async function initializePostgresEvents(db: SQL): Promise<void> {
  postgres(db);
  const migration = await readFile(
    new URL("../migrations/pg/0001_events.sql", import.meta.url),
    "utf8",
  );
  await db.begin(async (tx) => {
    // Serialize concurrent setup; IF NOT EXISTS alone does not protect catalog creation.
    await tx`SELECT pg_advisory_xact_lock(1818586739, 1702258030)`;
    await tx.unsafe(migration).simple();
  });
}

function postgres(db: SQL): void {
  if (db.options.adapter !== "postgres") throw new Error("Events requires a PostgreSQL client");
}

// Keep identical canonical JSON rules to SQLite without importing its native entry.
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

/** Borrowed Bun SQL PostgreSQL pool. No I/O, DDL or resource ownership on construction. */
export function createPostgresEventStore(
  db: SQL,
  options: { now?: () => number } = {},
): AsyncEventStore {
  postgres(db);
  async function now(tx: SQL): Promise<number> {
    // One database clock across hosts. Sample after row locks for fenced settlements.
    const value = options.now
      ? options.now()
      : Number((await tx`SELECT floor(extract(epoch FROM clock_timestamp()) * 1000) AS ms`)[0].ms);
    if (!Number.isSafeInteger(value) || value < 0 || value > Number.MAX_SAFE_INTEGER - 86_400_000)
      throw new Error("Invalid event store clock");
    return value;
  }
  const transaction = <T>(run: (tx: SQL) => Promise<T>): Promise<T> =>
    db.begin("ISOLATION LEVEL READ COMMITTED", run);

  async function settle(delivery: Delivery, delayMs?: number): Promise<boolean> {
    return transaction(async (tx) => {
      // Waiting for a concurrent cancellation/settlement must not preserve an old clock.
      const locked = await tx`SELECT 1 FROM lenso_event_deliveries
        WHERE event_id = ${delivery.event.id} AND subscription_id = ${delivery.subscriptionId}
        FOR UPDATE`;
      if (!locked.length) return false;
      const time = await now(tx);
      const rows =
        delayMs === undefined
          ? await tx`UPDATE lenso_event_deliveries SET state = 'delivered', token = NULL, lease_until = NULL
            WHERE event_id = ${delivery.event.id} AND subscription_id = ${delivery.subscriptionId}
            AND token = ${delivery.token} AND state = 'processing' AND lease_until > ${time}
            RETURNING event_id`
          : await tx`UPDATE lenso_event_deliveries SET state = 'pending', token = NULL, lease_until = NULL,
            available_at = ${time + delayMs}
            WHERE event_id = ${delivery.event.id} AND subscription_id = ${delivery.subscriptionId}
            AND token = ${delivery.token} AND state = 'processing' AND lease_until > ${time}
            RETURNING event_id`;
      return rows.length === 1;
    });
  }

  return {
    async register(id, topic) {
      name(id);
      name(topic);
      await transaction(async (tx) => {
        await tx`INSERT INTO lenso_event_subscriptions(id, topic, active) VALUES (${id}, ${topic}, true)
          ON CONFLICT (id) DO NOTHING`;
        const [row] =
          await tx`SELECT topic, active FROM lenso_event_subscriptions WHERE id = ${id} FOR UPDATE`;
        if (!row || row.topic !== topic || !row.active)
          throw new Error("Subscription ID cannot be reused");
      });
    },
    async cancel(id) {
      name(id);
      await transaction(async (tx) => {
        // Publishers hold SHARE locks on active subscriptions through fanout. This update
        // waits for them, then the next statement sees and cancels all their deliveries.
        await tx`UPDATE lenso_event_subscriptions SET active = false WHERE id = ${id}`;
        await tx`UPDATE lenso_event_deliveries SET state = 'cancelled', token = NULL, lease_until = NULL
          WHERE subscription_id = ${id} AND state IN ('pending','processing')`;
      });
    },
    async publish(event) {
      name(event.id);
      name(event.topic);
      const payload = encode(event.payload);
      if (new TextEncoder().encode(payload).byteLength > 1_048_576)
        throw new Error("Event payload exceeds 1 MiB");
      return transaction(async (tx) => {
        const inserted =
          await tx`INSERT INTO lenso_events(id, topic, payload) VALUES (${event.id}, ${event.topic}, ${payload})
          ON CONFLICT (id) DO NOTHING RETURNING id`;
        if (!inserted.length) {
          // Fresh READ COMMITTED statement sees the concurrent winning publisher.
          const [existing] =
            await tx`SELECT topic, payload FROM lenso_events WHERE id = ${event.id}`;
          if (!existing || existing.topic !== event.topic || existing.payload !== payload)
            throw new Error("Event ID conflicts with an existing event");
          return { created: false };
        }
        const time = await now(tx);
        await tx`INSERT INTO lenso_event_deliveries(event_id, subscription_id, state, available_at)
          SELECT ${event.id}, id, 'pending', ${time} FROM (
            SELECT id FROM lenso_event_subscriptions WHERE topic = ${event.topic} AND active
            ORDER BY id FOR SHARE
          ) AS subscribers`;
        return { created: true };
      });
    },
    async claim(ids, leaseMs) {
      duration(leaseMs);
      ids.forEach(name);
      if (!ids.length) return null;
      return transaction(async (tx) => {
        const time = await now(tx);
        const [row] = await tx.unsafe<Row[]>(
          `SELECT d.event_id, d.subscription_id, e.topic, e.payload, d.attempts
          FROM lenso_event_deliveries d JOIN lenso_events e ON e.id = d.event_id
          JOIN lenso_event_subscriptions s ON s.id = d.subscription_id
          WHERE s.active AND d.subscription_id IN (SELECT jsonb_array_elements_text($1::text::jsonb))
          AND ((d.state = 'pending' AND d.available_at <= $2)
            OR (d.state = 'processing' AND d.lease_until <= $2))
          ORDER BY d.available_at, d.event_id, d.subscription_id LIMIT 1 FOR UPDATE OF d SKIP LOCKED`,
          [JSON.stringify(ids), time],
        );
        if (!row) return null;
        const token = crypto.randomUUID();
        const leaseUntil = (await now(tx)) + leaseMs;
        await tx`UPDATE lenso_event_deliveries SET state = 'processing', attempts = attempts + 1,
          lease_until = ${leaseUntil}, token = ${token}
          WHERE event_id = ${row.event_id} AND subscription_id = ${row.subscription_id}`;
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
      });
    },
    acknowledge: (delivery) => settle(delivery),
    async retry(delivery, delayMs) {
      duration(delayMs);
      return settle(delivery, delayMs);
    },
    async status(id) {
      name(id);
      const rows = await db`SELECT subscription_id, state, attempts, available_at, lease_until
        FROM lenso_event_deliveries WHERE event_id = ${id} ORDER BY subscription_id`;
      return rows.map(
        (row: {
          subscription_id: string;
          state: DeliveryStatus["state"];
          attempts: number;
          available_at: string | number;
          lease_until: string | number | null;
        }): DeliveryStatus => ({
          subscriptionId: row.subscription_id,
          state: row.state,
          attempts: row.attempts,
          availableAt: Number(row.available_at),
          leaseUntil: row.lease_until === null ? null : Number(row.lease_until),
        }),
      );
    },
  };
}
