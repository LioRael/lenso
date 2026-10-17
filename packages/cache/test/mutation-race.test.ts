import { expect, test } from "bun:test";
import { createCache } from "../src/index";
import { createMemoryCacheAdapter } from "../src/memory";

function barrier() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

test("mutations drain dispatched load writes even after an earlier fence", async () => {
  for (const operation of ["set", "delete", "invalidate"] as const) {
    const adapter = createMemoryCacheAdapter();
    const entered = barrier();
    const resume = barrier();
    let first = true;
    const store = createCache({
      namespace: "race",
      adapter: {
        ...adapter,
        async set(...args) {
          if (first) {
            first = false;
            entered.resolve();
            await resume.promise;
          }
          return adapter.set(...args);
        },
      },
    });
    const load = store.getOrSet("k", async () => "old");
    await entered.promise;
    const earlierFence = store.delete("k");
    const mutation =
      operation === "set"
        ? store.set("k", "new")
        : operation === "delete"
          ? store.delete("k")
          : store.invalidate();
    // Let mutations reach the backend if they fail to wait for the old write.
    await Bun.sleep(0);
    resume.resolve();
    await Promise.all([load, earlierFence, mutation]);
    expect(await store.get("k")).toEqual(
      operation === "set" ? { status: "hit", value: "new" } : { status: "miss", reason: "absent" },
    );
  }
});

test("new set waits for a dispatched zero-TTL deletion", async () => {
  const adapter = createMemoryCacheAdapter();
  const entered = barrier();
  const resume = barrier();
  const store = createCache({
    namespace: "race",
    adapter: {
      ...adapter,
      async delete(...args) {
        entered.resolve();
        await resume.promise;
        return adapter.delete(...args);
      },
    },
  });
  const load = store.getOrSet("k", async () => "uncached", { ttlMs: 0 });
  await entered.promise;
  const replacement = store.set("k", "new");
  await Bun.sleep(0);
  resume.resolve();
  await Promise.all([load, replacement]);
  expect(await store.get("k")).toEqual({ status: "hit", value: "new" });
});

test("failed dispatched write does not prevent a later mutation", async () => {
  const adapter = createMemoryCacheAdapter();
  const entered = barrier();
  const resume = barrier();
  const store = createCache({
    namespace: "race",
    adapter: {
      ...adapter,
      async set() {
        entered.resolve();
        await resume.promise;
        throw new Error("backend failed");
      },
    },
  });
  const load = store.getOrSet("k", async () => "old").catch((error) => error);
  await entered.promise;
  const deletion = store.delete("k");
  resume.resolve();
  expect(await load).toMatchObject({ code: "backend" });
  expect(await deletion).toEqual({ outcome: "deleted-or-absent" });
});
