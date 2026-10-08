import {
  LimitError,
  maxDurationMs,
  type Acquisition,
  type AcquireInput,
  type ConsumeInput,
  type Decision,
  type Lease,
  type LimitScope,
} from "./contracts";

export interface CounterState {
  capacity: number;
  periodMs: number;
  windowStart: number;
  used: number;
  lastNow: number;
}

export interface LeaseState {
  capacity: number;
  lastNow: number;
  leases: Lease[];
}

export function clockTime(now: number, previous = 0): number {
  if (!Number.isSafeInteger(now) || now < 0 || now > Number.MAX_SAFE_INTEGER - maxDurationMs * 2)
    throw new LimitError("invalid-input");
  return Math.max(now, previous);
}

export function consumeState(
  previous: CounterState | undefined,
  input: ConsumeInput,
  clock: number,
): { state: CounterState; result: Decision } {
  if (previous && (previous.capacity !== input.capacity || previous.periodMs !== input.periodMs))
    throw new LimitError("policy-conflict");
  const now = clockTime(clock, previous?.lastNow);
  const windowStart = Math.floor(now / input.periodMs) * input.periodMs;
  const state: CounterState = {
    capacity: input.capacity,
    periodMs: input.periodMs,
    windowStart,
    used: previous?.windowStart === windowStart ? previous.used : 0,
    lastNow: now,
  };
  const allowed = input.quantity <= state.capacity - state.used;
  if (allowed) state.used += input.quantity;
  return { state, result: counterDecision(state, input.quantity, allowed) };
}

/** Describe a committed counter outcome; SQL adapters perform admission inside the backend. */
export function counterDecision(state: CounterState, quantity: number, allowed: boolean): Decision {
  return {
    allowed,
    remaining: state.capacity - state.used,
    retryAfter: allowed
      ? 0
      : quantity > state.capacity
        ? null
        : state.windowStart + state.periodMs - state.lastNow,
    reason: allowed ? "allowed" : quantity > state.capacity ? "too-large" : "exhausted",
  };
}

export function liveState(
  previous: LeaseState | undefined,
  capacity: number,
  clock: number,
): LeaseState {
  if (previous && previous.capacity !== capacity) throw new LimitError("policy-conflict");
  const now = clockTime(clock, previous?.lastNow);
  return {
    capacity,
    lastNow: now,
    leases: previous?.leases.filter((lease) => lease.expiresAt > now) ?? [],
  };
}

export function acquireState(state: LeaseState, input: AcquireInput, token: string): Acquisition {
  const used = state.leases.reduce((sum, lease) => sum + lease.quantity, 0);
  const allowed = input.quantity <= state.capacity - used;
  if (state.leases.some((lease) => lease.token === token)) throw new LimitError("invalid-input");
  const lease: Lease | null = allowed
    ? Object.freeze({
        scope: Object.freeze({ ...input.scope }),
        token,
        quantity: input.quantity,
        expiresAt: state.lastNow + input.ttlMs,
      })
    : null;
  if (lease) state.leases.push(lease);
  return { ...leaseDecision(state, input.quantity, allowed), lease };
}

/** State includes the new lease when allowed, otherwise it contains only current holders. */
export function leaseDecision(state: LeaseState, quantity: number, allowed: boolean): Decision {
  const used = state.leases.reduce((sum, lease) => sum + lease.quantity, 0);
  let retryAfter: number | null = allowed ? 0 : null;
  if (!allowed && quantity <= state.capacity) {
    let available = state.capacity - used;
    for (const item of [...state.leases].sort((a, b) => a.expiresAt - b.expiresAt)) {
      available += item.quantity;
      if (available >= quantity) {
        retryAfter = item.expiresAt - state.lastNow;
        break;
      }
    }
  }
  return {
    allowed,
    remaining: state.capacity - used,
    retryAfter,
    reason: allowed ? "allowed" : quantity > state.capacity ? "too-large" : "exhausted",
  };
}

export function renewState(
  state: LeaseState,
  scope: LimitScope,
  token: string,
  ttlMs: number,
): Lease | null {
  const index = state.leases.findIndex((lease) => lease.token === token);
  if (index === -1) return null;
  const previous = state.leases[index]!;
  const renewed = Object.freeze({
    ...previous,
    scope: Object.freeze({ ...scope }),
    expiresAt: Math.max(previous.expiresAt, state.lastNow + ttlMs),
  });
  state.leases[index] = renewed;
  return renewed;
}
