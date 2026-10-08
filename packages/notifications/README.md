# Notifications

`@lenso/notifications` provides ordinary async notification services, versioned
plain-text templates, preferences, durable logical records and separate delivery
attempts. Lenso, Auth, Tasks, Manage and Drizzle are optional subpath integrations.
Importing the root does not import those packages, a Resend SDK, Bun native drivers,
or an OpenTelemetry SDK. Install the optional peers used by your chosen entries.

## Ordinary service

```ts
import { z } from "zod";
import { createNotificationService } from "@lenso/notifications";
import { createResendChannel } from "@lenso/notifications/resend";
import { createPostgresNotificationStore } from "@lenso/notifications/postgres";

const mail = createResendChannel({
  id: "resend-account-a", // Stable provider/account identity, not a display label.
  apiKey: process.env.RESEND_API_KEY!, // Supply through the host's secret authority.
});
const notifications = createNotificationService({
  store: createPostgresNotificationStore(existingDrizzleDatabase),
  channels: [mail],
  templates: [
    {
      id: "order-confirmed",
      version: "1",
      category: "orders",
      necessity: "required", // Business explicitly declares required or optional.
      channels: [mail.id],
      from: "orders@example.com",
      variables: z.strictObject({ orderNumber: z.string().min(1).max(100) }),
      subject: "Order {{orderNumber}} confirmed",
      text: "Your order {{orderNumber}} is confirmed.",
    },
  ],
});

// Trusted business service assigns tenant, recipient and business association.
const record = await notifications.create({
  tenantId: tenant.id,
  scope: "orders.confirmed",
  idempotencyKey: order.id,
  businessId: order.id,
  recipientId: customer.id,
  email: customer.email,
  templateId: "order-confirmed",
  templateVersion: "1",
  variables: { orderNumber: order.number },
});
```

The snippet borrows an existing database. Apply the appropriate migration first;
constructing a service never creates tables or sends a message. `deliver(id)` is
the trusted ordinary service used by Tasks; with a real provider it sends a real
email. It is not a public HTTP handler.

Schemas use Standard Schema v1. Variables must validate to a flat object of finite
numbers, strings or booleans. `{{name}}` placeholders are plain-text only; HTML is
generated from escaped text with line breaks. There is no raw HTML, template
evaluation, URL interpolation into attributes, or arbitrary header facility.
Subjects reject CR/LF. Sender and recipient are single bare email addresses.
Input is finite plain JSON, bounded to 64 KiB and depth 32.

Templates are identified by `(id, version)`: never change the meaning of an
existing version. The logical record retains the rendered message, template
version, category, necessity, recipient, channel and business association. Retry
does not render against newer code or switch providers.

## Preferences and identity

Preferences are keyed by tenant, recipient, category and channel. `optional`
notifications default to disabled unless an explicit preference or
`optionalDefault: "enabled"` authorizes them. An enabled candidate is selected
from the requested/template channel order. All-disabled creates a `suppressed`
logical record with no delivery attempt. The selected channel's preference is
checked again before every send. There is no fallback to another channel on retry.

`required` is a trusted template declaration, not request input that a user can
toggle to evade unsubscribe. A concurrent preference change cannot recall an
in-flight request; this is not a strict-consistency consent barrier. Previously
uncertain effects stay uncertain if a preference subsequently disables retry.

Root services and stores are **trusted internal APIs**. Do not expose `create`,
`getRecord`, `deliver`, `recoverable` or store methods directly to untrusted callers.
Use `createAuthorizedNotificationService` from `/auth` with an existing Auth
`Access`, a trusted realm-aware `tenantFor(principal)` mapping and the genuine actor
from the entry's credential verification, as the method's **second parameter**.
`access.enforce` revalidates actor provenance, audience and session on every call.
Owner queries/list/preferences bind the verified tenant and subject. Unknown,
cross-user and cross-tenant record IDs share `access-denied`.

Management reads, attempt queries and retry additionally require an explicit
`managePolicy` and tenant match. No permission is granted by knowing a UUID or by
putting an actor/tenant membership into business JSON. A raw core summary contains
provider IDs and template/channel metadata; the authorized wrapper further reduces
that to status, counters and timestamps. Neither returns body/address or raw errors.

## Background delivery and recoverable handoff

```ts
import { createNotificationTask, createNotificationDispatcher } from "@lenso/notifications/tasks";
import { createTaskQueue } from "@lenso/tasks";

const task = createNotificationTask({
  name: "orders-notification-delivery",
  service: notifications,
  maxAttempts: 3,
});
const queue = createTaskQueue({ provider: existingTaskProvider, tasks: [task] });
const dispatcher = createNotificationDispatcher({ service: notifications, queue, task });
await dispatcher.reconcile(100); // Explicit bounded recovery at host startup.
// Submit uses the same create input and persists before enqueue.
// await dispatcher.submit(input);
// Start worker explicitly in the worker entry, using the same task/schema/store/provider account.
const worker = await queue.startWorker();
// Host shutdown must stop/drain worker before closing an owned queue or database.
```

The persisted logical notification is the outbox; it is committed **before**
enqueue. The two resources are not one transaction. Queue payload contains only
the notification UUID, with stable `notification/<UUID>` deduplication. A failed
or ambiguous enqueue leaves a recoverable notification; repeated submission or
`reconcile(limit)` reuses the same queue job. Once confirmed, its durable job ID
marks the handoff without changing the delivery revision. A crash before this
marker is repaired by the same deduplicated enqueue; already handed-off jobs
cannot starve later outbox batches. Recovery does not silently call
`queue.retry` on final failures or extend their attempt budget. Explicit
`dispatcher.requeue(id)` retries a final failed Tasks job and only retryable
`failed`/`unknown` records or expired `sending` claims. The latter repairs a lost
completion write even when early Tasks retries exhausted the budget before the
notification lease expired. Active claims cannot be requeued. No new queue,
timer or scheduler is created.
Hosts must invoke recovery on startup and arrange subsequent scans through their
existing operational/task entry. Tasks owns recovery and retries after handoff;
its final failed jobs require an explicit authorized retry. Retain the Tasks
deduplication/relationship row and never delete or repoint a notification's job
while the logical record is retained.
Periodic business notifications can later consume Scheduler.
The repository currently provides Tasks' PostgreSQL/Bun backend, not a native D1
queue backend. `/d1` supplies notification persistence only; it does not establish
a D1/Workers background-delivery path or introduce a replacement queue.

The caller owns `existingTaskProvider`; the queue follows the existing Tasks
lifetime contract. With a plugin-owned queue, the queue's plugin owns cleanup.
Notifications never close borrowed database, channel, Auth or queue resources.
Tasks already supplies safe logs and OpenTelemetry attempt spans. The notification
plugin uses the existing scoped logger with UUID/state/attempt/fixed error only.

## Deduplication and delivery state

- A persistent unique constraint scopes the business key to
  `(tenantId, scope, idempotencyKey)`. Same key and same canonical request returns
  the original logical record. Different parameters are rejected, including races.
- Revision CAS and an atomic notification/attempt write fence local claims.
  PostgreSQL uses a transaction; native SQLite uses its synchronous transaction;
  D1 uses transactional `batch` with a unique mutation-token-guarded attempt insert.
  D1 has no interactive transaction substitute.
- `accepted` means the provider accepted the HTTP request and returned a message
  ID, **not** inbox delivery. A trusted verified receipt integration may call
  `markDelivered(id, providerMessageId)`; no webhook route is installed here.
- Each HTTP call has a distinct attempt record. Timeouts, transport failures,
  malformed responses and 5xx preserve `unknown`; a later rejection does not
  erase an earlier uncertain effect. Expired sending leases become uncertain
  before a new attempt. Stale workers cannot overwrite a newer revision.
- Resend receives the same `Idempotency-Key` and immutable message on every retry.
  Its documented key window is **24 hours**. `firstRequestAt` is retained; retries
  stop before expiry with room for the lease and a one-minute margin. After expiry
  an uncertain record remains uncertain with `deduplication-expired`, and is not
  resent automatically. Use synchronized host clocks and a stable provider/account
  for the channel ID. Do not repoint a channel during an outstanding retry.
- Accepted/delivered/suppressed records and nonretryable failures are not resent.
  This is not exactly-once delivery or a guarantee that a recipient saw a message.

## Lenso, Config and Manage

`/plugin` exports `createNotificationPlugin({ id, database, store, channels,
templates, config? })`. `database` and each channel are the exact installed plugin
objects, not IDs. `store` adapts the borrowed native Drizzle handle. Setup is thin;
resources remain with their owning plugins. `notificationConfig` uses the current
Core Config contract (`optionalDefault`, `leaseMs`) and supports explicit sources.
Provider credentials belong in a sensitive host configuration, resolved at setup,
never template/operation metadata or top-level resource acquisition.

`createNotificationsManage()` from `/manage` returns `undefined` by default.
`enabled: true` requires exact `notifications` and `authentication` (an Auth
`Access` service) plugins, `tenantFor`, `managePolicy`, and a trusted
`requeue: dispatcher.requeue` callback. It returns `{ plugin, operations, manage }`.
Methods declare `context: true`; an authenticated entry binding supplies the actor.
Install its plugin and explicitly select the operations for the desired
CLI/Manage/HTTP adapter. Nothing automatically exposes HTTP, CLI or MCP, and the
default adapter permission rules still apply. Retrying reports dispatch only.

## Migrations and checks

Merge `notificationSchema` from `/postgres` or `/sqlite`/`/d1` into the application's
schema. Apply `migrations/pg/0001_notifications.sql` for PostgreSQL or
`migrations/sqlite/0001_notifications.sql` for SQLite/D1 using the application's
explicit authorized migration workflow. Track it once in that migration history;
these are plain SQL baselines, not an auto-running migrator or Drizzle journal.
Persistence contains private addresses/message bodies: restrict DB access,
retention and backups accordingly. Do not enable Drizzle query logging for this
data, and do not log requests, responses, credentials or raw provider exceptions.

Targeted checks: `bun run --cwd packages/notifications typecheck`, `build`, and
`test`. Tests use real in-memory Bun SQLite plus owned local HTTP fixtures.
Drizzle's D1 batch API is exercised by a SQLite fixture, **not a real D1 backend**.
Auth uses the real runtime with a local trusted verification source. Queue adapter
tests use the real Tasks boundary with a deterministic provider fixture, **not
PostgreSQL Tasks persistence**. PostgreSQL and real D1 migration/runtime require
separate disposable environments and are not established by these fixtures.
The PostgreSQL test is opt-in only: supply a disposable
`LENSO_NOTIFICATIONS_TEST_DATABASE_URL` and `LENSO_NOTIFICATIONS_PG_TEST=1`. It
creates and drops only its own random schema, using two native Bun SQL connections;
it never falls back to the application's `DATABASE_URL`.

Provider mappings were checked against current official
[send API](https://resend.com/docs/api-reference/emails/send-email),
[idempotency keys](https://resend.com/docs/dashboard/emails/idempotency-keys), and
[errors](https://resend.com/docs/api-reference/errors). The adapter uses actual
HTTP Fetch, not a logging sender. Only local fixtures are contacted by tests.
No real email or hosted-provider delivery is verified.
