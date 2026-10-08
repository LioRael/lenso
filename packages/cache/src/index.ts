import {
  CacheError,
  type Cache,
  type CacheBatchResult,
  type CacheEvent,
  type CacheLookup,
  type CacheOptions,
  type JsonValue,
} from "./contracts";
import { resolveCacheConfig, validateName, validateTtl } from "./config";
import { decode, encode } from "./json";

export * from "./contracts";
export { resolveCacheConfig } from "./config";

interface Waiter {
  finish(failed: boolean, value: unknown): void;
}

interface Flight {
  readonly controller: AbortController;
  readonly waiters: Set<Waiter>;
  writable: boolean;
}

export function createCache<T extends JsonValue = JsonValue>(options: CacheOptions<T>): Cache<T> {
  const config = resolveCacheConfig(options);
  const flights = new Map<string, Flight>();
  const active = new Set<Flight>();
  let closed = false;
  const adapter = options.adapter;

  function ready() {
    if (closed) throw new CacheError("closed");
  }
  function event(operation: CacheEvent["operation"], reason: CacheEvent["reason"]) {
    // Telemetry must not change cache outcomes or disclose raw driver errors.
    try {
      options.onEvent?.({ operation, reason });
    } catch {}
  }
  function backend(operation: CacheEvent["operation"]) {
    event(operation, "backend");
    if (config.failureMode === "fail-closed") throw new CacheError("backend");
  }
  function fence(namespace: string, key?: string) {
    const prefix = `${namespace}\n`;
    for (const [id, flight] of flights) {
      if (key === undefined ? id.startsWith(prefix) : id === `${prefix}${key}`) {
        flight.writable = false;
        flights.delete(id);
      }
    }
  }

  function scoped<U extends JsonValue>(
    path: readonly string[],
    validate?: (value: JsonValue) => value is U,
  ): Cache<U> {
    const namespace = JSON.stringify(path);
    async function read(keys: readonly string[]): Promise<{
      generation?: string;
      results: CacheBatchResult<U>[];
    }> {
      let generation: string;
      let raw;
      try {
        generation = await adapter.generation(namespace);
        raw = await adapter.getMany(namespace, generation, keys);
        if (raw.length !== keys.length) throw new CacheError("backend");
      } catch {
        event("get", "backend");
        return {
          results: keys.map(() =>
            config.failureMode === "fail-open"
              ? { status: "miss", reason: "backend" }
              : { status: "error", error: new CacheError("backend") },
          ),
        };
      }
      return {
        generation,
        results: raw.map((item): CacheBatchResult<U> => {
          if (item instanceof CacheError) {
            event("get", "backend");
            return config.failureMode === "fail-open"
              ? { status: "miss", reason: "backend" }
              : { status: "error", error: new CacheError("backend") };
          }
          if (item === null) return { status: "miss", reason: "absent" };
          try {
            const entry = decode(item, config.maxValueBytes, validate);
            if (entry.expiresAt <= Date.now()) return { status: "miss", reason: "expired" };
            return { status: "hit", value: entry.value };
          } catch {
            event("get", "corrupt");
            // Do not delete here: a concurrent writer may have replaced the corrupt entry.
            return { status: "miss", reason: "corrupt" };
          }
        }),
      };
    }
    function unwrap(result: CacheBatchResult<U>): CacheLookup<U> {
      if (result.status === "error") throw result.error;
      return result;
    }
    function ttl(value?: number): number {
      const result = value === undefined ? config.defaultTtlMs : value;
      validateTtl(result, config.maxTtlMs);
      return result;
    }
    async function write(key: string, text: string, duration: number, generation?: string) {
      if (generation === undefined) return { outcome: "bypassed" as const };
      try {
        const stored = await adapter.set(namespace, generation, key, text, duration);
        return { outcome: stored ? ("stored" as const) : ("superseded" as const) };
      } catch {
        backend("set");
        return { outcome: "bypassed" as const };
      }
    }
    const service: Cache<U> = {
      capabilities: adapter.capabilities,
      config,
      async get(key) {
        ready();
        validateName(key);
        return unwrap((await read([key])).results[0]!);
      },
      async getMany(keys) {
        ready();
        if (!Array.isArray(keys) || keys.length > 100) throw new CacheError("invalid-input");
        const copied = [...keys];
        copied.forEach(validateName);
        if (!copied.length) return [];
        return (await read(copied)).results;
      },
      async set(key, value, input = {}) {
        ready();
        validateName(key);
        const duration = ttl(input.ttlMs);
        const text = encode(value, Date.now() + duration, config.maxValueBytes, validate);
        fence(namespace, key);
        if (duration === 0) {
          const result = await service.delete(key);
          return { outcome: result.outcome === "bypassed" ? "bypassed" : "skipped" };
        }
        let generation;
        try {
          generation = await adapter.generation(namespace);
        } catch {
          backend("set");
          return { outcome: "bypassed" };
        }
        return write(key, text, duration, generation);
      },
      async delete(key) {
        ready();
        validateName(key);
        fence(namespace, key);
        try {
          const generation = await adapter.generation(namespace);
          await adapter.delete(namespace, generation, key);
          return { outcome: "deleted-or-absent" };
        } catch {
          backend("delete");
          return { outcome: "bypassed" };
        }
      },
      async invalidate() {
        ready();
        fence(namespace);
        try {
          await adapter.invalidate(namespace);
          return { outcome: "invalidated" };
        } catch {
          backend("invalidate");
          return { outcome: "bypassed" };
        }
      },
      scope<V extends JsonValue = U>(
        name: string,
        input?: { validate?: (value: JsonValue) => value is V },
      ) {
        ready();
        validateName(name);
        if (path.length >= 16) throw new CacheError("invalid-input");
        return scoped<V>(
          [...path, name],
          input?.validate ?? (validate as ((value: JsonValue) => value is V) | undefined),
        );
      },
      async getOrSet(key, loader, input = {}) {
        ready();
        validateName(key);
        const duration = ttl(input.ttlMs);
        if (input.signal?.aborted) throw new CacheError("aborted");
        const id = `${namespace}\n${key}`;
        let flight = flights.get(id);
        if (!flight) {
          if (active.size >= config.maxInFlight) throw new CacheError("busy");
          flight = {
            controller: new AbortController(),
            waiters: new Set(),
            writable: true,
          };
          const current = flight;
          active.add(current);
          flights.set(id, current);
          const work = (async () => {
            const snapshot = await read([key]);
            const lookup = unwrap(snapshot.results[0]!);
            if (duration > 0 && lookup.status === "hit")
              return encode(lookup.value, Date.now() + duration, config.maxValueBytes, validate);
            if (
              duration === 0 &&
              snapshot.generation !== undefined &&
              current.writable &&
              !closed &&
              !current.controller.signal.aborted
            ) {
              try {
                await adapter.delete(namespace, snapshot.generation, key);
              } catch {
                backend("delete");
              }
            }
            if (current.controller.signal.aborted) throw new CacheError("aborted");
            const value = await loader(current.controller.signal);
            if (current.controller.signal.aborted) throw new CacheError("aborted");
            const text = encode(value, Date.now() + duration, config.maxValueBytes, validate);
            if (duration > 0 && current.writable && !closed)
              await write(key, text, duration, snapshot.generation);
            return text;
          })();
          const settle = (failed: boolean, value: unknown) => {
            active.delete(current);
            if (flights.get(id) === current) flights.delete(id);
            for (const waiter of current.waiters) waiter.finish(failed, value);
          };
          // One subscription per load; cancelled waiters are removed, not retained by .then.
          void work.then(
            (value) => settle(false, value),
            (error) => settle(true, error),
          );
        }
        const current = flight;
        const text = await new Promise<string>((resolve, reject) => {
          let settled = false;
          const finish = (failed: boolean, value: unknown) => {
            if (settled) return;
            settled = true;
            input.signal?.removeEventListener("abort", abort);
            current.controller.signal.removeEventListener("abort", abort);
            current.waiters.delete(waiter);
            if (failed) reject(value);
            else resolve(value as string);
          };
          const waiter: Waiter = { finish };
          const abort = () => {
            finish(true, new CacheError("aborted"));
            if (current.waiters.size === 0) {
              current.writable = false;
              if (flights.get(id) === current) flights.delete(id);
              current.controller.abort();
            }
          };
          current.waiters.add(waiter);
          input.signal?.addEventListener("abort", abort, { once: true });
          current.controller.signal.addEventListener("abort", abort, { once: true });
          if (input.signal?.aborted || current.controller.signal.aborted) abort();
        });
        return decode(text, config.maxValueBytes, validate).value;
      },
      close() {
        if (closed) return;
        closed = true;
        flights.clear();
        for (const flight of active) {
          flight.writable = false;
          flight.controller.abort();
        }
      },
    };
    return service;
  }
  return scoped<T>([config.namespace], options.validate);
}
