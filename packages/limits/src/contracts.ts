/** Trusted application scope, never a client-selected identity or authorization grant. */
export interface LimitScope {
  readonly instance: string;
  readonly tenant: string;
  readonly key: string;
}

export interface ConsumeInput {
  readonly scope: LimitScope;
  readonly capacity: number;
  readonly quantity: number;
  readonly periodMs: number;
}

export interface AcquireInput {
  readonly scope: LimitScope;
  readonly capacity: number;
  readonly quantity: number;
  readonly ttlMs: number;
}

export type CounterKind = "rate" | "quota";
export type FailurePolicy = "throw" | "deny" | "allow";
export interface LimitConfig {
  readonly failurePolicy: FailurePolicy;
}

export interface Decision {
  readonly allowed: boolean;
  /** Null when the backend outcome is unknown. */
  readonly remaining: number | null;
  /** Milliseconds until this quantity can fit; null means no finite estimate. */
  readonly retryAfter: number | null;
  readonly reason: "allowed" | "exhausted" | "too-large" | "backend-failure";
}

export interface Lease {
  readonly scope: LimitScope;
  readonly token: string;
  readonly quantity: number;
  readonly expiresAt: number;
}

export interface Acquisition extends Decision {
  /** Fail-open never fabricates a lease. */
  readonly lease: Lease | null;
}

/** Each method must be atomic for its scope; this interface is not a transaction emulator. */
export interface LimitStore {
  consume(kind: CounterKind, input: ConsumeInput): Promise<Decision>;
  /** Trusted provider boundary: the service supplies a fresh UUID; callers must never reuse tokens. */
  acquire(input: AcquireInput, token: string): Promise<Acquisition>;
  renew(scope: LimitScope, token: string, ttlMs: number): Promise<Lease | null>;
  release(scope: LimitScope, token: string): Promise<void>;
}

export class LimitError extends Error {
  constructor(
    readonly code:
      | "invalid-input"
      | "policy-conflict"
      | "backend-failure"
      | "closed"
      | "denied"
      | "lease-lost",
    options?: ErrorOptions,
  ) {
    super(`Limit operation failed: ${code}`, options);
    this.name = "LimitError";
  }
}

export const maxDurationMs = 31_622_400_000;

export function positiveInteger(value: number, maximum = 2_147_483_647): void {
  if (!Number.isSafeInteger(value) || value < 1 || value > maximum)
    throw new LimitError("invalid-input");
}

export function scopeKey(scope: LimitScope): string {
  if (!scope || typeof scope !== "object") throw new LimitError("invalid-input");
  const parts = [scope.instance, scope.tenant, scope.key];
  if (parts.some((part) => typeof part !== "string" || part.length < 1 || part.length > 512))
    throw new LimitError("invalid-input");
  return JSON.stringify(parts);
}

export function validateConsume(input: ConsumeInput): void {
  if (!input) throw new LimitError("invalid-input");
  scopeKey(input.scope);
  positiveInteger(input.capacity);
  positiveInteger(input.quantity);
  positiveInteger(input.periodMs, maxDurationMs);
}

export function validateAcquire(input: AcquireInput): void {
  if (!input) throw new LimitError("invalid-input");
  scopeKey(input.scope);
  positiveInteger(input.capacity);
  positiveInteger(input.quantity);
  positiveInteger(input.ttlMs, maxDurationMs);
}

export function validateToken(token: string): void {
  if (typeof token !== "string" || token.length < 1 || token.length > 128)
    throw new LimitError("invalid-input");
}

export function validateKind(kind: CounterKind): void {
  if (kind !== "rate" && kind !== "quota") throw new LimitError("invalid-input");
}
