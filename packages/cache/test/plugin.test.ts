import { expect, test } from "bun:test";
import { valuesSource } from "@lenso/core/config";
import { defineApp, definePlugin, startApp } from "@lenso/core";
import { type CacheAdapter } from "../src/index";
import { createMemoryCacheAdapter } from "../src/memory";
import { cacheConfig, createCachePlugin } from "../src/plugin";

test("binds exact adapter instance, resolves shared Config and closes service only", async () => {
  const borrowed = createMemoryCacheAdapter();
  const adapter = definePlugin({ id: "adapter", setup: () => borrowed });
  const plugin = createCachePlugin<string>({
    id: "cache",
    adapter,
    config: [valuesSource({ namespace: "plugin", defaultTtlMs: 1000 })],
    validate: (value): value is string => typeof value === "string",
  });
  expect(plugin.requires?.[0]).toBe(adapter);
  const app = await startApp(defineApp({ plugins: [adapter, plugin] }));
  const service = app.get(plugin);
  try {
    expect(service.config.defaultTtlMs).toBe(1000);
    await service.set("key", "value");
  } finally {
    await app.stop();
  }
  await expect(service.get("key")).rejects.toMatchObject({ code: "closed" });
  const namespace = JSON.stringify(["plugin"]);
  const generation = await borrowed.generation(namespace);
  expect((await borrowed.getMany(namespace, generation, ["key"]))[0]).toBeString();
});

test("invalid config fails preflight before provider setup, without leaking values", async () => {
  let setups = 0;
  const adapter = definePlugin<CacheAdapter>({
    id: "adapter",
    setup() {
      setups++;
      return createMemoryCacheAdapter();
    },
  });
  const plugin = createCachePlugin({
    id: "cache",
    adapter,
    config: [valuesSource({ namespace: "private-name", defaultTtlMs: -1 })],
  });
  await expect(startApp(defineApp({ plugins: [adapter, plugin] }))).rejects.toThrow();
  expect(setups).toBe(0);
});

test("startup rollback cleans owned provider once and keeps borrowed adapter alive", async () => {
  const borrowed = createMemoryCacheAdapter();
  let cleanups = 0;
  const provider = definePlugin({
    id: "provider",
    setup(context) {
      context.onCleanup(() => {
        cleanups++;
      });
      return borrowed;
    },
  });
  const plugin = createCachePlugin({
    id: "cache",
    adapter: provider,
    config: { namespace: "rollback" },
  });
  const failure = definePlugin({
    id: "failure",
    requires: [plugin],
    setup() {
      throw new Error("setup failed");
    },
  });
  await expect(startApp(defineApp({ plugins: [provider, plugin, failure] }))).rejects.toThrow(
    "setup failed",
  );
  expect(cleanups).toBe(1);
  expect(await borrowed.generation("still-usable")).toBeString();
});

test("config contract has discoverable constraints but no secret defaults", () => {
  expect(cacheConfig.jsonSchema?.()).toMatchObject({
    type: "object",
    required: ["namespace"],
    properties: { defaultTtlMs: { minimum: 0, maximum: 86_400_000 } },
  });
  expect(JSON.stringify(cacheConfig.jsonSchema?.())).not.toContain('"default":');
});
