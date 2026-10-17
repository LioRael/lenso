import { expect, test } from "bun:test";
import { startApp } from "@lenso/core";
import { definePlugin } from "@lenso/core/plugin";
import { createWebhookReceiver, type WebhookInbox } from "@lenso/webhooks/receiver";
import { createWebhookReceiverPlugin } from "@lenso/webhooks/plugin";
import { signWebhook } from "@lenso/webhooks";

const key = { id: "test-key", secret: new Uint8Array(32).fill(7) };
const now = 1_700_000_000_000;
const event = {
  version: 1,
  id: "event-1",
  source: "orders",
  type: "order.completed",
  occurredAt: new Date(now).toISOString(),
  data: { order: 1 },
};
const bytes = (value: unknown) => new TextEncoder().encode(JSON.stringify(value));
function request(value: unknown = event, timestamp = now / 1000, signingKey = key) {
  const body = bytes(value);
  return new Request("http://localhost/hook", {
    method: "POST",
    body,
    headers: {
      "content-type": "application/json",
      ...signWebhook(body, event.id, timestamp, signingKey),
    },
  });
}
function fixture() {
  let calls = 0;
  const seen = new Map<string, string>();
  const inbox: WebhookInbox = {
    async commitOnce(receipt, accepted) {
      expect(accepted.id).toBe(event.id);
      expect(receipt.receiverId).toBe("tenant-a/orders");
      expect(Object.isFrozen(accepted.data)).toBe(true);
      calls++;
      const previous = seen.get(receipt.eventId);
      if (previous) return previous === receipt.bodySha256 ? "duplicate" : "conflict";
      seen.set(receipt.eventId, receipt.bodySha256);
      return "accepted";
    },
  };
  const options = {
    receiverId: "tenant-a/orders",
    source: "orders",
    eventTypes: [event.type],
    maxBodyBytes: 1024,
    bodyTimeoutMs: 50,
    toleranceSeconds: 300,
    keys: async () => [key],
    inbox,
    now: () => now,
  };
  return { options, receiver: createWebhookReceiver(options), calls: () => calls };
}

test("raw signature, time window and scoped envelope reject before persistence", async () => {
  const { receiver, calls } = fixture();
  for (const [input, status] of [
    [request(event, now / 1000 - 301), 401],
    [request(event, now / 1000 + 301), 401],
    [request(event, now / 1000, { ...key, secret: new Uint8Array(32).fill(8) }), 401],
    [request({ ...event, id: "different" }), 400],
    [request({ ...event, version: 2 }), 400],
    [request({ ...event, occurredAt: "invalid" }), 400],
    [request({ ...event, source: "other-tenant" }), 403],
    [request({ ...event, type: "unapproved" }), 403],
    [request({ ...event, tenantId: "forged" }), 400],
  ] as const)
    expect((await receiver.handle(input)).status).toBe(status);
  const changed = request();
  changed.headers.set("x-lenso-event-id", "changed");
  expect((await receiver.handle(changed)).status).toBe(401);
  expect(calls()).toBe(0);
});

test("redelivery with refreshed signature dedupes and changed content conflicts", async () => {
  const { receiver, calls } = fixture();
  expect((await receiver.handle(request())).status).toBe(204);
  expect((await receiver.handle(request(event, now / 1000 + 1))).status).toBe(204);
  expect((await receiver.handle(request({ ...event, data: { order: 2 } }))).status).toBe(409);
  expect(calls()).toBe(3);
});

test("bounded streams, unsupported encodings, duplicate headers and cancellation fail closed", async () => {
  const { receiver, calls } = fixture();
  expect((await receiver.handle(new Request("http://localhost/hook"))).status).toBe(405);
  const encoded = request();
  encoded.headers.set("content-encoding", "gzip");
  expect((await receiver.handle(encoded)).status).toBe(415);
  const duplicated = request();
  duplicated.headers.append("x-lenso-signature", "v1;other");
  expect((await receiver.handle(duplicated)).status).toBe(401);
  let cancelled = false;
  const oversize = new Request("http://localhost/hook", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: new ReadableStream({
      pull(c) {
        c.enqueue(new Uint8Array(2048));
      },
      cancel() {
        cancelled = true;
      },
    }),
  });
  expect((await receiver.handle(oversize)).status).toBe(413);
  expect(cancelled).toBe(true);
  const stalled = new Request("http://localhost/hook", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: new ReadableStream({
      pull() {
        return new Promise(() => {});
      },
      cancel() {
        return new Promise(() => {});
      },
    }),
  });
  expect((await receiver.handle(stalled)).status).toBe(408);
  const controller = new AbortController();
  const aborted = new Request("http://localhost/hook", {
    method: "POST",
    headers: { "content-type": "application/json" },
    signal: controller.signal,
    body: new ReadableStream({
      pull() {
        return new Promise(() => {});
      },
    }),
  });
  controller.abort();
  expect((await receiver.handle(aborted)).status).toBe(408);
  expect(calls()).toBe(0);
});

test("malformed UTF-8 and non-finite/deep JSON are not admitted even with valid signatures", async () => {
  const { receiver, calls } = fixture();
  let data: unknown = 1;
  for (let depth = 0; depth < 35; depth++) data = { data };
  expect((await receiver.handle(request({ ...event, data }))).status).toBe(400);
  for (const body of [
    new Uint8Array([0xff]),
    new TextEncoder().encode(JSON.stringify(event).replace('"order":1', '"order":1e999')),
  ]) {
    expect(
      (
        await receiver.handle(
          new Request("http://localhost/hook", {
            method: "POST",
            body,
            headers: {
              "content-type": "application/json",
              ...signWebhook(body, event.id, now / 1000, key),
            },
          }),
        )
      ).status,
    ).toBe(400);
  }
  expect(calls()).toBe(0);
});

test("resolver and persistence errors stay opaque; configuration is captured", async () => {
  const { options } = fixture();
  for (const failing of [
    {
      ...options,
      keys: async () => {
        throw new Error("SECRET");
      },
    },
    {
      ...options,
      inbox: {
        commitOnce: async () => {
          throw new Error("DATABASE SECRET");
        },
      },
    },
  ]) {
    const response = await createWebhookReceiver(failing).handle(request());
    expect(response.status).toBe(503);
    expect(await response.text()).toBe("");
  }
  expect(() => createWebhookReceiver({ ...options, bodyTimeoutMs: NaN })).toThrow();
  expect(() => createWebhookReceiver({ ...options, toleranceSeconds: -1 })).toThrow();
  const receiver = createWebhookReceiver(options);
  options.source = "mutated";
  options.eventTypes.length = 0;
  expect((await receiver.handle(request())).status).toBe(204);
});

test("optional receiver plugin resolves the exact host inbox and borrows its lifetime", async () => {
  const { options } = fixture();
  let acquired = 0;
  let released = 0;
  const inbox = definePlugin({
    id: "inbox",
    setup(context) {
      acquired++;
      context.onCleanup(() => {
        released++;
      });
      return options.inbox;
    },
  });
  const receiver = createWebhookReceiverPlugin({ id: "receiver", inbox, options: () => options });
  expect(acquired).toBe(0);
  const app = await startApp({ plugins: [inbox, receiver] });
  try {
    expect(acquired).toBe(1);
    expect((await app.get(receiver).handle(request())).status).toBe(204);
  } finally {
    await app.stop();
  }
  expect(released).toBe(1);
});
