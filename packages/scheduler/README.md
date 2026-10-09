# Persistent Scheduler

`@lenso/scheduler` decides **when to enqueue**. The existing `@lenso/tasks`
queue remains the only worker, retry budget and task-state authority. No worker,
timer, HTTP endpoint, CLI operation or Manage surface starts on import/setup.

Supported combinations: **Bun SQL + Drizzle PostgreSQL schedules with PostgreSQL
Tasks**, or **Drizzle D1 schedules with D1 Tasks on Workers**. Cross-kind
combinations are rejected. The schedule database and queue database may be separate:
the handoff uses a persistent outbox and Tasks deduplication, not a cross-database
transaction. Root runtime imports do not load Auth, Tasks, Drizzle, PostgreSQL
drivers, Manage or an OTel SDK. Integrations have separate entrypoints; their
peer dependencies must be installed when used.

## Optional Manage companion

`createSchedulerManage({ id, scheduler, tasks })` from `@lenso/scheduler/manage`
borrows the exact scheduler plugin and a host-selected registered-task catalog.
Install its returned plugin and explicitly select its Manage declarations.
The trusted binding supplies `{ actor, signal }`; input JSON cannot supply an
actor. Scheduler remains responsible for current business authorization.

The companion exposes create/list/get, revision-checked pause/resume/cancel,
idempotency-keyed trigger, and bounded occurrence reads. Reads omit task inputs
and job results. Creating a schedule validates the registered task schema.
Its read-only `catalog({})` returns input schemas only for the supplied task
whitelist. Supply `authorizeCatalog(actor, signal)` to authorize each disclosure;
without it the operation returns an empty task list. Schemas are converted from
each task's Standard Schema JSON Schema capability, sanitized to omit defaults
and examples, and bounded to 64 KiB. Tasks without a usable converter are
reported as `runtime-validation-only` with a null schema.
It starts neither a worker nor a tick driver, and cancelling a future schedule
does not retract jobs already handed to Tasks.

## Connect existing resources

For PostgreSQL, apply `migrations/0001_scheduler.sql` then
`migrations/0002_queue_binding.sql` through the application's explicit,
authorized migration workflow before setup. The tables use the default `public`
schema/search path. The exported `schedulerSchema` from `/postgres` mirrors the
SQL and can join an application's Drizzle schema. Startup checks the table
shape but never performs DDL. Provision the Tasks queue separately with its
existing migration API.

```ts
import type { ActorOf } from "@lenso/auth";
import { createScheduler } from "@lenso/scheduler";
import { createPostgresScheduleStore } from "@lenso/scheduler/postgres";

// Existing app-owned Drizzle BunSQLDatabase, Tasks queue and exact Task object.
const scheduler = createScheduler({
  store: await createPostgresScheduleStore(database),
  queue,
  tasks: [generateReport],
  scope: { namespace: "reports-v1", tenantId: trustedTenantId },
  async authorize(actor: ActorOf<typeof access>, action, scope, resource) {
    // App-owned Auth Access.enforce verifies the minted Actor, audience and
    // current credentials, then applies tenant/object/action/task policy.
    await access.enforce(actor, { action, scope, resource }, schedulePolicy);
    return true;
  },
  authorizeExecution: (initiator, scope, occurrence) =>
    permissions.mayEnqueue(initiator, scope.tenantId, occurrence.task, occurrence.input),
});

// Actor comes from the trusted entry's Auth access.required(evidence), not JSON.
const plan = await scheduler.create(
  {
    task: generateReport.name,
    input: { reportId: "monthly-42" },
    rule: { kind: "cron", expression: "0 9 1 * *", timezone: "Asia/Shanghai" },
    misfire: "coalesce",
    graceMs: 5000,
  },
  actor,
);

await scheduler.tick(); // finite, awaited host invocation
const status = await scheduler.get(plan.id, actor); // includes nextAt/revision
await scheduler.pause(plan.id, status.revision, actor);
```

The identifiers and policies above are application-owned, not defaults shipped
by this package. A branded Actor type alone is not proof: the authorization
adapter must use the existing Auth `Access.enforce` boundary. `authorizeExecution`
must check the durable subject reference against current application permission
and tenant/object ownership. A stored subject reference is not an authenticated
session or an authorization grant.

Payloads are ordinary business JSON, validated with the exact registered task's
Standard Schema before persistence and again by Tasks before execution. Raw input
is stored so schema transforms do not compound. Limits match Tasks: 64 KiB,
depth 32, finite plain JSON. No credentials, actors or resource handles belong
in payloads. Handlers still authorize their durable business objects at execution;
dispatch authorization cannot prevent a later permission change. Subject refs
stay in schedule/occurrence metadata, not `TaskContext` or the task payload.

Each `(namespace, tenantId)` is persistently bound to the **same durable Tasks
queue identity** on first use; a changed backing queue fails with `queue-mismatch`
before planning or dispatch. When upgrading existing unbound schedules, first
use must point to their original queue. A Lenso plugin ID or a process's random instance ID is
not that durable namespace. All SQL queries and occurrence/dedup IDs include the
scope; a scope is chosen by trusted configuration, never by an untrusted request.
Changing the backing queue under an existing scope is unsupported.

## Operations and configuration

- `create(definition, actor)`, `update(id, revision, definition, actor)`.
- `pause`, `resume`, `cancel` take `id`, expected `revision`, and Actor.
- `get`, `list`, `occurrences` require read authorization. Payload and lease
  tokens are omitted from results. Lists are bounded to 100 by default, at most
  1000; occurrences return newest first. No full history pagination/archive yet.
- `trigger(id, requestKey, actor)` reserves a manual occurrence, including on a
  paused/completed schedule. It does not execute synchronously. Reusing a key for
  that schedule returns the original immutable occurrence, even after an update.
  New intentional work requires a new key.
- `tick()` is host-only, not an untrusted operation. It performs bounded planning
  and outbox dispatch, never a task handler.

Revision advances on **every schedule write**, including automatic cursor
advancement. A stale update/pause/cancel fails with `conflict`; reread and
reconsider, rather than blindly retrying the write. Updating restarts the future
cron cursor strictly after the update clock; a one-time rule retains its explicit
absolute instant. Updating a completed plan reactivates it; a cancelled plan
cannot be reactivated. Resuming a paused plan retains the cursor and applies its
misfire policy on the next tick.

**Commit is the dispatch boundary:** once an occurrence is inserted in the
outbox, its task/input/initiator are immutable. Update, pause and cancel affect
only future reservations, not even a committed occurrence still awaiting enqueue.
Cancellation does not call Tasks cancellation and cannot stop running effects.
Use separately authorized Tasks cancellation/retry when appropriate.

Timers retain the creating subject reference. Manual triggers retain the
triggering subject reference. Trigger permission and current task-enqueue
permission are independent, enforced by `authorize` and `authorizeExecution`.
An updater does not silently replace the plan's durable initiator.

Factory options are the startup configuration contract:

| Option                 | Default    | Bound                         |
| ---------------------- | ---------- | ----------------------------- |
| `maxSchedulesPerTick`  | 100        | integer 1–1000                |
| `maxDispatchesPerTick` | 100        | integer 1–1000                |
| `dispatchLeaseMs`      | 30000      | integer 1–2147483647          |
| `clock`                | `Date.now` | nonnegative Unix milliseconds |

Set the dispatch lease above expected permission-check and enqueue latency.
Clock skew across hosts can cause early reclaim; deduplication still uses the
same occurrence key, but scheduling accuracy requires reasonably aligned clocks.

## Timezone, DST and misfires

One-time rules use an **absolute Unix millisecond instant**, never an ambiguous
local date string. Cron rules require an explicit IANA timezone and use pinned
`cron-parser 5.10.1`; five/six fields, aliases and mature extended syntax are
parsed by that library. DOM/DOW combinations follow its non-strict OR semantics.
Randomized `H` expressions are rejected to keep occurrence selection stable.

DST follows the parser's native **instant/cursor semantics**, not a custom
local-time parser. Tests pin these New York examples:

- Missing `02:30` on 2024-03-10 shifts to `03:30`, then returns to `02:30`.
- Forward traversal of repeated `01:30` on 2024-11-03 selects the earlier instant.
  A cursor initialized/rebased inside the later fold can select the later
  `01:30`. Rebase after a delayed tick can therefore include that later instant.

Deduplication is by UTC instant and schedule revision, **not by local wall-clock
text**. Do not use these rules for a product requiring exactly one effect per
local calendar day without an additional business idempotency constraint.
Other timezones/historical transitions are not exhaustively verified.

An occurrence is late only when `now - nextAt > graceMs`:

- `skip`: drop all missed occurrences; move the cursor strictly after now.
- `coalesce`: reserve **one** catch-up, labeled with the earliest missed instant,
  covering all misses through this tick, then move strictly after now.

There is no unbounded cron walk or unlimited catch-up. One tick reserves at most
one occurrence per selected schedule, with total planning/dispatch limits above.
A late one-time plan is either skipped or gets one catch-up.

## Crash recovery and honest status

Schedule advancement checks active state, revision and cursor. PostgreSQL inserts
the occurrence in the **same Drizzle transaction**; D1 uses an atomic batch whose
insert is SQL-gated on a unique write token from the winning conditional update.
A zero-row D1 CAS cannot insert an occurrence. PostgreSQL claims use
`SKIP LOCKED`; D1 claims use a single conditional `UPDATE … RETURNING`.
Both use expiring tokens and fenced settlement. Authorization is followed by a
live-token CAS before beginning new enqueue I/O.

Tasks receives `scheduler:<stable occurrence hash>` as its persistent dedup key.
After a process dies before enqueue, another tick reclaims and enqueues. After
enqueue but before acknowledgement, **read-only Tasks lookup** restores the
original job association without enqueueing again, even after execution
permission is revoked or the PostgreSQL job has been pruned. Transient dispatch failure stays pending and retries
after the lease; permanent `job-expired`/invalid dispatch and denied permission
become blocked. Keys/tombstones are never erased or replaced to force success.

`completed` means a plan has no future automatic instant, **not task success**.
`enqueued` means confirmed queue acceptance, not handler completion.
`occurrences()` joins the existing Tasks job status, including final failures.
`acceptance: "unknown"` with a null job ID means acceptance is unresolved, not
that no job ran. A known job pruned by Tasks also returns null status, not success.

Lease expiry can overlap enqueue already in flight. Lookup runs before renewed
enqueue authorization; an already accepted job is linked, never re-executed by
Scheduler. If lookup misses and authorization is revoked, no new enqueue starts.
A later in-flight acceptance is also resolved by authorized occurrence queries,
including when the stored dispatch status is blocked. Thus dispatch status and
actual task outcome remain distinct. A miss/unavailable lookup cannot prove
that no in-flight job will be accepted; that result remains unknown. Blocked
occurrences do not automatically replay when permissions return.

Keep PostgreSQL Tasks retention long enough to inspect actual outcomes. If a job
has been pruned, its persistent tombstone still resolves the job ID, but returns
null status; execution history cannot be reconstructed. D1 currently retains
job/dedup rows indefinitely. This is recoverable at-least-once
handoff within those boundaries, not exactly-once external effects or a claim of
strict consistency across separate stores.

## Optional Lenso and native driver

`createSchedulerPlugin` from `/plugin` takes exact `database` and `queue` Plugin
objects and a `connect(database, context)` store factory. It borrows both resources
and never closes them. The ordinary service does not need Lenso startup.

`startSchedulerDriver` from `/driver` starts only when explicitly called:

```ts
const driver = startSchedulerDriver(scheduler, { intervalMs: 1000 });
// In plugin setup, register this immediately: context.onCleanup(() => driver.stop()).
// The host must observe driver.done for failure; it owns shutdown/error reporting.
await driver.stop(); // stops new ticks and drains the current tick
```

Do not add this perpetual native loop to Workers. Use the D1 integration below
and await a finite tick plus Tasks `runBatch` from the platform handler.

Manage is **off by default**; no built-in surface is declared. Applications may
explicitly select thin contextual Operations over this authorized service using
the existing Manage API and shared application schemas. Never expose raw `tick`
or raw queue administration. HTTP/CLI/MCP must supply trusted Auth evidence;
business JSON cannot supply an Actor, tenant authority or a permission flag.

## Checks and integration-owner follow-ups

```sh
bun run --filter @lenso/core build
bun run --filter @lenso/auth --filter @lenso/tasks build
bun run --filter @lenso/scheduler build
bun run --filter @lenso/scheduler typecheck
bun test packages/scheduler/test
bun run --filter @lenso/scheduler test:d1
# Only against a disposable, explicitly authorized test database:
SCHEDULER_TEST_DATABASE_URL="$TEST_DATABASE_URL" bun test packages/scheduler/test
```

Without that test variable, PostgreSQL tests are skipped explicitly. Tests use
real PostgreSQL/Bun SQL, the existing pg-boss-backed Tasks provider/worker and
SIGKILL child processes. No external notifications, payments or production
resources are used.

Workspace and test dependencies are recorded in the single root Bun lockfile;
dependency updates remain owned by the integration owner. Shared Tasks now exports
identity/lookup and finite consumption; shared Auth/Manage behavior is unchanged.
Remote read-replica routing, multi-region behavior, Workers CPU/time-limit
termination and non-tested architectures remain unverified. Optional
application-specific Manage Operations, history pagination/retention and
Notifications/Payments periodic adapters remain follow-ups.

## Workers/D1 setup

Apply `migrations/d1/0001_scheduler.sql` and the Tasks
`migrations/d1/0001_tasks.sql`, then explicitly provision the Tasks queue with
`provisionD1TaskQueue`. Use `createD1ScheduleStore` from `/d1` over the exact
existing Drizzle D1 database, and `createD1TaskProvider` from `@lenso/tasks/d1`
over the borrowed platform binding. The store exports `schedulerD1Schema`.
Plain bindings are required; session-backed Drizzle databases are rejected so
coordination reads do not silently start at a stale replica.

```ts
async scheduled(controller, env) {
  // Application-owned assembly reuses exact plugin instances and current policies.
  const { scheduler, queue, close } = await assembleSchedules(env);
  try {
    await scheduler.tick();
    await queue.runBatch({ maxJobs: 100, concurrency: 2, timeoutMs: 30_000 });
  } finally {
    await close(); // stop/drain owned services; do not close borrowed env.DB
  }
}
```

Await the handler's work directly; no perpetual driver is needed. A later platform
invocation recovers expired claims. [Scheduled-event lifetime](https://developers.cloudflare.com/workers/runtime-apis/handlers/scheduled/)
and [platform limits](https://developers.cloudflare.com/workers/platform/limits/)
still apply: finite job counts do not bound a handler that ignores cancellation.
Keep `node:*` imports external and use Node compatibility. Local checks use
Node's test runner and the repository's pinned Miniflare/workerd, with disposable
D1 bindings. They verify actual `scheduled()` execution, not just Node-side calls
or an in-memory queue; they do not establish remote replica consistency.
Use a progressing clock (normally `Date.now`) for leases; do not freeze a
production worker's lease clock at the event's `scheduledTime`.
