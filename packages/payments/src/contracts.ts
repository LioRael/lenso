export type PaymentStatus =
  | "unknown"
  | "requires_payment_method"
  | "requires_confirmation"
  | "requires_action"
  | "processing"
  | "requires_capture"
  | "succeeded"
  | "canceled";
export type RefundStatus =
  | "unknown"
  | "pending"
  | "requires_action"
  | "succeeded"
  | "failed"
  | "canceled";

export interface PaymentResource {
  readonly paymentId: string;
  readonly tenantId: string;
  readonly orderId: string;
  readonly accountId: string;
  readonly live: boolean;
  readonly amount: number;
  readonly currency: string;
}
export interface CreatePayment {
  readonly tenantId: string;
  readonly orderId: string;
  readonly key: string;
  /** Trusted server-side amount, in the currency's Stripe API unit, never a major-unit float. */
  readonly amount: number;
  readonly currency: string;
}
export interface CreateRefund {
  readonly paymentId: string;
  readonly key: string;
  readonly amount: number;
}
export type PaymentAction = "create" | "read" | "client-secret" | "refund" | "results";
export type PaymentsAuthorization<A> = (
  actor: A,
  action: PaymentAction,
  resource: Readonly<PaymentResource>,
) => Promise<void>;

export interface Lease {
  token: string;
  until: number;
}
export interface RefundRecord {
  refundId: string;
  key: string;
  amount: number;
  status: RefundStatus;
  providerId: string | null;
  attemptedAt: number | null;
}
export interface PaymentResult extends PaymentResource {
  /** Persisted together with the state transition. The application deduplicates on this key. */
  resultId: string;
  kind:
    | "payment.succeeded"
    | "payment.canceled"
    | "refund.succeeded"
    | "refund.failed"
    | "refund.canceled";
  refundId?: string;
  refundAmount?: number;
}
export interface PaymentRecord extends PaymentResource {
  key: string;
  orderKey: string;
  status: PaymentStatus;
  providerId: string | null;
  revision: number;
  attemptedAt: number | null;
  lease: Lease | null;
  refunds: RefundRecord[];
  results: PaymentResult[];
  createdAt: number;
  updatedAt: number;
  /** Persisted scan cursor. Advanced after every attempt, including unresolved ones. */
  reconcileAt: number;
}
export interface PaymentView extends PaymentResource {
  status: PaymentStatus;
  providerId: string | null;
  refunds: readonly {
    refundId: string;
    amount: number;
    status: RefundStatus;
    providerId: string | null;
  }[];
}
export interface ProviderPayment {
  id: string;
  paymentId: string;
  tenantId: string;
  orderId: string;
  accountId: string;
  live: boolean;
  amount: number;
  received: number;
  currency: string;
  status: Exclude<PaymentStatus, "unknown">;
}
export interface ProviderRefund {
  id: string;
  refundId: string;
  paymentId: string;
  paymentProviderId: string;
  accountId: string;
  live: boolean;
  amount: number;
  currency: string;
  status: Exclude<RefundStatus, "unknown">;
}
export interface VerifiedPaymentEvent {
  eventId: string;
  accountId: string;
  live: boolean;
  object: ProviderPayment | ProviderRefund;
}
export interface PaymentInbox {
  eventKey: string;
  eventId: string;
  paymentId: string;
  /** Validated object identity retained even when the original provider response was lost. */
  objectId: string;
  refundId: string | null;
  accountId: string;
  live: boolean;
  createdAt: number;
  reconcileAt: number;
  done: boolean;
}

/** All operations must use authoritative reads. CAS covers the entire aggregate, including results. */
export interface PaymentsStore {
  /** False on a paymentId OR orderKey conflict. No replacement of the existing record. */
  insert(record: PaymentRecord): Promise<boolean>;
  get(paymentId: string): Promise<PaymentRecord | null>;
  getByOrder(orderKey: string): Promise<PaymentRecord | null>;
  compareAndSet(record: PaymentRecord, revision: number): Promise<boolean>;
  /** Ordered by reconcileAt/paymentId, scoped to the provider account and mode. */
  due(accountId: string, live: boolean, now: number, limit: number): Promise<PaymentRecord[]>;
  /** Unique eventKey; false means it is already durably received. */
  receive(event: PaymentInbox): Promise<boolean>;
  inbox(accountId: string, live: boolean, now: number, limit: number): Promise<PaymentInbox[]>;
  finishEvent(eventKey: string): Promise<void>;
  deferEvent(eventKey: string, reconcileAt: number): Promise<void>;
}

export interface PaymentsProvider {
  readonly accountId: string;
  readonly live: boolean;
  /** Provider's safe replay horizon, less than its minimum idempotency retention. */
  readonly replayWindowMs: number;
  validateAmount(amount: number, currency: string): void;
  /** Refunds need positive API units, but do not inherit the minimum charge amount. */
  validateRefundAmount(amount: number, currency: string): void;
  /** Invoke beforeWrite after adapter preflight, immediately before calling the SDK write. */
  createPayment(
    record: PaymentRecord,
    idempotencyKey: string,
    beforeWrite?: () => Promise<void>,
  ): Promise<ProviderPayment>;
  getPayment(id: string): Promise<ProviderPayment>;
  /** Query by immutable metadata; null is not proof that a timed-out request failed. */
  findPayment(record: PaymentRecord): Promise<ProviderPayment | null>;
  clientSecret(id: string): Promise<string | null>;
  createRefund(
    record: PaymentRecord,
    refund: RefundRecord,
    idempotencyKey: string,
    beforeWrite?: () => Promise<void>,
  ): Promise<ProviderRefund>;
  getRefund(id: string): Promise<ProviderRefund>;
  findRefund(record: PaymentRecord, refund: RefundRecord): Promise<ProviderRefund | null>;
  /** Official provider signature verification over the untouched raw bytes. */
  verifyWebhook(raw: Uint8Array, signature: string): Promise<VerifiedPaymentEvent | null>;
}

export class PaymentsError extends Error {
  constructor(
    readonly code:
      | "invalid-input"
      | "forbidden"
      | "not-found"
      | "conflict"
      | "provider-mismatch"
      | "bad-signature"
      | "unavailable",
  ) {
    super(`Payments: ${code}`);
    this.name = "PaymentsError";
  }
}

/** Only a provider's documented, definitive rejection before creating a refund may use this. */
export class RefundNotCreatedError extends Error {
  constructor() {
    super("Payments: refund-not-created");
    this.name = "RefundNotCreatedError";
  }
}
