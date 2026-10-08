import { describe, expect, test } from "bun:test";
import { heapStats } from "bun:jsc";
import { createCache, CacheError, type CacheAdapter, type JsonValue } from "../src/index";
import { createMemoryCacheAdapter } from "../src/memory";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
function cache(options: Partial<Parameters<typeof createCache>[0]> = {}) {
  return createCache({ adapter: createMemoryCacheAdapter(), namespace: "test", ...options });
}
function failing(): CacheAdapter {
  const fail = async () => {
    throw new Error("private credentials and value");
  };
  return {
    capabilities: createMemoryCacheAdapter().capabilities,
    generation: fail,
    getMany: fail,
    set: fail,
    delete: fail,
    invalidate: fail,
  };
}

describe("typed cache service", () => {
  test("distinguishes cached null from miss, clones JSON and preserves batch order", async () => {
    const store = cache();
    expect(await store.get("missing")).toEqual({ status: "miss", reason: "absent" });
    const value = { nested: [null, 1, "你好", true] };
    await store.set("value", value);
    value.nested[1] = 99;
    await store.set("null", null);
    expect(await store.getMany(["null", "missing", "value", "null"])).toEqual([
      { status: "hit", value: null },
      { status: "miss", reason: "absent" },
      { status: "hit", value: { nested: [null, 1, "你好", true] } },
      { status: "hit", value: null },
    ]);
    expect(await store.getMany([])).toEqual([]);
  });

  test("finite TTL defaults, zero removes old values, invalid boundaries reject", async () => {
    const store = cache({ maxTtlMs: 60_000 });
    expect(store.config.defaultTtlMs).toBe(60_000);
    await store.set("k", "old");
    expect(await store.set("k", "ignored", { ttlMs: 0 })).toEqual({ outcome: "skipped" });
    expect((await store.get("k")).status).toBe("miss");
    for (const ttlMs of [-1, 0.5, NaN, Infinity, 60_001, null]) {
      await expect(store.set("k", "bad", { ttlMs: ttlMs as number })).rejects.toMatchObject({
        code: "invalid-input",
      });
    }
    await store.set("k", "valid", { ttlMs: 60_000 });
    expect(await store.get("k")).toEqual({ status: "hit", value: "valid" });
    const disabled = cache({ defaultTtlMs: 0 });
    expect(await disabled.set("k", null)).toEqual({ outcome: "skipped" });
  });

  test("rejects invalid configuration, names, sparse and oversized batches", async () => {
    for (const config of [
      { namespace: "" },
      { defaultTtlMs: -1 },
      { maxTtlMs: 0 },
      { maxTtlMs: 86_400_001 },
      { maxValueBytes: 0 },
      { maxInFlight: 0 },
    ])
      expect(() => cache(config)).toThrow(CacheError);
    const store = cache();
    for (const key of ["", "\n", "\u0085", "x".repeat(257), "\ud800"]) {
      await expect(store.get(key)).rejects.toMatchObject({ code: "invalid-input" });
    }
    const sparse: string[] = [];
    sparse.length = 1;
    await expect(store.getMany(sparse)).rejects.toMatchObject({ code: "invalid-input" });
    await expect(store.getMany(Array(101).fill("k"))).rejects.toMatchObject({
      code: "invalid-input",
    });
  });

  test("scopes and plugin namespaces isolate, invalidation is exact and delimiter-safe", async () => {
    const adapter = createMemoryCacheAdapter();
    const root = cache({ adapter });
    const child = root.scope("a:b");
    const nested = root.scope("a").scope("b");
    const other = cache({ adapter, namespace: "other" });
    for (const store of [root, child, nested, other])
      await store.set("same", store === root ? "root" : "child");
    await root.invalidate();
    expect((await root.get("same")).status).toBe("miss");
    for (const store of [child, nested, other])
      expect((await store.get("same")).status).toBe("hit");
    await child.invalidate();
    expect((await child.get("same")).status).toBe("miss");
    expect((await nested.get("same")).status).toBe("hit");
  });

  test("strict JSON rejects lossy values and accessors without executing them", async () => {
    const store = cache();
    const cycle: Record<string, unknown> = {};
    cycle.self = cycle;
    let accessed = false;
    const accessor = Object.defineProperty({}, "x", {
      enumerable: true,
      get() {
        accessed = true;
        return 1;
      },
    });
    class CustomArray extends Array {
      toJSON() {
        accessed = true;
        return [];
      }
    }
    const sparse: JsonValue[] = [];
    sparse.length = 2;
    for (const value of [
      undefined,
      NaN,
      Infinity,
      1n,
      new Date(),
      new Map(),
      () => {},
      [undefined],
      sparse,
      { x: undefined },
      { x: Symbol() },
      cycle,
      accessor,
      new CustomArray(),
      Object.assign({}, { [Symbol()]: 1 }),
    ])
      await expect(store.set("k", value as JsonValue)).rejects.toMatchObject({
        code: "serialization",
      });
    expect(accessed).toBe(false);
    await store.set("plain", Object.assign(Object.create(null), { ok: 1 }));
    expect(await store.get("plain")).toEqual({ status: "hit", value: { ok: 1 } });
  });

  test("limits container nesting to exactly 64 layers", async () => {
    const store = cache();
    let value: JsonValue = null;
    for (let depth = 0; depth < 64; depth++) value = [value];
    await store.set("depth", value);
    expect(await store.get("depth")).toEqual({ status: "hit", value });
    await expect(store.set("deep", [value])).rejects.toMatchObject({ code: "serialization" });
  });

  test("enforces serialized envelope size, runtime guard and corruption policy", async () => {
    const adapter = createMemoryCacheAdapter();
    const store = createCache<string>({
      adapter,
      namespace: "typed",
      maxValueBytes: 128,
      validate: (value): value is string => typeof value === "string",
    });
    await expect(store.set("large", "x".repeat(129))).rejects.toMatchObject({
      code: "serialization",
    });
    await expect(store.set("type", 1 as unknown as string)).rejects.toMatchObject({
      code: "serialization",
    });
    const namespace = JSON.stringify(["typed"]);
    const generation = await adapter.generation(namespace);
    for (const raw of [
      "not json",
      JSON.stringify({ v: 2, expiresAt: Date.now() + 1000, value: "x" }),
      JSON.stringify({ v: 1, expiresAt: Date.now() + 1000, value: 1 }),
      "x".repeat(129),
    ]) {
      await adapter.set(namespace, generation, "bad", raw, 1000);
      expect(await store.get("bad")).toEqual({ status: "miss", reason: "corrupt" });
    }
  });

  test("never returns logical expiry even if adapter retains the entry", async () => {
    const adapter = createMemoryCacheAdapter();
    const store = cache({ adapter });
    const namespace = JSON.stringify(["test"]);
    const generation = await adapter.generation(namespace);
    await adapter.set(
      namespace,
      generation,
      "expired",
      JSON.stringify({ v: 1, expiresAt: Date.now(), value: "stale" }),
      1000,
    );
    expect(await store.get("expired")).toEqual({ status: "miss", reason: "expired" });
  });

  test("fail-closed sanitizes driver errors; fail-open explicitly bypasses without local fallback", async () => {
    const strict = cache({ adapter: failing() });
    await expect(strict.get("sensitive-key")).rejects.toMatchObject({
      code: "backend",
      message: "Cache operation failed (backend)",
    });
    await expect(strict.set("k", null)).rejects.toMatchObject({ code: "backend" });
    await expect(strict.delete("k")).rejects.toMatchObject({ code: "backend" });
    await expect(strict.invalidate()).rejects.toMatchObject({ code: "backend" });
    expect((await strict.getMany(["k", "k"])).map((result) => result.status)).toEqual([
      "error",
      "error",
    ]);
    const events: unknown[] = [];
    const open = cache({
      adapter: failing(),
      failureMode: "fail-open",
      onEvent: (event) => events.push(event),
    });
    expect(await open.get("sensitive-key")).toEqual({ status: "miss", reason: "backend" });
    expect(await open.set("k", "private-value")).toEqual({ outcome: "bypassed" });
    expect(await open.delete("k")).toEqual({ outcome: "bypassed" });
    expect(await open.invalidate()).toEqual({ outcome: "bypassed" });
    expect(await open.getOrSet("k", async () => "fresh")).toBe("fresh");
    expect(await open.get("k")).toEqual({ status: "miss", reason: "backend" });
    expect(JSON.stringify(events)).not.toContain("sensitive");
    expect(JSON.stringify(events)).not.toContain("private");
  });

  test("batch failure retains healthy keys and exposes per-key errors", async () => {
    const adapter = createMemoryCacheAdapter();
    const mixed = cache({
      adapter: {
        ...adapter,
        getMany: async () => [
          JSON.stringify({ v: 1, expiresAt: Date.now() + 1000, value: null }),
          new CacheError("backend"),
          null,
        ],
      },
    });
    const results = await mixed.getMany(["ok", "bad", "miss"]);
    expect(results[0]).toEqual({ status: "hit", value: null });
    expect(results[1]).toMatchObject({ status: "error", error: { code: "backend" } });
    expect(results[2]).toEqual({ status: "miss", reason: "absent" });
  });
});

describe("instance-local getOrSet", () => {
  test("coalesces loads, first loader/TTL wins, and returns independent JSON copies", async () => {
    const store = cache();
    const start = deferred<void>();
    const end = deferred<JsonValue>();
    let calls = 0;
    const loader = async () => {
      calls++;
      start.resolve();
      return end.promise;
    };
    const one = store.getOrSet("k", loader);
    const two = store.getOrSet("k", loader);
    await start.promise;
    expect(calls).toBe(1);
    end.resolve({ x: 1 });
    const [a, b] = await Promise.all([one, two]);
    expect(a).toEqual(b);
    expect(a).not.toBe(b);
    expect(
      await store.getOrSet("k", async () => {
        throw new Error("must not load");
      }),
    ).toEqual(a);
  });

  test("load exceptions reach all callers and allow retry, including undefined rejection", async () => {
    const store = cache();
    const start = deferred<void>();
    const end = deferred<JsonValue>();
    const error = new Error("loader failure");
    const loader = async () => {
      start.resolve();
      return end.promise;
    };
    const one = store.getOrSet("k", loader).catch((e) => e);
    const two = store.getOrSet("k", loader).catch((e) => e);
    await start.promise;
    end.reject(error);
    expect(await one).toBe(error);
    expect(await two).toBe(error);
    const result = await store
      .getOrSet("k", async () => {
        throw undefined;
      })
      .then(
        () => ({ rejected: false }),
        (reason) => ({ rejected: true, reason }),
      );
    expect(result).toEqual({ rejected: true, reason: undefined });
    expect(await store.getOrSet("k", async () => "retry")).toBe("retry");
  });

  test("one waiter's cancellation does not cancel other callers", async () => {
    const store = cache();
    const start = deferred<AbortSignal>();
    const end = deferred<JsonValue>();
    const caller = new AbortController();
    const loader = async (signal: AbortSignal) => {
      start.resolve(signal);
      return end.promise;
    };
    const one = store.getOrSet("k", loader, { signal: caller.signal }).catch((error) => error);
    const two = store.getOrSet("k", loader);
    const signal = await start.promise;
    caller.abort();
    expect(await one).toMatchObject({ code: "aborted" });
    expect(signal.aborted).toBe(false);
    end.resolve("done");
    expect(await two).toBe("done");
    expect(await store.get("k")).toEqual({ status: "hit", value: "done" });
  });

  test("all waiters cancelled abort loader, suppress write and permit a fresh load", async () => {
    const store = cache();
    const start = deferred<AbortSignal>();
    const end = deferred<JsonValue>();
    const caller = new AbortController();
    const load = store
      .getOrSet(
        "k",
        async (signal) => {
          start.resolve(signal);
          return end.promise;
        },
        { signal: caller.signal },
      )
      .catch((error) => error);
    const signal = await start.promise;
    caller.abort();
    expect(await load).toMatchObject({ code: "aborted" });
    expect(signal.aborted).toBe(true);
    expect(await store.getOrSet("k", async () => "fresh")).toBe("fresh");
    end.resolve("ignored cancellation");
    await Promise.resolve();
    await Promise.resolve();
    expect(await store.get("k")).toEqual({ status: "hit", value: "fresh" });
    await expect(
      store.getOrSet("new", async () => "never", { signal: AbortSignal.abort() }),
    ).rejects.toMatchObject({ code: "aborted" });
  });

  test("repeated cancelled joiners are not retained by one pending loader", async () => {
    const store = cache({ maxInFlight: 1 });
    const start = deferred<void>();
    const end = deferred<JsonValue>();
    const keeper = store.getOrSet("k", async () => {
      start.resolve();
      return end.promise;
    });
    await start.promise;
    await Bun.sleep(0);
    Bun.gc(true);
    const before = heapStats().heapSize;
    for (let index = 0; index < 20_000; index++) {
      const caller = new AbortController();
      const joining = store
        .getOrSet("k", async () => null, { signal: caller.signal })
        .catch(() => undefined);
      caller.abort();
      await joining;
    }
    await Bun.sleep(0);
    Bun.gc(true);
    const retained = heapStats().heapSize - before;
    end.resolve("keeper");
    expect(await keeper).toBe("keeper");
    store.close();
    // A pending load must not retain completed, cancelled callers.
    expect(retained).toBeLessThan(6 * 1024 * 1024);
  });

  test("set, delete and exact-scope invalidation suppress late load writes", async () => {
    for (const operation of ["set", "delete", "invalidate"] as const) {
      const store = cache();
      const start = deferred<void>();
      const end = deferred<JsonValue>();
      const load = store.getOrSet("k", async () => {
        start.resolve();
        return end.promise;
      });
      await start.promise;
      if (operation === "set") await store.set("k", "new");
      if (operation === "delete") await store.delete("k");
      if (operation === "invalidate") await store.invalidate();
      end.resolve("old");
      expect(await load).toBe("old"); // A caller already loading may receive its result.
      expect(await store.get("k")).toEqual(
        operation === "set"
          ? { status: "hit", value: "new" }
          : { status: "miss", reason: "absent" },
      );
    }
  });

  test("zero TTL ignores/removes old hit and does not store loader result", async () => {
    const store = cache();
    await store.set("k", "old");
    expect(await store.getOrSet("k", async () => "new", { ttlMs: 0 })).toBe("new");
    expect((await store.get("k")).status).toBe("miss");
  });

  test("first coalesced caller chooses TTL even when a later caller requests storage", async () => {
    const store = cache();
    const start = deferred<void>();
    const end = deferred<JsonValue>();
    const first = store.getOrSet(
      "k",
      async () => {
        start.resolve();
        return end.promise;
      },
      { ttlMs: 0 },
    );
    await start.promise;
    const second = store.getOrSet(
      "k",
      async () => {
        throw new Error("second loader");
      },
      { ttlMs: 1000 },
    );
    end.resolve("first");
    expect(await Promise.all([first, second])).toEqual(["first", "first"]);
    expect((await store.get("k")).status).toBe("miss");
  });

  test("zero-TTL load cannot delete a newer set or dispatch deletion after cancellation", async () => {
    for (const operation of ["set", "abort"] as const) {
      const adapter = createMemoryCacheAdapter();
      const entered = deferred<void>();
      const resume = deferred<void>();
      const completed = deferred<void>();
      let deletes = 0;
      const delayed: CacheAdapter = {
        ...adapter,
        async getMany(namespace, generation, keys) {
          entered.resolve();
          await resume.promise;
          const result = await adapter.getMany(namespace, generation, keys);
          completed.resolve();
          return result;
        },
        async delete(...args) {
          deletes++;
          await adapter.delete(...args);
        },
      };
      const store = cache({ adapter: delayed });
      const caller = new AbortController();
      const load = store
        .getOrSet("k", async () => "loaded", { ttlMs: 0, signal: caller.signal })
        .catch((error) => error);
      await entered.promise;
      if (operation === "set") await store.set("k", "newer");
      if (operation === "abort") caller.abort();
      resume.resolve();
      if (operation === "set") expect(await load).toBe("loaded");
      else expect(await load).toMatchObject({ code: "aborted" });
      await completed.promise;
      await Promise.resolve();
      expect(deletes).toBe(0);
      if (operation === "set")
        expect(await store.get("k")).toEqual({ status: "hit", value: "newer" });
      store.close();
    }
  });

  test("bounds active loaders; closing aborts loads and never closes borrowed adapter", async () => {
    const adapter = createMemoryCacheAdapter();
    const store = cache({ adapter, maxInFlight: 1 });
    const child = store.scope("child");
    const start = deferred<void>();
    const end = deferred<JsonValue>();
    const load = child
      .getOrSet("k", async () => {
        start.resolve();
        return end.promise;
      })
      .catch((error) => error);
    await start.promise;
    await expect(store.getOrSet("another", async () => null)).rejects.toMatchObject({
      code: "busy",
    });
    store.close();
    store.close();
    expect(await load).toMatchObject({ code: "aborted" });
    await expect(child.get("k")).rejects.toMatchObject({ code: "closed" });
    end.resolve("late");
    const borrowed = cache({ adapter });
    await borrowed.set("ok", "usable");
    expect(await borrowed.get("ok")).toEqual({ status: "hit", value: "usable" });
  });
});
