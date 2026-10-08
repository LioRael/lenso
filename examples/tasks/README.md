# Local durable report queue

Requires Bun, local PostgreSQL, and an application-configured authentication
source. Short-lived producers enqueue durable jobs; a separate worker calls an
ordinary async report service. This example includes no Notes, Web, email, paid
API, or storage plugins.

`@lenso/tasks/postgres` manages the queue. The business result lives in the
example-owned Drizzle table `task_example_reports`, not in logs or the queue's
result field. `reportId` is a stable business idempotency key; a primary-key
upsert writes `sum/count`. Repeated execution with the same `reportId` retains
one row. Resubmitting that key with different rows lets the last write replace
the previous summary; it does not detect conflicting input.

## Setup

Create a local database first. Supply `DATABASE_URL` through the environment;
there are no default credentials. Never commit real credentials. Use the same
database URL and queue name in every terminal.

Install dependencies and build the framework packages from the repository root:

```sh
bun install
bun run build
cd examples/tasks
export DATABASE_URL='postgres://localhost/lenso_tasks'
# Optional; defaults to reports. Use the same name for producer, worker, and migration.
export TASK_QUEUE_NAME=reports
bun src/migrate.ts
```

The explicit migration creates the provider and business tables. Producers,
workers, and module imports never run migrations. The example's
`CREATE TABLE IF NOT EXISTS` statements provision the initial tables; they are
not a migration system for future schema changes.

## Authentication and durable ownership

`lenso.config.ts` explicitly exposes `tasks.submit/query/cancel/retry/report`.
Each method accepts one ordinary business input. Authentication evidence is
not part of that JSON: callers cannot supply `actor`, `subjectId`, or
credentials to impersonate an owner. `producer.ts` is a compatibility entry
through the same `lenso-cli call` boundary, not a direct queue client.

The entry environment must provide `TASK_SESSION`, an existing short-lived
session credential, and `TASK_AUTH_SOURCE_MODULE`, the absolute path to a
trusted application module. Relative module paths resolve against the current
working directory. The module exports
`connectTaskAuth(): Promise<TaskAuthConnection>`, returning `{source, close?}`.
Its `source` is a real `@lenso/auth` `AuthSource<string | null>` that verifies
credentials and returns the actual subject. Reuse an application's existing
`createManagedSessions(...).source`, or adapt its real session service with
`sessionSource`. If the source declares a realm, it must be `task-example`.
Do not use an environment-supplied developer subject, a fixed test subject, or
a source that treats arbitrary tokens as subject IDs. Do not issue new
credentials at this entry. Fixed evidence mappings in tests are fixtures,
not production configuration.

Acquire resources inside `connectTaskAuth`, never at module top level. Return
`close` for connections owned by this entry; do not close borrowed shared
connections. `inspect` does not load the module. Calls fail when configuration
is missing, with no anonymous or administrator fallback. The real source must
reject expired, revoked, or unrecognized evidence.
`createAuth(realm(...)).for(audience(...))` authenticates first, and `enforce`
revalidates the evidence at each business boundary.

Inject credentials through the environment or a secret manager, not commands,
source files, or reports. All producer terminals must use the same identity
provider and realm. Workers and migrations need no user credentials. Once the
real source is configured, discover and call the shared schema:

```sh
# Run from the repository root. Inspect needs no DB or authentication connection.
bun packages/cli/src/bin.ts inspect tasks submit --root examples/tasks --json
printf '%s\n' '{"reportId":"authorized-daily","rows":[1,2]}' |
  bun packages/cli/src/bin.ts call tasks submit --root examples/tasks --stdin --json
# Supply the returned jobId as business input, without an actor:
printf '%s\n' '{"jobId":"<returned-job-id>"}' |
  bun packages/cli/src/bin.ts call tasks query --root examples/tasks --stdin --json
```

An MCP host can launch `bun /absolute/path/to/examples/tasks/src/mcp.ts` with
the same trusted environment. This optional stdio entry fixes the application
root and allowlists only the five authorized operations above. It starts no
worker and exposes neither a shell nor dynamic module selection.
`TASK_AUTH_SOURCE_MODULE` belongs to the launch environment; clients cannot
supply it. The tool catalog reuses operation descriptions. Cancelling an MCP
request still waits for the current call and cleanup; it is not `tasks.cancel`
and does not mean a background task has stopped. See
[`@lenso/mcp`](../../packages/mcp/README.md).

The explicit migration adds `task_example_report_owners` and
`task_example_job_reports`. Each `reportId` has one immutable
`(realmId, subjectId)` owner across the database, matching the business table's
global primary key. That owner can rewrite the report under the original
business rules. Application tables resolve a queue name and jobId to the
report and owner; knowing a jobId grants no access. Other owners are denied
query, cancellation, retry, report reads, and submission with the same
reportId, before any queue operation runs. Queue deduplication keys are scoped
by a hash including the owner and reportId, so reusing a key cannot retrieve
someone else's job.

Legacy reports and jobs are not assigned to the first caller. Resources
without durable ownership records deny access. An administrator must verify
historical ownership and migrate it explicitly; setup never claims it.
New submissions reserve report ownership, enqueue the job, then record the
job mapping. These steps are not a cross-database atomic transaction. A failed
mapping write or a process crash after enqueueing can leave an inaccessible
orphan job. The operation fails rather than bypassing ownership. Retrying
with the same deduplication key as the original owner can repair the mapping;
resubmitting without a key may create another job. Database administration is
part of the deployment trust boundary. Raw DB and queue clients are not user
interfaces.

Before upgrading an existing database, stop the old unauthorized producers
and drain or isolate the old queue before enabling the new entry. Legacy
worker payloads contain no ownership information. Do not let old jobs with
unknown owners write the same report keys concurrently with new submissions.
The migration command does not pause workers, scan old queues, or decide
historical ownership for the administrator.

## Two producers and one worker

In terminal A, enqueue two jobs. Each command opens and closes its own
connections, then exits:

```sh
A=$(printf '%s\n' '{"reportId":"daily-a","rows":[10,20,-5]}' | bun src/producer.ts enqueue)
B=$(printf '%s\n' '{"reportId":"daily-b","rows":[2,4,6]}' | bun src/producer.ts enqueue)
bun src/producer.ts get "$A"
bun src/producer.ts get "$B"
```

In terminal B, run the separate worker:

```sh
bun src/worker.ts
```

In terminal A, query until `state` is `succeeded`, then read the business table:

```sh
bun src/producer.ts get "$A"
bun src/producer.ts get "$B"
bun src/producer.ts report daily-a # {"sum":25,"count":3}
bun src/producer.ts report daily-b # {"sum":12,"count":3}
```

`enqueue` reads JSON from stdin and writes only the jobId to stdout. `get`
returns safe status fields or `null`, never payloads, database errors, or task
error text. A jobId without ownership records, or belonging to another owner,
is denied without revealing whether the job exists. `report` authorizes before
querying the business table and returns only `sum/count` or `null`. Errors use
fixed messages, without URLs, credentials, payloads, raw errors, or stacks.
Worker start logs on stderr contain only the framework jobId and attempt,
so they can confirm that execution has started.

### Input and scheduling

Input includes `reportId` (a required nonempty string), `rows` (an array of
finite numbers), `failUntilAttempt` (default 0), and `durationMs` (default 0).
Attempts start at 1. When `attempt <= failUntilAttempt`, the handler fails
intentionally before the business write. `durationMs` adds a cooperative wait
for observing cancellation during execution.

The producer also accepts `runAt` (an ISO timestamp with a timezone) and an
optional `deduplicationKey`. These scheduling fields are not part of the
business payload:

```sh
RUN_AT=$(bun -e 'console.log(new Date(Date.now() + 30000).toISOString())')
D=$(printf '{"reportId":"delayed","rows":[1,2,3],"runAt":"%s"}\n' "$RUN_AT" | bun src/producer.ts enqueue)
bun src/producer.ts get "$D"
# After runAt, let the worker claim the job, then query:
bun src/producer.ts report delayed
```

`deduplicationKey` deduplicates enqueueing in the queue. It does not replace
the business write idempotency provided by `reportId`:

```sh
printf '%s\n' '{"reportId":"deduplicated","rows":[3,7],"deduplicationKey":"deduplicated-v1"}' | bun src/producer.ts enqueue
printf '%s\n' '{"reportId":"deduplicated","rows":[3,7],"deduplicationKey":"deduplicated-v1"}' | bun src/producer.ts enqueue
```

### Bounded failure and manual retry

Jobs have at most 3 automatic attempts. Retry delay starts at 2 seconds and
backs off to a maximum of 10 seconds. These delays are not precise completion
times.

```sh
R=$(printf '%s\n' '{"reportId":"retry-demo","rows":[8,9],"failUntilAttempt":3}' | bun src/producer.ts enqueue)
bun src/producer.ts get "$R"
# Wait until get reports failed with attempt:3, then run:
bun src/producer.ts retry "$R" # true
# Manual retry preserves jobId/payload, keeps the attempt count, and adds one attempt.
# Attempt 4 no longer fails intentionally. Wait for succeeded, then run:
bun src/producer.ts get "$R"
bun src/producer.ts report retry-demo # {"sum":17,"count":2}
```

`retry` accepts only final failures; otherwise it returns false. It does not
change the intentional failure condition or rows. If that condition still
holds, the new attempt will fail again. Use `failUntilAttempt:2` to observe
success on the third automatic attempt without manual retry.

### Cancelling a running job is not rollback

```sh
C=$(printf '%s\n' '{"reportId":"cancel-demo","rows":[4,5],"durationMs":30000}' | bun src/producer.ts enqueue)
bun src/producer.ts get "$C"
# Confirm running, or report-started for this jobId in worker output, then run:
bun src/producer.ts cancel "$C"
bun src/producer.ts get "$C"
bun src/producer.ts report cancel-demo
```

Cancellation returns `requested`, `cancelled`, `terminal`, or `missing`.
For a running job, `requested` records a cancellation request; it does not mean
the handler has stopped. This example responds to AbortSignal during its wait
and checks the signal again before writing, so timely cancellation during the
wait prevents a new report write. Once the database write starts, there is no
promise of signal interruption or transaction rollback. Cancellation racing
with that write may leave a report record. Committed writes, writes from
previous attempts, and other external effects are not automatically undone.

## Worker crash and restart

Stop the foreground worker above with Ctrl-C and wait for its drained log.
For the crash demonstration, kill only the PID started below:

```sh
CRASH=$(printf '%s\n' '{"reportId":"crash-demo","rows":[11,12],"durationMs":30000}' | bun src/producer.ts enqueue)
bun src/worker.ts &
WORKER_PID=$!
bun src/producer.ts get "$CRASH"
# Query until running, then kill only the worker just started:
kill -KILL "$WORKER_PID"
wait "$WORKER_PID" || true
bun src/worker.ts
```

Keep `$CRASH` in another terminal, or copy its jobId, and query its status and
`report crash-demo`. A crash runs no cleanup. The provider's durable recovery
mechanism retries work when lease or timeout conditions permit; recovery is
not necessarily immediate. The database and queue survive producer or worker
exit. A crash after the business commit but before queue acknowledgement can
cause another execution. The `reportId` upsert prevents duplicate business
rows, but does not provide exactly-once execution.

## SIGINT/SIGTERM and resource ownership

The first SIGINT/SIGTERM requests `worker.stop({abort:true})`: stop claiming,
cooperatively notify active jobs, and await their actual handler Promises.
Repeated signals neither add a deadline nor force exit. After drain, close
the queue with `queue.close()`, then close the business pg pool created by
this process. There is no early `process.exit` or timeout that skips drain.
The entry also monitors `worker.done`, so a failed consumer does not leave
an idle process behind. Even on failure, it waits for all actual handlers
to settle before attempting connection cleanup.

A shutdown signal is not an explicit `queue.cancel(jobId)` and does not
guarantee a durable `cancelled` state. The provider may record failure or
schedule a retry for another worker. Handlers that ignore the signal and
unfinished SQL continue to occupy their slots; shutdown keeps waiting.
A signal proves neither that work has stopped nor that effects were rolled back.

The business service borrows a caller-owned Drizzle connection and never
closes it. Producers, workers, and migrations close only resources they
created. Shared modules contain declarations and factories; importing them
opens no connections, starts no workers, and registers no process signals.
Resource acquisition happens only in explicit entries.

## Checks

After installing dependencies and building the framework packages, run:

```sh
bun run typecheck
bun test src
```

Unit tests cover input defaults, aggregation, AbortSignal waits, the real
Auth core and CLI invocation lifecycle, owner and cross-owner behavior,
cancellation return values, and failed-retry boundaries. In-memory queue
and ownership fixtures do not prove PostgreSQL persistence or production
session-provider behavior.

`src/postgres.test.ts` uses the real PostgreSQL provider, business ownership
tables, and worker. It is explicitly skipped without `TASK_TEST_DATABASE_URL`.
Use a dedicated test database and run the explicit migration first; neither
setup nor the tests migrate it:

```sh
# DATABASE_URL is already supplied through the environment for a dedicated test DB.
TASK_QUEUE_NAME=authorization-test bun src/migrate.ts
TASK_TEST_DATABASE_URL="$DATABASE_URL" bun test src/postgres.test.ts src/entry.test.ts
```

`src/entry.test.ts` submits through the real CLI, queries and cancels through
the real SDK stdio entry, verifies cross-owner denial, then queries the durable
state through the CLI. Its authentication source is a temporary test fixture,
not a production identity provider.

Tests retain UUID-named reports and jobs for inspection, close their own
connections and workers, and do not delete shared data. They verify that
another resource instance reads the same durable ownership, cross-owner
access is denied, pending jobs can be cancelled, and running jobs actually
settle after a cancellation request. They also verify final failure at
attempt 3, followed by retry and a real report write at attempt 4. Production
identity providers and cross-process crash/recovery still require the
configured commands above. Skipped checks are not passing checks.
