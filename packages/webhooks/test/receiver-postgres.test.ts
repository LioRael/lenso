import { expect, test } from "bun:test";
import { readFile } from "node:fs/promises";
import { Pool } from "pg";
import { createWebhookReceiver, type WebhookInbox } from "@lenso/webhooks/receiver";
import { createPostgresWebhookInbox } from "@lenso/webhooks/receiver-postgres";
import { signWebhook } from "@lenso/webhooks";

const connectionString = process.env.WEBHOOK_TEST_DATABASE_URL;
if (!connectionString)
  console.warn(
    "Skipping inbound PostgreSQL integration: WEBHOOK_TEST_DATABASE_URL must name an owned disposable database.",
  );
const integration = connectionString ? test : test.skip;

integration(
  "localhost delivery: concurrent receipts, rollback, lost acknowledgement, reopen and namespace isolation",
  async () => {
    const schema = `inbox_${crypto.randomUUID().replaceAll("-", "")}`;
    const pools: Pool[] = [];
    const connect = () => {
      const pool = new Pool({ connectionString, max: 16, connectionTimeoutMillis: 2000 });
      pools.push(pool);
      return pool;
    };
    const owner = connect();
    let worker = connect();
    let server: ReturnType<typeof Bun.serve> | undefined;
    try {
      await owner.query(`CREATE SCHEMA "${schema}"`);
      const sql = await readFile(
        new URL("../migrations/0002_receiver.sql", import.meta.url),
        "utf8",
      );
      await owner.query(sql.replaceAll("__LENSO_WEBHOOK_SCHEMA__", `"${schema}"`));
      await owner.query(
        `CREATE TABLE "${schema}".orders (receiver_id text, event_id text, PRIMARY KEY(receiver_id,event_id))`,
      );
      let fail = true;
      let applies = 0;
      const makeInbox = (): WebhookInbox =>
        createPostgresWebhookInbox({
          pool: worker,
          schema,
          async apply(client, event, receipt) {
            applies++;
            await client.query(`INSERT INTO "${schema}".orders VALUES ($1,$2)`, [
              receipt.receiverId,
              event.id,
            ]);
            if (event.id === "rollback" && fail) throw new Error("private business detail");
            if (event.id === "aborted" && fail) {
              try {
                await client.query("SELECT 1/0");
              } catch {
                /* Deliberately swallowed by application code. */
              }
            }
            await client.query("SELECT pg_sleep(0.02)");
          },
        });
      const current = { id: "local-key", secret: new Uint8Array(32).fill(13) };
      const rotated = { id: "rotated-key", secret: new Uint8Array(32).fill(14) };
      let inbox = makeInbox();
      let loseAck = true;
      const options = {
        source: "orders",
        eventTypes: ["order.completed"],
        maxBodyBytes: 8192,
        toleranceSeconds: 300,
        bodyTimeoutMs: 1000,
        keys: async () => [current, rotated],
        inbox: {
          async commitOnce(receipt, event) {
            const result = await inbox.commitOnce(receipt, event);
            if (event.id === "lost-ack" && loseAck) {
              loseAck = false;
              throw new Error("response lost after commit");
            }
            return result;
          },
        } satisfies WebhookInbox,
      };
      const first = createWebhookReceiver({ ...options, receiverId: "tenant-a" });
      const second = createWebhookReceiver({ ...options, receiverId: "tenant-b" });
      server = Bun.serve({
        hostname: "127.0.0.1",
        port: 0,
        fetch(request) {
          return new URL(request.url).pathname === "/b"
            ? second.handle(request)
            : first.handle(request);
        },
      });
      const url = `http://127.0.0.1:${server.port}`;
      async function send(
        id: string,
        data: unknown = { order: 1 },
        path = "/a",
        key = current,
        timestamp = Math.floor(Date.now() / 1000),
      ) {
        // occurrence time stays fixed across retries, just like persisted outbound envelopes.
        const body = new TextEncoder().encode(
          JSON.stringify({
            version: 1,
            id,
            type: "order.completed",
            source: "orders",
            occurredAt: "2026-01-01T00:00:00.000Z",
            data,
          }),
        );
        const response = await fetch(url + path, {
          method: "POST",
          body,
          headers: { "content-type": "application/json", ...signWebhook(body, id, timestamp, key) },
        });
        expect(await response.text()).toBe("");
        return response.status;
      }
      expect(await Promise.all(Array.from({ length: 12 }, () => send("concurrent")))).toEqual(
        Array(12).fill(204),
      );
      expect(applies).toBe(1);
      expect(await send("concurrent", { order: 2 })).toBe(409);
      expect(await send("concurrent", { order: 1 }, "/a", rotated)).toBe(204);
      expect(await send("concurrent", { order: 1 }, "/b")).toBe(204);
      expect(applies).toBe(2);
      expect(await send("rollback")).toBe(503);
      expect(await send("aborted")).toBe(503);
      expect(
        Number(
          (
            await owner.query(
              `SELECT count(*) FROM "${schema}".lenso_webhook_receipt WHERE event_id='rollback'`,
            )
          ).rows[0].count,
        ),
      ).toBe(0);
      expect(
        Number(
          (await owner.query(`SELECT count(*) FROM "${schema}".orders WHERE event_id='rollback'`))
            .rows[0].count,
        ),
      ).toBe(0);
      fail = false;
      expect(await send("aborted")).toBe(204);
      expect(await send("rollback")).toBe(204);
      expect(await send("lost-ack")).toBe(503);
      const beforeRetry = applies;
      expect(await send("lost-ack")).toBe(204);
      expect(applies).toBe(beforeRetry);
      await worker.end();
      pools.splice(pools.indexOf(worker), 1);
      worker = connect();
      inbox = makeInbox();
      expect(await send("concurrent")).toBe(204);
      expect(await send("lost-ack")).toBe(204);
      expect(applies).toBe(beforeRetry);
      expect(await send("expired", {}, "/a", current, Math.floor(Date.now() / 1000) - 301)).toBe(
        401,
      );
      const receipts = (await owner.query(`SELECT * FROM "${schema}".lenso_webhook_receipt`)).rows;
      expect(receipts.length).toBe(5);
      expect(Object.keys(receipts[0]).sort()).toEqual([
        "body_sha256",
        "event_id",
        "received_at",
        "receiver_id",
      ]);
      expect(
        Number((await owner.query(`SELECT count(*) FROM "${schema}".orders`)).rows[0].count),
      ).toBe(5);
    } finally {
      await server?.stop(true);
      await owner.query(`DROP SCHEMA IF EXISTS "${schema}" CASCADE`);
      await Promise.all(pools.map((pool) => pool.end()));
    }
  },
  15000,
);
