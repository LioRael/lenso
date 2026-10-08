import { afterEach, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { drizzle } from "drizzle-orm/bun-sqlite";
import Stripe from "stripe";
import { createPayments } from "../src";
import { sqlitePaymentsStore } from "../src/drizzle/sqlite";
import { createPaymentsWebhookHandler } from "../src/fetch";
import { createStripeProvider } from "../src/stripe";

const databases: Database[] = [];
afterEach(() => {
  for (const db of databases.splice(0)) db.close();
});

test("Stripe writes, webhook recovery, SQLite durability and tenant authorization", async () => {
  const db = new Database(":memory:");
  databases.push(db);
  db.exec(await Bun.file(new URL("../migrations/sqlite.sql", import.meta.url)).text());
  const requests: { method: string; url: URL; body: URLSearchParams }[] = [];
  let payment: { status: string; amount_received: number; [key: string]: unknown } | undefined;
  let refund: { status: string; [key: string]: unknown } | undefined;
  let failPayment = true;
  let failRefund = true;
  const fetchFixture = Object.assign(
    async (input: RequestInfo | URL, init?: RequestInit) => {
      const request = new Request(input, init);
      const captured = {
        method: request.method,
        url: new URL(request.url),
        body: new URLSearchParams(await request.text()),
      };
      requests.push(captured);
      if (captured.url.pathname === "/v1/account") return Response.json({ id: "acct_owned" });
      if (captured.method === "POST" && captured.url.pathname === "/v1/payment_intents") {
        payment = {
          object: "payment_intent",
          id: "pi_fixture",
          amount: Number(captured.body.get("amount")),
          amount_received: 0,
          currency: captured.body.get("currency"),
          livemode: false,
          status: "requires_payment_method",
          client_secret: "dummy_secret",
          metadata: {
            lenso_payment_id: captured.body.get("metadata[lenso_payment_id]"),
            lenso_tenant_id: captured.body.get("metadata[lenso_tenant_id]"),
            lenso_order_id: captured.body.get("metadata[lenso_order_id]"),
          },
        };
        if (failPayment) {
          failPayment = false;
          return Response.json({ error: { type: "api_error" } }, { status: 500 });
        }
        return Response.json(payment);
      }
      if (captured.url.pathname === "/v1/payment_intents/pi_fixture") return Response.json(payment);
      if (captured.method === "POST" && captured.url.pathname === "/v1/refunds") {
        refund = {
          object: "refund",
          id: "re_fixture",
          payment_intent: "pi_fixture",
          amount: Number(captured.body.get("amount")),
          currency: "usd",
          status: "pending",
          metadata: {
            lenso_refund_id: captured.body.get("metadata[lenso_refund_id]"),
            lenso_payment_id: captured.body.get("metadata[lenso_payment_id]"),
          },
        };
        if (failRefund) {
          failRefund = false;
          return Response.json({ error: { type: "api_error" } }, { status: 500 });
        }
        return Response.json(refund);
      }
      if (captured.url.pathname === "/v1/refunds/re_fixture") return Response.json(refund);
      throw new Error(`Unexpected fixture request: ${captured.method} ${captured.url.pathname}`);
    },
    { preconnect: fetch.preconnect },
  );
  const client = new Stripe("sk_test_dummy", {
    maxNetworkRetries: 0,
    httpClient: Stripe.createFetchHttpClient(fetchFixture),
  });
  const provider = createStripeProvider({
    client,
    accountId: "acct_owned",
    live: false,
    webhookSecret: "whsec_fixture",
    currencies: { usd: { min: 50, max: 100_000 } },
  });
  const runtime = createPayments({
    store: sqlitePaymentsStore(drizzle(db)),
    provider,
    authorize: async (tenant: string, _action, record) => {
      if (tenant !== record.tenantId) throw new Error("forbidden");
    },
  });
  const input = {
    tenantId: "tenant-a",
    orderId: "order-a",
    key: "checkout-a",
    amount: 1000,
    currency: "usd",
  };
  const created = await runtime.payments.create(input, input.tenantId);
  expect(created.status).toBe("unknown");
  expect(
    requests.filter((r) => r.method === "POST" && r.url.pathname === "/v1/payment_intents"),
  ).toHaveLength(1);
  expect((await runtime.payments.create(input, input.tenantId)).paymentId).toBe(created.paymentId);
  expect(
    requests.filter((r) => r.method === "POST" && r.url.pathname === "/v1/payment_intents"),
  ).toHaveLength(1);
  expect(requests.find((r) => r.url.pathname === "/v1/payment_intents")!.body.get("amount")).toBe(
    "1000",
  );
  expect(requests.find((r) => r.url.pathname === "/v1/payment_intents")!.body.get("currency")).toBe(
    "usd",
  );
  expect(
    requests
      .find((r) => r.url.pathname === "/v1/payment_intents")!
      .body.get("metadata[lenso_payment_id]"),
  ).toBe(created.paymentId);

  const handler = createPaymentsWebhookHandler({ webhook: runtime.webhook });
  const send = async (
    object: unknown,
    type = "payment_intent.updated",
    eventId: string = crypto.randomUUID(),
    bad = false,
  ) => {
    const payload = JSON.stringify({
      id: eventId,
      object: "event",
      type,
      livemode: false,
      data: { object },
    });
    const signature = await client.webhooks.generateTestHeaderStringAsync({
      payload,
      secret: "whsec_fixture",
      cryptoProvider: Stripe.createSubtleCryptoProvider(),
    });
    return handler(
      new Request("https://fixture.invalid/hook", {
        method: "POST",
        headers: { "stripe-signature": bad ? "bad" : signature },
        body: payload,
      }),
    );
  };
  payment!.status = "succeeded";
  payment!.amount_received = 1000;
  expect((await send(payment, "payment_intent.updated", "paid-event")).status).toBe(204);
  expect((await send(payment, "payment_intent.updated", "paid-event")).status).toBe(204);
  expect(
    (await send({ ...payment, status: "requires_payment_method", amount_received: 0 })).status,
  ).toBe(204);
  expect((await send({ ...payment, amount_received: 999 })).status).toBe(400);
  expect((await send(payment, "payment_intent.updated", "bad-signature", true)).status).toBe(400);
  await runtime.reconciliation.drain();
  expect(
    (await runtime.payments.get({ paymentId: created.paymentId }, input.tenantId)).status,
  ).toBe("succeeded");
  const results = await runtime.payments.results({ paymentId: created.paymentId }, input.tenantId);
  expect(results.filter((r) => r.kind === "payment.succeeded").map((r) => r.resultId)).toEqual([
    `${created.paymentId}:succeeded`,
  ]);
  expect(
    JSON.stringify(await runtime.payments.get({ paymentId: created.paymentId }, input.tenantId)),
  ).not.toContain("client_secret");
  expect(JSON.stringify(results)).not.toContain("dummy_secret");
  await expect(
    runtime.payments.get({ paymentId: created.paymentId }, "tenant-b"),
  ).rejects.toThrow();
  await expect(
    runtime.payments.refund(
      { paymentId: created.paymentId, key: "foreign", amount: 1 },
      "tenant-b",
    ),
  ).rejects.toThrow();

  const refunded = await runtime.payments.refund(
    { paymentId: created.paymentId, key: "refund-a", amount: 1000 },
    input.tenantId,
  );
  expect(refunded.refunds[0].status).toBe("unknown");
  refund!.status = "succeeded";
  expect((await send(refund, "refund.updated")).status).toBe(204);
  await runtime.reconciliation.drain();
  expect(
    (await runtime.payments.get({ paymentId: created.paymentId }, input.tenantId)).refunds[0]
      .status,
  ).toBe("succeeded");
  await runtime.payments.refund(
    { paymentId: created.paymentId, key: "refund-a", amount: 1000 },
    input.tenantId,
  );
  expect(
    requests.filter((r) => r.method === "POST" && r.url.pathname === "/v1/refunds"),
  ).toHaveLength(1);
  expect(
    (await runtime.payments.results({ paymentId: created.paymentId }, input.tenantId)).filter(
      (result) => result.kind === "refund.succeeded",
    ),
  ).toHaveLength(1);
});
