import { expect, test } from "bun:test";
import { createLimits, createMemoryLimitStore, LimitError, type LimitStore } from "../src/index";

const scope = { instance: "service", tenant: "tenant", key: "work" };
const input = { scope, capacity: 1, quantity: 1, ttlMs: 1000 };
const consume = { scope, capacity: 1, quantity: 1, periodMs: 1000 };
function deferred<T>() {
  return Promise.withResolvers<T>();
}

test("fault admission is explicit and never invents a counter or lease", async () => {
  const cause = new Error("test backend offline");
  const broken: LimitStore = {
    consume: async () => {
      throw cause;
    },
    acquire: async () => {
      throw cause;
    },
    renew: async () => {
      throw cause;
    },
    release: async () => {
      throw cause;
    },
  };
  for (const policy of ["deny", "allow"] as const) {
    const limits = createLimits({ store: broken, config: { failurePolicy: policy } });
    expect(await limits.consumeRate(consume)).toEqual({
      allowed: policy === "allow",
      remaining: null,
      retryAfter: null,
      reason: "backend-failure",
    });
    expect(await limits.acquire(input)).toEqual({
      allowed: policy === "allow",
      remaining: null,
      retryAfter: null,
      reason: "backend-failure",
      lease: null,
    });
    if (policy === "allow")
      await limits.withLease(input, async ({ lease }) => {
        expect(lease).toBeNull();
      });
    else
      await expect(
        limits.withLease(input, async () => {
          throw new Error("must not execute");
        }),
      ).rejects.toMatchObject({ code: "denied" });
    await limits.close();
  }
  const strict = createLimits({ store: broken, config: { failurePolicy: "throw" } });
  await expect(strict.consumeQuota(consume)).rejects.toMatchObject({
    code: "backend-failure",
    cause,
  });
  await expect(strict.acquire(input)).rejects.toMatchObject({ code: "backend-failure", cause });
  await strict.close();
});

test("finally releases on success, exception, cooperative cancellation and pre-abort", async () => {
  const store = createMemoryLimitStore();
  const limits = createLimits({ store, config: { failurePolicy: "throw" } });
  expect(await limits.withLease(input, async () => 42)).toBe(42);
  const cause = new Error("callback failure");
  await expect(
    limits.withLease(input, async () => {
      throw cause;
    }),
  ).rejects.toBe(cause);
  const entered = deferred<void>();
  const controller = new AbortController();
  const work = limits.withLease(
    input,
    async ({ signal }) => {
      entered.resolve();
      await new Promise<void>((resolve) =>
        signal.addEventListener("abort", () => resolve(), { once: true }),
      );
      signal.throwIfAborted();
    },
    { signal: controller.signal },
  );
  void work.catch(() => {});
  await entered.promise;
  controller.abort(cause);
  await expect(work).rejects.toBe(cause);
  const result = await limits.acquire(input);
  expect(result.allowed).toBe(true);
  await limits.release(result.lease!);
  let executed = false;
  await expect(
    limits.withLease(
      input,
      async () => {
        executed = true;
      },
      { signal: controller.signal },
    ),
  ).rejects.toBe(cause);
  expect(executed).toBe(false);
  await limits.close();
});

test("close drains cooperative wrappers, owns only its leases and leaves borrowed store usable", async () => {
  const store = createMemoryLimitStore();
  const limits = createLimits({ store, config: { failurePolicy: "throw" } });
  const unrelated = await store.acquire(
    { ...input, scope: { ...scope, key: "borrowed" } },
    "borrowed-token",
  );
  const entered = deferred<void>();
  const finished = deferred<void>();
  const work = limits.withLease(
    input,
    async ({ signal }) => {
      entered.resolve();
      await new Promise<void>((resolve) =>
        signal.addEventListener("abort", () => resolve(), { once: true }),
      );
      finished.resolve();
      signal.throwIfAborted();
    },
    { renewEveryMs: 100 },
  );
  void work.catch(() => {});
  await entered.promise;
  const close = limits.close();
  expect(limits.close()).toBe(close);
  await close;
  await expect(work).rejects.toMatchObject({ code: "closed" });
  await finished.promise;
  expect((await store.acquire(input, "new-owner")).allowed).toBe(true);
  expect((await store.acquire({ ...input, scope: unrelated.lease!.scope }, "other")).allowed).toBe(
    false,
  );
  await expect(limits.acquire(input)).rejects.toMatchObject({ code: "closed" });
});

test("close waits for in-flight acquisition before releasing it", async () => {
  const store = createMemoryLimitStore();
  const waiting = deferred<void>();
  const delayed: LimitStore = {
    ...store,
    acquire: async (...args) => {
      await waiting.promise;
      return store.acquire(...args);
    },
  };
  const limits = createLimits({ store: delayed, config: { failurePolicy: "throw" } });
  const acquisition = limits.acquire(input);
  const close = limits.close();
  waiting.resolve();
  await acquisition;
  await close;
  expect((await store.acquire(input, "after-close")).allowed).toBe(true);
});

test("abort listeners reentering close share one cleanup completion", async () => {
  const store = createMemoryLimitStore();
  let releases = 0;
  const limits = createLimits({
    store: {
      ...store,
      release: async (...args) => {
        releases++;
        await store.release(...args);
      },
    },
    config: { failurePolicy: "throw" },
  });
  const entered = deferred<void>();
  let reentered: Promise<void> | undefined;
  const work = limits.withLease(input, async ({ signal }) => {
    entered.resolve();
    await new Promise<void>((resolve) =>
      signal.addEventListener(
        "abort",
        () => {
          reentered = limits.close();
          resolve();
        },
        { once: true },
      ),
    );
  });
  void work.catch(() => {});
  await entered.promise;
  const close = limits.close();
  await close;
  expect(reentered).toBe(close);
  expect(releases).toBe(1);
  await expect(work).rejects.toMatchObject({ code: "closed" });
});

test("manual leases do not start hidden renewal; wrapper renewal has an explicit owner", async () => {
  const store = createMemoryLimitStore();
  const renewed = deferred<void>();
  let renewals = 0;
  const observed: LimitStore = {
    ...store,
    renew: async (...args) => {
      renewals++;
      const result = await store.renew(...args);
      renewed.resolve();
      return result;
    },
  };
  const limits = createLimits({ store: observed, config: { failurePolicy: "throw" } });
  const manual = (await limits.acquire(input)).lease!;
  await Bun.sleep(15);
  expect(renewals).toBe(0);
  await limits.release(manual);
  await limits.withLease(
    input,
    async () => {
      await renewed.promise;
    },
    { renewEveryMs: 5 },
  );
  const before = renewals;
  expect(before).toBeGreaterThan(0);
  await Bun.sleep(15);
  expect(renewals).toBe(before);
  expect((await limits.acquire(input)).allowed).toBe(true);
  await limits.close();
});

test("caller mutation cannot change the wrapper's signal or validated renewal schedule", async () => {
  const store = createMemoryLimitStore();
  const renewed = deferred<void>();
  const entered = deferred<void>();
  const original = new AbortController();
  const replacement = new AbortController();
  const cause = new Error("original cancellation");
  const limits = createLimits({
    store: {
      ...store,
      renew: async (...args) => {
        const lease = await store.renew(...args);
        renewed.resolve();
        return lease;
      },
    },
    config: { failurePolicy: "throw" },
  });
  const options: { signal: AbortSignal; renewEveryMs: number | undefined } = {
    signal: original.signal,
    renewEveryMs: 5,
  };
  const work = limits.withLease(
    input,
    async ({ signal }) => {
      entered.resolve();
      await new Promise<void>((resolve) =>
        signal.addEventListener("abort", () => resolve(), { once: true }),
      );
      signal.throwIfAborted();
    },
    options,
  );
  void work.catch(() => {});
  await entered.promise;
  options.signal = replacement.signal;
  options.renewEveryMs = undefined;
  await renewed.promise;
  original.abort(cause);
  await expect(work).rejects.toBe(cause);
  expect((await store.acquire(input, "after-cancellation")).allowed).toBe(true);
  await limits.close();
});

test("lost renewal aborts execution and old finally cannot release a replacement", async () => {
  let now = 0;
  const store = createMemoryLimitStore({ now: () => now });
  const entered = deferred<void>();
  const replacementReady = deferred<void>();
  const limits = createLimits({ store, config: { failurePolicy: "throw" } });
  const work = limits.withLease(
    input,
    async ({ signal }) => {
      now = 1000;
      entered.resolve();
      await replacementReady.promise;
      if (!signal.aborted)
        await new Promise<void>((resolve) =>
          signal.addEventListener("abort", () => resolve(), { once: true }),
        );
      signal.throwIfAborted();
    },
    { renewEveryMs: 5 },
  );
  void work.catch(() => {});
  await entered.promise;
  const replacement = await store.acquire(input, "replacement");
  replacementReady.resolve();
  await expect(work).rejects.toMatchObject({ code: "lease-lost" });
  expect(replacement.allowed).toBe(true);
  expect((await store.acquire(input, "should-deny")).allowed).toBe(false);
  await limits.close();
});

test("renewal backend faults never silently continue under fail-open admission", async () => {
  const store = createMemoryLimitStore();
  const cause = new Error("renewal backend failed");
  const limits = createLimits({
    store: {
      ...store,
      renew: async () => {
        throw cause;
      },
    },
    config: { failurePolicy: "allow" },
  });
  await expect(
    limits.withLease(
      input,
      async ({ signal }) => {
        await new Promise<void>((resolve) =>
          signal.addEventListener("abort", () => resolve(), { once: true }),
        );
        signal.throwIfAborted();
      },
      { renewEveryMs: 5 },
    ),
  ).rejects.toBe(cause);
  expect((await store.acquire(input, "after-fault")).allowed).toBe(true);
  await limits.close();
});

test("business and release failure both survive; shutdown retries an owned release", async () => {
  const store = createMemoryLimitStore();
  const business = new Error("business");
  const cleanup = new Error("cleanup");
  let broken = true;
  const limits = createLimits({
    store: {
      ...store,
      release: async (...args) => {
        if (broken) throw cleanup;
        await store.release(...args);
      },
    },
    config: { failurePolicy: "allow" },
  });
  try {
    await limits.withLease(input, async () => {
      throw business;
    });
    throw new Error("must reject");
  } catch (error) {
    expect(error).toBeInstanceOf(AggregateError);
    expect((error as AggregateError).errors).toEqual([business, cleanup]);
  }
  broken = false;
  await limits.close();
  expect((await store.acquire(input, "retry-succeeded")).allowed).toBe(true);
});

test("invalid renewal schedules fail before acquire", () => {
  const limits = createLimits({
    store: createMemoryLimitStore(),
    config: { failurePolicy: "throw" },
  });
  for (const renewEveryMs of [0, -1, 1000, 1001])
    expect(() => limits.withLease(input, async () => {}, { renewEveryMs })).toThrow(LimitError);
});
