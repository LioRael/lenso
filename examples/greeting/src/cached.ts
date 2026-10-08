import { defineApp, definePlugin, startApp } from "@lenso/core";
import type { CacheAdapter } from "@lenso/cache";
import { createMemoryCacheAdapter } from "@lenso/cache/memory";
import { createCachePlugin } from "@lenso/cache/plugin";
import { createGreetingPlugin } from "./greeting";

export const cacheAdapter = definePlugin<CacheAdapter>({
  id: "greeting-cache-memory",
  setup: () => createMemoryCacheAdapter({ maxEntries: 100, maxNamespaces: 8 }),
});
export const messageCache = createCachePlugin<string>({
  id: "greeting-message-cache",
  adapter: cacheAdapter,
  config: { namespace: "greeting:messages", failureMode: "fail-open" },
  validate: (value): value is string => typeof value === "string",
});
export const cachedGreeting = createGreetingPlugin({ id: "greeting-cached", cache: messageCache });
export const cachedApp = defineApp({ plugins: [cacheAdapter, messageCache, cachedGreeting] });

if (import.meta.main) {
  const app = await startApp(cachedApp);
  try {
    const greeting = app.get(cachedGreeting);
    console.log(await greeting.greet({ name: "Bun" }));
    console.log(await greeting.greet({ name: "Bun" }));
  } finally {
    await app.stop();
  }
}
