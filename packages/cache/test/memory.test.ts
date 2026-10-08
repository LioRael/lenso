import { describe, expect, test } from "bun:test";
import { CacheError } from "../src/contracts";
import { createMemoryCacheAdapter } from "../src/memory";

describe("memory cache adapter", () => {
  test("expires at the exact TTL boundary and overwrites old expiry", async () => {
    let time = 100;
    const adapter = createMemoryCacheAdapter({ now: () => time });
    const generation = await adapter.generation("scope");
    await adapter.set("scope", generation, "key", "old", 10);
    time = 105;
    await adapter.set("scope", generation, "key", "new", 20);
    time = 110;
    expect(await adapter.getMany("scope", generation, ["key"])).toEqual(["new"]);
    time = 125;
    expect(await adapter.getMany("scope", generation, ["key"])).toEqual([null]);
  });

  test("isolates namespaces and safely handles colliding key combinations", async () => {
    const adapter = createMemoryCacheAdapter();
    const a = await adapter.generation("a");
    const b = await adapter.generation("a\u0000b");
    await adapter.set("a", a, "b\u0000c", "one", 100);
    await adapter.set("a\u0000b", b, "c", "two", 100);
    expect(await adapter.getMany("a", a, ["b\u0000c"])).toEqual(["one"]);
    expect(await adapter.getMany("a\u0000b", b, ["c"])).toEqual(["two"]);
  });

  test("fences stale generations after invalidation", async () => {
    const adapter = createMemoryCacheAdapter();
    const old = await adapter.generation("scope");
    await adapter.set("scope", old, "key", "value", 100);
    await adapter.invalidate("scope");
    const current = await adapter.generation("scope");
    expect(current).not.toBe(old);
    expect(await adapter.getMany("scope", old, ["key"])).toEqual([null]);
    expect(await adapter.set("scope", old, "other", "stale", 100)).toBe(false);
    await adapter.delete("scope", old, "key");
    expect(await adapter.getMany("scope", current, ["key"])).toEqual([null]);
  });

  test("enforces entry LRU and refreshes entries on read", async () => {
    const adapter = createMemoryCacheAdapter({ maxEntries: 2 });
    const generation = await adapter.generation("scope");
    await adapter.set("scope", generation, "a", "A", 100);
    await adapter.set("scope", generation, "b", "B", 100);
    await adapter.getMany("scope", generation, ["a"]);
    await adapter.set("scope", generation, "c", "C", 100);
    expect(await adapter.getMany("scope", generation, ["a", "b", "c"])).toEqual(["A", null, "C"]);
  });

  test("enforces serialized byte capacity", async () => {
    const adapter = createMemoryCacheAdapter({ maxBytes: 70 });
    const generation = await adapter.generation("scope");
    await adapter.set("scope", generation, "a", "A", 100);
    await adapter.set("scope", generation, "b", "B", 100);
    expect(await adapter.getMany("scope", generation, ["a", "b"])).toEqual([null, "B"]);
  });

  test("bounds namespace generations and rejects an oversized entry", async () => {
    const adapter = createMemoryCacheAdapter({ maxBytes: 100, maxNamespaces: 1 });
    const first = await adapter.generation("first");
    await adapter.set("first", first, "k", "v", 100);
    const second = await adapter.generation("second");
    expect(await adapter.getMany("first", first, ["k"])).toEqual([null]);
    await expect(
      adapter.set("second", second, "large", "x".repeat(200), 100),
    ).rejects.toMatchObject({ code: "serialization" });
    expect(await adapter.getMany("second", second, ["large"])).toEqual([null]);
  });

  test("rejects invalid limits and TTLs with CacheError", async () => {
    for (const options of [{ maxEntries: 0 }, { maxBytes: Infinity }, { maxNamespaces: 1.5 }]) {
      expect(() => createMemoryCacheAdapter(options)).toThrow(CacheError);
    }
    const adapter = createMemoryCacheAdapter();
    const generation = await adapter.generation("scope");
    await expect(adapter.set("scope", generation, "key", "value", 0)).rejects.toThrow(CacheError);
  });

  test("rejects batches larger than the adapter contract limit", async () => {
    const adapter = createMemoryCacheAdapter();
    const generation = await adapter.generation("scope");
    await expect(
      adapter.getMany("scope", generation, Array(101).fill("key")),
    ).rejects.toMatchObject({ code: "invalid-input" });
  });
});
