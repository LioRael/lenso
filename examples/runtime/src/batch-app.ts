import { defineApp } from "@lenso/core";
import { definePlugin } from "@lenso/core/plugin";
import type { CacheAdapter, Cache } from "@lenso/cache";
import { createMemoryCacheAdapter } from "@lenso/cache/memory";
import { createCachePlugin } from "@lenso/cache/plugin";
import { createLogger, type Logger } from "@lenso/log";
import { createHash } from "node:crypto";
import { aggregate } from "./aggregate";
import type { AggregateInput, AggregateOutput } from "./generated/logs";

function cacheTtlMs(): number {
  const configured = process.env.LENSO_BATCH_CACHE_TTL_MS;
  if (configured === undefined) return 60_000;
  if (!/^\d+$/.test(configured))
    throw new Error("LENSO_BATCH_CACHE_TTL_MS must be an integer from 0 to 86400000");
  const ttl = Number(configured);
  if (!Number.isSafeInteger(ttl) || ttl > 86_400_000)
    throw new Error("LENSO_BATCH_CACHE_TTL_MS must be an integer from 0 to 86400000");
  return ttl;
}

const defaultTtlMs = cacheTtlMs();

export const batchCacheAdapter = definePlugin<CacheAdapter>({
  id: "batch-cache-memory",
  setup: () => createMemoryCacheAdapter({ maxEntries: 1000, maxNamespaces: 4 }),
});
export const batchCache = createCachePlugin<AggregateOutput>({
  id: "batch-aggregate-cache",
  adapter: batchCacheAdapter,
  config: { namespace: "batch:aggregate", defaultTtlMs, maxTtlMs: 86_400_000 },
  validate: (value): value is AggregateOutput =>
    !!value &&
    typeof value === "object" &&
    !Array.isArray(value) &&
    typeof value.count === "number" &&
    typeof value.error_count === "number" &&
    typeof value.total_duration_ms === "number",
});

export const batchLogger = definePlugin<Logger>({
  id: "batch-logger",
  setup(context) {
    return createLogger().child({ plugin: "batch", instance: context.instanceId });
  },
});

export const batch = definePlugin({
  id: "batch",
  requires: [batchCache, batchLogger],
  setup(context) {
    console.error(JSON.stringify({ event: "plugin_initialized", plugin: "batch", count: 1 }));
    const cache = context.get(batchCache) as Cache<AggregateOutput>;
    const logger = context.get(batchLogger);
    let closed = false;
    context.onCleanup(() => {
      closed = true;
    });
    return {
      async aggregate(input: AggregateInput, signal?: AbortSignal): Promise<AggregateOutput> {
        if (closed) throw new Error("Batch service is closed");
        if (!input.logs.length) return aggregate(input, signal);
        signal?.throwIfAborted();
        const started = performance.now();
        const serialized = JSON.stringify(input);
        const key = createHash("sha256").update(serialized).digest("hex");
        let loaded = false;
        const output = await cache.getOrSet(
          key,
          async (loaderSignal) => {
            loaded = true;
            loaderSignal.throwIfAborted();
            return aggregate(JSON.parse(serialized) as AggregateInput, loaderSignal);
          },
          { signal },
        );
        const business_ms = performance.now() - started;
        if (process.env.LENSO_PERF_TIMING === "1")
          console.error(JSON.stringify({ event: "aggregate_completed", business_ms }));
        logger.debug({ event: "batch_completed", loader_started: loaded });
        return output;
      },
    };
  },
});

export const batchPlugins = [batchCacheAdapter, batchCache, batchLogger, batch] as const;
export default defineApp({ plugins: batchPlugins });
