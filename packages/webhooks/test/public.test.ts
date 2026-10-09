import { expect, test } from "bun:test";
import { definePlugin } from "@lenso/core/plugin";
import { describeOperation } from "@lenso/engine/operations";
import {
  createWebhooks,
  webhookConfig,
  retryDelay,
  WebhookError,
  type Webhooks,
} from "@lenso/webhooks";
import { createPostgresWebhookRepository } from "@lenso/webhooks/postgres";
import { createWebhooksPlugin } from "@lenso/webhooks/plugin";
import { createWebhooksManage } from "@lenso/webhooks/manage";

const input = {
  instanceId: "unit",
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
};
test("public built package entries import and configuration fails closed", () => {
  expect(typeof createWebhooks).toBe("function");
  expect(typeof createPostgresWebhookRepository).toBe("function");
  expect(typeof createWebhooksPlugin).toBe("function");
  const config = webhookConfig(input);
  expect(config.enabled).toBe(false);
  expect(Object.isFrozen(config.outbound.allowedHosts)).toBe(true);
  expect(() => webhookConfig({ ...input, maxAttempts: 101 })).toThrow(WebhookError);
  expect(() =>
    webhookConfig({ ...input, outbound: { ...input.outbound, allowedHosts: ["localhost"] } }),
  ).toThrow(WebhookError);
  expect(() =>
    webhookConfig({ ...input, outbound: { ...input.outbound, dnsTimeoutMs: 1001 } }),
  ).toThrow(WebhookError);
});

test("backoff jitter and Retry-After are finite and capped", () => {
  for (let index = 0; index < 100; index++) {
    expect(retryDelay(3, 1000, 10_000, null, 0)).toBeGreaterThanOrEqual(2000);
    expect(retryDelay(3, 1000, 10_000, null, 0)).toBeLessThanOrEqual(4000);
  }
  expect(retryDelay(2, 1000, 10_000, "99999999", 0)).toBe(10_000);
  expect(retryDelay(2, 1000, 10_000, "Thu, 01 Jan 1970 00:00:05 GMT", 0)).toBe(5000);
  const delay = retryDelay(100, 1000, 10_000, "x".repeat(129), 0);
  expect(delay).toBeGreaterThanOrEqual(5000);
  expect(delay).toBeLessThanOrEqual(10_000);
});

test("Manage is optional and replay declarations cannot manufacture trusted context", () => {
  expect(createWebhooksManage()).toBeUndefined();
  expect(createWebhooksManage({ enabled: false })).toBeUndefined();
  const service = definePlugin<Webhooks<unknown>>({
    id: "test-webhooks",
    setup() {
      throw new Error("Discovery must not start services");
    },
  });
  const extension = createWebhooksManage({
    enabled: true,
    id: "test-webhooks.manage",
    webhooks: service,
  })!;
  expect(extension.plugin.requires).toEqual([service]);
  expect(extension.operations.map((operation) => operation.method)).toEqual([
    "list",
    "detail",
    "attempts",
    "replay",
  ]);
  const replay = extension.operations.find((operation) => operation.method === "replay")!;
  const description = describeOperation(replay, "lenso.config.ts");
  expect(replay.context).toBe(true);
  expect(description.confirmation).toBe("required");
  expect(description.approval).toBe("required");
  expect(description.retry).toBe("unsafe");
});
