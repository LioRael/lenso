import {
  maxDurationMs,
  positiveInteger,
  scopeKey,
  validateAcquire,
  validateConsume,
  validateKind,
  validateToken,
  type LimitStore,
} from "./contracts";
import {
  acquireState,
  consumeState,
  liveState,
  renewState,
  type CounterState,
  type LeaseState,
} from "./state";

/** State belongs to this store object only. Sharing a process does not imply sharing a store. */
export function createMemoryLimitStore(options: { now?: () => number } = {}): LimitStore {
  const now = options.now ?? Date.now;
  const counters = new Map<string, CounterState>();
  const concurrency = new Map<string, LeaseState>();
  return {
    async consume(kind, input) {
      validateKind(kind);
      validateConsume(input);
      const key = JSON.stringify([kind, scopeKey(input.scope)]);
      const { state, result } = consumeState(counters.get(key), input, now());
      counters.set(key, state);
      return result;
    },
    async acquire(input, token) {
      validateAcquire(input);
      validateToken(token);
      const key = scopeKey(input.scope);
      const state = liveState(concurrency.get(key), input.capacity, now());
      const result = acquireState(state, input, token);
      concurrency.set(key, state);
      return result;
    },
    async renew(scope, token, ttlMs) {
      const key = scopeKey(scope);
      validateToken(token);
      positiveInteger(ttlMs, maxDurationMs);
      const previous = concurrency.get(key);
      if (!previous) return null;
      const state = liveState(previous, previous.capacity, now());
      const lease = renewState(state, scope, token, ttlMs);
      concurrency.set(key, state);
      return lease;
    },
    async release(scope, token) {
      const key = scopeKey(scope);
      validateToken(token);
      const previous = concurrency.get(key);
      if (!previous) return;
      const state = liveState(previous, previous.capacity, now());
      state.leases = state.leases.filter((lease) => lease.token !== token);
      concurrency.set(key, state);
    },
  };
}
