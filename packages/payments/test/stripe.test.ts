import { describe, expect, test } from "bun:test";
import Stripe from "stripe";
import { createStripeProvider } from "../src/stripe";
import type { PaymentRecord, RefundRecord } from "../src/contracts";
import { RefundNotCreatedError } from "../src/contracts";
import { PaymentsError } from "../src/contracts";

const now = 1_800_000_000_000;
function record(): PaymentRecord {
  return {
    paymentId: "payment-1",
    tenantId: "tenant-1",
    orderId: "order-1",
    accountId: "acct_owned",
    live: false,
    amount: 500,
    currency: "usd",
    key: "key",
    orderKey: "order-key",
    status: "unknown",
    providerId: null,
    revision: 0,
    attemptedAt: now,
    lease: null,
    refunds: [],
    results: [],
    createdAt: now - 10_000,
    updatedAt: now,
    reconcileAt: now,
  };
}
function refundRecord(): RefundRecord {
  return {
    refundId: "refund-1",
    key: "refund-key",
    amount: 1,
    status: "unknown",
    providerId: null,
    attemptedAt: now,
  };
}
function pi(overrides: Record<string, unknown> = {}) {
  return {
    object: "payment_intent",
    id: "pi_owned",
    amount: 500,
    amount_received: 0,
    currency: "usd",
    livemode: false,
    status: "requires_payment_method",
    client_secret: "pi_owned_secret_not_projected",
    metadata: {
      lenso_payment_id: "payment-1",
      lenso_tenant_id: "tenant-1",
      lenso_order_id: "order-1",
    },
    ...overrides,
  };
}
function re(overrides: Record<string, unknown> = {}) {
  return {
    object: "refund",
    id: "re_owned",
    payment_intent: "pi_owned",
    amount: 1,
    currency: "usd",
    status: "succeeded",
    metadata: { lenso_refund_id: "refund-1", lenso_payment_id: "payment-1" },
    ...overrides,
  };
}
type Captured = { url: URL; method: string; headers: Headers; body: URLSearchParams };
function fixture(
  handler?: (request: Captured) => unknown,
  routing: { stripeAccount?: string; stripeContext?: string } = {},
) {
  const requests: Captured[] = [];
  const client = new Stripe("sk_test_fixture", {
    maxNetworkRetries: 3,
    ...routing,
    httpClient: Stripe.createFetchHttpClient(
      Object.assign(
        async (input: RequestInfo | URL, init?: RequestInit) => {
          const req = new Request(input, init);
          const captured = {
            url: new URL(req.url),
            method: req.method,
            headers: req.headers,
            body: new URLSearchParams(await req.text()),
          };
          requests.push(captured);
          let result = handler?.(captured);
          if (result instanceof Response) return result;
          result ??=
            captured.url.pathname === "/v1/account"
              ? { id: "acct_owned" }
              : captured.url.pathname.startsWith("/v1/refunds")
                ? re()
                : pi();
          return Response.json(result);
        },
        { preconnect: fetch.preconnect },
      ),
    ),
  });
  const options = {
    client,
    accountId: "acct_owned",
    live: false,
    webhookSecret: "whsec_fixture",
    currencies: {
      usd: { min: 50, max: 100_000 },
      isk: { min: 100, max: 100_000 },
      ugx: { min: 100, max: 100_000 },
      huf: { min: 1, max: 100_000 },
      twd: { min: 1, max: 100_000 },
      eur: { min: 1, max: 100_000, multiple: 5 },
    },
  };
  return { client, options, requests, provider: createStripeProvider(options) };
}
async function signed(
  client: Stripe,
  object: unknown = pi(),
  extra = {},
  timestamp = Math.floor(Date.now() / 1000),
) {
  const payload = JSON.stringify({
    id: "evt_fixture",
    object: "event",
    type: "payment_intent.created",
    livemode: false,
    data: { object },
    ...extra,
  });
  const signature = await client.webhooks.generateTestHeaderStringAsync({
    payload,
    secret: "whsec_fixture",
    timestamp,
    cryptoProvider: Stripe.createSubtleCryptoProvider(),
  });
  return { raw: new TextEncoder().encode(payload), signature };
}

describe("official Stripe SDK provider, local HTTP fixtures", () => {
  test("write gates run after SDK account/PI preflight and can prevent both POSTs", async () => {
    for (const refund of [false, true]) {
      const { provider, requests } = fixture();
      const guard = async () => {
        expect(requests.at(-1)?.method).toBe("GET");
        expect(requests.at(-1)?.url.pathname).toBe(
          refund ? "/v1/payment_intents/pi_owned" : "/v1/account",
        );
        throw new PaymentsError("conflict");
      };
      const operation = refund
        ? provider.createRefund(
            { ...record(), providerId: "pi_owned" },
            refundRecord(),
            "guarded-key",
            guard,
          )
        : provider.createPayment(record(), "guarded-key", guard);
      await expect(operation).rejects.toMatchObject({ code: "conflict" });
      expect(requests.filter((request) => request.method === "POST")).toHaveLength(0);
    }
  });

  test("creates an unconfirmed PI and projects no secret; retrieves secret separately", async () => {
    const { provider, requests } = fixture();
    const payment = await provider.createPayment(record(), "persisted-payment-key");
    expect(payment.status).toBe("requires_payment_method");
    expect(JSON.stringify(payment)).not.toContain("secret");
    const request = requests.find((item) => item.method === "POST")!;
    expect(request.url.pathname).toBe("/v1/payment_intents");
    expect(Object.fromEntries(request.body)).toEqual({
      amount: "500",
      currency: "usd",
      capture_method: "automatic",
      "automatic_payment_methods[enabled]": "true",
      "metadata[lenso_payment_id]": "payment-1",
      "metadata[lenso_tenant_id]": "tenant-1",
      "metadata[lenso_order_id]": "order-1",
    });
    expect(request.headers.get("idempotency-key")).toBe("persisted-payment-key");
    expect(request.headers.get("stripe-version")).toBe("2026-09-30.endive");
    expect(request.headers.get("stripe-account")).toBeNull();
    expect(request.headers.get("stripe-context")).toBeNull();
    expect(requests[0].url.pathname).toBe("/v1/account");
    expect(await provider.clientSecret("pi_owned")).toBe("pi_owned_secret_not_projected");
    expect(requests.at(-1)!.url.pathname).toBe("/v1/payment_intents/pi_owned");
  });

  test("refund accepts amounts below charge min and uses PI plus metadata, no reason/transfer", async () => {
    const { provider, requests } = fixture();
    const value = await provider.createRefund(
      { ...record(), providerId: "pi_owned" },
      refundRecord(),
      "persisted-refund-key",
    );
    expect(value.live).toBe(false);
    expect(value.amount).toBe(1);
    const request = requests.find((item) => item.method === "POST")!;
    expect(request.url.pathname).toBe("/v1/refunds");
    expect(Object.fromEntries(request.body)).toEqual({
      payment_intent: "pi_owned",
      amount: "1",
      "metadata[lenso_refund_id]": "refund-1",
      "metadata[lenso_payment_id]": "payment-1",
    });
    expect(request.headers.get("idempotency-key")).toBe("persisted-refund-key");
    expect(await provider.getRefund("re_owned")).toEqual(value);
    expect(requests.some((item) => item.url.pathname === "/v1/refunds/re_owned")).toBe(true);
  });

  test("integer API units, allowlist, product bounds and ISK/UGX special units", () => {
    const { provider, options } = fixture();
    for (const amount of [0, -1, 1.2, NaN, Infinity, Number.MAX_SAFE_INTEGER + 1, 49, 100_001]) {
      expect(() => provider.validateAmount(amount, "usd")).toThrow();
    }
    for (const currency of ["USD", "xxx", " usd"]) {
      expect(() => provider.validateAmount(100, currency)).toThrow();
    }
    for (const currency of ["isk", "ugx"]) {
      expect(() => provider.validateAmount(101, currency)).toThrow();
      expect(() => provider.validateRefundAmount(1, currency)).toThrow();
      provider.validateAmount(100, currency);
    }
    for (const currency of ["huf", "twd"]) provider.validateAmount(1, currency);
    provider.validateRefundAmount(1, "usd");
    expect(() => provider.validateAmount(6, "eur")).toThrow();
    expect(() => provider.validateRefundAmount(6, "eur")).toThrow();
    expect(() => createStripeProvider({ ...options, toleranceSeconds: 0 })).toThrow();
    expect(() =>
      createStripeProvider({ ...options, currencies: { usd: { min: 0, max: 500 } } }),
    ).toThrow();
    expect(provider.replayWindowMs).toBe(23 * 60 * 60 * 1000);
  });

  test("no SDK retry even for a 500 unknown write outcome", async () => {
    const { provider, requests } = fixture((req) =>
      req.method === "POST"
        ? Response.json(
            { error: { type: "api_error", message: "private detail" } },
            { status: 500 },
          )
        : undefined,
    );
    await expect(provider.createPayment(record(), "persisted-key")).rejects.toMatchObject({
      code: "unavailable",
    });
    expect(requests.filter((item) => item.method === "POST")).toHaveLength(1);
  });

  test("refund HTTP errors and transport failures are unavailable without SDK replay", async () => {
    for (const transportFailure of [false, true]) {
      const { provider, requests } = fixture((req) => {
        if (req.method !== "POST") return;
        if (transportFailure) throw new Error("private connection error");
        return Response.json({ error: { type: "api_error" } }, { status: 500 });
      });
      await expect(
        provider.createRefund(
          { ...record(), providerId: "pi_owned" },
          refundRecord(),
          "durable-refund-key",
        ),
      ).rejects.toMatchObject({ code: "unavailable", message: "Payments: unavailable" });
      expect(requests.filter((item) => item.method === "POST")).toHaveLength(1);
    }
  });

  test("only documented definitive refund refusals are classified not-created", async () => {
    for (const code of [
      "charge_already_refunded",
      "charge_disputed",
      "refund_disputed_payment",
      "idempotency_key_in_use",
      "balance_insufficient",
      "unrecognized",
    ]) {
      const { provider } = fixture((req) =>
        req.method === "POST"
          ? Response.json(
              { error: { type: "invalid_request_error", code, message: "private detail" } },
              { status: 400 },
            )
          : undefined,
      );
      const operation = provider.createRefund(
        { ...record(), providerId: "pi_owned" },
        refundRecord(),
        "rejected-key",
      );
      if (code.startsWith("charge_") || code === "refund_disputed_payment") {
        await expect(operation).rejects.toBeInstanceOf(RefundNotCreatedError);
      } else await expect(operation).rejects.toMatchObject({ code: "unavailable" });
    }
  });

  test("read failures also bypass SDK retries and do not leak upstream details", async () => {
    for (const path of [
      "/v1/account",
      "/v1/payment_intents/pi_owned",
      "/v1/refunds/re_owned",
      "/v1/payment_intents",
      "/v1/refunds",
    ]) {
      const { provider, requests } = fixture((req) =>
        req.url.pathname === path
          ? Response.json(
              { error: { type: "api_error", message: "sensitive upstream content" } },
              { status: 500, headers: { "stripe-should-retry": "true" } },
            )
          : undefined,
      );
      const operation =
        path === "/v1/refunds"
          ? provider.findRefund({ ...record(), providerId: "pi_owned" }, refundRecord())
          : path === "/v1/refunds/re_owned"
            ? provider.getRefund("re_owned")
            : path === "/v1/payment_intents"
              ? provider.findPayment(record())
              : provider.getPayment("pi_owned");
      await expect(operation).rejects.toMatchObject({
        code: "unavailable",
        message: "Payments: unavailable",
      });
      expect(requests.filter((item) => item.url.pathname === path)).toHaveLength(1);
    }
  });

  test("runtime account, PI mode, identity and requested ID are checked", async () => {
    for (const override of [{ livemode: true }, { id: "pi_other" }, { metadata: {} }]) {
      const { provider } = fixture((req) =>
        req.url.pathname.includes("/payment_intents/") ? pi(override) : undefined,
      );
      await expect(provider.getPayment("pi_owned")).rejects.toMatchObject({
        code: "provider-mismatch",
      });
    }
    const wrongAccount = fixture((req) =>
      req.url.pathname === "/v1/account" ? { id: "acct_other" } : undefined,
    );
    await expect(wrongAccount.provider.getPayment("pi_owned")).rejects.toMatchObject({
      code: "provider-mismatch",
    });
    const connected = fixture(undefined, { stripeAccount: "acct_connect" });
    await expect(connected.provider.getPayment("pi_owned")).rejects.toMatchObject({
      code: "provider-mismatch",
    });
    const { provider } = fixture();
    await expect(
      provider.createPayment({ ...record(), tenantId: "x".repeat(257) }, "key"),
    ).rejects.toMatchObject({ code: "invalid-input" });
    await expect(
      provider.createPayment({ ...record(), tenantId: "x".repeat(256) }, "key"),
    ).rejects.toMatchObject({ code: "provider-mismatch" });
  });

  test("256-character identities fit official metadata and persisted keys remain unchanged", async () => {
    const { provider, requests } = fixture((req) =>
      req.method === "POST"
        ? pi({
            metadata: {
              lenso_payment_id: req.body.get("metadata[lenso_payment_id]"),
              lenso_tenant_id: req.body.get("metadata[lenso_tenant_id]"),
              lenso_order_id: req.body.get("metadata[lenso_order_id]"),
            },
          })
        : undefined,
    );
    const value = {
      ...record(),
      tenantId: "t".repeat(256),
      orderId: "o".repeat(256),
      paymentId: "p".repeat(256),
    };
    await provider.createPayment(value, "same-persisted-key");
    await provider.createPayment(value, "same-persisted-key");
    const writes = requests.filter((item) => item.method === "POST");
    expect(writes).toHaveLength(2);
    expect(writes[0].body.toString()).toBe(writes[1].body.toString());
    expect(
      writes.every((item) => item.headers.get("idempotency-key") === "same-persisted-key"),
    ).toBe(true);
    for (const key of ["", "x".repeat(256)]) {
      await expect(provider.createPayment(record(), key)).rejects.toMatchObject({
        code: "invalid-input",
      });
    }
  });

  test("PI recovery scans all created-filtered pages and detects duplicates", async () => {
    const { provider, requests } = fixture((req) => {
      if (req.url.pathname !== "/v1/payment_intents") return;
      return req.url.searchParams.has("starting_after")
        ? { object: "list", data: [pi()], has_more: false }
        : { object: "list", data: [pi({ id: "pi_unrelated", metadata: {} })], has_more: true };
    });
    expect((await provider.findPayment(record()))!.id).toBe("pi_owned");
    const pages = requests.filter((item) => item.url.pathname === "/v1/payment_intents");
    expect(pages[0].url.searchParams.get("created[gte]")).toBe(String(now / 1000 - 300));
    expect(pages[1].url.searchParams.get("starting_after")).toBe("pi_unrelated");
    const duplicate = fixture((req) =>
      req.url.pathname === "/v1/payment_intents"
        ? { data: [pi(), pi({ id: "pi_duplicate" })], has_more: false }
        : undefined,
    );
    await expect(duplicate.provider.findPayment(record())).rejects.toMatchObject({
      code: "provider-mismatch",
    });
  });

  test("complete empty recovery is null; creation-time fallback remains bounded", async () => {
    const { provider, requests } = fixture((req) =>
      ["/v1/payment_intents", "/v1/refunds"].includes(req.url.pathname)
        ? { data: [], has_more: false }
        : undefined,
    );
    expect(await provider.findPayment({ ...record(), attemptedAt: null })).toBeNull();
    expect(requests.at(-1)!.url.searchParams.get("created[gte]")).toBe(
      String((now - 10_000) / 1000 - 300),
    );
    expect(
      await provider.findRefund({ ...record(), providerId: "pi_owned" }, refundRecord()),
    ).toBeNull();
  });

  test("a truncated scan is unavailable, never a false absence", async () => {
    let page = 0;
    const { provider } = fixture((req) =>
      req.url.pathname === "/v1/payment_intents"
        ? { data: [pi({ id: `pi_${++page}`, metadata: {} })], has_more: true }
        : undefined,
    );
    await expect(provider.findPayment(record())).rejects.toMatchObject({ code: "unavailable" });
    expect(page).toBe(100);
  });

  test("refund recovery paginates and checks linked PI metadata and mode", async () => {
    const payment = { ...record(), providerId: "pi_owned" };
    const { provider, requests } = fixture((req) => {
      if (req.url.pathname !== "/v1/refunds") return;
      return req.url.searchParams.has("starting_after")
        ? { data: [re()], has_more: false }
        : { data: [re({ id: "re_unrelated", metadata: {} })], has_more: true };
    });
    expect((await provider.findRefund(payment, refundRecord()))!.id).toBe("re_owned");
    const pages = requests.filter((item) => item.url.pathname === "/v1/refunds");
    expect(pages[0].url.searchParams.get("payment_intent")).toBe("pi_owned");
    expect(pages[1].url.searchParams.get("starting_after")).toBe("re_unrelated");
    const bad = fixture((req) =>
      req.url.pathname === "/v1/refunds/re_owned"
        ? re({ metadata: { lenso_refund_id: "refund-1", lenso_payment_id: "wrong-payment" } })
        : undefined,
    );
    await expect(bad.provider.getRefund("re_owned")).rejects.toMatchObject({
      code: "provider-mismatch",
    });
    const badMode = fixture((req) =>
      req.url.pathname === "/v1/payment_intents/pi_owned" ? pi({ livemode: true }) : undefined,
    );
    await expect(badMode.provider.getRefund("re_owned")).rejects.toMatchObject({
      code: "provider-mismatch",
    });
  });

  test("refund scans fail on duplicates/truncation and mismatched linked PI identity", async () => {
    const payment = { ...record(), providerId: "pi_owned" };
    const duplicate = fixture((req) =>
      req.url.pathname === "/v1/refunds"
        ? { data: [re(), re({ id: "re_duplicate" })], has_more: false }
        : undefined,
    );
    await expect(duplicate.provider.findRefund(payment, refundRecord())).rejects.toMatchObject({
      code: "provider-mismatch",
    });
    let page = 0;
    const truncated = fixture((req) =>
      req.url.pathname === "/v1/refunds"
        ? { data: [re({ id: `re_${++page}`, metadata: {} })], has_more: true }
        : undefined,
    );
    await expect(truncated.provider.findRefund(payment, refundRecord())).rejects.toMatchObject({
      code: "unavailable",
    });
    expect(page).toBe(100);
    for (const overrides of [{ id: "pi_wrong" }, { metadata: {} }]) {
      const bad = fixture((req) =>
        req.url.pathname === "/v1/payment_intents/pi_owned" ? pi(overrides) : undefined,
      );
      await expect(bad.provider.getRefund("re_owned")).rejects.toMatchObject({
        code: "provider-mismatch",
      });
    }
  });

  test("official WebCrypto webhook verification preserves raw bytes and checks both time directions", async () => {
    const { provider, client } = fixture();
    const valid = await signed(client);
    expect((await provider.verifyWebhook(valid.raw, valid.signature))!.object).toMatchObject({
      paymentId: "payment-1",
    });
    await expect(
      provider.verifyWebhook(new Uint8Array([...valid.raw, 32]), valid.signature),
    ).rejects.toMatchObject({ code: "bad-signature" });
    await expect(
      provider.verifyWebhook(valid.raw, valid.signature.replace(/v1=./, "v1=x")),
    ).rejects.toMatchObject({ code: "bad-signature" });
    for (const offset of [-301, 301]) {
      const expired = await signed(client, pi(), {}, Math.floor(Date.now() / 1000) + offset);
      await expect(provider.verifyWebhook(expired.raw, expired.signature)).rejects.toMatchObject({
        code: "bad-signature",
      });
    }
    for (const extra of [
      { livemode: true },
      { account: "acct_owned" },
      { account: "acct_connect" },
    ]) {
      const event = await signed(client, pi(), extra);
      await expect(provider.verifyWebhook(event.raw, event.signature)).rejects.toMatchObject({
        code: "provider-mismatch",
      });
    }
    const irrelevant = await signed(client, pi(), { type: "customer.created" });
    expect(await provider.verifyWebhook(irrelevant.raw, irrelevant.signature)).toBeNull();
  });

  test("PI webhooks use actual status, validate amount/currency/metadata and reject organization routing", async () => {
    const { provider, client, options } = fixture();
    for (const status of [
      "requires_payment_method",
      "requires_confirmation",
      "requires_action",
      "processing",
      "requires_capture",
      "succeeded",
      "canceled",
    ]) {
      const event = await signed(client, pi({ status }), { type: "payment_intent.updated" });
      expect((await provider.verifyWebhook(event.raw, event.signature))!.object).toMatchObject({
        status,
      });
    }
    for (const overrides of [
      { amount: 0 },
      { currency: "USD" },
      { metadata: {} },
      { status: "future_unknown_status" },
      { livemode: true },
    ]) {
      const event = await signed(client, pi(overrides));
      await expect(provider.verifyWebhook(event.raw, event.signature)).rejects.toMatchObject({
        code: "provider-mismatch",
      });
    }
    const organizationEvent = await signed(client, pi(), { context: "acct_owned" });
    await expect(
      provider.verifyWebhook(organizationEvent.raw, organizationEvent.signature),
    ).rejects.toMatchObject({ code: "provider-mismatch" });
    const organization = fixture(undefined, { stripeContext: "acct_owned" });
    const event = await signed(client);
    await expect(
      organization.provider.verifyWebhook(event.raw, event.signature),
    ).rejects.toMatchObject({ code: "provider-mismatch" });
    const liveFixture = fixture((req) =>
      req.url.pathname === "/v1/payment_intents/pi_owned" ? pi({ livemode: true }) : undefined,
    );
    const liveProvider = createStripeProvider({ ...liveFixture.options, live: true });
    const liveEvent = await signed(liveFixture.client, pi({ livemode: true }), { livemode: true });
    expect((await liveProvider.verifyWebhook(liveEvent.raw, liveEvent.signature))!.live).toBe(true);
    const wrongLive = await signed(liveFixture.client);
    await expect(
      liveProvider.verifyWebhook(wrongLive.raw, wrongLive.signature),
    ).rejects.toMatchObject({ code: "provider-mismatch" });
    expect(options.client).toBe(client);
  });

  test("refund webhook requires online linked PI verification, failures cannot be acknowledged", async () => {
    const { provider, client } = fixture();
    for (const type of ["refund.created", "refund.updated", "refund.failed"]) {
      const event = await signed(client, re(), { type });
      expect((await provider.verifyWebhook(event.raw, event.signature))!.object).toMatchObject({
        refundId: "refund-1",
        live: false,
      });
    }
    const offline = fixture((req) =>
      req.url.pathname === "/v1/payment_intents/pi_owned"
        ? Response.json({ error: { type: "api_error" } }, { status: 500 })
        : undefined,
    );
    const event = await signed(offline.client, re(), { type: "refund.created" });
    await expect(offline.provider.verifyWebhook(event.raw, event.signature)).rejects.toMatchObject({
      code: "unavailable",
    });
  });
});
