# @lenso/realtime

Resource-authorized, bounded SSE subscriptions for ordinary async services and Fetch hosts.
This package is not a queue, a general event bus, an Agent token stream, or a WebSocket adapter.
Only `packages/realtime` is owned by this delivery. Core, Engine, Web, Manage, root configuration
and the shared lockfile are unchanged.

## Minimal use

```ts
import { createRealtime } from "@lenso/realtime";
import { createMemoryProvider } from "@lenso/realtime/memory";

// Execute during application setup, not when importing trusted configuration.
const realtime = await createRealtime({
  provider: createMemoryProvider(),
  async authorize(identity, resource, signal) {
    // Host-owned policy must verify the opaque principal/session and load the
    // actual resource. Do not treat subject/scope strings as proof of authority.
    const allowed = await policy.canRead(identity.principal, resource, signal);
    return allowed ? { validUntil: identity.expiresAt } : false;
  },
});
// Register realtime.close() with the host immediately after acquiring it.
const connection = realtime.connect(verifiedIdentity, { signal: requestSignal });
const subscription = await connection.subscribe({
  scope: verifiedIdentity.scope,
  type: "note",
  id: noteId,
});
const snapshot = await connection.snapshot(subscription, (signal) =>
  notes.read(verifiedIdentity.principal, noteId, signal),
);
// snapshot is returned to the caller, NOT implicitly sent through SSE.
// If !snapshot.stable, refetch rather than treating it as an atomic snapshot.
const response = connection.response();

// Trusted business code only, after its write has committed:
await realtime.publish(subscription.resource, "note.updated", { revision: 2 });
subscription.unsubscribe();
connection.close();
await realtime.close();
```

No authentication, User table, Organization dependency, HTTP publisher or automatic
operation exposure is introduced. Keep the instance/publisher in trusted service setup;
give entries a connection, not a publishing capability.

**Identity:** `{scope, subject, principal, expiresAt}` comes from a verified host entry.
`principal` is opaque host/Auth evidence, not business JSON. `subject` is an admission key,
not an authorization grant. The host defines `scope`: a single application, tenant,
workspace or another domain. **Resource:** `{scope, type, id}` is resolved by the server
from the actual record. A mismatch with the identity scope is rejected before authorization.
Each component is nonempty, at most 256 UTF-8 bytes, without Unicode normalization.
Topics are collision-free percent-encoded server constructions, never client channel names.
Provider access is privileged infrastructure access, not a tenant boundary.

**Subscription:** an opaque handle with `id`, immutable `resource`, `unsubscribe()` and
`renew()`. Another connection cannot use its handle for snapshot access. Reservations
count toward limits before async authorization; cancellation/revocation cannot resurrect
a late grant. A connection exposes subscribe/snapshot/iterate/response/close, not publish.
Use either one iterator or one Response per connection, not both.

## Authorization and lifetime

- The business authorizer runs at subscription and explicit renewal only. It returns
  `false` or an absolute `validUntil` timestamp. There is no per-message DB query.
- A grant expires at the earliest of the policy deadline, identity/session expiry,
  and authorization **start time** + `authorizationLeaseMs`. The maximum is 30 seconds,
  including time spent waiting for authorization. Slow grants cannot extend that bound.
- Hosts with ongoing streams call `handle.renew()` before expiry, using the same
  revalidating business policy. Renewal does not renew the session. Without renewal,
  the handle closes. A raw EventSource bridge without a renewal endpoint may simply
  close/reconnect and reauthorize every lease; do not silently lengthen leases.
- After committing an ACL change or deletion, call `revokeResource()` or `deleteResource()`.
  They immediately purge/abort local handles, then publish control to other instances.
  Remote delivery is best effort; a lost notice is bounded by the lease. Persisted
  policy must reject new subscriptions/renewals. Notices are not durable ACL storage.
- `revokeSubject(scope, subject)` is a local fast path for logout/session revocation.
  It does not claim distributed session cancellation. Other instances must revalidate
  the revoked session at renewal and stop no later than their last grant's deadline.
- **Revocation bound:** no new package delivery/dequeue after the grant deadline;
  idle subscription cleanup within deadline + `sweepMs` (default 250 ms, maximum 1 s),
  assuming a running event loop and correct policy. Thus at most 30 s of authorization
  and 31 s to reclaim an idle handle. Already emitted bytes cannot be recalled.
- `AbortSignal`, body cancellation, iterator return, unsubscribe, session expiry,
  resource revocation/deletion, provider failure and host close remove subscriptions,
  abort pending work, purge private queued events and settle blocked readers.
  Only fixed lifecycle diagnostics are emitted; no token, identity, private topic,
  backend exception or business payload is logged.
- Authorization/snapshot callbacks must cooperate with their signal and release their
  own resources. Their default deadline is 5 s. Timed-out underlying work retains a
  pending slot until it settles; the package cannot forcibly stop arbitrary JS/DB work.
  Shutdown aborts it but does not wait forever for an uncooperative callback.

## Wire contract and reconnect

Each UTF-8 SSE frame uses `event: <kind>`, optional `id: <cursor>`, and one JSON `data`
line containing:

```ts
{
  version: 1,
  kind: "ready" | "update" | "gap" | "closed" | "heartbeat",
  subscription?: string,
  cursor?: string,
  type?: string, // business event name, e.g. note.updated
  data?: Json,   // update invalidation metadata
  reason?: string
}
```

`ready` means authorized subscription and provider watermark established; it requires
a fresh authoritative snapshot, not that the application has received one. `update`
is an **invalidation hint**, not an ordered patch to apply blindly. `closed` names
`revoked`, `deleted`, `expired`, `shutdown` or `unsubscribed`. A connection-level terminal
`gap` names `overflow` or `provider`, and is followed by EOF. Per-handle gaps name
`sequence`, `generation`, `out-of-order`, `snapshot-race`, `reconnect` or `cursor-expired`.
The client closes its EventSource when intentionally unsubscribing or permanently denied;
native EventSource otherwise retries EOF. The first frame supplies configurable `retry`.
Custom clients should use jittered exponential retry, starting at `retryMs`, capped by
their host policy; the server/provider never automatically retry ambiguous publishes.

Payloads must be acyclic, plain finite JSON. No BigInt, undefined, Date/classes, accessors,
symbols, serialization hooks, non-enumerable properties, sparse/custom-property arrays,
NaN or Infinity. Event structure is limited to 32 levels and 10,000 visited values.
`maxPayloadBytes` bounds the serialized **ProviderEvent** (including kind/type), default
16 KiB, hard maximum 32 KiB. SSE metadata adds at most a 512-byte envelope budget and
a small framing overhead, below Web's default 64 KiB chunk budget. Provider wire limit
is 48 KiB. Mismatched publisher/receiver payload configurations fail the receiving
instance closed with a provider gap; use the same limits across the deployment.

**Cursor:** opaque `generation.sequence.issuedAt` detection token. Sequence is a
nonnegative safe integer, ordered only inside one resource/generation; generation is
reset on watermark expiry/provider state loss. Cursor age defaults to 10 minutes.
`issuedAt` is the observation/token issuance time, not the publication time. Different
instances observing the same generation/sequence may mint different token strings;
the watermark identity is generation + sequence, not string equality across instances.
Malformed, future-dated or older reconnect tokens produce `cursor-expired`. All other
reconnect tokens produce `reconnect`, even if they equal the current cursor.
Tokens confer no authorization and are never accepted as read positions.
There is **no persistent event log, event retention or replay**. No exactly-once claim.

Client consistency rules:

1. Subscribe first. The package's `snapshot(handle, read)` samples provider cursors
   before/after an authorized business read. Changed cursor => `stable:false` and
   `snapshot-race` gap. Refetch. Revoke/expiry during a read prevents its value returning.
2. For an ordinary HTTP snapshot endpoint, start consuming SSE first, mark dirty on
   every update/gap while the snapshot request is in flight, and refetch if dirty.
   Continue treating future updates as invalidations. Never fetch first and subscribe later.
3. On every reconnect, generation change, sequence gap, overflow, stale cursor or
   provider replacement: discard assumptions about incremental state and obtain a snapshot.
   Equal duplicate cursors are suppressed; older sequence events cause gap and are dropped.
4. `stable` means no **published** change during that read, not a transaction spanning
   the business DB and Redis. A write committed before delayed publication is eventually
   invalidated when its event arrives. Failed/missing publication cannot be inferred from
   the cursor. Hosts must preserve publish-after-commit ordering; if their product needs
   atomic reliable publication, an owned outbox/event log is a separate integration,
   not implemented here. Do not retry an already committed business write on publish failure.

## Configuration and limits

`resolveRealtimeConfig` is the same validator used by `/plugin` and direct async setup.
`createRealtimePlugin({id, requires, config, provider, authorize})` uses existing Core
`bindConfig`/`definePluginConfig`; config accepts values or ordered Core sources.
The provider factory executes in setup, not at config import. Cleanup is registered
before the next fallible step. Provider selection is host configuration:

```ts
provider: () =>
  settings.provider === "redis"
    ? createRedisProvider({ url: bindings.REDIS_URL, namespace: settings.namespace })
    : createMemoryProvider();
```

Import `/redis` only in a Bun host; Redis URL/credentials belong in trusted runtime
bindings, never manifests, JSON operations, logs or committed configuration. No new
environment/configuration system is created.

| Instance-local limit                                    |            Default |
| ------------------------------------------------------- | -----------------: |
| Connections / connections per scoped subject            |          1,000 / 8 |
| Subscriptions per connection / per scoped subject       |            16 / 64 |
| Subscribers per topic / active topics                   |      1,000 / 1,000 |
| Pending service operations                              |                 64 |
| Buffered events / encoded envelope bytes per connection |       64 / 131,072 |
| ProviderEvent payload bytes                             |             16,384 |
| Authorization lease / sweep interval                    |    30,000 / 250 ms |
| Heartbeat / SSE reconnect starting delay                |  15,000 / 2,000 ms |
| Cursor maximum age / callback timeout                   | 600,000 / 5,000 ms |

Limits are positive safe integers; unknown keys are rejected. `sweepMs` must not exceed
the lease. Heartbeat and retry intervals are configurable. Byte budget must be at least
payload limit + 512 bytes. One instance timer handles all leases and heartbeats.
Slow consumers never block publish: when either queue budget is exceeded, discard the
queue, retain one small terminal overflow gap, remove all references and disconnect.
Host/proxy/socket buffers are outside this package; configure their own limits and idle
deadlines. This package's admission counts are not distributed rate limits; a multi-node
deployment still needs host ingress/subject abuse controls.

## Providers and host matrix

**Memory:** independent per-provider Map, no global shared singleton and no cross-process
broadcast. Maximum 10,000 watermarks; inactivity TTL 10 minutes. Expired keys are reclaimed
on admission, or all on close; the map is always bounded. A current call/publication
refreshes metadata TTL. New provider/expired watermark means new generation. Delivery is
synchronous, once per active service callback in normal operation, with no persistence.

**Redis:** chosen instead of PostgreSQL LISTEN/NOTIFY or DO because standalone Redis
and Bun are available for real local testing; native Pub/Sub supplies broadcast, not
competing consumption. No new client dependency or lockfile change.

- One dedicated namespace subscription socket plus one command socket **per instance**,
  independent of browser connection count. All active topics share the subscription;
  each instance receives its namespace's messages and filters locally. Trusted instances
  sharing a namespace can see all its provider payloads, so scope is not a Redis ACL.
- Subscribe ACK before start resolves. Atomic Lua increments the per-topic watermark
  and publishes one delivery. The originating instance also receives it through Pub/Sub;
  no optimistic local update is duplicated.
- Standalone Redis endpoint only. No verified Redis Cluster, Sentinel failover, sharded
  topology or load-balanced Pub/Sub. Configure the same endpoint/namespace on all nodes.
  A Redis ACL must allow connection/subscription, EVAL, EXISTS/HSET/HGET/HINCRBY/PEXPIRE
  and PUBLISH. Use the host's Redis security/TLS/network owner, not client-side credentials.
- Normal connected delivery is ephemeral, best effort/at-most-once transport. Per-topic
  Lua sequence gives order at Redis; service detects duplicate/out-of-order delivery
  without promising exactly-once application effects.
- No offline queue or automatic reconnect. Any socket loss, command error/deadline or
  malformed wire is terminal, closes both sockets and notifies once. Replace the failed
  instance/provider, reconnect and snapshot. A failed/timed-out publish has unknown outcome;
  no retry or rollback. No events survive for a disconnected subscriber.
- Expiring metadata only, **not retained events**. `watermarkTtlMs` 600,000,
  `commandTimeoutMs` 5,000, `connectionTimeoutMs` 2,000, `maxPendingOperations` 64.
  Trusted publication controls the global TTL key cardinality; active-topic admission
  is instance-local. Production Redis memory/traffic quotas remain deployment-owned.

| Host/path                         | Memory                       | Redis                        | Validation                                                |
| --------------------------------- | ---------------------------- | ---------------------------- | --------------------------------------------------------- |
| Bun Fetch + Web raw hook          | Single long-lived instance   | Multi-instance               | Real HTTP SSE/cancel; real Redis subprocesses             |
| oRPC 2.0.0-beta.42 async iterator | Same service/iterator        | Same service                 | Existing Web path consumed/cancelled                      |
| Workers Fetch adapter             | Request-local lifecycle only | Unsupported Bun entry        | Local adapter test/browser bundle, no platform deployment |
| Other Fetch hosts (Node/Deno)     | Portable primitives          | Unsupported native Bun entry | Browser bundle only; no Node/Deno runtime claim           |

Workers currently assemble one application per request. A per-request memory provider
does **not** broadcast to another request/isolate; no useful distributed Workers provider
is delivered. No upgrade/WebSocket capability or DO binding/deployment is invented.
No PostgreSQL LISTEN connection or platform portability claim is made.

Web's raw hook can select an explicitly authorized resource route, obtain the exact
Realtime plugin instance, connect with `WebContext.signal`, register connection cleanup
with `WebContext.onCleanup`, subscribe, and return `connection.response()`. Returning
undefined leaves existing oRPC routes unchanged. Raw routes must independently enforce
Auth, method, Origin/Host and input policy. Web deadlines cover the entire SSE body;
omit `timeoutMs` for a long stream or intentionally allow expiry/reconnect. Do not put
a never-ending producer into `waitUntil` if it can only settle during cleanup.

oRPC already consumes an async generator: `try { for await (const event of connection)
yield event; } finally { connection.close(); }`, with its procedure signal and existing
Auth middleware. Its wire format is oRPC's format, not this package's raw SSE framing;
use its client and optionally `withEventMeta` for IDs/retry. No Web refactor is needed.

## Notes and Tasks

`examples/notes.ts` bridges an authorized ordinary async `NotesPort` into subscribe-first
snapshot and publish-after-update invalidation. The existing Notes service can implement
that port with its read/update audience-specific Auth actors; the port does not fabricate
an Auth actor from a subject string. No dependency on another parallel package is required.
`examples/run-notes.ts` is a runnable trusted in-process Notes fixture with two readers,
not a demo app or production login. `examples/tasks.ts` is a thin post-commit status hook;
it neither polls Tasks nor changes their queue contract.

```sh
bun run --cwd packages/realtime build
bun packages/realtime/examples/run-notes.ts
```

## Integration owner handoff

1. Landing integrates this manifest into the shared Bun lockfile and records a package
   changeset. Both candidate and clean-source frozen installs are verified without changing
   the lock hash or existing dependency resolutions. CI and release verification install
   Redis binaries so the real provider tests run. These preparation steps do not publish
   the package, prepare release versions or authorize deployment.
2. Select the provider factory/runtime bindings and existing Core config sources in the
   application, with exact `requires` references for Web/business dependencies. No new
   Core/Engine interface is needed.
3. Add application-owned raw or oRPC authorized ingress, snapshot transport and lease
   renewal policy. Hook committed Notes updates/deletes and ACL/session invalidation into
   the trusted service. Existing Auth must revalidate session and resource policy.
4. Manage can explicitly select finite `stats` or host-owned revoke operations via existing
   Operations/Manage declarations, audience binding and approval policy. This package
   intentionally exposes no stream, publisher, principal-bearing method or revoke route
   automatically. No Manage/core contribution or registry patch is necessary.
5. Configure proxy buffering/idle timeouts, ingress and distributed abuse limits. Validate
   production Redis topology/TLS/ACL and any Workers bindings separately. For atomic
   durable replay/publication, propose an owned event log/outbox with retention and recovery
   tests before changing these semantics.

## Verification evidence

The implementation measurements and checks below are local, not deployment/release approval. Environment:
Bun 1.4.2, macOS 27.0.1 / Darwin 27.0.0 arm64, Apple M2 Pro, Redis 8.10.2.
Tests start owned loopback Redis processes with persistence disabled and isolated temporary
directories, then stop/remove only those owned resources. If `redis-server` is absent,
integration explicitly skips and prints the missing executable; no simulated Redis pass.

| Command/check                                                            | Final result                                                                                                             |
| ------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------ |
| `bun run --cwd packages/realtime build`                                  | Passed, portable root/memory/plugin and Bun Redis JS + declarations                                                      |
| `bun run --cwd packages/realtime typecheck`                              | Passed, source/tests/integration/examples/measurement                                                                    |
| `bun run --cwd packages/realtime test`                                   | 50 passed, 0 failed, 303 assertions; Bun's `test` filter also includes Redis integration                                 |
| `bun run --cwd packages/realtime test:redis`                             | 12 passed, 0 failed, 180 assertions                                                                                      |
| `bun packages/realtime/examples/run-notes.ts`                            | Snapshot + the same note revision delivered to two readers                                                               |
| `node_modules/.bin/oxlint packages/realtime --deny-warnings`             | 0 errors, 0 warnings                                                                                                     |
| `node_modules/.bin/oxfmt --check packages/realtime`                      | Passed                                                                                                                   |
| `bun run --cwd packages/realtime measure`                                | Passed with the measurements below                                                                                       |
| `cd packages/realtime && bun pm pack --destination "$DELTA_SCRATCH_DIR"` | Local tarball produced, 18 files; no publishing                                                                          |
| Extracted tarball smoke                                                  | All four public JS/type export targets exist; imports, memory subscription/publication and packaged Notes example passed |

Packed `/plugin` smoke borrowed the already built local Core optional peer through a
temporary symlink. This is a local artifact/peer compatibility check, **not a registry
installation or release verification**. No package was uploaded. Examples import public
package entries and work without packing `src`.

Dependency builds actually run:
`bun run --cwd packages/lenso build`, `bun run --cwd packages/web build`,
`bun run --cwd packages/workers build`, `bun run --cwd packages/log build`,
`bun run --cwd packages/otel build`. All passed.

Affected host checks:

```sh
bun test packages/web/test/stream.test.ts \
  packages/web/test/openapi-stream.test.ts packages/web/test/bun.test.ts \
  packages/web/test/http.test.ts packages/workers/test/fetch.test.ts \
  packages/workers/test/rpc.test.ts packages/workers/test/config.test.ts
bun run --cwd packages/web typecheck
bun run --cwd packages/workers typecheck
```

40 host tests passed, 0 failed, 205 assertions; both host typechecks passed. The first Web
typecheck failed because `@lenso/otel/bun` declarations had not been built; it passed after
the Log/Otel prerequisite builds. Initial Realtime adapter tests exposed the oRPC initial
comment frame and incorrect test cleanup registration; final tests consume the actual wire
and use request cleanup correctly. Review caught and regression-tested near-node-limit
envelope serialization, deferred local control delivery during snapshots, and hidden
serialization hooks. All final checks above include those fixes.

Coverage includes allow/deny, cross-scope rejection/isolation, resource/session/subject
revocation, lease renewal/expiry and slow grants, two-reader fan-out, unsubscribe/abort/body
cancel/iterator return/host stop, bounded event/byte overflow, JSON/size limits, reconnect and
expired cursor, subscribe-first snapshot races and revocation during snapshots, provider
failure/startup rollback, duplicate/sequence/out-of-order/generation changes, pending
operation saturation, heartbeat/retry, and per-subject/topic/instance admission.
Redis evidence includes two distinct child PIDs receiving identical ordered provider
deliveries/cursors, plus two independently constructed Realtime instances receiving updates
and revocation through Redis. No shared in-process Map substitutes for Redis.
Provider restart recovery is a new instance + snapshot, not replay: server termination,
fresh/expired generations, and absence of late-listener history are tested independently.

### Bounded measurements

Command: `bun run --cwd packages/realtime measure` (sets
`BUN_CONFIG_MAX_HTTP_REQUESTS=1024`). One process sequentially tests 100 and 500 real
loopback Fetch/SSE connections, admitted in batches of 50, all simultaneously open for
measurement. One resource, one subscription and distinct subject per connection;
20 publications per scale, each with 256 ASCII payload-text bytes plus event metadata.
Clients await every read before the next publish. Timing is publish-start to each client's
read completion, including serialization, fan-out and loopback HTTP; no TLS/proxy/Redis.
Memory uses `process.memoryUsage()` after `Bun.gc(true)` before/after admission.

| Connections | Deliveries | Setup ms | p50 ms | p95 ms | Max ms | Baseline RSS bytes | Active RSS bytes | Baseline JS heap bytes | Active JS heap bytes |
| ----------: | ---------: | -------: | -----: | -----: | -----: | -----------------: | ---------------: | ---------------------: | -------------------: |
|         100 |      2,000 |    11.31 |  0.665 |  1.088 |  1.535 |         16,400,384 |       35,438,592 |                263,179 |           27,424,154 |
|         500 |     10,000 |    37.48 |  3.381 |  3.889 |  4.541 |         55,066,624 |       69,337,088 |                933,108 |          134,957,872 |

The second scale retains runtime allocations from the first; these deltas are not
per-connection estimates. Bun's JS heap accounting can exceed resident RSS; report the
two separately rather than treating either as allocated physical memory.

Slow-consumer run: 500 unread async iterables, one topic, 32-event cap including ready,
31 updates of 256 payload-text bytes fill queues. Full-GC JS heap rises from 2,686,292
to 10,538,758 bytes (queue delta **7,852,466 bytes**); RSS from 135,479,296 to 137,084,928.
The next publication produces **500 overflow gaps**, then connections/subscriptions/topics
are all **0**. It exercises the package buffer, not an arbitrary proxy/network send buffer.

The first benchmark attempt timed out at 120 s with default HTTP client concurrency;
another burst-admission attempt encountered ECONNRESET. The final runner sets explicit
client concurrency and batches admission rather than hiding these failures or retrying
missed updates. The final numbers above come from one complete successful final run.
They are a bounded local measurement, not production capacity/latency guarantees.

At implementation handoff, not run: root-wide build/typecheck/test/release verification, registry installation,
production deployment, Redis TLS/ACL/Cluster/Sentinel/proxy failure tests, sustained
distributed throughput/memory load, Node/Deno execution, or deployed Workers/DO runtime.
There is no persistent replay recovery test because no replay is promised or implemented.
Application Auth/ingress wiring remains integration-owner work; the package and examples
themselves are runnable and locally packed.

### Landing integration verification

The landing candidate adds the root workspace lock entry and manifest-matching peer
metadata, preserving every existing external dependency resolution. It also records a
Realtime changeset and installs Redis alongside PostgreSQL in Checks/release verification.
`bun install --frozen-lockfile` passes both in the candidate and in an archived clean
source copy without `node_modules` or `dist`; both preserve the lock hash.

`bash scripts/ci-checks.sh` passes locally, including root lint, format, build, typecheck,
test, release-script tests and the script's independently owned disposable PostgreSQL
fixtures. Realtime has 50 passing tests with all 12 real Redis integration cases executed.
No shared database or connection-string credential is used for those fixtures.

Local Core and Realtime archives are installed together in a standalone temporary consumer,
without workspace symlinks. TypeScript checks and runtime checks pass for all public imports,
Core plugin setup, subscribe/snapshot/publish/unsubscribe/cleanup, and the packaged Notes
example. This is not a registry installation or package publication.

The same bounded measurement command passes again during landing: 100/500 loopback SSE
connections have p95 1.272/4.018 ms respectively; the 500 unread connections again produce
500 overflow gaps and release all subscriptions. This second local sample does not replace
the implementation sample or extend its production/host guarantees.
