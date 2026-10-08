import type { Logger } from "@lenso/core/plugin";
import {
  LimitError,
  maxDurationMs,
  positiveInteger,
  scopeKey,
  validateAcquire,
  validateConsume,
  validateToken,
  type Acquisition,
  type AcquireInput,
  type ConsumeInput,
  type CounterKind,
  type Decision,
  type Lease,
  type LimitConfig,
  type LimitStore,
} from "./contracts";
import { validateConfig } from "./config";

export interface LeaseRunOptions {
  readonly signal?: AbortSignal;
  /** The wrapper owns renewal only when explicitly requested, and stops it in finally. */
  readonly renewEveryMs?: number;
}

export interface LeaseExecution {
  readonly signal: AbortSignal;
  /** Null is possible only with explicitly configured fail-open admission. */
  readonly lease: Lease | null;
}

export interface Limits {
  consumeRate(input: ConsumeInput): Promise<Decision>;
  consumeQuota(input: ConsumeInput): Promise<Decision>;
  acquire(input: AcquireInput): Promise<Acquisition>;
  renew(lease: Lease, ttlMs: number): Promise<Lease | null>;
  release(lease: Lease): Promise<void>;
  withLease<T>(
    input: AcquireInput,
    execute: (context: LeaseExecution) => Promise<T>,
    options?: LeaseRunOptions,
  ): Promise<T>;
  /** Stops admission, aborts/drains wrappers, and releases this service's leases, not the store. */
  close(): Promise<void>;
}

export function createLimits(options: {
  readonly store: LimitStore;
  readonly config: LimitConfig;
  readonly logger?: Logger;
}): Limits {
  const config = validateConfig(options.config);
  const owned = new Map<string, Lease>();
  const pending = new Set<Promise<unknown>>();
  const runs = new Map<AbortController, Promise<unknown>>();
  let closed = false;
  let closing: Promise<void> | undefined;

  function operation<T>(work: () => Promise<T>): Promise<T> {
    if (closed) return Promise.reject(new LimitError("closed"));
    const promise = Promise.resolve().then(work);
    pending.add(promise);
    void promise.then(
      () => pending.delete(promise),
      () => pending.delete(promise),
    );
    return promise;
  }

  function failure(error: unknown): Decision {
    if (error instanceof LimitError && error.code !== "backend-failure") throw error;
    // Raw backend messages and scope/token values must not enter the shared logger.
    try {
      options.logger?.warn(
        { event: "limits.backend-failure", policy: config.failurePolicy },
        "Limit backend failed",
      );
    } catch {}
    if (config.failurePolicy === "throw") throw new LimitError("backend-failure", { cause: error });
    return {
      allowed: config.failurePolicy === "allow",
      remaining: null,
      retryAfter: null,
      reason: "backend-failure",
    };
  }

  function consume(kind: CounterKind, input: ConsumeInput): Promise<Decision> {
    validateConsume(input);
    const snapshot = { ...input, scope: { ...input.scope } };
    return operation(async () => {
      try {
        return await options.store.consume(kind, snapshot);
      } catch (error) {
        return failure(error);
      }
    });
  }

  async function releaseOwned(lease: Lease): Promise<void> {
    await options.store.release(lease.scope, lease.token);
    owned.delete(lease.token);
  }

  function validateOwnedScope(lease: Lease): void {
    if (!lease || typeof lease !== "object") throw new LimitError("invalid-input");
    const key = scopeKey(lease.scope);
    const previous = owned.get(lease.token);
    if (previous && scopeKey(previous.scope) !== key) throw new LimitError("invalid-input");
  }

  const service: Limits = {
    consumeRate: (input) => consume("rate", input),
    consumeQuota: (input) => consume("quota", input),
    acquire(input) {
      validateAcquire(input);
      const snapshot = { ...input, scope: { ...input.scope } };
      return operation(async () => {
        try {
          const result = await options.store.acquire(snapshot, crypto.randomUUID());
          if (result.lease) owned.set(result.lease.token, result.lease);
          return result;
        } catch (error) {
          return { ...failure(error), lease: null };
        }
      });
    },
    renew(lease, ttlMs) {
      validateOwnedScope(lease);
      const scope = { ...lease.scope };
      scopeKey(scope);
      validateToken(lease.token);
      positiveInteger(ttlMs, maxDurationMs);
      const token = lease.token;
      return operation(async () => {
        // Renewal has no fail-open equivalent: an unknown outcome is not a valid lease.
        const renewed = await options.store.renew(scope, token, ttlMs);
        if (owned.has(token)) {
          if (renewed) owned.set(token, renewed);
          else owned.delete(token);
        }
        return renewed;
      });
    },
    release(lease) {
      validateOwnedScope(lease);
      validateToken(lease.token);
      const snapshot = { ...lease, scope: { ...lease.scope } };
      return operation(() => releaseOwned(snapshot));
    },
    withLease(input, execute, runOptions = {}) {
      validateAcquire(input);
      input = { ...input, scope: { ...input.scope } };
      runOptions = { ...runOptions };
      if (runOptions.renewEveryMs !== undefined) {
        positiveInteger(runOptions.renewEveryMs);
        if (runOptions.renewEveryMs >= input.ttlMs) throw new LimitError("invalid-input");
      }
      if (closed) return Promise.reject(new LimitError("closed"));
      const controller = new AbortController();
      const abort = () => controller.abort(runOptions.signal?.reason);
      if (runOptions.signal?.aborted) abort();
      else runOptions.signal?.addEventListener("abort", abort, { once: true });
      let timer: ReturnType<typeof setTimeout> | undefined;
      let renewal: Promise<void> | undefined;
      let renewalError: unknown;
      let renewalFailed = false;
      let finishing = false;
      const stopRenewal = () => {
        if (timer !== undefined) clearTimeout(timer);
      };
      controller.signal.addEventListener("abort", stopRenewal, { once: true });

      async function run(): Promise<unknown> {
        let lease: Lease | null = null;
        let value: unknown;
        const errors: unknown[] = [];
        try {
          controller.signal.throwIfAborted();
          const acquisition = await service.acquire(input);
          lease = acquisition.lease;
          if (!acquisition.allowed) throw new LimitError("denied");
          controller.signal.throwIfAborted();
          function scheduleRenewal() {
            if (!lease || finishing || controller.signal.aborted || !runOptions.renewEveryMs)
              return;
            timer = setTimeout(() => {
              renewal = (async () => {
                try {
                  const renewed = await service.renew(lease!, input.ttlMs);
                  if (!renewed) throw new LimitError("lease-lost");
                  lease = renewed;
                  scheduleRenewal();
                } catch (error) {
                  renewalFailed = true;
                  renewalError = error;
                  controller.abort(error);
                }
              })();
            }, runOptions.renewEveryMs);
          }
          scheduleRenewal();
          value = await execute({ signal: controller.signal, lease });
          controller.signal.throwIfAborted();
        } catch (error) {
          errors.push(error);
        } finally {
          finishing = true;
          stopRenewal();
          await renewal;
          if (renewalFailed && !errors.includes(renewalError)) errors.push(renewalError);
          if (lease) {
            try {
              await releaseOwned(lease);
            } catch (error) {
              errors.push(error);
            }
          }
          runOptions.signal?.removeEventListener("abort", abort);
          controller.signal.removeEventListener("abort", stopRenewal);
        }
        if (errors.length === 1) throw errors[0];
        if (errors.length > 1)
          throw new AggregateError(errors, "Lease execution and cleanup failed");
        return value;
      }
      const promise = run() as Promise<Awaited<ReturnType<typeof execute>>>;
      runs.set(controller, promise);
      void promise.then(
        () => runs.delete(controller),
        () => runs.delete(controller),
      );
      return promise;
    },
    close() {
      if (!closing) {
        closed = true;
        // Cache completion before synchronous abort listeners can reenter close().
        closing = Promise.resolve().then(async () => {
          for (const controller of runs.keys()) controller.abort(new LimitError("closed"));
          // Wrappers release in finally before the borrowed database owner's cleanup.
          await Promise.allSettled([...runs.values(), ...pending]);
          const results = await Promise.allSettled([...owned.values()].map(releaseOwned));
          const errors = results.flatMap((result) =>
            result.status === "rejected" ? [result.reason] : [],
          );
          if (errors.length) throw new AggregateError(errors, "Limit lease cleanup failed");
        });
      }
      return closing;
    },
  };
  return service;
}
