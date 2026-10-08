import {
  PaymentsError,
  RefundNotCreatedError,
  type CreatePayment,
  type CreateRefund,
  type PaymentAction,
  type PaymentInbox,
  type PaymentRecord,
  type PaymentResource,
  type PaymentResult,
  type PaymentView,
  type PaymentsAuthorization,
  type PaymentsProvider,
  type PaymentsStore,
  type ProviderPayment,
  type ProviderRefund,
  type RefundRecord,
} from "./contracts";

export * from "./contracts";

function identifier(value: string): void {
  if (
    typeof value !== "string" ||
    !value.length ||
    value.length > 256 ||
    [...value].some((character) => character.charCodeAt(0) < 32)
  )
    throw new PaymentsError("invalid-input");
}
async function digest(parts: readonly unknown[]): Promise<string> {
  const bytes = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(JSON.stringify(parts)),
  );
  return Array.from(new Uint8Array(bytes), (byte) => byte.toString(16).padStart(2, "0")).join("");
}
function resource(record: PaymentResource): PaymentResource {
  return Object.freeze({
    paymentId: record.paymentId,
    tenantId: record.tenantId,
    orderId: record.orderId,
    accountId: record.accountId,
    live: record.live,
    amount: record.amount,
    currency: record.currency,
  });
}
function view(record: PaymentRecord): PaymentView {
  return {
    ...resource(record),
    status: record.status,
    providerId: record.providerId,
    refunds: record.refunds.map(({ refundId, amount, status, providerId }) => ({
      refundId,
      amount,
      status,
      providerId,
    })),
  };
}
function verifyPayment(record: PaymentRecord, payment: ProviderPayment): void {
  if (
    payment.paymentId !== record.paymentId ||
    payment.tenantId !== record.tenantId ||
    payment.orderId !== record.orderId ||
    payment.accountId !== record.accountId ||
    payment.live !== record.live ||
    payment.amount !== record.amount ||
    payment.currency !== record.currency ||
    (record.providerId !== null && payment.id !== record.providerId) ||
    (payment.status === "succeeded" && payment.received !== record.amount)
  )
    throw new PaymentsError("provider-mismatch");
}
function verifyRefund(record: PaymentRecord, refund: RefundRecord, observed: ProviderRefund): void {
  if (
    observed.paymentId !== record.paymentId ||
    observed.refundId !== refund.refundId ||
    observed.paymentProviderId !== record.providerId ||
    observed.accountId !== record.accountId ||
    observed.live !== record.live ||
    observed.amount !== refund.amount ||
    observed.currency !== record.currency ||
    (refund.providerId !== null && observed.id !== refund.providerId)
  )
    throw new PaymentsError("provider-mismatch");
}
function paymentTerminal(status: PaymentRecord["status"]): boolean {
  return status === "succeeded" || status === "canceled";
}
function refundTerminal(status: RefundRecord["status"]): boolean {
  return status === "succeeded" || status === "failed" || status === "canceled";
}
function appendResult(record: PaymentRecord, result: PaymentResult): void {
  if (!record.results.some((existing) => existing.resultId === result.resultId))
    record.results.push(result);
}

export interface Payments<A> {
  create(input: CreatePayment, actor: A): Promise<PaymentView>;
  get(input: { paymentId: string }, actor: A): Promise<PaymentView>;
  refund(input: CreateRefund, actor: A): Promise<PaymentView>;
  results(input: { paymentId: string }, actor: A): Promise<readonly PaymentResult[]>;
  /** Sensitive capability for the authenticated payer, excluded from ordinary status results. */
  clientSecret(input: { paymentId: string }, actor: A): Promise<string | null>;
}
export interface PaymentsRuntime<A> {
  readonly payments: Payments<A>;
  readonly webhook: {
    receive(raw: Uint8Array, signature: string): Promise<{ accepted: boolean; duplicate: boolean }>;
  };
  /** Trusted worker capability, not a user-facing or Manage operation. */
  readonly reconciliation: {
    run(paymentId: string): Promise<boolean>;
    drain(
      limit?: number,
    ): Promise<{ payments: number; events: number; unresolved: number; nextRunAt: number | null }>;
  };
}

export function createPayments<A>(options: {
  store: PaymentsStore;
  provider: PaymentsProvider;
  authorize: PaymentsAuthorization<A>;
  clock?: () => number;
  leaseMs?: number;
  reconcileDelayMs?: number;
  maxRefunds?: number;
}): PaymentsRuntime<A> {
  const { store, provider, authorize } = options;
  const clock = options.clock ?? Date.now;
  const leaseMs = options.leaseMs ?? 60_000;
  const delay = options.reconcileDelayMs ?? 60_000;
  const maxRefunds = options.maxRefunds ?? 100;
  if (
    typeof authorize !== "function" ||
    !Number.isSafeInteger(leaseMs) ||
    leaseMs < 1_000 ||
    leaseMs > 3_600_000 ||
    !Number.isSafeInteger(delay) ||
    delay < 1_000 ||
    delay > 86_400_000 ||
    !Number.isSafeInteger(maxRefunds) ||
    maxRefunds < 1 ||
    maxRefunds > 1_000 ||
    !Number.isSafeInteger(provider.replayWindowMs) ||
    provider.replayWindowMs <= 0
  )
    throw new PaymentsError("invalid-input");
  identifier(provider.accountId);
  function now(): number {
    const value = clock();
    if (!Number.isSafeInteger(value) || value < 0) throw new PaymentsError("unavailable");
    return value;
  }
  async function load(paymentId: string): Promise<PaymentRecord> {
    identifier(paymentId);
    const record = await store.get(paymentId);
    if (!record) throw new PaymentsError("not-found");
    if (record.accountId !== provider.accountId || record.live !== provider.live)
      throw new PaymentsError("forbidden");
    return record;
  }
  async function allowed(actor: A, action: PaymentAction, record: PaymentResource) {
    await authorize(actor, action, resource(record));
  }
  function renewLease(record: PaymentRecord, token: string): void {
    const time = now();
    if (record.lease?.token !== token || record.lease.until <= time)
      throw new PaymentsError("conflict");
    record.lease.until = time + leaseMs;
  }
  async function mutate(
    paymentId: string,
    change: (record: PaymentRecord) => boolean,
  ): Promise<PaymentRecord> {
    for (let attempt = 0; attempt < 12; attempt++) {
      const current = await load(paymentId);
      const next = structuredClone(current);
      if (!change(next)) return current;
      next.revision = current.revision + 1;
      next.updatedAt = now();
      if (await store.compareAndSet(next, current.revision)) return next;
    }
    throw new PaymentsError("conflict");
  }
  async function observePayment(paymentId: string, token: string, observed: ProviderPayment) {
    return mutate(paymentId, (record) => {
      renewLease(record, token);
      verifyPayment(record, observed);
      // Current provider retrieval, not event timestamps, drives nonterminal states.
      // Conflicting terminal observations require operator review; never regress.
      if (paymentTerminal(record.status) && observed.status !== record.status)
        throw new PaymentsError("provider-mismatch");
      record.providerId = observed.id;
      record.status = observed.status;
      if (paymentTerminal(record.status)) {
        appendResult(record, {
          ...resource(record),
          kind: record.status === "succeeded" ? "payment.succeeded" : "payment.canceled",
          resultId: `${record.paymentId}:${record.status}`,
        });
      }
      return true;
    });
  }
  async function observeRefund(
    paymentId: string,
    token: string,
    refundId: string,
    observed: ProviderRefund,
  ) {
    return mutate(paymentId, (record) => {
      renewLease(record, token);
      const refund = record.refunds.find((item) => item.refundId === refundId)!;
      verifyRefund(record, refund, observed);
      if (refundTerminal(refund.status) && observed.status !== refund.status)
        throw new PaymentsError("provider-mismatch");
      refund.providerId = observed.id;
      refund.status = observed.status;
      if (refundTerminal(refund.status)) {
        appendResult(record, {
          ...resource(record),
          kind:
            refund.status === "succeeded"
              ? "refund.succeeded"
              : refund.status === "failed"
                ? "refund.failed"
                : "refund.canceled",
          resultId: `${refund.refundId}:${refund.status}`,
          refundId,
          refundAmount: refund.amount,
        });
      }
      return true;
    });
  }
  async function run(paymentId: string): Promise<boolean> {
    const token = crypto.randomUUID();
    let record = await mutate(paymentId, (current) => {
      if (current.lease && current.lease.until > now()) return false;
      current.lease = { token, until: now() + leaseMs };
      return true;
    });
    if (record.lease?.token !== token) return false;
    let resolved = true;
    try {
      const previousAttempt = record.attemptedAt;
      if (previousAttempt === null) {
        record = await mutate(paymentId, (current) => {
          if (current.lease?.token !== token) throw new PaymentsError("conflict");
          current.attemptedAt = now();
          return true;
        });
      }
      let observed = record.providerId
        ? await provider.getPayment(record.providerId)
        : previousAttempt === null
          ? null
          : await provider.findPayment(record);
      if (!observed) {
        record = await load(paymentId);
        if (record.providerId) {
          observed = await provider.getPayment(record.providerId);
        } else if (mayWrite(record, token, record.attemptedAt)) {
          observed = await provider.createPayment(
            record,
            `payment:${record.paymentId}`,
            async () => {
              const current = await load(paymentId);
              if (!mayWrite(current, token, current.attemptedAt) || current.providerId !== null)
                throw new PaymentsError("conflict");
            },
          );
        }
      }
      if (!observed) return false;
      record = await observePayment(paymentId, token, observed);
      for (const pending of record.refunds) {
        if (refundTerminal(pending.status) && pending.providerId === null) continue;
        let refund = pending;
        const previousRefundAttempt = refund.attemptedAt;
        record = await mutate(paymentId, (current) => {
          renewLease(current, token);
          const item = current.refunds.find((candidate) => candidate.refundId === refund.refundId)!;
          item.attemptedAt ??= now();
          return true;
        });
        refund = record.refunds.find((item) => item.refundId === refund.refundId)!;
        let observedRefund = refund.providerId
          ? await provider.getRefund(refund.providerId)
          : previousRefundAttempt === null
            ? null
            : await provider.findRefund(record, refund);
        if (!observedRefund) {
          record = await load(paymentId);
          refund = record.refunds.find((item) => item.refundId === refund.refundId)!;
          if (refund.providerId) {
            observedRefund = await provider.getRefund(refund.providerId);
          } else if (mayWrite(record, token, refund.attemptedAt)) {
            try {
              observedRefund = await provider.createRefund(
                record,
                refund,
                `refund:${refund.refundId}`,
                async () => {
                  const current = await load(paymentId);
                  const item = current.refunds.find(
                    (candidate) => candidate.refundId === refund.refundId,
                  );
                  if (
                    !item ||
                    !mayWrite(current, token, item.attemptedAt) ||
                    item.providerId !== null ||
                    item.status !== "unknown" ||
                    current.providerId !== record.providerId
                  )
                    throw new PaymentsError("conflict");
                },
              );
            } catch (error) {
              if (!(error instanceof RefundNotCreatedError)) throw error;
              record = await mutate(paymentId, (current) => {
                renewLease(current, token);
                const item = current.refunds.find(
                  (candidate) => candidate.refundId === refund.refundId,
                )!;
                if (item.providerId !== null) throw new PaymentsError("provider-mismatch");
                item.status = "failed";
                appendResult(current, {
                  ...resource(current),
                  kind: "refund.failed",
                  resultId: `${item.refundId}:failed`,
                  refundId: item.refundId,
                  refundAmount: item.amount,
                });
                return true;
              });
              continue;
            }
          }
        }
        if (observedRefund)
          record = await observeRefund(paymentId, token, refund.refundId, observedRefund);
        else resolved = false;
      }
      return resolved;
    } catch (error) {
      // The reservation survives all provider errors, including timeouts and HTTP 500.
      if (error instanceof PaymentsError && error.code === "provider-mismatch") throw error;
      return false;
    } finally {
      await mutate(paymentId, (current) => {
        if (current.lease?.token !== token) return false;
        current.lease = null;
        const finished =
          paymentTerminal(current.status) &&
          current.refunds.every((item) => refundTerminal(item.status));
        current.reconcileAt = finished ? Number.MAX_SAFE_INTEGER : now() + delay;
        return true;
      });
    }
  }

  function mayWrite(record: PaymentRecord, token: string, attemptedAt: number | null): boolean {
    const time = now();
    return (
      record.lease?.token === token &&
      record.lease.until > time &&
      attemptedAt !== null &&
      time >= attemptedAt &&
      time - attemptedAt < provider.replayWindowMs
    );
  }

  const payments: Payments<A> = {
    async create(input, actor) {
      identifier(input.tenantId);
      identifier(input.orderId);
      identifier(input.key);
      provider.validateAmount(input.amount, input.currency);
      const paymentId = await digest([
        provider.accountId,
        provider.live,
        input.tenantId,
        input.key,
      ]);
      const orderKey = await digest([
        provider.accountId,
        provider.live,
        input.tenantId,
        input.orderId,
      ]);
      const time = now();
      const draft: PaymentRecord = {
        tenantId: input.tenantId,
        orderId: input.orderId,
        key: input.key,
        amount: input.amount,
        currency: input.currency,
        paymentId,
        orderKey,
        accountId: provider.accountId,
        live: provider.live,
        providerId: null,
        status: "unknown",
        revision: 0,
        lease: null,
        attemptedAt: null,
        refunds: [],
        results: [],
        createdAt: time,
        updatedAt: time,
        reconcileAt: time,
      };
      await allowed(actor, "create", draft);
      if (!(await store.insert(draft))) {
        const existing = (await store.get(paymentId)) ?? (await store.getByOrder(orderKey));
        if (!existing) throw new PaymentsError("conflict");
        await allowed(actor, "create", existing);
        if (
          existing.paymentId !== paymentId ||
          existing.orderKey !== orderKey ||
          existing.amount !== input.amount ||
          existing.currency !== input.currency ||
          existing.key !== input.key
        )
          throw new PaymentsError("conflict");
      }
      // Public retries never replay provider writes. Only the initial reservation or trusted worker does.
      const current = await load(paymentId);
      if (current.attemptedAt === null) await run(paymentId);
      return view(await load(paymentId));
    },
    async get(input, actor) {
      const record = await load(input.paymentId);
      await allowed(actor, "read", record);
      return view(record);
    },
    async refund(input, actor) {
      identifier(input.key);
      let record = await load(input.paymentId);
      await allowed(actor, "refund", record);
      provider.validateRefundAmount(input.amount, record.currency);
      const refundId = await digest([record.paymentId, input.key]);
      record = await mutate(record.paymentId, (current) => {
        const existing = current.refunds.find((refund) => refund.key === input.key);
        if (existing) {
          if (existing.amount !== input.amount) throw new PaymentsError("conflict");
          return false;
        }
        if (current.status !== "succeeded" || !current.providerId)
          throw new PaymentsError("conflict");
        // Unknown and pending refunds retain their reservation until a verified terminal failure.
        const reserved = current.refunds.reduce(
          (sum, refund) =>
            sum + (refund.status === "failed" || refund.status === "canceled" ? 0 : refund.amount),
          0,
        );
        if (input.amount > current.amount - reserved || current.refunds.length >= maxRefunds)
          throw new PaymentsError("conflict");
        current.refunds.push({
          refundId,
          key: input.key,
          amount: input.amount,
          providerId: null,
          status: "unknown",
          attemptedAt: null,
        });
        current.reconcileAt = now();
        return true;
      });
      if (record.refunds.find((refund) => refund.refundId === refundId)?.attemptedAt === null)
        await run(record.paymentId);
      return view(await load(record.paymentId));
    },
    async results(input, actor) {
      const record = await load(input.paymentId);
      await allowed(actor, "results", record);
      return structuredClone(record.results);
    },
    async clientSecret(input, actor) {
      const record = await load(input.paymentId);
      await allowed(actor, "client-secret", record);
      if (!record.providerId || paymentTerminal(record.status)) return null;
      const observed = await provider.getPayment(record.providerId);
      verifyPayment(record, observed);
      try {
        return await provider.clientSecret(record.providerId);
      } catch {
        throw new PaymentsError("unavailable");
      }
    },
  };
  return {
    payments,
    webhook: {
      async receive(raw, signature) {
        const event = await provider.verifyWebhook(raw, signature);
        if (!event) return { accepted: false, duplicate: false };
        if (event.accountId !== provider.accountId || event.live !== provider.live)
          throw new PaymentsError("provider-mismatch");
        const record = await load(event.object.paymentId);
        if ("refundId" in event.object) {
          const object = event.object;
          const refund = record.refunds.find((item) => item.refundId === object.refundId);
          if (!refund) throw new PaymentsError("provider-mismatch");
          verifyRefund(record, refund, object);
        } else verifyPayment(record, event.object);
        const time = now();
        const eventKey = await digest([provider.accountId, provider.live, event.eventId]);
        const inserted = await store.receive({
          eventKey,
          eventId: event.eventId,
          paymentId: record.paymentId,
          objectId: event.object.id,
          refundId: "refundId" in event.object ? event.object.refundId : null,
          accountId: record.accountId,
          live: record.live,
          createdAt: time,
          reconcileAt: time,
          done: false,
        });
        return { accepted: true, duplicate: !inserted };
      },
    },
    reconciliation: {
      run,
      async drain(limit = 50) {
        if (!Number.isSafeInteger(limit) || limit < 1 || limit > 200)
          throw new PaymentsError("invalid-input");
        let events = 0;
        let paymentCount = 0;
        let unresolved = 0;
        async function processEvent(event: PaymentInbox) {
          try {
            await mutate(event.paymentId, (record) => {
              if (event.refundId !== null) {
                const refund = record.refunds.find((item) => item.refundId === event.refundId);
                if (!refund || (refund.providerId !== null && refund.providerId !== event.objectId))
                  throw new PaymentsError("provider-mismatch");
                if (refund.providerId !== null) return false;
                refund.providerId = event.objectId;
              } else {
                if (record.providerId !== null && record.providerId !== event.objectId)
                  throw new PaymentsError("provider-mismatch");
                if (record.providerId !== null) return false;
                record.providerId = event.objectId;
              }
              return true;
            });
            if (await run(event.paymentId)) {
              await store.finishEvent(event.eventKey);
              events++;
              return;
            }
          } catch {}
          unresolved++;
          await store.deferEvent(event.eventKey, now() + delay);
        }
        const inbox = await store.inbox(provider.accountId, provider.live, now(), limit);
        for (const event of inbox) await processEvent(event);
        const due = await store.due(provider.accountId, provider.live, now(), limit);
        for (const record of due) {
          try {
            if (!(await run(record.paymentId))) unresolved++;
          } catch {
            unresolved++;
          }
          paymentCount++;
        }
        const [nextPayment] = await store.due(
          provider.accountId,
          provider.live,
          Number.MAX_SAFE_INTEGER - 1,
          1,
        );
        const [nextEvent] = await store.inbox(
          provider.accountId,
          provider.live,
          Number.MAX_SAFE_INTEGER,
          1,
        );
        const next = [nextPayment?.reconcileAt, nextEvent?.reconcileAt].filter(
          (time): time is number => time !== undefined,
        );
        return {
          payments: paymentCount,
          events,
          unresolved,
          nextRunAt: next.length ? Math.min(...next) : null,
        };
      },
    },
  };
}
