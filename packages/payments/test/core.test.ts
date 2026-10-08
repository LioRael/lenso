import { afterEach, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { drizzle } from "drizzle-orm/bun-sqlite";
import {
  createPayments,
  PaymentsError,
  RefundNotCreatedError,
  type PaymentsProvider,
  type ProviderPayment,
  type ProviderRefund,
} from "../src";
import { sqlitePaymentsStore } from "../src/drizzle/sqlite";
import { createPaymentsWebhookHandler } from "../src/fetch";
import { createPaymentsReconciliationTask } from "../src/tasks";

const opened: Database[] = [];
afterEach(() => {
  for (const db of opened.splice(0)) db.close();
});
const input = {
  tenantId: "tenant-a",
  orderId: "order-a",
  key: "checkout-a",
  amount: 1000,
  currency: "usd",
};

async function fixture() {
  const db = new Database(":memory:");
  opened.push(db);
  db.exec(await Bun.file(new URL("../migrations/sqlite.sql", import.meta.url)).text());
  const store = sqlitePaymentsStore(drizzle(db));
  let time = Date.now();
  let paymentPosts = 0;
  let refundPosts = 0;
  let queries = 0;
  let losePayment = false;
  let loseRefund = false;
  let available = true;
  const payments = new Map<string, ProviderPayment>();
  const refunds = new Map<string, ProviderRefund>();
  const validate = (amount: number, currency: string) => {
    if (!Number.isSafeInteger(amount) || amount < 1 || currency !== "usd")
      throw new PaymentsError("invalid-input");
  };
  const provider: PaymentsProvider = {
    accountId: "acct_fixture",
    live: false,
    replayWindowMs: 23 * 3_600_000,
    validateAmount: validate,
    validateRefundAmount: validate,
    async createPayment(record, _key, beforeWrite) {
      await beforeWrite?.();
      paymentPosts++;
      const result: ProviderPayment = {
        ...record,
        id: `pi_${record.paymentId}`,
        status: "requires_payment_method",
        received: 0,
      };
      payments.set(result.id, result);
      if (losePayment) throw new Error("fixture lost response after commit");
      return result;
    },
    async getPayment(id) {
      queries++;
      if (!available) throw new Error("fixture unavailable");
      return structuredClone(payments.get(id)!);
    },
    async findPayment(record) {
      queries++;
      if (!available) throw new Error("fixture unavailable");
      return structuredClone(
        [...payments.values()].find((item) => item.paymentId === record.paymentId) ?? null,
      );
    },
    async clientSecret() {
      return "fixture-client-secret";
    },
    async createRefund(record, refund, _key, beforeWrite) {
      await beforeWrite?.();
      refundPosts++;
      const result: ProviderRefund = {
        id: `re_${refund.refundId}`,
        refundId: refund.refundId,
        paymentId: record.paymentId,
        paymentProviderId: record.providerId!,
        accountId: record.accountId,
        live: record.live,
        amount: refund.amount,
        currency: record.currency,
        status: "pending",
      };
      refunds.set(result.id, result);
      if (loseRefund) throw new Error("fixture lost refund response");
      return result;
    },
    async getRefund(id) {
      if (!available) throw new Error("fixture unavailable");
      return structuredClone(refunds.get(id)!);
    },
    async findRefund(_record, refund) {
      if (!available) throw new Error("fixture unavailable");
      return structuredClone(
        [...refunds.values()].find((item) => item.refundId === refund.refundId) ?? null,
      );
    },
    // Signature verification is separately exercised with the official Stripe SDK HTTP fixture.
    async verifyWebhook(raw, signature) {
      if (signature !== "fixture-signature") throw new PaymentsError("bad-signature");
      return JSON.parse(new TextDecoder().decode(raw));
    },
  };
  const make = (customStore = store) =>
    createPayments({
      store: customStore,
      provider,
      clock: () => time,
      authorize: async (tenant: string, _action, record) => {
        if (tenant !== record.tenantId) throw new PaymentsError("forbidden");
      },
    });
  const runtime = make();
  async function paid() {
    const payment = await runtime.payments.create(input, input.tenantId);
    const remote = payments.get(payment.providerId!)!;
    remote.status = "succeeded";
    remote.received = input.amount;
    await runtime.reconciliation.run(payment.paymentId);
    return payment;
  }
  return {
    store,
    provider,
    runtime,
    make,
    payments,
    refunds,
    paid,
    counts: () => ({ paymentPosts, refundPosts, queries }),
    advance: (delta: number) => {
      time += delta;
    },
    losePayment: () => {
      losePayment = true;
    },
    loseRefund: () => {
      loseRefund = true;
    },
    unavailable: () => {
      available = false;
    },
    event: (eventId: string, object: ProviderPayment | ProviderRefund) =>
      new TextEncoder().encode(
        JSON.stringify({
          eventId,
          object,
          accountId: provider.accountId,
          live: provider.live,
        }),
      ),
  };
}

test("concurrent create persists one request; same key/different parameters and same order/new key reject", async () => {
  const f = await fixture();
  const results = await Promise.all(
    Array.from({ length: 6 }, () => f.runtime.payments.create(input, input.tenantId)),
  );
  expect(new Set(results.map((payment) => payment.paymentId)).size).toBe(1);
  expect(f.counts().paymentPosts).toBe(1);
  for (const patch of [{ amount: 1001 }, { orderId: "other-order" }, { key: "new-key" }]) {
    await expect(
      f.runtime.payments.create({ ...input, ...patch }, input.tenantId),
    ).rejects.toMatchObject({ code: "conflict" });
  }
  expect(JSON.stringify(results)).not.toContain("secret");
});

test("lost response remains unknown; restart/repeat does not POST and query recovers it", async () => {
  const f = await fixture();
  f.losePayment();
  const payment = await f.runtime.payments.create(input, input.tenantId);
  expect(payment.status).toBe("unknown");
  const restarted = f.make();
  await restarted.payments.create(input, input.tenantId);
  expect(f.counts().paymentPosts).toBe(1);
  await restarted.reconciliation.run(payment.paymentId);
  expect(f.counts().queries).toBeGreaterThan(0);
  expect(f.counts().paymentPosts).toBe(1);
  expect((await restarted.payments.get(payment, input.tenantId)).status).toBe(
    "requires_payment_method",
  );
});

test("expired unknown never replays POST even if provider query returns nothing", async () => {
  const f = await fixture();
  f.losePayment();
  const payment = await f.runtime.payments.create(input, input.tenantId);
  f.payments.clear();
  f.advance(24 * 3_600_000);
  expect(await f.runtime.reconciliation.run(payment.paymentId)).toBe(false);
  expect(f.counts().paymentPosts).toBe(1);
  expect((await f.runtime.payments.get(payment, input.tenantId)).status).toBe("unknown");
});

test("refund reservations guard concurrency; duplicate refund and changed amount use durable keys", async () => {
  const f = await fixture();
  const payment = await f.paid();
  const attempts = await Promise.allSettled(
    ["a", "b"].map((key) =>
      f.runtime.payments.refund({ paymentId: payment.paymentId, amount: 600, key }, input.tenantId),
    ),
  );
  expect(attempts.filter((attempt) => attempt.status === "fulfilled")).toHaveLength(1);
  expect(f.counts().refundPosts).toBe(1);
  const persisted = (await f.store.get(payment.paymentId))!;
  const key = persisted.refunds[0].key;
  await f.runtime.payments.refund(
    { paymentId: payment.paymentId, amount: 600, key },
    input.tenantId,
  );
  expect(f.counts().refundPosts).toBe(1);
  await expect(
    f.runtime.payments.refund({ paymentId: payment.paymentId, amount: 599, key }, input.tenantId),
  ).rejects.toMatchObject({ code: "conflict" });
});

test("unknown refund reserves its cap and query recovers without another refund POST", async () => {
  const f = await fixture();
  const payment = await f.paid();
  f.loseRefund();
  const refunded = await f.runtime.payments.refund(
    { paymentId: payment.paymentId, key: "r", amount: 1000 },
    input.tenantId,
  );
  expect(refunded.refunds[0].status).toBe("unknown");
  await expect(
    f.runtime.payments.refund(
      { paymentId: payment.paymentId, key: "other", amount: 1 },
      input.tenantId,
    ),
  ).rejects.toMatchObject({ code: "conflict" });
  await f.runtime.reconciliation.run(payment.paymentId);
  expect(f.counts().refundPosts).toBe(1);
  expect((await f.runtime.payments.get(payment, input.tenantId)).refunds[0].status).toBe("pending");
});

test("durable webhook dedup and out-of-order snapshots do not regress or duplicate business results", async () => {
  const f = await fixture();
  const payment = await f.runtime.payments.create(input, input.tenantId);
  const remote = f.payments.get(payment.providerId!)!;
  const stale = structuredClone(remote);
  remote.status = "succeeded";
  remote.received = input.amount;
  const raw = f.event("event-paid", remote);
  expect(await f.runtime.webhook.receive(raw, "fixture-signature")).toEqual({
    accepted: true,
    duplicate: false,
  });
  expect(await f.runtime.webhook.receive(raw, "fixture-signature")).toEqual({
    accepted: true,
    duplicate: true,
  });
  expect((await f.runtime.reconciliation.drain()).events).toBe(1);
  await f.runtime.webhook.receive(f.event("event-stale", stale), "fixture-signature");
  await f.runtime.webhook.receive(f.event("event-another-success", remote), "fixture-signature");
  await f.runtime.reconciliation.drain();
  expect((await f.runtime.payments.get(payment, input.tenantId)).status).toBe("succeeded");
  const results = await f.runtime.payments.results(payment, input.tenantId);
  expect(results).toHaveLength(1);
  expect(results[0].resultId).toBe(`${payment.paymentId}:succeeded`);
  await f.runtime.payments.refund(
    { paymentId: payment.paymentId, key: "r", amount: 1000 },
    input.tenantId,
  );
  const refund = [...f.refunds.values()][0];
  refund.status = "succeeded";
  await f.runtime.webhook.receive(f.event("refund-success", refund), "fixture-signature");
  await f.runtime.webhook.receive(f.event("refund-success-again", refund), "fixture-signature");
  await f.runtime.reconciliation.drain();
  expect(await f.runtime.payments.results(payment, input.tenantId)).toHaveLength(2);
});

test("bad signature/wrong amount/object/account/mode do not reach the inbox", async () => {
  const f = await fixture();
  const payment = await f.runtime.payments.create(input, input.tenantId);
  const object = f.payments.get(payment.providerId!)!;
  await expect(f.runtime.webhook.receive(f.event("bad", object), "invalid")).rejects.toMatchObject({
    code: "bad-signature",
  });
  for (const patch of [
    { amount: 999 },
    { id: "pi_other" },
    { currency: "jpy" },
    { accountId: "acct_other" },
    { live: true },
  ]) {
    await expect(
      f.runtime.webhook.receive(f.event("mismatch", { ...object, ...patch }), "fixture-signature"),
    ).rejects.toMatchObject({ code: "provider-mismatch" });
  }
  expect(
    await f.store.inbox(f.provider.accountId, false, Number.MAX_SAFE_INTEGER, 100),
  ).toHaveLength(0);
});

test("non-callback methods all authorize the loaded tenant; no forged browser identity for webhook", async () => {
  const f = await fixture();
  const payment = await f.runtime.payments.create(input, input.tenantId);
  for (const method of ["get", "results", "clientSecret"] as const) {
    await expect(f.runtime.payments[method](payment, "tenant-b")).rejects.toMatchObject({
      code: "forbidden",
    });
  }
  await expect(
    f.runtime.payments.refund({ paymentId: payment.paymentId, key: "r", amount: 1 }, "tenant-b"),
  ).rejects.toMatchObject({ code: "forbidden" });
  await expect(f.runtime.payments.create(input, "tenant-b")).rejects.toMatchObject({
    code: "forbidden",
  });
});

test("HTTP ingress preserves raw bytes, applies bound, and does not acknowledge failed persistence", async () => {
  const f = await fixture();
  const payment = await f.runtime.payments.create(input, input.tenantId);
  const raw = f.event("event-http", f.payments.get(payment.providerId!)!);
  const handler = createPaymentsWebhookHandler({ webhook: f.runtime.webhook });
  const request = (body: Uint8Array = raw) =>
    new Request("https://fixture.invalid/hook", {
      method: "POST",
      headers: { "stripe-signature": "fixture-signature" },
      body: new Uint8Array(body),
    });
  expect((await handler(request())).status).toBe(204);
  expect(
    (await createPaymentsWebhookHandler({ webhook: f.runtime.webhook, maxBytes: 1 })(request()))
      .status,
  ).toBe(413);
  const failing = f.make({
    ...f.store,
    async receive() {
      throw new Error("fixture DB unavailable");
    },
  });
  expect((await createPaymentsWebhookHandler({ webhook: failing.webhook })(request())).status).toBe(
    503,
  );
  expect(
    (
      await createPaymentsWebhookHandler({
        webhook: f.runtime.webhook,
        wake: async () => {
          throw new PaymentsError("invalid-input");
        },
      })(request())
    ).status,
  ).toBe(503);
});

test("the write guard rechecks expiration after provider preflight, before its POST", async () => {
  const f = await fixture();
  const original = f.provider.createPayment;
  f.provider.createPayment = async (record, key, beforeWrite) => {
    f.advance(61_000);
    return original(record, key, beforeWrite);
  };
  expect((await f.runtime.payments.create(input, input.tenantId)).status).toBe("unknown");
  expect(f.counts().paymentPosts).toBe(0);
});

test("historical terminal refund reads renew the lease instead of starving a new refund", async () => {
  const f = await fixture();
  const payment = await f.paid();
  const current = (await f.store.get(payment.paymentId))!;
  const history = Array.from({ length: 20 }, (_, index) => ({
    refundId: `historical-${index}`,
    key: `historical-${index}`,
    amount: 1,
    status: "failed" as const,
    providerId: `re_historical_${index}`,
    attemptedAt: current.createdAt,
  }));
  for (const refund of history) {
    f.refunds.set(refund.providerId, {
      ...refund,
      id: refund.providerId,
      paymentId: payment.paymentId,
      paymentProviderId: payment.providerId!,
      accountId: current.accountId,
      live: current.live,
      currency: current.currency,
    });
  }
  expect(
    await f.store.compareAndSet(
      {
        ...current,
        refunds: history,
        revision: current.revision + 1,
      },
      current.revision,
    ),
  ).toBe(true);
  const original = f.provider.getRefund;
  f.provider.getRefund = async (id) => {
    f.advance(4_000);
    return original(id);
  };
  const result = await f.runtime.payments.refund(
    {
      paymentId: payment.paymentId,
      key: "after-history",
      amount: 1000,
    },
    input.tenantId,
  );
  expect(result.refunds.at(-1)?.status).toBe("pending");
  expect(f.counts().refundPosts).toBe(1);
});

test("Tasks schedules continuation at persisted due time, including empty early scans", async () => {
  const f = await fixture();
  f.losePayment();
  await f.runtime.payments.create(input, input.tenantId);
  f.unavailable();
  f.advance(60_000);
  const task = createPaymentsReconciliationTask({
    name: "fixture.payments",
    runtime: () => f.runtime.reconciliation,
    queue: () => ({
      async enqueue(registered, payload, options) {
        expect(Object.is(registered, task)).toBe(true);
        expect(payload).toEqual({ limit: 10 });
        expect(options?.runAt?.getTime()).toBeGreaterThan(Date.now());
        expect(options?.deduplicationKey).toContain("fixture");
        return "fixture-continuation";
      },
    }),
  });
  const context = { jobId: "fixture", attempt: 1, signal: new AbortController().signal };
  expect(await task.handler({ limit: 10 }, context)).toMatchObject({ unresolved: 1 });
  expect(await task.handler({ limit: 10 }, context)).toMatchObject({ unresolved: 0 });
  expect(task.retry?.backoff).toBe(true);
});

test("persisted webhook identity recovers unknown payment and refund after replay expiry", async () => {
  const f = await fixture();
  f.losePayment();
  const payment = await f.runtime.payments.create(input, input.tenantId);
  const remote = [...f.payments.values()][0];
  remote.status = "succeeded";
  remote.received = input.amount;
  f.provider.findPayment = async () => null;
  f.advance(24 * 3_600_000);
  await f.runtime.webhook.receive(f.event("recover-payment", remote), "fixture-signature");
  await f.runtime.reconciliation.drain();
  expect((await f.runtime.payments.get(payment, input.tenantId)).status).toBe("succeeded");
  expect(f.counts().paymentPosts).toBe(1);
  f.loseRefund();
  await f.runtime.payments.refund(
    { paymentId: payment.paymentId, key: "r", amount: 1000 },
    input.tenantId,
  );
  const refund = [...f.refunds.values()][0];
  refund.status = "succeeded";
  f.provider.findRefund = async () => null;
  f.advance(24 * 3_600_000);
  await f.runtime.webhook.receive(f.event("recover-refund", refund), "fixture-signature");
  await f.runtime.reconciliation.drain();
  expect((await f.runtime.payments.get(payment, input.tenantId)).refunds[0].status).toBe(
    "succeeded",
  );
  expect(f.counts().refundPosts).toBe(1);
});

test("a delayed first attempt or lost lease is rechecked before provider POST", async () => {
  for (const lostLease of [false, true]) {
    const f = await fixture();
    let delayed = false;
    const runtime = f.make({
      ...f.store,
      async compareAndSet(record, revision) {
        const success = await f.store.compareAndSet(record, revision);
        if (success && record.attemptedAt !== null && !delayed) {
          delayed = true;
          if (lostLease) {
            await f.store.compareAndSet(
              {
                ...record,
                revision: record.revision + 1,
                lease: { token: "new-owner", until: Date.now() + 60_000 },
              },
              record.revision,
            );
          } else f.advance(24 * 3_600_000);
        }
        return success;
      },
    });
    expect((await runtime.payments.create(input, input.tenantId)).status).toBe("unknown");
    expect(f.counts().paymentPosts).toBe(0);
  }
});

test("state observation DB failure keeps reservation; later webhook repairs it", async () => {
  const f = await fixture();
  let fail = true;
  const runtime = f.make({
    ...f.store,
    async compareAndSet(record, revision) {
      if (record.providerId !== null && fail) {
        fail = false;
        throw new Error("fixture DB failed before commit");
      }
      return f.store.compareAndSet(record, revision);
    },
  });
  const payment = await runtime.payments.create(input, input.tenantId);
  expect(payment.status).toBe("unknown");
  const remote = [...f.payments.values()][0];
  await runtime.webhook.receive(f.event("repair", remote), "fixture-signature");
  await runtime.reconciliation.drain();
  expect((await runtime.payments.get(payment, input.tenantId)).providerId).toBe(remote.id);
  expect(f.counts().paymentPosts).toBe(1);
});

test("verified failed refund or definitive not-created rejection releases only that reservation", async () => {
  const f = await fixture();
  const payment = await f.paid();
  await f.runtime.payments.refund(
    { paymentId: payment.paymentId, key: "r1", amount: 1000 },
    input.tenantId,
  );
  [...f.refunds.values()][0].status = "failed";
  await f.runtime.reconciliation.run(payment.paymentId);
  f.provider.createRefund = async () => {
    throw new RefundNotCreatedError();
  };
  await f.runtime.payments.refund(
    { paymentId: payment.paymentId, key: "r2", amount: 1000 },
    input.tenantId,
  );
  const before = await f.runtime.payments.results(payment, input.tenantId);
  expect(before.filter((result) => result.kind === "refund.failed")).toHaveLength(2);
  await f.runtime.payments.refund(
    { paymentId: payment.paymentId, key: "r2", amount: 1000 },
    input.tenantId,
  );
  expect(await f.runtime.payments.results(payment, input.tenantId)).toEqual(before);
  await f.runtime.payments.refund(
    { paymentId: payment.paymentId, key: "r3", amount: 1000 },
    input.tenantId,
  );
  expect((await f.runtime.payments.get(payment, input.tenantId)).refunds).toHaveLength(3);
});

test("a full inbox batch reports pending continuation even when all processed events succeed", async () => {
  const f = await fixture();
  const payment = await f.paid();
  for (let index = 0; index < 3; index++) {
    await f.runtime.webhook.receive(
      f.event(`batch-${index}`, f.payments.get(payment.providerId!)!),
      "fixture-signature",
    );
  }
  const result = await f.runtime.reconciliation.drain(1);
  expect(result.events).toBe(1);
  expect(result.unresolved).toBe(0);
  expect(result.nextRunAt).not.toBeNull();
});

test("queue enqueue failure keeps durable recovery and triggers the existing Tasks retry policy", async () => {
  const f = await fixture();
  await f.runtime.payments.create(input, input.tenantId);
  const task = createPaymentsReconciliationTask({
    name: "fixture.retry",
    runtime: () => f.runtime.reconciliation,
    queue: () => ({
      async enqueue() {
        throw new Error("fixture queue unavailable");
      },
    }),
  });
  await expect(
    task.handler(
      { limit: 10 },
      { jobId: "fixture", attempt: 1, signal: new AbortController().signal },
    ),
  ).rejects.toThrow("fixture queue unavailable");
  expect(task.retry?.backoff).toBe(true);
});
