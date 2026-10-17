# @lenso/events

Explicit, durable event fanout for Bun SQLite, PostgreSQL and Workers/D1. This package owns only events and
per-subscription delivery. Notifications and Scheduler keep their specialized
outboxes; Realtime remains an invalidation signal.

The SQLite and PostgreSQL stores are **Native-only (Bun)**. `@lenso/events/sqlite` is a
Bun-conditional export and is unavailable to Workers, browsers and Node.js. It
accepts a borrowed `bun:sqlite` connection; it never opens one on import. The root
and `/plugin` entries contain no SQLite imports. `/postgres` borrows a Bun SQL
PostgreSQL pool; `/async` and `/async-plugin` contain no database driver imports.
`/d1` accepts a borrowed Cloudflare D1 binding and uses the portable async service;
see [Workers/D1 semantics and local acceptance](docs/D1.md).

```ts
import { Database } from "bun:sqlite";
import { createEvents } from "@lenso/events";
import { initializeEvents, createSqliteEventStore } from "@lenso/events/sqlite";

const db = new Database("events.sqlite"); // application owns this connection
initializeEvents(db); // explicit setup/migration, never in a request path
const events = createEvents({ store: createSqliteEventStore(db) });
events.subscribe("receipt-v1", "order.created", async (event, context) => {
  await destination.send(event.payload, {
    idempotencyKey: context.idempotencyKey,
    signal: context.signal,
  });
});
events.publish({ id: "order:42:created", topic: "order.created", payload: { orderId: 42 } });
await events.dispatch(100);
await events.close();
db.close();
```

`destination` is application-owned. The executable example below supplies a local
fake. Inputs are trusted internal calls; exposing publication, registration or
cancellation requires the application's own authorization and topic/payload validation.

## Delivery and recovery contract

- Register stable subscription IDs **before** publishing. Registration persists;
  restarting with the same ID/topic reattaches to the backlog. A new subscription
  receives future publications, with no implicit historical replay.
- Supply a stable event ID for producer retries. Identical topic/canonical JSON
  returns `{ created: false }`; conflicting content throws. Deduplication records
  are retained indefinitely. Payloads are plain JSON, at most 1 MiB.
- Publication and the snapshot of active subscribers commit in one SQLite
  transaction. Each subscriber has its own delivery state, so one failure does
  not repeat already acknowledged subscribers. No ordering guarantee is provided.
- `dispatch(limit)` claims one delivery at a time, for attached subscriptions only.
  Independent callers/connections may dispatch concurrently. An unexpired lease
  prevents another claim; expired claims recover on the next explicit dispatch.
  Expired/replaced tokens cannot acknowledge or reschedule newer attempts.
- Handler failures become pending with `retryDelayMs` (default 1000). Retries have
  no automatic maximum/dead-letter transition. `status(eventId)` reports state,
  attempts, retry time and lease expiry. Storage errors reject dispatch; an
  unconfirmed claim remains recoverable. Dispatch returns delivered/retried/lost
  counts, with `lost` indicating a claim that expired or was cancelled/replaced.
- Delivery is **at least once**, conditional on continued dispatch and eventual
  handler/store availability. A process can fail after an external effect but
  before acknowledgement. Pass `context.idempotencyKey` to an idempotent
  destination, or atomically record it with the business effect in its database.
  The stable key identifies the event/subscription pair; namespace IDs across
  stores/apps that share a destination's idempotency scope. No external exactly-once
  guarantee is made. The default lease is 30 seconds, with no automatic renewal;
  size it for handlers and make destinations idempotent even for slow executions.
- `unsubscribe(id)` (also returned by `subscribe`) permanently cancels pending
  and processing deliveries; cancelled IDs cannot be reused. It sends a cooperative
  AbortSignal to local active handlers. It cannot undo effects or stop handlers
  already running in another process. Completed delivery records stay completed.
- `close()` stops new claims, signals local handlers and waits for active dispatch
  passes. It retains subscriptions/backlog and does not close the borrowed store.
  Handlers must finish after cancellation; do not await `close()` from a handler.

## Integration boundaries

`createEventsPlugin({ id, store, leaseMs?, retryDelayMs? })` from
`@lenso/events/plugin` requires the exact `Plugin<EventStore>` provider. The
provider initializes storage during setup and registers connection cleanup
immediately; Events registers its own drain cleanup before returning its service.
No resources are acquired on import or plugin declaration.

Tasks/Scheduler may call `events.dispatch(limit)` from an existing task or tick.
Task enqueue/completion is not event acknowledgement, and no task queue is the
source of event truth. No Tasks/Scheduler dependency or changes are needed.

For atomic business writes plus publication, use the **same** SQLite connection
and synchronous `db.transaction(() => { businessWrite(); events.publish(event); })()`.
Nested store transactions use savepoints. Separate databases cannot provide this
atomicity. SQLite connection configuration (busy timeout, durability, backup) is
application-owned. No pruning is supplied.

## PostgreSQL and asynchronous stores

The existing `EventStore`, `createEvents` and SQLite methods remain synchronous.
Network stores implement `AsyncEventStore`, with the same method names, inputs,
results and fencing contract, returning Promises. Use `createAsyncEvents` from
`@lenso/events/async`; await registration before publishing or dispatching.
This service accepts either store interface. Every service method except `close`
corresponds to the synchronous service but returns a Promise; `close` also returns
a Promise and drains all accepted operations, including pending registrations,
cancellations and claims. A claim that completes after shutdown never starts a
handler and can be recovered after its lease expires.

```ts
import { SQL } from "bun";
import { createAsyncEvents } from "@lenso/events/async";
import { createPostgresEventStore, initializePostgresEvents } from "@lenso/events/postgres";

const sql = new SQL(connectionString, { adapter: "postgres" });
try {
  await initializePostgresEvents(sql); // explicit setup; never a request-path migration
  const events = createAsyncEvents({ store: createPostgresEventStore(sql) });
  try {
    await events.subscribe("receipt-v1", "order.created", async (event, context) => {
      await destination.send(event.payload, {
        idempotencyKey: context.idempotencyKey,
        signal: context.signal,
      });
    });
    await events.publish({
      id: "order:42:created",
      topic: "order.created",
      payload: { orderId: 42 },
    });
    await events.dispatch();
  } finally {
    await events.close();
  }
} finally {
  await sql.close();
}
```

`connectionString` and `destination` belong to the application. The optional
`createAsyncEventsPlugin` from `/async-plugin` requires the exact store provider,
registers service drain cleanup, and never closes the borrowed pool.

The PostgreSQL migration is also available at
`@lenso/events/migrations/pg/0001_events.sql`. The initializer applies it in a
transaction, serializing concurrent setup. Configure one stable trusted
`search_path` on **every pool connection** if using a dedicated schema. No schema
or connection is created by the adapter constructor, import or request operations.
The connection owner controls TLS, pool limits and connection cleanup.

Publication uses a unique event ID and commits fanout in the same transaction.
Subscription row locks serialize cancellation with in-flight fanout. Concurrent
claimers use `FOR UPDATE SKIP LOCKED`; expired or replaced tokens cannot settle a
later attempt. The database clock governs leases across hosts, and settlement
checks time after acquiring its row lock. `now` is an optional clock override for
deterministic tests only. No global delivery ordering is promised.

Driver/connection failures reject the operation; they are not successful delivery.
A connection loss around COMMIT can leave an unknown outcome: retry publication
with the **same** event ID/content, or recover unconfirmed delivery after lease
expiry with the same destination idempotency key. The adapter does not retry
external effects, reconnect an application-owned pool, or offer exactly-once
delivery. Separate application transactions are not atomic with publication;
the SQLite nested-transaction example above does not apply to PostgreSQL.

## Local verification

Build Core and Events first (exports resolve to `dist`), then from this directory:

```sh
bun run build
bun run typecheck
bun test test
bun run example:recovery
```

The sample uses temporary SQLite files and a fake idempotent destination. It
injects failure after the effect, reopens the event database, expires the lease
using a fake clock and asserts two attempts but one effect. It makes no network
calls and removes its temporary files. Tests also cover transaction rollback,
competing connections, cancellation, shutdown and per-subscription retry isolation.

For PostgreSQL, explicitly designate a disposable local test database (never a
user database). The suite creates/removes unique schemas and terminates only one
of its own connections to verify failure recovery:

```sh
LENSO_EVENTS_PG_TEST=1 \
LENSO_EVENTS_TEST_DATABASE_URL=postgres://user@127.0.0.1:55428/postgres \
bun test test/postgres.test.ts
```

There is no `DATABASE_URL` fallback. Coverage includes duplicate publishers,
fanout rollback, concurrent claims, locked-row skipping, expired/stale tokens,
retry isolation, cancellation racing publication, backend termination and async
service recovery. Without the two explicit variables these integration tests skip;
a skipped run is not PostgreSQL validation.
