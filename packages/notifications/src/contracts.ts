import type { StandardSchemaV1 } from "@standard-schema/spec";

export type NotificationState =
  | "pending"
  | "sending"
  | "accepted"
  | "delivered"
  | "suppressed"
  | "failed"
  | "unknown";
export type AttemptState = "sending" | "accepted" | "failed" | "unknown";
export type FailureCode =
  | "rejected"
  | "rate-limited"
  | "provider-unavailable"
  | "idempotency-conflict"
  | "transport-unknown"
  | "invalid-response"
  | "deduplication-expired";

export interface EmailMessage {
  readonly from: string;
  readonly to: string;
  readonly subject: string;
  readonly text: string;
  readonly html: string;
}

export type DeliveryResult =
  | { state: "accepted"; providerMessageId: string }
  | { state: "failed" | "unknown"; code: FailureCode; retryable: boolean };

export interface NotificationChannel {
  /** Stable provider/account configuration identity; never repoint this ID during a retry. */
  readonly id: string;
  readonly kind: "email";
  readonly idempotencyWindowMs: number;
  send(
    message: EmailMessage,
    options: { idempotencyKey: string; signal?: AbortSignal },
  ): Promise<DeliveryResult>;
}

export interface NotificationTemplate {
  readonly id: string;
  readonly version: string;
  readonly variables: StandardSchemaV1;
  readonly category: string;
  /** Business explicitly chooses whether this notification can be unsubscribed. */
  readonly necessity: "required" | "optional";
  readonly channels: readonly string[];
  readonly from: string;
  /** Plain-text interpolation only. HTML is generated from escaped text, never raw markup. */
  readonly subject: string;
  readonly text: string;
}

export interface NotificationInput {
  readonly tenantId: string;
  readonly scope: string;
  readonly idempotencyKey: string;
  readonly businessId: string;
  readonly recipientId: string;
  readonly email: string;
  readonly templateId: string;
  readonly templateVersion: string;
  readonly variables: unknown;
  readonly channels?: readonly string[];
}

/** Private persisted snapshot. Do not return this from management or transport entries. */
export interface NotificationRecord {
  id: string;
  tenantId: string;
  scope: string;
  idempotencyKey: string;
  fingerprint: string;
  businessId: string;
  recipientId: string;
  templateId: string;
  templateVersion: string;
  category: string;
  necessity: "required" | "optional";
  channelId: string;
  message: EmailMessage;
  state: NotificationState;
  revision: number;
  attemptCount: number;
  firstRequestAt: number | null;
  leaseUntil: number | null;
  taskJobId: string | null;
  providerMessageId: string | null;
  error: FailureCode | null;
  retryable: boolean;
  createdAt: number;
  updatedAt: number;
}

export interface DeliveryAttempt {
  id: string;
  notificationId: string;
  number: number;
  state: AttemptState;
  startedAt: number;
  finishedAt: number | null;
  providerMessageId: string | null;
  error: FailureCode | null;
}

export interface Preference {
  tenantId: string;
  recipientId: string;
  category: string;
  channelId: string;
  enabled: boolean;
}

export interface NotificationFilter {
  tenantId: string;
  recipientId?: string;
  limit: number;
}

export interface NotificationStore {
  /** Persistent unique (tenantId, scope, idempotencyKey); race loser returns winner. */
  insertOrGet(record: NotificationRecord): Promise<NotificationRecord>;
  findKey(tenantId: string, scope: string, key: string): Promise<NotificationRecord | null>;
  get(id: string): Promise<NotificationRecord | null>;
  list(filter: NotificationFilter): Promise<NotificationRecord[]>;
  /** Atomic CAS plus optional attempt upsert; record.revision must equal expectedRevision + 1. */
  save(
    record: NotificationRecord,
    expectedRevision: number,
    attempt?: DeliveryAttempt,
  ): Promise<boolean>;
  attempts(notificationId: string): Promise<DeliveryAttempt[]>;
  getPreference(key: Omit<Preference, "enabled">): Promise<Preference | null>;
  setPreference(preference: Preference): Promise<void>;
  /** Mark queue handoff independently of delivery revision; same job is repeatable, repointing is rejected. */
  markEnqueued(id: string, taskJobId: string): Promise<boolean>;
  /** Unhanded pending/retryable/unknown or expired sending records, bounded for reconciliation. */
  recoverable(now: number, limit: number): Promise<NotificationRecord[]>;
}
