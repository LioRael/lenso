# @lenso/events

Explicit, durable event fanout for Bun SQLite. The baseline had no general event
plugin: Notifications and Scheduler own their specialized outboxes; Realtime is
an invalidation signal. This package owns only events and per-subscription delivery.

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
application-owned. No pruning, PostgreSQL/D1 adapter or cross-host queue is supplied.

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

Integration owner: add the new workspace to the root `bun.lock` using the existing
repository workflow. This independent change intentionally leaves that lockfile
and all shared package/CLI/Engine entry points untouched.
