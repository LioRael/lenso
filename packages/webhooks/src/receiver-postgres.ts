import type { Pool, PoolClient } from "pg";
import type { EventEnvelope } from "./contracts";
import type { WebhookInbox, WebhookReceipt } from "./receiver";

/** Borrows the host pool. Migrations and receipt retention are explicit host responsibilities. */
export function createPostgresWebhookInbox(options: {
  readonly pool: Pool;
  readonly schema: string;
  /** Only transactional database effects through this client; do not commit or release it. */
  readonly apply: (
    client: PoolClient,
    event: EventEnvelope,
    receipt: WebhookReceipt,
  ) => Promise<void>;
}): WebhookInbox {
  if (!/^[A-Za-z_][A-Za-z0-9_]{0,62}$/.test(options.schema) || typeof options.apply !== "function")
    throw new Error("Invalid webhook inbox configuration");
  const table = `"${options.schema}".lenso_webhook_receipt`;
  const { pool, apply } = options;
  return {
    async commitOnce(receipt, event) {
      const client = await pool.connect();
      let discard = false;
      try {
        await client.query("BEGIN ISOLATION LEVEL READ COMMITTED");
        const inserted = await client.query(
          `INSERT INTO ${table} (receiver_id, event_id, body_sha256, received_at)
           VALUES ($1, $2, $3, $4) ON CONFLICT (receiver_id, event_id) DO NOTHING RETURNING event_id`,
          [receipt.receiverId, receipt.eventId, receipt.bodySha256, receipt.receivedAt],
        );
        let result: "accepted" | "duplicate" | "conflict";
        if (inserted.rowCount === 1) {
          await apply(client, event, receipt);
          result = "accepted";
        } else {
          const existing = await client.query(
            `SELECT body_sha256 FROM ${table} WHERE receiver_id = $1 AND event_id = $2`,
            [receipt.receiverId, receipt.eventId],
          );
          if (existing.rowCount !== 1) throw new Error("Webhook receipt unavailable");
          result = existing.rows[0].body_sha256 === receipt.bodySha256 ? "duplicate" : "conflict";
        }
        // PostgreSQL reports ROLLBACK for COMMIT on an aborted transaction, even
        // if an application callback caught the statement error and returned.
        const committed = await client.query("COMMIT");
        if (committed.command !== "COMMIT") throw new Error("Webhook transaction aborted");
        return result;
      } catch {
        try {
          await client.query("ROLLBACK");
        } catch {
          discard = true;
        }
        throw new Error("Webhook inbox transaction failed");
      } finally {
        client.release(discard);
      }
    },
  };
}
