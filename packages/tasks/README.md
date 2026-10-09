# Durable tasks

`@lenso/tasks` runs ordinary async services through PostgreSQL on Bun or D1 on
Workers. Both providers use the same schema validation, executor and task status.
It is optional and independent of Engine, Web, Auth, oRPC and `@lenso/workers`
(the Cloudflare Fetch adapter). There is no global queue or default HTTP admin API.
PostgreSQL consumers install the optional peers `pg@8.23.1` and
`pg-boss@12.37.0`; D1 consumers do not need them.

`TaskQueueError` accepts an optional internal `cause` and restricts runtime codes
to its declared set; an unsupported code becomes `provider-unavailable`.
`taskErrorDiagnostic(error)` projects only recognized instances into fixed
`{code, phase, message}` metadata. Queue/provider causes and combined startup,
migration, rollback or cleanup failures remain available in-process, not in the
public projection. Classification adds no retry guarantee: writes with a lost
response can already have completed.

## Bounded management reads

`queue.list({ tasks, limit, after })` reads one queue-scoped page, ordered by
ascending immutable job ID. The task allowlist is required; an empty list returns
no rows, and the maximum page size is 100. The result is `{ items, nextCursor }`.
Items contain job ID, task, state, attempts and cancellation-request state only,
never input, result or raw failure text. A custom provider without `list` fails
with `unsupported`; the queue does not substitute an unbounded scan.

This is a queue query, not tenant authorization. An application management service
must check actual job ownership and current authority before projecting rows or
calling retry/cancel. Task names do not identify tenants. Retry also requires the
business owner's explicit replay-safety policy. A cancellation request is not
proof that execution stopped or that external effects were undone.

## Define, enqueue, consume

```ts
import { z } from "zod";
import { createTaskQueue, defineTask } from "@lenso/tasks";
import { createPostgresTaskProvider, migratePostgresTaskQueue } from "@lenso/tasks/postgres";

const generateReport = defineTask({
  name: "generateReport",
  input: z
    .object({
      reportId: z.string().min(1), // stable business idempotency key
      rows: z.array(z.number().finite()),
    })
    .strict(),
  maxAttempts: 3, // total initial execution budget, including the first attempt
  retry: { delaySeconds: 2, backoff: true, maxDelaySeconds: 60 },
  async handler(input, { jobId, attempt, signal }) {
    // An ordinary service, not a workflow DSL. Use reportId to make writes idempotent.
    signal.throwIfAborted();
    return reportService.generate(input, { jobId, attempt, signal });
  },
  result: (report) => ({ reportId: report.id, rows: report.rowCount }),
});

const connection = {
  connectionString: process.env.DATABASE_URL!,
  queueName: "reports",
};

// Run this only from a separate, explicitly invoked migration/provisioning script.
await migratePostgresTaskQueue(connection);

// Producer and worker processes each create their own provider for the same queue.
const provider = await createPostgresTaskProvider(connection);
const queue = createTaskQueue({ provider, tasks: [generateReport] });
try {
  const jobId = await queue.enqueue(
    generateReport,
    { reportId: "monthly-42", rows: [10, 20, 30] },
    { runAt: new Date(Date.now() + 1_000), deduplicationKey: "monthly-42" },
  );
  const status = await queue.get(jobId);
  await queue.cancel(jobId);
  // Only a final failed job is eligible. Adds one attempt without resetting its counter.
  await queue.retry(jobId);
} finally {
  await queue.close();
}
```

In a **separate worker entrypoint**, register the same task names and schemas:

```ts
const queue = createTaskQueue({
  provider: await createPostgresTaskProvider(connection),
  tasks: [generateReport],
});
const worker = await queue.startWorker({ concurrency: 2, timeoutMs: 120_000 });
// Host owns signals. On shutdown: stop claiming, wait for handlers, then close clients.
await worker.stop(); // stop({ abort: true }) also sends cooperative abort to active handlers
await queue.close();
```

The second snippet's `stop()` belongs in the host shutdown path, not immediately
after `startWorker()` in a long-running process. See `examples/tasks` for runnable
producer, worker and signal handling. Observe `worker.done` to detect a failed
worker and drain/close its resources instead of leaving an idle process alive.

## Lenso lifecycle

No worker starts implicitly. A producer-only plugin omits `worker`:

```ts
import { createTaskPlugin } from "@lenso/tasks";
import { startApp } from "@lenso/core";

const reports = createTaskPlugin({
  id: "report-queue",
  tasks: [generateReport],
  connect: () => createPostgresTaskProvider(connection),
  // Explicitly opt into an in-process worker, or omit this in the producer.
  worker: { concurrency: 2 },
});
const app = await startApp({ plugins: [reports] });
await app.get(reports).enqueue(generateReport, { reportId: "report-1", rows: [1] });
await app.stop();
```

Each setup obtains a fresh provider and immediately registers the existing
`context.onCleanup` disposer. The plugin's public `close()` uses that same
disposer, so early release and application stop share its completion, including
failures. Startup rollback and application stop drain workers before closing
connections. `requires` and `connect(context)` can
reuse the application's existing resources. PostgreSQL options accept a
caller-owned `pg.Pool` **instead of** `connectionString`; neither normal close,
startup failure nor migration closes a borrowed pool. This driver uses
node-postgres through Bun's Node compatibility, not a cast of Bun SQL into a
`pg.Pool`. Business services can still use the existing native Drizzle DB plugin.

Different `queueName` values isolate durable work; different plugin `id` values
isolate Lenso instances. Workers sharing a queue compete for its jobs. Register
the full task set for that queue in each worker; an unknown task fails safely.
`concurrency` is per worker instance, not a global or cluster-wide quota.

## Data and authority

`queue.identity()` returns `{kind, id}` where `id` is an opaque, persisted queue
UUID. It survives process restart and repeated explicit provisioning; it is not
a plugin ID, connection string or authorization grant. The PostgreSQL upgrade
adds `0003_queue_identity.sql`; rerun the explicit queue migration before startup.
Cloning a database also copies its identity, so independently operated clones
must not be treated as one queue.

`queue.lookupDeduplicationKey(key)` is read-only and returns `{jobId, status}` or
null if no acceptance mapping exists in this queue. A pruned PostgreSQL job can
still have an accepted mapping with `status: null`. It never enqueues, returns
payload/trace metadata, or assigns a new key. Like `get`, this is a trusted
internal API: authorize the caller and durable owner/tenant before exposing it.
Custom `TaskProvider` implementations must implement both new methods with their
real durable backend, not invent process-local identities or enqueue on lookup.

Tracing imports only the official OpenTelemetry API, never an SDK. A host may
explicitly initialize `@lenso/otel/bun`. Enqueue creates a producer span; each
durable attempt creates a fresh root linked to the original producer. Only
bounded `traceparent`/`tracestate` are stored in the separate `trace_metadata`
column, never baggage, payload or identity. Deduplication retains the original
producer metadata, and retries receive new attempt spans. Trace context is not
authority. Re-run the explicit migration before upgrading an existing queue;
`0002_trace_metadata.sql` adds the nullable metadata column without changing jobs.
The queue uses the official W3C propagator for this fixed carrier format,
independently of any custom propagator selected for the host's HTTP traffic.

Pass `logger`, `instanceId` and `pluginId` to `createTaskQueue`, or use
`createTaskPlugin` to inherit the app's scope. A standalone queue gets its own
instance ID. Each handler's `TaskContext.logger` is scoped to jobId/task/attempt
and correlates the currently active span; logs still work without an SDK.
Framework logs contain only bounded safe summaries, never the task payload or
exception text. Task IDs and instance IDs belong in logs/traces, not metric
labels. Counts/durations/errors label only outcome.
The pg-boss driver's default telemetry is disabled: Lenso's portable queue
boundary owns producer/attempt semantics and avoids duplicate send spans and
driver-specific schema/queue metric labels.

- Input is validated with the **same Standard Schema v1 object** before enqueue
  and before execution. Its input/output types are inferred, including defaults
  and transformations. Raw JSON is persisted so transformations do not compound
  across processes. Schema validation must be deterministic and side-effect free.
- Payloads and validated inputs must be plain, acyclic, finite JSON: null,
  booleans, numbers, strings, arrays and plain objects. No functions, class
  instances, Date, BigInt, undefined, sparse arrays, symbols or accessors.
  Maximum UTF-8 input size is **64 KiB**, maximum depth is **32**.
- Handlers may return ordinary values, but results are **discarded by default**.
  Only a task's explicit `result` projection is persisted and returned, limited
  to **16 KiB** of the same JSON format. Project an allowlist of safe summary
  fields; never include credentials, tokens, actor objects or full service data.
- Errors are fixed codes: `handler-failed`, `invalid-input`, `invalid-result`,
  `aborted`. Original exception messages, input values and stacks are not saved
  or exposed by the core queue API. Optional framework logging records attempt
  start/completion and fixed error codes, not arbitrary handler output.
- `get` omits input and internal queue metadata. Knowing a jobId is **not**
  authority. Queue services are trusted in-process APIs: application services
  must load ownership and authorize the authenticated actor before querying,
  retrying or cancelling private jobs. Do not accept a “trusted actor” from JSON.
  If Web is needed, wrap that authorized service, not the raw queue.
- Persistent deduplication is scoped to queue + key. Matching task/input returns
  the original jobId; a different task/input is rejected. Deduplication does
  **not** make external effects exactly-once.

## Execution and cancellation boundaries

Delivery is **at-least-once**, not exactly-once. A process can die after a business
write and before acknowledging the job. Use a stable business idempotency key
and DB constraints/transactions, or the external provider's idempotency support.
Fenced queue acknowledgement does not fence an external payment or file write.

`cancel()` returns:

| Value       | Meaning                                                                 |
| ----------- | ----------------------------------------------------------------------- |
| `cancelled` | A pending job was atomically prevented from being claimed.              |
| `requested` | A running job has a durable cancellation request; it may still execute. |
| `terminal`  | The job has already ended.                                              |
| `missing`   | No retained job in this queue.                                          |

Running cancellation signals `AbortSignal` through persistent polling. Status
stays `running` with `cancelRequested: true` until the handler actually settles;
only then is the current claim cancelled. Side effects are **not rolled back**.
Handlers must propagate the signal and check it before further side effects.
This describes a live owned claim. A terminal queue status cannot prove that an
abandoned process or an earlier attempt that lost its lease has physically stopped.

`timeoutMs`, claim loss and `stop({abort:true})` send cooperative abort. No
`Promise.race` frees the worker slot: a handler ignoring abort keeps its slot
and can delay shutdown indefinitely. There is no hard-kill handler API. A
cooperative timeout/stop is retriable failure, not a durable user cancellation.
An in-flight fetch returning after stop is acknowledged as an aborted attempt
without starting business work; that claim has already consumed an attempt.

Lease loss or `expireInSeconds` can permit another process to retry while the
old handler is still unwinding. The old process holds its own local slot and
cannot acknowledge over the new attempt; business effects still need idempotency.
An unresponsive handler is not forcibly stopped by a heartbeat or deadline.

## PostgreSQL ownership, migration and retention

The provider pins **pg-boss 12.37.0**. pg-boss owns its internal schema, claim
locks, attempt counters, heartbeat recovery, retry scheduling and pruning.
Lenso owns its Drizzle relationship and durable queue-identity tables;
see `src/schema.ts` and the explicit SQL in `migrations/`.

`migratePostgresTaskQueue` uses pg-boss's official construction/upgrade path and
explicitly provisions the nonpartitioned named queue plus the Lenso table.
Run it against an already-created database before producer or worker startup.
Import, application start and worker start never install, upgrade or create
tables. Runtime fixes `migrate:false`, `reindex:false`, `schedule:false`; missing
or mismatched database state is an error, not a fallback migration.

Runtime provider options include:

| Option                     | Default  | Meaning                                                 |
| -------------------------- | -------- | ------------------------------------------------------- |
| `schema`                   | `pgboss` | pg-boss-owned schema, shared by queues                  |
| `pollIntervalMs`           | 1000     | Claim/cancellation polling interval                     |
| `heartbeatSeconds`         | 30       | pg-boss claim heartbeat window, minimum 10              |
| `expireInSeconds`          | 900      | Independent attempt expiry; heartbeats do not extend it |
| `retentionSeconds`         | 604800   | Job retention window, 7 days                            |
| `superviseIntervalSeconds` | 30       | Recovery supervision cadence                            |

`onError` optionally receives a fixed `TaskQueueError` for supervisor/owned-pool
operational failures, never a raw driver error. Hosts should monitor it and
`worker.done`. A borrowed pool's error handling remains its caller's responsibility.

Recovery needs a live provider/supervisor. PostgreSQL does not itself run a
JavaScript recovery loop while all processes are offline; restart a worker to
recover stale claims. Set attempt expiry above the expected handler duration.
Backoff uses pg-boss's jittered exponential strategy; `backoff:false` is fixed
delay, and `maxDelaySeconds` requires `backoff:true`.

Job status/results and explicit retry are available while pg-boss retains the
job. After pruning, `get` returns null, cancellation returns `missing`, and
retry returns false. Dedup mappings intentionally remain; re-enqueueing an
expired mapping is rejected rather than silently creating a duplicate. This
version does not provide a history archive or automatic dedup-table pruning.
Choose retention for your product and use a new explicit business key when
appropriate.

Why not Graphile Worker here: its current public startup APIs automatically
migrate, and the investigated version's successful completion SQL does not
fence the claim owner. pg-boss has attempt fencing, but its `work()` timeout
wrapper can finish waiting before a handler finishes. Lenso therefore uses its
public `fetch`, `touch` and attempt-fenced settlement APIs with fixed local
execution lanes. It does not reimplement locks, leases or scheduling.

## Run checks

From the repository root:

```sh
bun install
bun run --filter lenso --filter @lenso/tasks build
bun run --filter @lenso/tasks typecheck
bun test packages/tasks/test/core.test.ts
TASK_TEST_DATABASE_URL="$DATABASE_URL" bun test packages/tasks/test/postgres.test.ts
```

PostgreSQL tests create uniquely named queues in the supplied **test database**.
They use real processes and SIGKILL, not an in-memory persistence substitute.
Without `TASK_TEST_DATABASE_URL`, those tests are explicitly skipped.

Not included: Cloudflare Queues, cron, DAGs, durable workflows, Console,
multi-region scheduling, cluster-wide concurrency quotas or forced handler
termination.

## D1 and finite Workers consumption

Explicitly apply `migrations/d1/0001_tasks.sql` using the application's authorized
migration workflow, then explicitly call `provisionD1TaskQueue(database, queueName)`
from `@lenso/tasks/d1`. The latter provisions a stable queue identity, not tables.
`createD1TaskProvider({database, queueName})` fails when schema/provisioning is
missing; import, setup and worker start perform no DDL.

```ts
import { createTaskQueue } from "@lenso/tasks";
import { createD1TaskProvider } from "@lenso/tasks/d1";

const queue = createTaskQueue({
  provider: await createD1TaskProvider({ database: env.DB, queueName: "reports" }),
  tasks: [generateReport],
});
try {
  await queue.runBatch({ maxJobs: 100, concurrency: 2, timeoutMs: 30_000 });
} finally {
  await queue.close(); // drains only owned workers; env.DB stays borrowed
}
```

Await this finite call from a platform `scheduled()` handler or another trusted
entry. It uses the existing worker/executor, exits on idle and drains active
handlers and heartbeats. `maxJobs` is a **global fetch-attempt budget** across
lanes, including empty fetches, default 100, maximum 1000; it is not a guarantee
that that many jobs complete. `startWorker({maxJobs, stopWhenIdle:true})` offers
the same finite mode. Native continuous polling remains explicitly opt-in; do
not leave it running beyond a Workers invocation.

D1 uses conditional `UPDATE … RETURNING` claims, monotonically increasing
attempt fences, and transactional batches for enqueue/recovery/cancel. Supply a
**plain D1 binding**, not a session-backed facade: authoritative reads use the
primary. [Cloudflare batch/session semantics](https://developers.cloudflare.com/d1/worker-api/d1-database/)
are not an interactive transaction API; zero-row CAS is not a batch failure.

Options: `leaseMs` defaults to 30000, independent `expireInMs` to 900000, polling
to 1000 ms; each is an integer 1–2147483647. Recovery runs during the next fetch,
so it needs a later platform invocation, not a permanent supervisor. Lease loss,
expiry or platform termination can overlap handler attempts; external effects
still need idempotency/fencing. Cooperative cancellation does not physically
kill a handler or free its lane before settlement.

D1 exponential backoff is deterministic, unlike pg-boss jitter. Its base is at
least one second when `backoff:true`; fixed delay can be zero. Optional
`maxDelaySeconds` caps it. The first D1 version retains job/dedup rows indefinitely;
it has no automatic pruning or archive. Do not manually delete dedup rows as a
retry mechanism.

Workers must preserve `node:*` externals and enable Node compatibility for the
common JSON/tracing code. Package builds preserve these specifiers; otherwise a
browser bundler can replace native `node:util` with an incomplete polyfill.
The actual local workerd suite lives in Scheduler:
`bun run --filter @lenso/scheduler test:d1`. It validates D1 plus finite worker
execution and real scheduled events using Miniflare, not a SQLite mock.
Remote D1 replication, CPU quotas and all platform/architecture combinations
are not verified by that local suite.
