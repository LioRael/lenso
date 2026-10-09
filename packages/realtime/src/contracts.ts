export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };

/** Scope is host-defined: an application, tenant, workspace or another isolation domain. */
export interface Resource {
  readonly scope: string;
  readonly type: string;
  readonly id: string;
}

export interface Identity<P = unknown> {
  readonly scope: string;
  readonly subject: string;
  readonly principal: P;
  readonly expiresAt: number;
}

export interface Cursor {
  readonly generation: string;
  readonly sequence: number;
}

export type ProviderEvent =
  | { readonly kind: "update"; readonly type: string; readonly data: Json }
  | { readonly kind: "deleted" }
  | { readonly kind: "revoke" };

export interface Delivery {
  readonly topic: string;
  readonly cursor: Cursor;
  readonly event: ProviderEvent;
}

/** A provider belongs to one Realtime instance. start subscribes before resolving. */
export interface RealtimeProvider {
  readonly kind: "memory" | "redis";
  start(deliver: (delivery: Delivery) => void, fail: () => void): Promise<void>;
  current(topic: string): Promise<Cursor>;
  publish(topic: string, event: ProviderEvent): Promise<Cursor>;
  close(): Promise<void>;
}

export type ErrorCode =
  | "invalid-input"
  | "denied"
  | "expired"
  | "limit"
  | "closed"
  | "provider"
  | "payload";

export class RealtimeError extends Error {
  constructor(readonly code: ErrorCode) {
    super(`Realtime ${code}`);
    this.name = "RealtimeError";
  }
}

export type GapReason =
  | "reconnect"
  | "cursor-expired"
  | "sequence"
  | "generation"
  | "out-of-order"
  | "snapshot-race"
  | "overflow"
  | "provider";

export interface Envelope {
  readonly version: 1;
  readonly kind: "ready" | "update" | "gap" | "closed" | "heartbeat";
  readonly subscription?: string;
  readonly cursor?: string;
  readonly type?: string;
  readonly data?: Json;
  readonly reason?: GapReason | "revoked" | "deleted" | "expired" | "shutdown" | "unsubscribed";
}

export interface RealtimeConfig {
  readonly maxConnections?: number;
  readonly maxConnectionsPerSubject?: number;
  readonly maxSubscriptionsPerConnection?: number;
  readonly maxSubscriptionsPerSubject?: number;
  readonly maxSubscribersPerTopic?: number;
  readonly maxTopics?: number;
  readonly maxPendingOperations?: number;
  readonly maxPayloadBytes?: number;
  readonly maxBufferedEvents?: number;
  readonly maxBufferedBytes?: number;
  readonly authorizationLeaseMs?: number;
  readonly sweepMs?: number;
  readonly heartbeatMs?: number;
  readonly retryMs?: number;
  readonly cursorMaxAgeMs?: number;
  readonly snapshotTimeoutMs?: number;
}

export interface RealtimeOptions<P> {
  readonly provider: RealtimeProvider;
  readonly config?: RealtimeConfig;
  /** Called only on subscribe/explicit renewal, never once per event. */
  readonly authorize: (
    identity: Identity<P>,
    resource: Resource,
    signal: AbortSignal,
  ) => Promise<{ validUntil: number } | false>;
  /** Only fixed categories, never identity, topics, credentials, errors or payloads. */
  readonly onDiagnostic?: (event: "overflow" | "provider" | "authorization") => void;
}

export interface Subscription {
  readonly id: string;
  readonly resource: Resource;
  unsubscribe(): void;
  renew(): Promise<void>;
}

export interface RealtimeConnection extends AsyncIterable<Envelope> {
  subscribe(resource: Resource, options?: { cursor?: string }): Promise<Subscription>;
  snapshot<T>(
    subscription: Subscription,
    read: (signal: AbortSignal) => Promise<T>,
  ): Promise<{ value: T; cursor: string; stable: boolean }>;
  response(): Response;
  close(): void;
}

export interface Realtime<P = unknown> {
  connect(identity: Identity<P>, options?: { signal?: AbortSignal }): RealtimeConnection;
  publish(resource: Resource, type: string, data: Json): Promise<string>;
  revokeResource(resource: Resource): Promise<void>;
  deleteResource(resource: Resource): Promise<void>;
  /** Local fast path. Other instances still enforce the bounded lease. */
  revokeSubject(scope: string, subject: string): void;
  stats(): { connections: number; subscriptions: number; topics: number; pending: number };
  close(): Promise<void>;
}
