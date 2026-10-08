import { CacheError, type CacheAdapter, type CacheCapabilities } from "./contracts";
import { MAX_TTL_MS, validateTtl } from "./config";

export interface MemoryCacheAdapterOptions {
  maxEntries?: number;
  maxBytes?: number;
  maxNamespaces?: number;
  now?: () => number;
}

interface Entry {
  generation: string;
  value: string;
  expiresAt: number;
  bytes: number;
}

const DEFAULT_MAX_ENTRIES = 1_000;
const DEFAULT_MAX_BYTES = 8 * 1024 * 1024;
const DEFAULT_MAX_NAMESPACES = 128;
const MAX_BATCH_SIZE = 100;
const encoder = new TextEncoder();

function validateLimit(value: number | undefined, fallback: number): number {
  const limit = value ?? fallback;
  if (!Number.isSafeInteger(limit) || limit <= 0) throw new CacheError("invalid-input");
  return limit;
}

function freshGeneration(): string {
  return crypto.randomUUID();
}

export function createMemoryCacheAdapter(options: MemoryCacheAdapterOptions = {}): CacheAdapter {
  const maxEntries = validateLimit(options.maxEntries, DEFAULT_MAX_ENTRIES);
  const maxBytes = validateLimit(options.maxBytes, DEFAULT_MAX_BYTES);
  const maxNamespaces = validateLimit(options.maxNamespaces, DEFAULT_MAX_NAMESPACES);
  const now = options.now ?? Date.now;
  const namespaces = new Map<string, string>();
  const entries = new Map<string, Entry>();
  let bytes = 0;

  const capabilities: CacheCapabilities = Object.freeze({
    provider: "memory",
    sharing: "process",
    invalidation: "namespace-generation",
    batch: "per-key",
  });

  function composite(namespace: string, key: string): string {
    return JSON.stringify([namespace, key]);
  }

  function removeEntry(id: string): void {
    const entry = entries.get(id);
    if (!entry) return;
    entries.delete(id);
    bytes -= entry.bytes;
  }

  function evictNamespace(namespace: string): void {
    // The map is globally bounded by maxEntries, so this traversal is bounded.
    const prefix = `[${JSON.stringify(namespace)},`;
    for (const [id] of entries) {
      if (id.startsWith(prefix)) removeEntry(id);
    }
  }

  function touchNamespace(namespace: string): string {
    let generation = namespaces.get(namespace);
    if (generation !== undefined) {
      namespaces.delete(namespace);
      namespaces.set(namespace, generation);
      return generation;
    }
    if (namespaces.size >= maxNamespaces) {
      const oldest = namespaces.keys().next().value as string;
      namespaces.delete(oldest);
      evictNamespace(oldest);
    }
    generation = freshGeneration();
    namespaces.set(namespace, generation);
    return generation;
  }

  return {
    capabilities,

    async generation(namespace) {
      return touchNamespace(namespace);
    },

    async getMany(namespace, generation, keys) {
      if (keys.length > MAX_BATCH_SIZE) throw new CacheError("invalid-input");
      const current = namespaces.get(namespace);
      if (current !== generation) return keys.map(() => null);
      touchNamespace(namespace);
      return keys.map((key) => {
        const id = composite(namespace, key);
        const entry = entries.get(id);
        if (!entry || entry.generation !== generation) return null;
        if (entry.expiresAt <= now()) {
          removeEntry(id);
          return null;
        }
        entries.delete(id);
        entries.set(id, entry);
        return entry.value;
      });
    },

    async set(namespace, generation, key, value, ttlMs) {
      validateTtl(ttlMs, MAX_TTL_MS);
      if (ttlMs === 0) throw new CacheError("invalid-input");
      if (namespaces.get(namespace) !== generation) return false;
      touchNamespace(namespace);
      const id = composite(namespace, key);
      const size =
        encoder.encode(namespace).byteLength +
        encoder.encode(generation).byteLength +
        encoder.encode(key).byteLength +
        encoder.encode(value).byteLength;
      if (size > maxBytes) throw new CacheError("serialization");

      removeEntry(id);
      while (entries.size >= maxEntries || bytes + size > maxBytes) {
        const oldest = entries.keys().next().value as string | undefined;
        if (oldest === undefined) break;
        removeEntry(oldest);
      }
      entries.set(id, { generation, value, expiresAt: now() + ttlMs, bytes: size });
      bytes += size;
      return true;
    },

    async delete(namespace, generation, key) {
      if (namespaces.get(namespace) !== generation) return;
      touchNamespace(namespace);
      removeEntry(composite(namespace, key));
    },

    async invalidate(namespace) {
      if (namespaces.has(namespace)) evictNamespace(namespace);
      // Replacing the token immediately fences all outstanding users.
      namespaces.delete(namespace);
      touchNamespace(namespace);
    },
  };
}
