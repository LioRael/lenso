# @lenso/limits

Request rate, periodic numeric quota, and weighted concurrency leases. These are
admission controls, not balances, money, billing, or an exactly-once system.

## Ordinary service

```ts
import { createLimits, createMemoryLimitStore } from "@lenso/limits";

const limits = createLimits({
  store: createMemoryLimitStore(),
  config: { failurePolicy: "throw" }, // required: "throw" | "deny" | "allow"
});

// Trusted application code derives tenant and subject after authentication.
const scope = { instance: "reports", tenant: tenantId, key: `run:${subjectId}` };
const rate = await limits.consumeRate({
  scope,
  capacity: 100,
  quantity: 1,
  periodMs: 60_000,
});
const quota = await limits.consumeQuota({
  scope,
  capacity: 1000,
  quantity: 5,
  periodMs: 86_400_000,
});
await limits.close(); // releases this service's leases, never closes its store
```

Rate and quota occupy separate namespaces even with identical scopes. Both use
**fixed UTC epoch-aligned windows**:
`windowStart = floor(now / periodMs) * periodMs`. This is intentionally not a
sliding-window or token-bucket limiter: traffic can burst on both sides of a
window boundary. Periods are fixed elapsed durations, not calendar months,
customer billing cycles, or a caller-supplied reset timestamp.

`capacity` and `quantity` must be integers in `1..2147483647`; `periodMs` and
`ttlMs` must be integers in `1..31622400000` (366 days). Fractional, zero,
negative, nonfinite and out-of-range values fail. A valid quantity larger than
capacity is denied without consuming anything.

All admission results include `allowed`, `remaining`, `retryAfter`, and `reason`.
`retryAfter` is **milliseconds**, not an HTTP header:

- allowed: `0`;
- exhausted counter: time to the next window;
- exhausted lease capacity: earliest expiry that could fit this quantity,
  assuming no renewals or earlier releases;
- quantity larger than capacity or unknown backend outcome: `null`.

For HTTP, the application can turn a finite value into Retry-After seconds with
`Math.ceil(retryAfter / 1000)`. It must not turn `null` into zero.

Policy is pinned per namespace/scope: changing counter capacity/period or lease
capacity returns `LimitError("policy-conflict")`. All workers must agree on
policies. Version a policy's application key deliberately when changing it;
doing so creates a new allowance, not a migration of already consumed quota.

## Modes and shared storage

`createMemoryLimitStore()` belongs to one store object in one process. Separate
objects, CLI invocations and processes have separate limits. There is no hidden
singleton, background timer or cluster coordination.

The optional `@lenso/limits/sqlite` entry accepts **native Bun SQLite Drizzle**.
Multiple processes on the **same host**, opening the same local database file,
share limits. The root entry does not import Drizzle, `bun:sqlite`, Auth, Web,
Manage or Tasks. Install `drizzle-orm@0.45.3` for a SQL adapter, not memory mode.

Apply `migrations/0001_sqlite.sql` through your explicit migration workflow
before starting consumers. Exported `limitSchema` can join the application's
Drizzle schema. Setup never creates tables or changes PRAGMAs.
The SQL file is also exported at
`@lenso/limits/migrations/0001_sqlite.sql`.

```ts
import { createBunSqlitePlugin } from "@lenso/db/bun-sqlite";
import { createLimitsPlugin } from "@lenso/limits";
import { createSqliteLimitStore, limitSchema } from "@lenso/limits/sqlite";

const db = createBunSqlitePlugin({
  id: "limit-db",
  filename: "./state/limits.sqlite",
  schema: limitSchema,
});
const limits = createLimitsPlugin({
  id: "limits",
  requires: [db],
  config: { failurePolicy: "deny" },
  connect: (context) => createSqliteLimitStore(context.get(db)),
});
// Install these exact objects. A consumer declares requires: [limits] and
// obtains context.get(limits); creating another object with the same ID is not DI.
```

The factory also accepts existing Config sources as `config: [source, ...]`;
`limitConfig` is its public Standard Schema/JSON Schema contract. The final
Config-bound plugin is the installed instance. Resource/plugin handles never
belong in serialized config. `scope.instance` names the logical application
policy namespace, **not** a per-process `context.instanceId`: giving each replica
a different scope intentionally isolates its allowance.

The application/DB owner should configure WAL and a suitable `busy_timeout`
on each connection through its existing DB setup. The adapter borrows the DB
and never closes it. SQLite lock contention remains a backend error after busy
handling, not an exhausted-limit result. The synchronous adapter blocks the
Bun event loop while waiting on locks; do not nest calls inside a deferred
application read transaction or long-running DB transaction.

Each mutation uses Drizzle's real `BEGIN IMMEDIATE` transaction: acquire the
SQLite writer lock, sample DB time, read/check/prune/update, then commit. The
callback is synchronous and never yields. Atomicity comes from that database
write transaction, not from JavaScript read/modify/write or an in-process lock.
The lease list is persisted as JSON per scope; this is intended for moderate
concurrency, not millions of concurrent leases. All writers must use the store
contract; direct table edits bypass its invariants.

Time is sampled once **after lock acquisition** from SQLite's UTC VFS clock.
Each bucket persists its highest observed time; backward jumps freeze progress
until the clock catches up rather than reopening consumed windows or reviving
expired leases. Forward jumps reset windows/expire leases sooner. Memory mode
uses `Date.now()` with the same per-bucket clamp; injected `now` is for tests.

SQLite remains local-file only: no cross-host/network-filesystem coordination.
PG and D1 use the separate adapters below, not casts of SQLite's database type.
No Redis dependency or universal database execution layer is introduced.

### PostgreSQL shared mode

`@lenso/limits/postgres` exports `createPostgresLimitStore` and
`postgresLimitSchema`, accepting native Drizzle `PgDatabase` types. The public
Bun-first resource pairs with it directly:

```ts
import { createBunSqlPlugin } from "@lenso/db/bun-sql";
import { createLimitsPlugin } from "@lenso/limits";
import { createPostgresLimitStore, postgresLimitSchema } from "@lenso/limits/postgres";

const db = createBunSqlPlugin({
  id: "limit-pg",
  connection: databaseUrl, // Trusted application configuration, not request JSON.
  schema: postgresLimitSchema,
});
const limits = createLimitsPlugin({
  id: "limits",
  requires: [db],
  config: { failurePolicy: "deny" },
  connect: (context) => createPostgresLimitStore(context.get(db)),
});
```

Apply `@lenso/limits/migrations/0001_postgres.sql` explicitly. Counters use
BIGINT and concurrency buckets use JSONB; all values remain within the common
JavaScript integer bounds. There is no startup migration or new pool owned by
the store.

Each operation runs in a database transaction. Bucket creation uses
`INSERT ... ON CONFLICT DO NOTHING`, then `SELECT ... FOR UPDATE` serializes
changes to the same namespace/scope. Only **after acquiring that row lock**, a
fresh statement samples `clock_timestamp()`; `now()`/transaction-start time
would wrongly include time spent waiting. Shared state rules then update the
locked row before commit, including expiry pruning and weighted admission.
Different scopes are not held behind a single application/global lock.

Processes or hosts connected to the **same authoritative PG database** with the
same namespace/policies share admission state. The application's DB owner
configures connection TLS, timeouts and availability; the store borrows the
existing resource and does not change them. Lock/statement timeouts and
serialization errors reach the explicit failure policy; the package does not
automatically retry unknown writes. Use the primary database, not a read
replica. JSON lease buckets target moderate concurrency.

Tests run real PostgreSQL on loopback with independent clients and owned
temporary clusters. They verify row-lock waits, post-lock expiry checks,
scope isolation, timeout faults and borrowed-resource cleanup. They do not
simulate a production failover or claim tested multi-host networking.

### D1 and Workers shared mode

`@lenso/limits/d1` exports `createD1LimitStore` and `d1LimitSchema`, accepting
native `DrizzleD1Database`. Install the exact DB and limits instances inside the
application's existing `@lenso/workers` request assembly:

```ts
import type { D1Database } from "@cloudflare/workers-types";
import { createD1Plugin } from "@lenso/db/d1";
import { createLimitsPlugin } from "@lenso/limits";
import { createD1LimitStore, d1LimitSchema } from "@lenso/limits/d1";

function limitsForRequest(bindings: { DB: D1Database }) {
  const db = createD1Plugin({ id: "limit-d1", binding: bindings.DB, schema: d1LimitSchema });
  const limits = createLimitsPlugin({
    id: "limits",
    requires: [db],
    config: { failurePolicy: "deny" },
    connect: (context) => createD1LimitStore(context.get(db)),
  });
  return { plugins: [db, limits], limits };
}
```

Apply `@lenso/limits/migrations/0001_d1.sql` with the existing D1 migration
workflow before serving requests. Its counter/bucket/lease tables are
D1-specific and are **not interchangeable** with the local SQLite migration.
The platform binding stays borrowed. The runtime graph imports no Bun database,
filesystem or PG driver, and no listener is started by the adapter.

D1 has no interactive transaction callback here. Each method sends a fixed
`db.batch` transaction: advance the bucket's persisted database time,
reset/prune as needed, perform guarded SQL admission, and return its snapshot.
Counter consumption is a conditional `UPDATE`; lease admission is
`INSERT ... SELECT` conditioned on the live quantity sum. JavaScript never
reads an allowance and writes back a proposed replacement. A zero-row
mutation means denial or policy mismatch, **not** a batch error, so every
mutating statement carries the relevant scope/policy predicate.

These batches start with a write and keep result reads in that same primary
transaction. Do not replace them with replica observations, separate
`SELECT`/mutation calls, or `db.transaction`. The adapter never uses a session
bookmark as a lock. Initial window calculation explicitly casts the bound
period to INTEGER because D1 binds JS numbers as REAL; otherwise integer
window alignment is lost.

Database time is sampled by the batch's first bucket mutation and reused via
`last_now` for the rest of the operation. Persistent high-water semantics match
the other modes. Leases use separate indexed rows so SQL can prune and count
them atomically; explaining weighted retry times reads the holders within the
same batch. Intended for moderate concurrency; D1 query/response limits and
network faults remain backend errors.

Tests execute local **workerd's native D1 binding**, not a hand-written SQLite
double: concurrent batches, whole-batch rollback, expiry/old tokens, failure
policies and real `createWorkerHandler` + `createD1Plugin` request assembly.
Requests can share the same D1 database independently of per-request app
instances. Cloudflare-hosted D1 network, replication/failover and geographic
behavior are **not validated by local workerd tests**.

## Leases and execution lifetime

```ts
await limits.withLease(
  { scope, capacity: 4, quantity: 1, ttlMs: 30_000 },
  async ({ signal, lease }) => {
    // Use the same application-owned Auth/Tasks service and honor signal.
    await doWork({ signal });
  },
  { signal: requestSignal, renewEveryMs: 10_000 },
);
```

Manual `acquire` returns `lease` with an unguessable UUID token, scope, weight and
epoch-millisecond `expiresAt`. Use `renew(lease, ttlMs)` and `release(lease)`.
Renewal never shortens a valid lease; expired/missing tokens return `null`.
Duplicate release is successful. Only the matching token is changed, so a stale
holder cannot renew or release a replacement. Tokens are capabilities, not
monotonic fencing numbers; do not publish them to untrusted clients or logs.
Raw `LimitStore.acquire(input, token)` is a trusted provider boundary, not the
application lease API: its caller must supply a fresh, never-reused token.
Applications use `createLimits(...).acquire` to get that behavior.

Manual leases have **no automatic renewal**. In `withLease`, renewal starts only
with an explicit `renewEveryMs` integer below TTL (and at most 2147483647 ms).
The wrapper owns its timer, prevents overlapping renewals, aborts on lost/failed
renewal, awaits in-flight renewal, and releases in `finally`. Body and release
errors both survive. Long stalls can still miss expiry.

Request cancellation signals the callback cooperatively and stops renewal;
`finally` releases after the callback settles. `close()` stops new admission,
aborts and drains owned wrappers and pending store calls, then releases manual
leases. A callback ignoring cancellation can delay close indefinitely: the
library does not pretend it has stopped execution. Do not await `limits.close()`
from inside its own running callback. The host must drain business work using
manual leases before closing their owning service/DB.

**TTL recovers valid lease weight, not necessarily running work.** An expired
task may still execute while a replacement runs. No strict execution bound,
fencing, revocation acknowledgement or exactly-once guarantee is claimed.
Stronger guarantees require the protected execution/effect endpoint to enforce
fencing or confirm cancellation itself.

## Identity, faults and optional exposure

These services are trusted internal APIs, not public authorization endpoints.
Obtain an actor using the application's existing Auth audience, call its
`access.enforce` policy, derive tenant/subject from verified identity/membership,
and only then construct scope and consume. Do not copy `tenant`, `instance`,
`key`, `capacity` or `quantity` from arbitrary request JSON. An application may
derive a business key from an authorized resource, but a client-selected key must
not partition away that subject's limit.

`failurePolicy` has no default:

| Policy  | Backend failure on consume/acquire                                                |
| ------- | --------------------------------------------------------------------------------- |
| `throw` | Reject with `LimitError("backend-failure")`, retaining the cause in-process       |
| `deny`  | Denied, `reason: "backend-failure"`, unknown remaining/retry                      |
| `allow` | Allowed, visibly degraded, unknown remaining/retry; acquire returns `lease: null` |

Choose `allow` only when executing without a confirmed quota/lease is acceptable.
It also applies to `withLease`, whose callback then receives `lease: null`.
An unknown write outcome may already have consumed quota or created a lease:
there is **no automatic retry, refund or fabricated token**. Unknown leases
recover via TTL. Renewal and release failures always reject regardless of policy;
there is no meaningful fail-open renewal or confirmation of an unknown release.
Shutdown retries still-owned releases after a wrapper failure and aggregates
remaining cleanup failures.

The plugin reuses the existing contextual logger for a fixed backend-failure
warning without scope, token or backend error text. It does not bootstrap Log,
OTel, workers or a provider. Existing application telemetry can wrap calls.

No Web, CLI, MCP or Manage operation is exposed automatically, and no Manage
adapter is shipped in this first version. If needed, application-owned adapters
should declare only an explicit Operation/Manage subset over an already
authorized business service, with trusted identity binding and tenant/object
policy; never expose the raw scope-taking service as an unauthenticated tool.
Do not treat a Manage declaration or CLI input as an identity or permission.

## Evidence and current limits

Focused tests cover memory boundaries, clock rollback, scope/namespace isolation,
quantity overflow, policy conflicts, stale/duplicate/expired tokens, wrapper
renewal/cancellation/shutdown, failure policies, real SQLite writer contention,
and existing public Config/Auth/DB lifecycle integration. A gated four-process
file-backed SQLite test proves shared admission rather than same-loop mocks.
Auth credentials in that test are local fixtures, not a production provider.
PostgreSQL tests use owned local clusters; D1/Workers tests use local Miniflare
and workerd with a disposable native D1 binding. No production connection,
Cloudflare credential or deployment is needed.

Bucket policy rows are retained, including empty buckets, to preserve policy
and clock history. Scope cardinality and DB maintenance are application-owned;
there is no built-in sweep/reset/admin permission. Deleting a bucket explicitly
resets its allowance/history and must not be exposed to untrusted callers.

Backend semantics checked against current official docs:
[Bun transactions](https://bun.sh/docs/api/sqlite#transactions),
[SQLite transactions](https://sqlite.org/lang_transaction.html),
[SQLite time](https://sqlite.org/lang_datefunc.html),
[PG row locks](https://www.postgresql.org/docs/current/explicit-locking.html#LOCKING-ROWS),
[PG time](https://www.postgresql.org/docs/current/functions-datetime.html#FUNCTIONS-DATETIME-CURRENT),
[D1 batch and sessions](https://developers.cloudflare.com/d1/worker-api/d1-database/).

Development checks: build `@lenso/core`, `@lenso/db`, `@lenso/auth` and
`@lenso/workers` before this package, then run its `build`, `typecheck` and `test`
scripts. Set `LENSO_REQUIRE_POSTGRES=1` to require the local PG binaries rather
than skip PG tests. Miniflare/workerd are test-only dependencies; no provider
credentials are read. The root Bun lockfile is intentionally left to the integration owner; register this new
workspace/dependencies there before relying on a fresh frozen install or the
full workspace pipeline.
