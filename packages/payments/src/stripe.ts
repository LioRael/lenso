import Stripe from "stripe";
import {
  PaymentsError,
  RefundNotCreatedError,
  type PaymentRecord,
  type PaymentsProvider,
  type ProviderPayment,
  type ProviderRefund,
  type RefundRecord,
  type VerifiedPaymentEvent,
} from "./contracts";

export interface StripeProviderOptions {
  /** Borrowed, single-account SDK client. Its owner retains its lifetime. */
  client: Stripe;
  accountId: string;
  live: boolean;
  webhookSecret: string;
  currencies: Record<string, { min: number; max: number; multiple?: number }>;
  toleranceSeconds?: number;
  requestTimeoutMs?: number;
}

const paymentStatuses = new Set([
  "requires_payment_method",
  "requires_confirmation",
  "requires_action",
  "processing",
  "requires_capture",
  "succeeded",
  "canceled",
]);
const refundStatuses = new Set(["pending", "requires_action", "succeeded", "failed", "canceled"]);
const positiveInteger = (value: number) => Number.isSafeInteger(value) && value > 0;
const identity = (value: unknown): value is string =>
  typeof value === "string" && value.length > 0 && value.length <= 256;
const mismatch = () => new PaymentsError("provider-mismatch");

export function createStripeProvider(options: StripeProviderOptions): PaymentsProvider {
  const { client, accountId, live, webhookSecret } = options;
  const tolerance = options.toleranceSeconds ?? 300;
  const timeout = options.requestTimeoutMs ?? 10_000;
  if (
    !identity(accountId) ||
    typeof live !== "boolean" ||
    !webhookSecret ||
    !positiveInteger(tolerance) ||
    !positiveInteger(timeout) ||
    timeout > 60_000
  ) {
    throw new PaymentsError("invalid-input");
  }
  const currencies = new Map(
    Object.entries(options.currencies).map(([currency, rule]) => {
      if (
        !/^[a-z]{3}$/.test(currency) ||
        !positiveInteger(rule.min) ||
        !positiveInteger(rule.max) ||
        rule.min > rule.max ||
        (rule.multiple !== undefined && !positiveInteger(rule.multiple))
      ) {
        throw new PaymentsError("invalid-input");
      }
      return [currency, { ...rule }] as const;
    }),
  );
  const requestOptions: Stripe.RequestOptions = { maxNetworkRetries: 0, timeout };

  function validate(amount: number, currency: string, charge: boolean): void {
    const rule = currencies.get(currency);
    if (
      !rule ||
      !positiveInteger(amount) ||
      amount > rule.max ||
      (charge && amount < rule.min) ||
      amount % (rule.multiple ?? 1) !== 0 ||
      ((currency === "isk" || currency === "ugx") && amount % 100 !== 0)
    ) {
      throw new PaymentsError("invalid-input");
    }
  }

  async function request<T>(operation: () => Promise<T>): Promise<T> {
    try {
      return await operation();
    } catch (error) {
      if (error instanceof PaymentsError) throw error;
      // An HTTP 500 may have committed a write. Never classify it as absence.
      throw new PaymentsError("unavailable");
    }
  }

  function checkClient(): void {
    // v23 has no public configuration snapshot. Reject inherited routing rather
    // than accidentally verifying or operating on a connected/organization account.
    if (client.getApiField("stripeAccount") || client.getApiField("stripeContext")) {
      throw mismatch();
    }
  }
  let accountCheck: Promise<void> | undefined;
  async function checkAccount(): Promise<void> {
    checkClient();
    accountCheck ??= request(async () => {
      const account = await client.accounts.retrieve(null, {}, requestOptions);
      if (account.id !== accountId) throw mismatch();
    }).catch((error: unknown) => {
      accountCheck = undefined;
      throw error;
    });
    await accountCheck;
  }

  function projectPayment(pi: Stripe.PaymentIntent): ProviderPayment {
    const metadata = pi.metadata;
    if (
      pi.object !== "payment_intent" ||
      !identity(pi.id) ||
      pi.livemode !== live ||
      !metadata ||
      !identity(metadata.lenso_payment_id) ||
      !identity(metadata.lenso_tenant_id) ||
      !identity(metadata.lenso_order_id) ||
      !paymentStatuses.has(pi.status) ||
      !Number.isSafeInteger(pi.amount_received) ||
      pi.amount_received < 0 ||
      pi.amount_received > pi.amount
    )
      throw mismatch();
    try {
      validate(pi.amount, pi.currency, true);
    } catch {
      throw mismatch();
    }
    return {
      id: pi.id,
      paymentId: metadata.lenso_payment_id,
      tenantId: metadata.lenso_tenant_id,
      orderId: metadata.lenso_order_id,
      accountId,
      live,
      amount: pi.amount,
      received: pi.amount_received,
      currency: pi.currency,
      status: pi.status as ProviderPayment["status"],
    };
  }

  function matchPayment(payment: ProviderPayment, record: PaymentRecord): void {
    if (
      record.accountId !== accountId ||
      record.live !== live ||
      payment.paymentId !== record.paymentId ||
      payment.tenantId !== record.tenantId ||
      payment.orderId !== record.orderId ||
      payment.amount !== record.amount ||
      payment.currency !== record.currency ||
      (record.providerId !== null && payment.id !== record.providerId)
    )
      throw mismatch();
  }

  async function retrievePayment(id: string): Promise<Stripe.PaymentIntent> {
    if (!identity(id)) throw new PaymentsError("invalid-input");
    await checkAccount();
    const pi = await request(() => client.paymentIntents.retrieve(id, {}, requestOptions));
    if (pi.id !== id) throw mismatch();
    projectPayment(pi);
    return pi;
  }

  async function projectRefund(refund: Stripe.Refund): Promise<ProviderRefund> {
    const piId =
      typeof refund.payment_intent === "string" ? refund.payment_intent : refund.payment_intent?.id;
    if (
      refund.object !== "refund" ||
      !identity(refund.id) ||
      !identity(piId) ||
      !refund.metadata ||
      !identity(refund.metadata.lenso_refund_id) ||
      !identity(refund.metadata.lenso_payment_id) ||
      !refundStatuses.has(refund.status ?? "")
    ) {
      throw mismatch();
    }
    const payment = projectPayment(await retrievePayment(piId));
    if (
      refund.metadata.lenso_payment_id !== payment.paymentId ||
      refund.currency !== payment.currency ||
      refund.amount > payment.amount
    )
      throw mismatch();
    try {
      validate(refund.amount, refund.currency, false);
    } catch {
      throw mismatch();
    }
    return {
      id: refund.id,
      refundId: refund.metadata.lenso_refund_id,
      paymentId: payment.paymentId,
      paymentProviderId: payment.id,
      accountId,
      live: payment.live,
      amount: refund.amount,
      currency: refund.currency,
      status: refund.status as ProviderRefund["status"],
    };
  }

  function matchRefund(value: ProviderRefund, record: PaymentRecord, refund: RefundRecord): void {
    if (
      value.refundId !== refund.refundId ||
      value.paymentId !== record.paymentId ||
      value.paymentProviderId !== record.providerId ||
      value.amount !== refund.amount ||
      value.currency !== record.currency ||
      (refund.providerId !== null && value.id !== refund.providerId)
    )
      throw mismatch();
  }

  function writeOptions(key: string): Stripe.RequestOptions {
    if (typeof key !== "string" || !key.length || key.length > 255) {
      throw new PaymentsError("invalid-input");
    }
    return { ...requestOptions, idempotencyKey: key };
  }

  async function scan<T extends { id: string }>(
    page: (cursor?: string) => Promise<Stripe.ApiList<T>>,
    matches: (value: T) => boolean,
  ): Promise<T | null> {
    let cursor: string | undefined;
    let found: T | null = null;
    for (let count = 0; count < 100; count++) {
      const result = await request(() => page(cursor));
      for (const value of result.data) {
        if (matches(value)) {
          if (found) throw mismatch();
          found = value;
        }
      }
      if (!result.has_more) return found;
      const next = result.data.at(-1)?.id;
      if (!next || next === cursor) throw new PaymentsError("unavailable");
      cursor = next;
    }
    throw new PaymentsError("unavailable");
  }

  return {
    accountId,
    live,
    replayWindowMs: 23 * 60 * 60 * 1000,
    validateAmount: (amount, currency) => validate(amount, currency, true),
    validateRefundAmount: (amount, currency) => validate(amount, currency, false),
    async createPayment(record, key, beforeWrite) {
      validate(record.amount, record.currency, true);
      if (
        record.accountId !== accountId ||
        record.live !== live ||
        !identity(record.paymentId) ||
        !identity(record.tenantId) ||
        !identity(record.orderId)
      ) {
        throw new PaymentsError("invalid-input");
      }
      const opts = writeOptions(key);
      await checkAccount();
      await beforeWrite?.();
      const payment = projectPayment(
        await request(() =>
          client.paymentIntents.create(
            {
              amount: record.amount,
              currency: record.currency,
              capture_method: "automatic",
              automatic_payment_methods: { enabled: true },
              metadata: {
                lenso_payment_id: record.paymentId,
                lenso_tenant_id: record.tenantId,
                lenso_order_id: record.orderId,
              },
            },
            opts,
          ),
        ),
      );
      matchPayment(payment, record);
      return payment;
    },
    async getPayment(id) {
      return projectPayment(await retrievePayment(id));
    },
    async findPayment(record) {
      if (record.accountId !== accountId || record.live !== live) throw mismatch();
      const since = record.attemptedAt ?? record.createdAt;
      if (!Number.isSafeInteger(since) || since < 0) throw new PaymentsError("invalid-input");
      await checkAccount();
      const pi = await scan(
        (cursor) =>
          client.paymentIntents.list(
            {
              limit: 100,
              created: { gte: Math.max(0, Math.floor(since / 1000) - tolerance) },
              ...(cursor ? { starting_after: cursor } : {}),
            },
            requestOptions,
          ),
        (value) => value.metadata?.lenso_payment_id === record.paymentId,
      );
      if (!pi) return null;
      const payment = projectPayment(pi);
      matchPayment(payment, record);
      return payment;
    },
    async clientSecret(id) {
      const pi = await retrievePayment(id);
      if (pi.client_secret !== null && typeof pi.client_secret !== "string") throw mismatch();
      return pi.client_secret;
    },
    async createRefund(record, refund, key, beforeWrite) {
      validate(refund.amount, record.currency, false);
      if (
        !identity(refund.refundId) ||
        !identity(record.providerId) ||
        refund.amount > record.amount
      ) {
        throw new PaymentsError("invalid-input");
      }
      const opts = writeOptions(key);
      const payment = projectPayment(await retrievePayment(record.providerId));
      matchPayment(payment, record);
      await beforeWrite?.();
      let created: Stripe.Refund;
      try {
        created = await client.refunds.create(
          {
            payment_intent: record.providerId!,
            amount: refund.amount,
            metadata: { lenso_refund_id: refund.refundId, lenso_payment_id: record.paymentId },
          },
          opts,
        );
      } catch (error) {
        // These documented refund rejections cannot create a Refund. Do not generalize to
        // arbitrary 400s, idempotency errors, balance errors, transport failures or 500s.
        if (
          error instanceof Stripe.errors.StripeInvalidRequestError &&
          error.statusCode === 400 &&
          ["charge_already_refunded", "charge_disputed", "refund_disputed_payment"].includes(
            error.code ?? "",
          )
        ) {
          throw new RefundNotCreatedError();
        }
        throw new PaymentsError("unavailable");
      }
      const value = await projectRefund(created);
      matchRefund(value, record, refund);
      return value;
    },
    async getRefund(id) {
      if (!identity(id)) throw new PaymentsError("invalid-input");
      await checkAccount();
      const refund = await request(() => client.refunds.retrieve(id, {}, requestOptions));
      if (refund.id !== id) throw mismatch();
      return projectRefund(refund);
    },
    async findRefund(record, refund) {
      if (!identity(record.providerId)) throw new PaymentsError("invalid-input");
      const payment = projectPayment(await retrievePayment(record.providerId));
      matchPayment(payment, record);
      const value = await scan(
        (cursor) =>
          client.refunds.list(
            {
              payment_intent: record.providerId!,
              limit: 100,
              ...(cursor ? { starting_after: cursor } : {}),
            },
            requestOptions,
          ),
        (item) => item.metadata?.lenso_refund_id === refund.refundId,
      );
      if (!value) return null;
      const result = await projectRefund(value);
      matchRefund(result, record, refund);
      return result;
    },
    async verifyWebhook(raw, signature): Promise<VerifiedPaymentEvent | null> {
      let event: Stripe.Event;
      try {
        event = await client.webhooks.constructEventAsync(
          raw,
          signature,
          webhookSecret,
          tolerance,
          Stripe.createSubtleCryptoProvider(),
        );
        // The SDK checks old timestamps but accepts arbitrarily future ones.
        let timestamp = NaN;
        for (const part of signature.split(",")) {
          const [name, value] = part.split("=");
          if (name === "t") timestamp = Number(value);
        }
        if (
          !Number.isSafeInteger(timestamp) ||
          timestamp > Math.floor(Date.now() / 1000) + tolerance
        ) {
          throw new Error();
        }
      } catch {
        throw new PaymentsError("bad-signature");
      }
      if (
        !identity(event.id) ||
        event.livemode !== live ||
        event.account !== undefined ||
        event.context !== undefined
      ) {
        throw mismatch();
      }
      if (event.type.startsWith("payment_intent.")) {
        await checkAccount();
        return {
          eventId: event.id,
          accountId,
          live,
          object: projectPayment(event.data.object as Stripe.PaymentIntent),
        };
      }
      if (
        event.type === "refund.created" ||
        event.type === "refund.updated" ||
        event.type === "refund.failed"
      ) {
        return {
          eventId: event.id,
          accountId,
          live,
          object: await projectRefund(event.data.object as Stripe.Refund),
        };
      }
      return null;
    },
  };
}
