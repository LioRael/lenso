import type { RedisClient } from "bun";
import { CacheError, type CacheAdapter, type RawCacheRead } from "./contracts";
import { MAX_TTL_MS, validateTtl } from "./config";

const DEFAULT_PREFIX = "lenso-cache";
const MAX_BATCH_SIZE = 100;
const MAX_PREFIX_LENGTH = 256;
const SAFE_PREFIX = /^[A-Za-z0-9:_-]+$/;

const GENERATION_SCRIPT = `
local generation = redis.call("GET", KEYS[1])
if not generation then
  redis.call("SET", KEYS[1], ARGV[1], "NX")
  generation = redis.call("GET", KEYS[1])
end
return generation
`;

const READ_SCRIPT = `
local generation = redis.call("GET", KEYS[1])
if generation ~= ARGV[1] then
  return cjson.encode({false})
end
local result = {true}
for i = 2, #KEYS do
  local value = redis.pcall("GET", KEYS[i])
  if type(value) == "table" and value.err then
    result[#result + 1] = {error = true}
  else
    result[#result + 1] = value or cjson.null
  end
end
return cjson.encode(result)
`;

const SET_SCRIPT = `
if redis.call("GET", KEYS[1]) ~= ARGV[1] then return 0 end
redis.call("SET", KEYS[2], ARGV[2], "PX", ARGV[3])
return 1
`;

const DELETE_SCRIPT = `
if redis.call("GET", KEYS[1]) ~= ARGV[1] then return 0 end
redis.call("DEL", KEYS[2])
return 1
`;

const INVALIDATE_SCRIPT = `
redis.call("SET", KEYS[1], ARGV[1])
return 1
`;

export interface RedisCacheAdapterOptions {
  readonly client: RedisClient;
  readonly prefix?: string;
}

function encode(part: string): string {
  return Buffer.from(part, "utf8").toString("hex");
}

function scopeKey(prefix: string, namespace: string): string {
  const encoded = encode(namespace);
  return `${prefix}:scope:${encoded.length}:${encoded}`;
}

function dataKey(prefix: string, namespace: string, generation: string, key: string): string {
  const scope = encode(namespace);
  const epoch = encode(generation);
  const item = encode(key);
  return `${prefix}:data:${scope.length}:${scope}:${epoch.length}:${epoch}:${item.length}:${item}`;
}

function uuid(): string {
  return crypto.randomUUID();
}

function backendError(): CacheError {
  return new CacheError("backend");
}

export function createRedisCacheAdapter({
  client,
  prefix = DEFAULT_PREFIX,
}: RedisCacheAdapterOptions): CacheAdapter {
  if (
    typeof prefix !== "string" ||
    prefix.length === 0 ||
    prefix.length > MAX_PREFIX_LENGTH ||
    !SAFE_PREFIX.test(prefix)
  ) {
    throw new CacheError("invalid-input");
  }

  async function evalScript(script: string, keys: string[], args: string[]): Promise<unknown> {
    try {
      return await client.send("EVAL", [script, String(keys.length), ...keys, ...args]);
    } catch {
      throw backendError();
    }
  }

  return {
    capabilities: {
      provider: "redis",
      sharing: "shared",
      invalidation: "namespace-generation",
      batch: "per-key",
    },

    async generation(namespace) {
      const value = await evalScript(GENERATION_SCRIPT, [scopeKey(prefix, namespace)], [uuid()]);
      if (typeof value !== "string") throw backendError();
      return value;
    },

    async getMany(namespace, generation, keys): Promise<RawCacheRead[]> {
      if (keys.length > MAX_BATCH_SIZE) throw new CacheError("invalid-input");
      if (keys.length === 0) return [];
      const result = await evalScript(
        READ_SCRIPT,
        [
          scopeKey(prefix, namespace),
          ...keys.map((key) => dataKey(prefix, namespace, generation, key)),
        ],
        [generation],
      );
      if (typeof result !== "string") throw backendError();
      let values: unknown;
      try {
        values = JSON.parse(result);
      } catch {
        throw backendError();
      }
      if (!Array.isArray(values)) throw backendError();
      if (values[0] !== true) {
        return keys.map(() => null);
      }
      if (values.length !== keys.length + 1) throw backendError();
      return values.slice(1).map((value) => {
        if (value === null) return null;
        if (
          typeof value === "object" &&
          value !== null &&
          "error" in value &&
          value.error === true
        ) {
          return backendError();
        }
        if (typeof value === "string") return value;
        throw backendError();
      });
    },

    async set(namespace, generation, key, value, ttlMs) {
      validateTtl(ttlMs, MAX_TTL_MS);
      if (ttlMs === 0) throw new CacheError("invalid-input");
      const result = await evalScript(
        SET_SCRIPT,
        [scopeKey(prefix, namespace), dataKey(prefix, namespace, generation, key)],
        [generation, value, String(ttlMs)],
      );
      if (result === 1 || result === "1") return true;
      if (result === 0 || result === "0") return false;
      throw backendError();
    },

    async delete(namespace, generation, key) {
      await evalScript(
        DELETE_SCRIPT,
        [scopeKey(prefix, namespace), dataKey(prefix, namespace, generation, key)],
        [generation],
      );
    },

    async invalidate(namespace) {
      await evalScript(INVALIDATE_SCRIPT, [scopeKey(prefix, namespace)], [uuid()]);
    },
  };
}
