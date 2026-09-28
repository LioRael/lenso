# Knowledge settings: bounded local Workers bridge contract

Status: implementation interface, not target qualification. This contract is
specific to the reference knowledge base. It does not make arbitrary Component
imports, PostgreSQL, or deployed Workers available to other Plugins.

The private Host entrypoint and bridge implementation belong to the reference
Plugin's `workers-host/` sources in `lenso-examples`. They are selected through
the [generic, explicitly trusted integration input](../../crates/lenso-cli/docs/workers-host-integrations.md),
not a knowledge-base branch in Engine.

## Selected source and Component boundary

The reference knowledge base owns one Rust settings business-core module. Its
validation and HTTP result mapping are compiled into both the linked Native
Plugin and the Workers Component. The bounded Workers slice uses Plugin ID
and runtime package ID `lenso.reference.knowledge-settings`, with Plan Instance
key `lenso.reference.knowledge-settings/default`. This is a separate release
from the full linked `lenso.reference.knowledge-base` Plugin, whose Auth,
Secrets, and Jobs requirements the first Workers target cannot admit. Both
use the same settings business-core source and the same PostgreSQL rows; this
does not claim the full KB Plugin is portable. The Component is selected from
a locally verified V4/V6 Bundle by the Workers App builder. Its manifest and
selected Artifact digest are recorded in the Plan/build receipt and rechecked
against copied bytes. The generic Rust builder matches the operator-pinned
integration to the selected Plugin, Instance, Descriptor and Artifact. The
Plugin-owned JS Host enforces this private world's identities and exports
before a bridge call. The sidecar authenticates the local Bearer token and validates
only the bounded store operation; it does not inspect Plugin or Plan identity.
The selected HTTP Endpoint remains
`lenso.http.endpoint@1`, Descriptor version `1.1.0`, Descriptor digest
`sha256:701deedf705cb1a3b2f35fcae72f20ae85d46c6da6a008405a519018bbcdd3fe`,
with request operations `describe` and `handle` and ABI
`lenso.json-request@1`. No new Capability or generic database import is added.

The private Component WIT world is
`lenso:knowledge-settings-local@1.0.0/plugin`. It preserves the canonical
request ABI's synchronous `describe()` and `invoke(capability, operation,
request-json)` exports and adds only the private synchronous
`prepare-settings(request-json)` and `complete-settings(result-json)` exports.
The canonical `lenso:runtime@1.0.0/plugin` WIT and generic JS adapter are not
modified; their exact two-export admission remains intact. The private
Component has **zero imports**. In particular, there is no `wasi:http`,
PostgreSQL, or synchronous storage import. Pinned Jco 1.35.0 currently
transpiles with `--instantiation sync`; Workerd Fetch and the local PostgreSQL
bridge are asynchronous, so a synchronous WIT import could not truthfully
perform the database operation. The Worker Host awaits the bridge between two
pure Component calls. The generic import-free Worker entrypoint remains the
default for every Bundle without an explicitly selected integration.

`prepare-settings` accepts one of these bounded JSON values (UTF-8, at most
64 KiB):

```json
{"schema":"lenso.knowledge-settings-prepare.v1","route_id":"knowledge-base.settings.read"}
{"schema":"lenso.knowledge-settings-prepare.v1","route_id":"knowledge-base.settings.update","body":{"excerpt_limit":48,"predecessor_revision":1},"idempotency_key":"request-001"}
```

The read route returns `{"schema":"lenso.knowledge-settings-command.v1",
"kind":"read"}`. The update route returns
`{"schema":"lenso.knowledge-settings-command.v1", "kind":"cas",
"excerpt_limit":48,"predecessor_revision":1,"idempotency_key":"request-001",
"payload_sha256":"sha256:<64 lowercase hex>"}`. The key may be absent to
preserve existing Native `PUT /settings` behavior; a present key is 1-128
printable ASCII bytes and the hash covers the canonical validated command,
not the raw JSON. Invalid fields, values outside 16..=512, or an invalid key
return `Err(<JSON-serialized standard HTTP Endpoint HandleResponse>)` before
any store call. Its response is HTTP 400 with an RFC 9457-compatible Problem;
the specialized Worker Host validates and emits that response. An unexpected
error payload is an Endpoint failure, never a success. The key comes only from
the optional `Idempotency-Key` HTTP header; more than one header is rejected,
and a coalesced comma value is invalid. No user-supplied actor identifier or
credential enters this command.

`complete-settings` receives
`{"schema":"lenso.knowledge-settings-complete.v1", "route_id":
"knowledge-base.settings.read|knowledge-base.settings.update", "result":
<Host-validated store result>}`, never a raw credential. It returns a standard
HTTP Endpoint `HandleResponse` JSON for
success or a stable domain-error response for unauthorized, stale revision,
idempotency conflict, or unavailable storage. The Host must still validate its
output against the exact Endpoint response bounds. `invoke(describe)` declares
the two routes; `invoke(handle)` deliberately rejects with
`settings_host_binding_required`. Only the specialized Host's prepare →
authorized store call → complete path implements Endpoint handling. This
private two-step protocol is admitted only for the exact reference KB Bundle;
it is not generic Component/Workers Endpoint support, and the generic Worker
adapter never guesses it from route names.

## Async Host bridge

The local Worker Host owns a single `KnowledgeSettingsStoreV1` bridge. It
accepts only `read` and `compare-and-set` and a Bearer credential of 1..=4096
ASCII bytes without whitespace or comma from
the incoming request. The credential is never forwarded to the Guest or put
in generated build receipts. The bridge resolves it to a user actor under its
Host-owned local policy; an absent, invalid, or non-user actor fails closed.
Native continues to resolve the actor through its existing Auth Capability.
For the local acceptance only, the sidecar reads a 0600 Host-owned policy with
SHA-256 digests of two high-entropy tokens issued by the existing Auth Plugin
operator, mapped only to that disposable App's `user-a` and `user-b`. The
policy expires within one hour; raw tokens and the policy never enter the
Guest or generated Workers distribution. This is evidence of local actor
isolation, not a production Workers Auth Capability or general cross-target
identity integration.

The Worker-to-bridge messages are **loopback HTTP only** to the explicitly
configured `http://127.0.0.1:<port>` origin, never implicit network fallback:

```text
POST /v1/knowledge-settings/read
Authorization: Bearer <incoming opaque credential>
Content-Type: application/json
{"schema":"lenso.knowledge-settings-store.v1","request_id":"<uuid>"}

POST /v1/knowledge-settings/compare-and-set
Authorization: Bearer <incoming opaque credential>
Content-Type: application/json
{"schema":"lenso.knowledge-settings-store.v1","request_id":"<uuid>",
 "command":{"excerpt_limit":48,"predecessor_revision":1,
 "idempotency_key":"request-001","payload_sha256":"sha256:<64 lowercase hex>"}}
```

The bridge returns exactly one of:

```json
{"schema":"lenso.knowledge-settings-result.v1","kind":"ok","settings":{"excerpt_limit":48,"revision":2}}
{"schema":"lenso.knowledge-settings-result.v1","kind":"stale_revision"}
{"schema":"lenso.knowledge-settings-result.v1","kind":"idempotency_conflict"}
{"schema":"lenso.knowledge-settings-result.v1","kind":"unauthorized"}
{"schema":"lenso.knowledge-settings-result.v1","kind":"storage_unavailable"}
```

The sidecar binds only `127.0.0.1`, checks a loopback peer and its exact `Host`
header, rejects requests with an `Origin` header, bounds the body, and times
out database work after 900 ms. The JS Host admits only an explicit loopback
origin and disallows redirects on its outbound Fetch. The sidecar revalidates
the command rather than trusting
the Component-produced hash. `read` and CAS use the same disposable PostgreSQL
database and `knowledge_reference.settings` rows as the Native App. CAS and
the `(owner_id, idempotency_key, payload_sha256, outcome)` ledger write are
atomic in one PostgreSQL transaction. A repeated key with the same payload
returns the recorded result; a repeated key with a different payload returns
`idempotency_conflict`. A stale predecessor leaves the row unchanged. A
missing key preserves the prior CAS-only semantics and must not be retried
after an uncertain response. Native uses the same transaction function, not a
second interpretation of idempotency.

This is local-workerd qualification only. The first validation must prove
Native write, real local workerd read and CAS, Native readback, stale CAS,
same-key replay, changed-payload conflict, concurrent CAS, two-user
isolation, and restart persistence against one real PostgreSQL database. It
must record exact Rust/Examples/JS SHAs, Bundle and selected Artifact digests,
Plan digest, Jco version, workerd version, and the bridge origin policy. No
production Workers deployment or cloud database support is implied.
