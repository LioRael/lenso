import type { Plugin } from "@lenso/core/plugin";
import { bindConfig, definePluginConfig, type ConfigSource } from "@lenso/core/config";
import {
  createCache,
  resolveCacheConfig,
  type Cache,
  type CacheAdapter,
  type CacheConfig,
  type JsonValue,
} from "./index";

const schema = {
  "~standard": {
    version: 1 as const,
    vendor: "lenso-cache",
    types: undefined as { input: CacheConfig; output: Readonly<Required<CacheConfig>> } | undefined,
    validate(input: unknown) {
      try {
        if (!input || typeof input !== "object" || Array.isArray(input))
          return { issues: [{ message: "Invalid cache configuration" }] };
        const allowed = new Set([
          "namespace",
          "defaultTtlMs",
          "maxTtlMs",
          "maxValueBytes",
          "maxInFlight",
          "failureMode",
        ]);
        if (Object.keys(input).some((key) => !allowed.has(key)))
          return { issues: [{ message: "Unknown cache configuration field" }] };
        return { value: resolveCacheConfig(input as CacheConfig) };
      } catch {
        return { issues: [{ message: "Invalid cache configuration" }] };
      }
    },
  },
};

export const cacheConfig = definePluginConfig({
  schema,
  description: "Finite-TTL JSON cache. No authorization decision caching or management exposure.",
  fields: [
    { path: ["namespace"], description: "Plugin-owned namespace, not an authorization boundary." },
    { path: ["defaultTtlMs"], description: "Default TTL in milliseconds. Zero disables storage." },
    {
      path: ["failureMode"],
      description: "Fail closed by default; fail open explicitly bypasses the backend.",
    },
  ],
  jsonSchema: () => ({
    type: "object",
    additionalProperties: false,
    required: ["namespace"],
    properties: {
      namespace: { type: "string", minLength: 1, maxLength: 256 },
      defaultTtlMs: { type: "integer", minimum: 0, maximum: 86_400_000 },
      maxTtlMs: { type: "integer", minimum: 1, maximum: 86_400_000 },
      maxValueBytes: { type: "integer", minimum: 1, maximum: 1_048_576 },
      maxInFlight: { type: "integer", minimum: 1, maximum: 10_000 },
      failureMode: { enum: ["fail-open", "fail-closed"] },
    },
  }),
});

export function createCachePlugin<T extends JsonValue = JsonValue>(options: {
  id: string;
  adapter: Plugin<CacheAdapter>;
  config: CacheConfig | readonly ConfigSource[];
  validate?: (value: JsonValue) => value is T;
}): Plugin<Cache<T>> {
  return bindConfig(cacheConfig, options.config, {
    id: options.id,
    requires: [options.adapter],
    setup(context, config) {
      const cache = createCache<T>({
        ...config,
        adapter: context.get(options.adapter),
        validate: options.validate,
        onEvent(event) {
          context.logger?.warn(
            { operation: event.operation, reason: event.reason },
            "Cache backend bypass or corrupt entry",
          );
        },
      });
      context.onCleanup(() => cache.close());
      return cache;
    },
  });
}
