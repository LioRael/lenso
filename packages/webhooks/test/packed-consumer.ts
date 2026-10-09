import assert from "node:assert/strict";
import {
  webhookConfig,
  signWebhook,
  verifyWebhook,
  createWebhooks,
  createPinnedHttpsTransport,
} from "@lenso/webhooks";
import { createPostgresWebhookRepository } from "@lenso/webhooks/postgres";
import { createWebhooksPlugin } from "@lenso/webhooks/plugin";
import { createWebhooksManage } from "@lenso/webhooks/manage";

assert.equal(typeof createWebhooks, "function");
assert.equal(typeof createPostgresWebhookRepository, "function");
assert.equal(typeof createWebhooksPlugin, "function");
assert.equal(typeof createWebhooksManage, "function");
assert.equal(typeof createPinnedHttpsTransport, "function");
assert.equal(createWebhooksManage(), undefined);
assert.equal(
  webhookConfig({
    instanceId: "pack-consumer",
    source: "orders",
    eventTypes: ["order.completed"],
    outbound: {
      allowedHosts: ["partner.invalid"],
      timeoutMs: 1000,
      dnsTimeoutMs: 100,
      connectTimeoutMs: 100,
      maxRequestBytes: 1024,
      maxResponseBytes: 1024,
    },
  }).enabled,
  false,
);
const body = new TextEncoder().encode(
  '{"version":1,"id":"stable-event","data":{"orderId":"fixture"}}',
);
const key = { id: "pack-test", secret: new Uint8Array(32).fill(17) };
const signature = signWebhook(body, "stable-event", 1_700_000_000, key);
assert.ok(
  verifyWebhook({
    body,
    eventId: signature["x-lenso-event-id"]!,
    timestamp: signature["x-lenso-timestamp"]!,
    signature: signature["x-lenso-signature"]!,
    keys: [key],
    now: 1_700_000_000,
  }),
);
const { createPinnedHttpsTransportForTest } = (await import("@lenso/webhooks")) as Record<
  string,
  unknown
>;
assert.equal(createPinnedHttpsTransportForTest, undefined);
console.log(
  "Packed Webhooks consumer: all public entries and signing verified; no test transport export",
);
