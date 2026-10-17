# @lenso/webhooks

Durable outbound events using PostgreSQL and `@lenso/tasks`, with an optional
signed inbound receiver and transactional PostgreSQL inbox. No owned listener,
automatic migration, production provisioning or global resource lookup.
Payments' Stripe inbound handling is unchanged.

## Assembly and configuration

Import services from `@lenso/webhooks`, the borrowed `pg.Pool` repository from
`@lenso/webhooks/postgres`, and optional assembly/Manage helpers from `/plugin`
and `/manage`. `examples/host.ts` shows the explicit task → queue → service wiring;
`examples/receiver.ts` wires the optional receiver with host-supplied keys and inbox.

## Optional signed inbound receiver

`@lenso/webhooks/receiver` exports `createWebhookReceiver` and the `WebhookInbox`
contract. It verifies this package's v1 HMAC wire format; it is not a verifier for
Stripe, GitHub, Resend or other providers' signature formats. Existing Notifications
idempotency, retry and provider-uncertainty handling remain separate.

```ts
import { createWebhookReceiver } from "@lenso/webhooks/receiver";
import { createPostgresWebhookInbox } from "@lenso/webhooks/receiver-postgres";

const inbox = createPostgresWebhookInbox({
  pool: hostPool,
  schema: "partner_inbox",
  async apply(transaction, event, receipt) {
    // Use this same transaction for the host's business write or existing transactional
    // event mechanism. Do not send notifications or perform external HTTP here.
    await transaction.query(
      "INSERT INTO partner_orders (receiver_id, event_id, data) VALUES ($1, $2, $3)",
      [receipt.receiverId, event.id, event.data],
    );
  },
});
const receiver = createWebhookReceiver({
  receiverId: "tenant-a/orders-partner", // Host-assigned durable trust namespace.
  source: "orders-service",
  eventTypes: ["order.completed"],
  maxBodyBytes: 262144,
  bodyTimeoutMs: 5000,
  toleranceSeconds: 300,
  keys: () => hostSecrets.verificationKeys("tenant-a/orders-partner"),
  inbox,
});
// Mount receiver.handle(request) on one route of the existing host Web listener.
```

Apply `migrations/0002_receiver.sql` explicitly to that schema before mounting the
route. This migration is independent of the outbound tables. The package does not
create keys, read environment secrets, choose a tenant from JSON or change ingress
or outbound network policy. The host selects a distinct `receiverId` and verification
key authority for each trust/tenant domain, validates Host/route admission, limits
connections, and provides synchronized clocks. Never reuse a namespace for a
different sender. Retain old verification keys during rotation for accepted requests;
never assign different key bytes to an existing key ID.

Verification covers exact raw bytes, signed event ID and timestamp before JSON
processing or inbox access. The receiver requires POST and uncompressed JSON;
limits bytes and stream duration; rejects malformed UTF-8, non-finite/deep data,
unsupported envelope fields, mismatched IDs, sources and event types. A key resolver
failure fails closed. The resolver and DB/application callbacks are host-owned and
must have their own bounded I/O deadlines; `bodyTimeoutMs` bounds only body reads.

The PostgreSQL inbox uses a unique `(receiver_id,event_id)` receipt and SHA-256 of
the raw body. Receipt insertion and `apply` commit together. Concurrent duplicates
wait for the winning transaction, identical redelivery returns 204 without `apply`,
and reusing an ID with different bytes returns 409. Timestamp/key refresh does not
change receipt identity. A failed transaction rolls back both receipt and business
writes. A lost acknowledgement after commit is safe to retry with identical bytes;
its receipt survives receiver or database-client restart. `apply` must only perform
transactional database work through the supplied client, and must not commit,
release, or change its transaction. External effects are not made exactly-once by
this API. Reuse the host's existing Events/Tasks handoff and reconciliation semantics
for asynchronous work; the receiver adds no queue or worker.

Receipts store only namespace, event ID, body digest and receive time, with no body
or key bytes. There is no automatic pruning: deleting a receipt permits a sender to
resubmit that ID with a fresh signature. The host must define retention covering
its business idempotency horizon and outbound/manual replay window, or retain
receipts indefinitely. Timestamp tolerance alone cannot prevent a freshly signed
redelivery from repeating a business effect after its receipt was deleted.

Responses are empty: 204 accepted/duplicate, 400 invalid envelope, 401 invalid or
stale signature, 403 unapproved source/type, 405 method, 408 read deadline/abort,
409 content conflict, 413 size, 415 encoding/media type, 503 host dependency or
transaction failure. Do not treat 503 as evidence that nothing committed.

`createWebhookReceiverPlugin` from `/plugin` accepts the exact `Plugin<WebhookInbox>`
as `inbox`, optional explicit `requires`, and `options(context)` for the receiver
configuration/keys. Construction acquires no resources; it borrows the host's inbox,
listener and secret authority. The ordinary receiver has no Core dependency.

The host creates a dedicated PostgreSQL schema, replaces
`__LENSO_WEBHOOK_SCHEMA__` in `migrations/0001_webhooks.sql` with its reviewed quoted
identifier, and applies the migration once using its migration runner.
Pass that same `schema` to `createPostgresWebhookRepository({pool,schema})`
(default `public`). One schema has one Webhooks execution-policy owner; use separate
schemas for instances with different queues, keys or outbound policies.
The host
provisions the existing PostgreSQL Tasks queue, and supplies durable Audit and
Limits services. For strict replay audit, use an Audit repository with
`durableIntents: true`. Neither the service nor Plugin acquires/closes the supplied
database, starts workers, or owns a port.

`webhookConfig` requires `instanceId`, `source`, `eventTypes` and `outbound`.
Defaults: `enabled:false`, `maxAttempts:5`, `baseDelayMs:1000`,
`maxDelayMs:3600000`, `concurrency:4`, `retentionMs:2592000000`.
Outbound requires exact `allowedHosts`, `dnsTimeoutMs`, `connectTimeoutMs`,
`timeoutMs`, `maxRequestBytes` and `maxResponseBytes`. Example:

```ts
const config = webhookConfig({
  instanceId: "orders-partners",
  source: "orders-service",
  eventTypes: ["order.completed"],
  enabled: false,
  outbound: {
    allowedHosts: ["partner.example"],
    dnsTimeoutMs: 2000,
    connectTimeoutMs: 3000,
    timeoutMs: 10000,
    maxRequestBytes: 262144,
    maxResponseBytes: 16384,
  },
});
```

Use `createPinnedHttpsTransport(config.outbound)`. It performs fresh DNS validation
on each send, pins a public resolved address, preserves original Host/SNI and TLS
hostname verification, checks the actual peer before sending the body, and disables
redirects and connection reuse. HTTPS port 443 only; URL credentials, unsafe
addresses and any unsafe DNS answer are rejected. Exact host allowlisting is
mandatory. No environment proxy is used. Response bodies are counted then discarded;
no response text or arbitrary response headers enter attempts.

The transport targets Node-compatible networking, tested on Node 26 and Bun 1.4.2.
Workers/D1 are not implemented. A custom injected transport is a **trusted security
boundary**, not a reason to claim fetch has rebinding protection. A runtime lacking
pinned connection/TLS checks must use a host-controlled, policy-enforcing gateway
or refuse the deployment; ordinary fetch is not an equivalent substitute.

Supply keys through `keys.active(secretRef, scope)`, never DB secret bytes.
The resolver must authorize references within the persisted tenant/scope and
provide strong random key bytes (at least 32 bytes recommended).
Configure endpoints with `putEndpoint` and subscriptions with
`putSubscription`, under the host authority's `configure` permission. Publish only
`{type,data}`; it cannot override destinations, headers, credentials or occurrence
time. The service creates a version-1 envelope with UUID, event type, occurrence
time, configured source and finite JSON data. One publish supports at most 1,000
active matching subscriptions; larger fanout fails atomically.

Every caller method receives a separate trusted `{principal,scope}` context.
`authority.authorize` must authenticate/revalidate it and authorize the exact
tenant/scope/action on every call. Do not copy these fields from request JSON.
Lists, details, configuration and replay all use that policy and scoped SQL.
`execute`, `recover` and `prune` are host/worker-only entries, not business operations.
Manage defaults off, declares only status/attempt queries and replay, and needs
trusted context binding. Replay additionally declares approval/confirmation; its
shared service always requires replay permission and durable audit even outside Manage.

## Delivery semantics and lifecycle

An event and all initial deliveries commit together in the Webhooks repository.
Each delivery freezes URL, secret reference, endpoint revision and exact body.
Endpoint updates affect only later events, not queued deliveries or replays.
Attempt caps are frozen per delivery; runtime deadlines, retry delays and outbound
policy use the current service configuration. Removing an allowed host fails old
deliveries rather than retargeting them; removing an event type blocks new publish
and subscription configuration, not previously committed deliveries.
Disabling an endpoint or subscription stops subsequent claims and replay creation;
queued records become `failed` with `endpoint-disabled` or `unsubscribed` at claim.
A request already claimed may finish after disabling. Subscription identity cannot
be retargeted; create a new subscription. Re-enable before a permitted manual replay.

Transitions: `pending → running`; `running → succeeded | failed | retry`;
`retry → running`; disabled unsent `pending | retry → failed`.
Claim persists an attempt and a fenced lease before HTTP. Expired `running`
attempts become `lease-expired`, then `retry` or `failed` when attempts are exhausted.
Automatic retries preserve delivery and event identity; manual replay creates a
new delivery with `replayOf` and `auditIntentId`, never overwriting history.
Delivery/attempt queries are ascending cursor pages, at most 100 records.

Successful 2xx ends delivery. Timeout, connection failure, 429, 5xx and key resolver
unavailability retry within the attempt cap. Other HTTP statuses, redirects,
policy rejection and oversize responses fail permanently. Backoff is exponential
with half-to-full jitter; bounded integer/date `Retry-After` is capped by
`maxDelayMs`. Task payloads contain only delivery ID and scheduling generation.
Webhooks owns HTTP retry accounting; each Tasks job has one attempt.
Limits concurrency uses a real lease, never fail-open admission.
Hosts must keep worker clocks synchronized for delivery lease comparisons.

There is no exactly-once guarantee. The host's business transaction, event submit,
task enqueue and external HTTP are separate boundaries. **Commit business data
then publish** has a crash/loss window before publish; host reconciliation or its
existing transactional event mechanism must compensate. Repeated publish creates
new events. An uncertain DB commit can have persisted despite an error. An HTTP
receiver may have accepted bytes before response/DB failure, so redelivery is possible.
Only the receiver's event-ID dedupe plus business transaction can prevent its duplicate
business effects.

## Recovery, replay, retention and errors

Publish returns `eventId`, `deliveryIds` and `dispatch` (`queued`, `disabled`,
`recovery-required`). Queued means Tasks accepted work, not that HTTP succeeded.
Disabled still persists pending events/deliveries but never sends them.

Call bounded `recover({limit:100})` at startup and periodically through the host's
existing maintenance entry or Scheduler. It repairs the commit/enqueue gap,
failed/pruned Tasks jobs, expired delivery leases and deferred concurrency admission.
It checks Tasks deduplication status and leaves healthy pending/running jobs alone.
Only terminal/missing-status Tasks tombstones or expired delivery leases advance
scheduling generations; stale jobs cannot claim the new generation. Use a bounded
maintenance cadence (for example, 30 seconds), not a second dispatch loop.
No separate outbox platform or scheduler is introduced.

Replay persists a strict Audit intent before creating the new delivery and records
the outcome. Audit outcomes are not atomic with the Webhooks database. A
`replay-outcome-unknown` error carries the new delivery's `referenceId`; inspect
that record and its intent before repeating an operation. Recovery may dispatch a
committed replay whose audit outcome is unknown; the intent and relation survive.
Manage returns `status:"reconciliation-required"` with that same safe reference
instead of losing it through transport error sanitization. Do not blindly retry replay.

`prune` deletes bounded expired terminal deliveries/attempts and unreferenced events.
It never prunes active work and preserves replay parents until descendants are
pruned. Replay is unavailable after retention deletion. Endpoint/subscription records
remain for host review. Host owns backup/access controls for sensitive payloads.

`WebhookError.code` is a fixed structured diagnostic: invalid input/config,
unauthorized, not found, conflict, storage failure, disabled, audit failure or
unknown replay outcome. No driver error, key, full payload, URL or response body
is returned in status/attempts/errors. The package emits no payload-bearing logs.
Signing uses `v1` HMAC-SHA256 over `${timestamp}\n${eventId}\n` followed by the actual
raw body, with explicit key ID. Retries may refresh timestamp and active key, not
event/body identity. Keep old verification keys for the receiver's accepted time
window and in-flight requests; do not reuse a key ID for different key bytes.

Checks: run package `lint`, `typecheck`, `test`, `build`, then `bun pm pack`.
PostgreSQL tests require `WEBHOOK_TEST_DATABASE_URL` pointing at a disposable owned
database; absent configuration is reported as skipped, not integration success.
Controlled networking tests never send to real partners.
