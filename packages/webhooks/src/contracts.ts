import type { AuditScope, AuditService } from "@lenso/audit";
import type { Limits } from "@lenso/limits";
import type { TaskQueue } from "@lenso/tasks";
import type { OutboundPolicy, HttpResult } from "./network";
import type { SigningKey } from "./signing";
import type { WebhookTask } from "./task";

export interface WebhookScope extends AuditScope {
  readonly tenantId: string;
}

export type WebhookAction = "publish" | "read" | "configure" | "replay";
export interface WebhookContext<P> {
  readonly scope: WebhookScope;
  readonly principal: P;
}
export interface WebhookAuthority<P> {
  authorize(principal: P, scope: WebhookScope, action: WebhookAction): Promise<void>;
}
export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };
export interface EventEnvelope {
  readonly version: 1;
  readonly id: string;
  readonly type: string;
  readonly occurredAt: string;
  readonly source: string;
  readonly data: JsonValue;
}
export interface Endpoint {
  readonly id: string;
  readonly scope: WebhookScope;
  readonly url: string;
  readonly secretRef: string;
  readonly enabled: boolean;
  readonly revision: number;
  readonly createdAt: number;
}
export interface Subscription {
  readonly id: string;
  readonly scope: WebhookScope;
  readonly endpointId: string;
  readonly eventType: string;
  readonly enabled: boolean;
  readonly createdAt: number;
}
export type DeliveryState = "pending" | "running" | "retry" | "succeeded" | "failed";
export type AttemptCode =
  | "success"
  | "timeout"
  | "connection-failed"
  | "rate-limited"
  | "server-error"
  | "permanent-http"
  | "policy-rejected"
  | "response-too-large"
  | "request-too-large"
  | "redirect-rejected"
  | "key-unavailable"
  | "lease-expired"
  | "endpoint-disabled"
  | "unsubscribed";
export interface Delivery {
  readonly id: string;
  readonly scope: WebhookScope;
  readonly eventId: string;
  readonly endpointId: string;
  readonly subscriptionId: string;
  readonly endpointRevision: number;
  readonly state: DeliveryState;
  readonly attemptCount: number;
  readonly maxAttempts: number;
  readonly dueAt: number;
  readonly generation: number;
  readonly replayOf: string | null;
  readonly auditIntentId: string | null;
  readonly createdAt: number;
  readonly updatedAt: number;
  readonly lastCode: AttemptCode | null;
}
export interface StoredDelivery extends Delivery {
  readonly url: string;
  readonly secretRef: string;
  readonly body: string;
  readonly leaseToken: string | null;
  readonly leaseUntil: number | null;
}
export interface Attempt {
  readonly id: string;
  readonly deliveryId: string;
  readonly number: number;
  readonly startedAt: number;
  readonly finishedAt: number | null;
  readonly code: AttemptCode | null;
  readonly status: number | null;
  readonly keyId: string | null;
}
export interface PageInput {
  readonly limit?: number;
  readonly cursor?: { readonly createdAt: number; readonly id: string };
}
export interface Page<T> {
  readonly items: readonly T[];
  readonly nextCursor: PageInput["cursor"] | null;
}
export interface ClaimedDelivery {
  readonly delivery: StoredDelivery;
  readonly attempt: Attempt;
}
/** Trusted persistence boundary; all writes below must be atomic as a unit. */
export interface WebhookRepository {
  putEndpoint(input: Omit<Endpoint, "revision" | "createdAt">, now: number): Promise<Endpoint>;
  getEndpoint(scope: WebhookScope, id: string): Promise<Endpoint | null>;
  listEndpoints(
    scope: WebhookScope,
    page: Required<Pick<PageInput, "limit">> & PageInput,
  ): Promise<Endpoint[]>;
  putSubscription(input: Omit<Subscription, "createdAt">, now: number): Promise<Subscription>;
  listSubscriptions(
    scope: WebhookScope,
    page: Required<Pick<PageInput, "limit">> & PageInput,
  ): Promise<Subscription[]>;
  publish(
    scope: WebhookScope,
    event: EventEnvelope,
    body: string,
    maxAttempts: number,
    now: number,
  ): Promise<Delivery[]>;
  getDelivery(scope: WebhookScope, id: string): Promise<Delivery | null>;
  listDeliveries(
    scope: WebhookScope,
    page: Required<Pick<PageInput, "limit">> & PageInput,
  ): Promise<Delivery[]>;
  listAttempts(
    scope: WebhookScope,
    deliveryId: string,
    page: Required<Pick<PageInput, "limit">> & PageInput,
  ): Promise<Attempt[]>;
  replay(
    scope: WebhookScope,
    id: string,
    newId: string,
    auditIntentId: string,
    now: number,
  ): Promise<Delivery>;
  claim(
    id: string,
    generation: number,
    token: string,
    now: number,
    leaseMs: number,
  ): Promise<ClaimedDelivery | null>;
  finish(
    id: string,
    token: string,
    result: {
      readonly code: AttemptCode;
      readonly status: number | null;
      readonly keyId: string | null;
      readonly retryAt: number | null;
    },
    now: number,
  ): Promise<Delivery | null>;
  /** Bounded repair: abandon expired attempts; queued work keeps its generation. */
  recover(now: number, limit: number): Promise<Delivery[]>;
  /** Compare-and-swap only an unsent scheduling generation after a terminal Tasks job. */
  advanceSchedule(id: string, generation: number, now: number): Promise<Delivery | null>;
  /** Only terminal records older than cutoff, never an active delivery/event, may be pruned. */
  prune(cutoff: number, limit: number): Promise<number>;
}
export interface WebhookConfig {
  readonly enabled: boolean;
  readonly instanceId: string;
  readonly source: string;
  readonly eventTypes: readonly string[];
  readonly maxAttempts: number;
  readonly baseDelayMs: number;
  readonly maxDelayMs: number;
  readonly concurrency: number;
  readonly retentionMs: number;
  readonly outbound: OutboundPolicy;
}
export interface WebhookOptions<P> {
  readonly repository: WebhookRepository;
  readonly authority: WebhookAuthority<P>;
  readonly audit: AuditService<P>;
  readonly limits: Limits;
  readonly config: WebhookConfig;
  /** Borrow an explicitly registered queue. The service never starts or closes it. */
  readonly queue: TaskQueue;
  readonly task: WebhookTask;
  /** Host must authorize the reference within this persisted tenant/scope. */
  readonly keys: { active(secretRef: string, scope: WebhookScope): Promise<SigningKey> };
  /** Trusted transport DI. Use the shipped pinned transport, not fetch, in production. */
  readonly transport: {
    send(input: {
      readonly url: string;
      readonly body: Uint8Array;
      readonly headers: Readonly<Record<string, string>>;
      readonly signal: AbortSignal;
    }): Promise<HttpResult>;
  };
}
export class WebhookError extends Error {
  constructor(
    readonly code:
      | "invalid-input"
      | "invalid-config"
      | "unauthorized"
      | "not-found"
      | "conflict"
      | "storage-failed"
      | "audit-failed"
      | "replay-outcome-unknown"
      | "disabled",
    readonly referenceId?: string,
  ) {
    super(`Webhook ${code}`);
    this.name = "WebhookError";
  }
}
