# @lenso/cache

Typed finite-TTL JSON caching. The ordinary service is independent of Lenso.
`/memory`, `/redis` and `/plugin` are separate entries; the root entry imports
neither Bun Redis nor Lenso, Auth, DB, Manage, Tasks, Log or OTel. Core is an
optional peer used only by `/plugin`. No SQL migration is needed.

## Ordinary service

```ts
import { createCache } from "@lenso/cache";
import { createMemoryCacheAdapter } from "@lenso/cache/memory";

const cache = createCache<string | null>({
  namespace: "catalog:labels",
  adapter: createMemoryCacheAdapter({ maxEntries: 1000 }),
  validate: (value): value is string | null => value === null || typeof value === "string",
});
await cache.set("label:42", null, { ttlMs: 30_000 });
const result = await cache.get("label:42");
// { status: "hit", value: null }, not a miss
const tenant = cache.scope("tenant:42");
const value = await tenant.getOrSet("public-label", async (signal) => {
  signal.throwIfAborted();
  return "Local fixture";
});
await tenant.delete("public-label");
await tenant.invalidate();
cache.close();
```

`get`, `set`, `delete`, `getMany`, `scope`, `invalidate`, `getOrSet` are async
business methods (except scope creation/close). `getMany` accepts at most 100
keys, preserves order and duplicates, and returns a per-key hit, miss or error.
It is not a transactional snapshot across adapters.

## TTL, isolation and serialization

- TTL units are **integer milliseconds**. Default: 60,000. All stored entries
  have a positive finite TTL; default maximum: 86,400,000 (24 hours), which
  applications may lower, not raise. Negative, fractional, non-finite and
  above-maximum TTLs are rejected. No immortal-entry mode.
- `set(..., {ttlMs: 0})` deletes the old entry and returns `skipped`, rather than
  storing forever. A zero default applies this rule too. `getOrSet` with zero
  ignores/removes an existing hit, loads fresh, and does not store its result.
  `get` still reads entries created with an explicit positive TTL.
- Expiry starts when the service serializes the entry before writing. Both
  adapter expiry and the versioned JSON envelope's `expiresAt <= Date.now()`
  reject expired reads. Redis physical expiry may be later because of transport
  latency. Hosts must have synchronized clocks; clock jumps/skew can shorten or
  extend effective lifetime. This is not a monotonic time guarantee.
- Namespaces, scope segments and keys are nonempty well-formed Unicode,
  <=256 UTF-8 bytes, without control characters. Paths are collision-safe and
  limited to 16 segments including the root. Use application-controlled scopes,
  not arbitrary request-driven namespace churn. `scope("a:b")` differs from
  `scope("a").scope("b")`.
- `invalidate()` affects **only the exact current scope**, not descendants,
  parents, another plugin, or the whole backend. There is no flush-all, wildcard
  invalidation or key listing. Plugins sharing an explicit namespace intentionally
  share values; a namespace is isolation by convention, not authorization.
- Values allow null, booleans, strings, finite numbers, dense ordinary arrays,
  and plain/null-prototype objects containing those types. JSON normalizes `-0`
  to `0`. Reject undefined (including object fields), BigInt, Date, Map/Set,
  class instances, symbols, functions, accessors, custom array prototypes, sparse
  arrays and cycles. Maximum nesting: 64. Values are copied by serialization.
- Default maximum is 65,536 UTF-8 bytes **including the JSON envelope**, tunable
  up to 1 MiB. Optional `validate` is a runtime type guard for writes and reads;
  a TypeScript generic alone does not verify data from another writer. Different
  scope types should provide their own guard and keys/schema versions.
- Malformed JSON, unsupported envelope versions, oversized entries or failed
  read guards become `{status:"miss", reason:"corrupt"}`. The entry is not
  returned or eagerly deleted (a concurrent writer could have replaced it);
  it expires normally or is replaced by a later load. Invalid writes throw a
  sanitized `CacheError("serialization")` in both failure modes.

## Adapters and actual guarantees

| Adapter   | Visibility                                                          | Capacity / invalidation                                                                                                                                                                                                                                                                                                       |
| --------- | ------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Memory    | Only users of the same adapter object in one process                | LRU, defaults 1000 entries, 8 MiB approximate serialized key/namespace/generation/value bytes, 128 namespace tokens. Expired reads remove entries. Namespace eviction removes its entries and uses a fresh token. Invalidation traverses at most the bounded entry map. No timers.                                            |
| Bun Redis | Instances connected to the same standalone Redis primary and prefix | Parameterized Lua fences namespace generations; data keys include the generation. Invalidation changes one control key, no scanning. Old-generation entries expire within their finite TTL. Control keys persist: bound the number of provisioned scopes operationally. Redis memory/eviction policy belongs to its operator. |

Memory byte accounting is not a JavaScript heap measurement. Names and entry
counts also have limits. Oversized single entries can exceed the adapter byte
budget independently of the service envelope limit and cause a write failure.
Backend limits should be aligned with `maxValueBytes`.

Redis uses Bun's built-in `RedisClient` and requires no third-party Redis
dependency. Its documented target is Bun with Redis >=7.2. Local validation used
Bun 1.4.2 / Redis 8.10.2, not every Redis version. No Redis Cluster, Sentinel,
replica-read, failover durability, global ordering or exactly-once claim. Across
instances, reads/writes fence completed namespace invalidation at the primary;
operations already in flight can still return their earlier results. Deleted or
evicted generation metadata is recreated with a new UUID, never a fallback
generation that could resurrect old values.

Individual getMany WRONGTYPE failures leave healthy keys intact. Whole-command
or generation-read failure marks every affected item failed. No atomic bulk
write API or automatic write retry is supplied; a lost connection may leave a
write's outcome unknown. Concurrent writes to a key are ordinary last backend
write wins. Cross-instance `delete` does not fence another instance's loader:
use namespace invalidation when those loads must not repopulate a scope.

## Failures, loading and lifetime

`failureMode` defaults to `fail-closed`: single reads/writes/delete/invalidate
throw sanitized `CacheError("backend")`; batches expose per-key errors.
`fail-open` explicitly turns backend reads into misses with reason `backend`,
and mutations into `bypassed`. `getOrSet` then loads from the authoritative
source without substituting a local cache. Write results are `stored`,
`skipped`, `superseded` (generation changed) or `bypassed`. Source loader errors
are business errors, propagate unchanged, and are never cached or logged here.

`getOrSet` merges loads by exact scope/key **within one service family**, not
across independently created services, processes or Redis clients. First caller
chooses loader/TTL; all other callers must use the same semantic loader. Each
waiter receives an independent JSON copy. One waiter's cancellation only rejects
that waiter. All cancelled waiters abort the shared loader signal, detach that
load from new callers and prevent its result being written. Loader cancellation
is cooperative: a loader ignoring the signal keeps consuming an in-flight slot
until it settles. Default maximum: 128 active loads across scopes; overflow
throws `busy`. Closing aborts loads and rejects subsequent operations across
the service family, never closes the adapter. Cancelled waiter records are
removed immediately; only one settlement subscription is retained per load.
The host remains responsible for bounding concurrent live requests/waiters.

Within the family, set/delete/exact invalidation detach and suppress earlier
load writes. Namespace generation checks suppress late load writes across
instances too. These are not distributed locks. An already dispatched backend
command cannot be recalled by cancellation/close; writes that raced an operation
may complete. A caller already loading may receive its earlier result after
invalidation. Do not use this cache as a financial ledger, idempotency record,
session store or permission source of truth.

`onEvent` emits only operation and `backend`/`corrupt` reason. Observer errors
do not alter results. No values, driver errors, credentials or keys are logged.
The optional plugin sends these fields to Lenso's existing logger; application
Log/OTel setup remains unchanged. Avoid sensitive span attributes in callers.

## Optional plugin and owned Redis connection

```ts
import { RedisClient } from "bun";
import { definePlugin, type Plugin } from "@lenso/core/plugin";
import { createRedisCacheAdapter } from "@lenso/cache/redis";
import { createCachePlugin } from "@lenso/cache/plugin";
import type { CacheAdapter } from "@lenso/cache";

const redisAdapter: Plugin<CacheAdapter> = definePlugin({
  id: "catalog-redis",
  async setup(context) {
    // Supply CACHE_REDIS_URL through trusted configuration, not committed code.
    const url = process.env.CACHE_REDIS_URL;
    if (!url) throw new Error("CACHE_REDIS_URL is required");
    const client = new RedisClient(url, {
      connectionTimeout: 1000,
      autoReconnect: false,
      enableOfflineQueue: false,
      maxRetries: 0,
    });
    context.onCleanup(() => client.close()); // register before connecting
    await client.connect();
    return createRedisCacheAdapter({ client, prefix: "catalog-cache" });
  },
});
const cachePlugin = createCachePlugin({
  id: "catalog-cache",
  adapter: redisAdapter, // exact installed instance
  config: { namespace: "catalog:public", failureMode: "fail-closed" },
});
// Install both instances. Consumers use requires: [cachePlugin] and context.get(cachePlugin).
```

The adapter borrows its client. If a DB/resource plugin owns a driver, keep
cleanup with that plugin, not the cache. No resource acquisition at module
top level. The cache plugin uses existing `definePluginConfig`/`bindConfig`;
`config` also accepts ordered Config sources (`valuesSource`, explicit
`envSource`, etc.) and resolves before any service setup. Exported `cacheConfig`
provides a Standard Schema contract and explicit JSON Schema converter.
Source failures are never made fail-open by cache policy.

No Manage, HTTP, CLI, MCP or Auth operation is automatically exposed. Management
is absent/disabled by default. If an application exposes delete/invalidate, it
must choose an explicit operation allowlist, obtain a trusted identity at the
entry, and authorize the exact scope in its ordinary service with existing Auth
and Manage. Business JSON is not identity. Do not expose raw values or an
arbitrary backend prefix/namespace selector to remote clients.

Permission decisions are not cached by any integration here. Authenticate and
authorize every request against existing Auth/fresh authority, even for cached
business projections; a TTL cannot guarantee timely revocation. Drizzle remains
the authoritative store; apply cache invalidation only after a successful DB
change, using existing Tasks if retry/outbox delivery is needed. No implicit
transaction coupling, task registration or exactly-once invalidation is added.

## Existing application and checks

`examples/greeting/src/cached.ts` is an opt-in assembly using a bounded memory
adapter. It caches only the pure formatted text, not input validation or the
per-call counter. Keys are SHA-256 digests of JSON-encoded names; names whose
JSON representation exceeds 16 KiB bypass this projection cache to stay within
its envelope budget. The default greeting
config and CLI operation remain unchanged.

```sh
bun run --cwd packages/lenso build
bun run --cwd packages/cache build
bun run --cwd packages/cache typecheck
bun run --cwd packages/cache test
bun examples/greeting/src/cached.ts
bun test examples/greeting/src/cached.test.ts
```

Redis tests launch only a disposable owned loopback server, persistence off,
using the real Bun driver. They skip explicitly if `redis-server` is unavailable.
Do not point tests at an existing/production Redis endpoint. Root/memory contain
no native imports, but Workers deployment/runtime compatibility is unverified;
Workers can select memory/custom adapters without installing or invoking Redis.
No additional Workers backend or cache platform is introduced.

The shared `bun.lock` includes this workspace and greeting dependency using the
integration owner's supplied revision. Landing validates it with
`bun install --frozen-lockfile`; no third-party dependency upgrade is required.

### Local validation record

- Bun 1.4.2, Redis 8.10.2. Cache build/typecheck passed; 40 cache tests passed,
  including seven tests using the real Redis driver and an owned disposable
  server. The cancellation retention fixture joins/cancels 20,000 callers while
  keeping one loader pending.
- Greeting build/typecheck passed; six existing/added application tests passed.
  The opt-in cached assembly ran with counts 1 then 2. Default CLI inspect and
  stdin call returned successful JSON.
- Scoped oxlint, oxfmt checks and `git diff --check` passed. Initial development
  used `--no-save` without changing the root lockfile; landing uses the supplied
  matching lockfile and frozen installation. Builds use existing scripts.
- Not run: full repository suite, release verification, deployed Workers,
  Redis Cluster/Sentinel/failover or other Redis/server versions.
