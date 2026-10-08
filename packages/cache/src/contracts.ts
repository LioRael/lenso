export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };

export type CacheErrorCode =
  | "invalid-input"
  | "serialization"
  | "backend"
  | "aborted"
  | "busy"
  | "closed";

/** Messages deliberately exclude driver errors, keys, values and credentials. */
export class CacheError extends Error {
  constructor(readonly code: CacheErrorCode) {
    super(`Cache operation failed (${code})`);
    this.name = "CacheError";
  }
}

export interface CacheCapabilities {
  readonly provider: string;
  readonly sharing: "process" | "shared";
  readonly invalidation: "namespace-generation";
  readonly batch: "per-key";
}

export type RawCacheRead = string | null | CacheError;

/** Adapter methods receive opaque service-encoded namespaces and keys.
 * Generation tokens must never be reused. Reads and writes fence old generations.
 * Positive finite TTL is mandatory; implementations must not return expired data.
 * Borrowed adapters/drivers are not closed by the service.
 */
export interface CacheAdapter {
  readonly capabilities: CacheCapabilities;
  generation(namespace: string): Promise<string>;
  getMany(namespace: string, generation: string, keys: readonly string[]): Promise<RawCacheRead[]>;
  set(
    namespace: string,
    generation: string,
    key: string,
    value: string,
    ttlMs: number,
  ): Promise<boolean>;
  delete(namespace: string, generation: string, key: string): Promise<void>;
  invalidate(namespace: string): Promise<void>;
}

export interface CacheConfig {
  readonly namespace: string;
  readonly defaultTtlMs?: number;
  readonly maxTtlMs?: number;
  readonly maxValueBytes?: number;
  readonly maxInFlight?: number;
  readonly failureMode?: "fail-open" | "fail-closed";
}

export interface CacheEvent {
  readonly operation: "get" | "set" | "delete" | "invalidate";
  readonly reason: "backend" | "corrupt";
}

export interface CacheOptions<T extends JsonValue> extends CacheConfig {
  readonly adapter: CacheAdapter;
  /** Optional runtime type guard. JSON constraints apply even without this guard. */
  readonly validate?: (value: JsonValue) => value is T;
  readonly onEvent?: (event: CacheEvent) => void;
}

export type CacheLookup<T> =
  | { readonly status: "hit"; readonly value: T }
  | { readonly status: "miss"; readonly reason: "absent" | "expired" | "corrupt" | "backend" };
export type CacheBatchResult<T> =
  | CacheLookup<T>
  | { readonly status: "error"; readonly error: CacheError };
export type CacheWriteResult = {
  readonly outcome: "stored" | "skipped" | "bypassed" | "superseded";
};

export interface Cache<T extends JsonValue = JsonValue> {
  readonly capabilities: CacheCapabilities;
  readonly config: Readonly<Required<CacheConfig>>;
  get(key: string): Promise<CacheLookup<T>>;
  /** Ordered, duplicates preserved, at most 100 keys. No transactional batch guarantee. */
  getMany(keys: readonly string[]): Promise<CacheBatchResult<T>[]>;
  set(key: string, value: T, options?: { ttlMs?: number }): Promise<CacheWriteResult>;
  delete(key: string): Promise<{ outcome: "deleted-or-absent" | "bypassed" }>;
  /** Invalidates only this exact scope, not parents, children or other plugins. */
  invalidate(): Promise<{ outcome: "invalidated" | "bypassed" }>;
  scope(name: string): Cache<T>;
  scope<U extends JsonValue>(
    name: string,
    options: { validate: (value: JsonValue) => value is U },
  ): Cache<U>;
  getOrSet(
    key: string,
    loader: (signal: AbortSignal) => Promise<T>,
    options?: { ttlMs?: number; signal?: AbortSignal },
  ): Promise<T>;
  /** Closes the service family and aborts its loads, never the borrowed adapter. */
  close(): void;
}
