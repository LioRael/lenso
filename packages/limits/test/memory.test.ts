import { expect, test } from "bun:test";
import {
  createLimits,
  createMemoryLimitStore,
  LimitError,
  type AcquireInput,
  type ConsumeInput,
} from "../src/index";

const scope = { instance: "reports", tenant: "tenant-a", key: "subject:123" };
const consume: ConsumeInput = { scope, capacity: 5, quantity: 1, periodMs: 1000 };
const acquire: AcquireInput = { scope, capacity: 5, quantity: 2, ttlMs: 1000 };

test("fixed windows, quantity-aware retry, exact boundary and backward clock", async () => {
  let now = 999;
  const limits = createLimits({
    store: createMemoryLimitStore({ now: () => now }),
    config: { failurePolicy: "throw" },
  });
  expect(await limits.consumeRate({ ...consume, quantity: 5 })).toEqual({
    allowed: true,
    remaining: 0,
    retryAfter: 0,
    reason: "allowed",
  });
  expect(await limits.consumeRate(consume)).toEqual({
    allowed: false,
    remaining: 0,
    retryAfter: 1,
    reason: "exhausted",
  });
  now = 1000;
  expect((await limits.consumeRate(consume)).remaining).toBe(4);
  now = 999;
  expect((await limits.consumeRate(consume)).remaining).toBe(3);
  now = 1500;
  expect(await limits.consumeRate({ ...consume, quantity: 4 })).toEqual({
    allowed: false,
    remaining: 3,
    retryAfter: 500,
    reason: "exhausted",
  });
  expect(await limits.consumeRate({ ...consume, quantity: 2_147_483_647 })).toEqual({
    allowed: false,
    remaining: 3,
    retryAfter: null,
    reason: "too-large",
  });
  now = 9000;
  expect((await limits.consumeRate(consume)).remaining).toBe(4);
  await limits.close();
});

test("namespaces, tenants, instances and ambiguous strings stay isolated", async () => {
  const store = createMemoryLimitStore({ now: () => 100 });
  const limits = createLimits({ store, config: { failurePolicy: "deny" } });
  await limits.consumeRate({ ...consume, quantity: 5 });
  expect((await limits.consumeQuota(consume)).remaining).toBe(4);
  expect(
    (await limits.consumeRate({ ...consume, scope: { ...scope, tenant: "tenant-b" } })).allowed,
  ).toBe(true);
  expect(
    (await limits.consumeRate({ ...consume, scope: { ...scope, instance: "other" } })).allowed,
  ).toBe(true);
  const a = { instance: "a:b", tenant: "c", key: "d" };
  const b = { instance: "a", tenant: "b:c", key: "d" };
  await limits.consumeRate({ ...consume, scope: a, quantity: 5 });
  expect((await limits.consumeRate({ ...consume, scope: b })).allowed).toBe(true);
  const otherStore = createLimits({
    store: createMemoryLimitStore(),
    config: { failurePolicy: "throw" },
  });
  expect((await otherStore.consumeRate(consume)).allowed).toBe(true);
  await Promise.all([limits.close(), otherStore.close()]);
});

test("concurrent consumption and leases count quantity, not calls", async () => {
  let now = 100;
  const limits = createLimits({
    store: createMemoryLimitStore({ now: () => now }),
    config: { failurePolicy: "throw" },
  });
  const results = await Promise.all(Array.from({ length: 50 }, () => limits.consumeRate(consume)));
  expect(results.filter((result) => result.allowed)).toHaveLength(5);
  const first = (await limits.acquire(acquire)).lease!;
  now = 500;
  const second = (await limits.acquire(acquire)).lease!;
  expect(first.token).not.toBe(second.token);
  expect(await limits.acquire({ ...acquire, quantity: 4 })).toMatchObject({
    allowed: false,
    remaining: 1,
    retryAfter: 1000,
    lease: null,
  });
  expect(await limits.acquire({ ...acquire, quantity: 6 })).toMatchObject({
    allowed: false,
    retryAfter: null,
    reason: "too-large",
  });
  now = 1100;
  const third = (await limits.acquire(acquire)).lease!;
  expect(await limits.renew(first, 1000)).toBeNull();
  await limits.release(first);
  await limits.release(first);
  expect((await limits.acquire(acquire)).allowed).toBe(false);
  expect((await limits.renew(third, 1000))!.token).toBe(third.token);
  now = 100;
  expect((await limits.renew(third, 1))!.expiresAt).toBe(third.expiresAt);
  await limits.release(second);
  expect((await limits.acquire(acquire)).allowed).toBe(true);
  await limits.close();
});

test("invalid numeric inputs and policy changes never become fail-open", async () => {
  const limits = createLimits({
    store: createMemoryLimitStore(),
    config: { failurePolicy: "allow" },
  });
  for (const field of ["capacity", "quantity", "periodMs"] as const) {
    const maximum = field === "periodMs" ? 31_622_400_000 : 2_147_483_647;
    for (const value of [0, -1, 1.5, NaN, Infinity, maximum + 1, Number.MAX_SAFE_INTEGER]) {
      expect(() => limits.consumeRate({ ...consume, [field]: value })).toThrow(LimitError);
    }
  }
  for (const field of ["capacity", "quantity", "ttlMs"] as const) {
    expect(() => limits.acquire({ ...acquire, [field]: 0 })).toThrow(LimitError);
  }
  expect(() => limits.consumeRate({ ...consume, scope: { ...scope, tenant: "" } })).toThrow(
    LimitError,
  );
  expect(() => createLimits({ store: createMemoryLimitStore(), config: {} as never })).toThrow(
    LimitError,
  );
  await limits.consumeRate(consume);
  await expect(limits.consumeRate({ ...consume, capacity: 100 })).rejects.toMatchObject({
    code: "policy-conflict",
  });
  await expect(limits.consumeRate({ ...consume, periodMs: 2000 })).rejects.toMatchObject({
    code: "policy-conflict",
  });
  await limits.acquire(acquire);
  await expect(limits.acquire({ ...acquire, capacity: 100 })).rejects.toMatchObject({
    code: "policy-conflict",
  });
  await limits.close();
});

test("input and returned scope snapshots cannot rewrite limiter state", async () => {
  const store = createMemoryLimitStore();
  const limits = createLimits({ store, config: { failurePolicy: "throw" } });
  const mutable = { ...acquire, scope: { ...scope } };
  const promise = limits.acquire(mutable);
  mutable.scope.tenant = "changed";
  const lease = (await promise).lease!;
  expect(lease.scope.tenant).toBe(scope.tenant);
  expect(() => {
    (lease.scope as { tenant: string }).tenant = "changed";
  }).toThrow();
  expect(() => {
    (lease as { quantity: number }).quantity = 0;
  }).toThrow();
  expect((await limits.acquire(acquire)).remaining).toBe(1);
  const wrongScope = { ...lease, scope: { ...lease.scope, tenant: "wrong" } };
  expect(() => limits.release(wrongScope)).toThrow(LimitError);
  expect(() => limits.renew(wrongScope, 1000)).toThrow(LimitError);
  await limits.close();
  expect((await store.acquire(acquire, "after-close")).allowed).toBe(true);
});
