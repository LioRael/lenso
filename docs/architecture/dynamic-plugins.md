# Dynamic Plugin control-plane contract

Status: proposed implementation contract for [ADR 0065](../adr/0065-govern-dynamic-plugins-above-the-kernel.md).

This contract becomes architecture authority only if ADR 0065 is accepted.
Until then, its vocabulary and mechanisms are provisional. Process Protocol
packages and Bun smoke tests are an implementation spike for the process/wire
slice; they do not claim that the Plugin Store, authority records, Generation
Supervisor, routing fences, recovery, Session fencing, or Agent Tool Plugin
vertical proof exists. Acceptance requires the complete proof below, not only
a successful child-process handshake.

Read this document when implementing or reviewing Plugin manifests, admission,
the Plugin Store, App Generation resolution and supervision, Process Plugin
execution, or the first Agent Harness Plugin slice. It defines the intended
contract; the current repositories do not yet implement the complete control
plane.

Read [Plugin execution classes](plugin-execution-classes.md) when adding an
Artifact variant, Host Execution Policy, Data contribution, Execution Adapter,
or product UI that presents execution support. That companion owns the
multi-execution branch contracts; this document remains the control-plane and
Process Protocol authority.

## Ownership map

| Concern | Owner | Completion criterion |
| --- | --- | --- |
| Plugin identity, Bundle admission, Store, locks, and Generation Specs | Lenso platform outside portable core | A precompiled App installs and switches an exact Plugin Release without changing its binary |
| Product categories, curation, configuration UX, and durable work provenance | Product repository such as Agent Harness | Product concepts remain absent from generic schemas and Kernel |
| Module graph, Capability bindings, lanes, and supervision policy | Resolved App Plan | Kernel receives one complete, immutable, validated Plan |
| Artifact materialization, process launch, wire, confinement, and host failures | Execution Adapter and host | Every Instance executes only its admitted bytes and Effective Host Grants |
| Execution-class allowance, deterministic variant preference, and support status | Product-owned Host Execution Policy | Resolution selects one exact implementation and runtime cannot fall back |
| Lifecycle, invocation, cancellation, readiness, and terminal outcomes | Kernel | No Plugin-specific branch or mutable registry is added |

Logical ownership does not assign repositories. ADR 0064 still requires a
durable Interface, owner, release cadence, and conformance surface before a new
repository is created.

## Authority chain

```text
Plugin Bundle
  -> Plugin Manifest + Product Plugin Metadata + Artifacts
  -> Admission Receipts + content-addressed Plugin Store
  -> App-local Plugin Set Lock
  -> Resolved Artifact Set + Effective Host Grant Set

Product build -> Host Build Manifest
Product policy -> Host Execution Policy

Plugin Set Lock + Resolved Artifact Set + Effective Host Grant Set
  + Host Build Manifest + Host Execution Policy
  -> Resolved App Plan
  -> App Generation Spec

active Generation Spec + candidate Generation Spec
  + State Compatibility Receipts when required
  -> App Generation Transition Spec
  -> configured Execution Adapter catalog
  -> Runner -> Kernel
```

Every arrow is fail closed. No later stage may discover a missing contribution,
select a newer version, broaden a grant, change an Artifact, or repair an
ambiguous binding.

### Canonical document rules

All authority documents use versioned, strict UTF-8 JSON with `snake_case`
fields. Decoders reject unknown and duplicate fields. Authority values use
strings, booleans, bounded arrays, and non-negative integers; wide integers use
decimal strings. A schema may declare a field nullable; otherwise `null` is
rejected. Floating-point values are excluded.

For digesting, objects are recursively sorted by key and encoded as compact JSON.
Each schema defines the sort key for every semantically unordered array; input
with duplicates or a non-canonical order is rejected rather than silently
reinterpreted. Digests use `sha256:` followed by 64 lowercase hexadecimal
characters. A document never embeds its own digest.

Paths inside a Bundle are normalized relative paths. Admission rejects absolute
paths, empty segments, `.` or `..`, platform-prefix ambiguity, duplicate paths,
special files, and symlink or hard-link escape. Artifact size and digest are
verified before Store commit and again before staging.

### Plugin Manifest: `lenso-plugin.json`

The Plugin publisher owns one platform-neutral Manifest per immutable Plugin
Release. Its canonical digest is the content-identity component of the
`PluginReleaseRef` tuple `(plugin_id, release_version, manifest_digest)` used by
every later document.

| Field | Meaning |
| --- | --- |
| `schema_version` | Exact Manifest schema version; V1 is `1` |
| `plugin_id` | Stable reverse-domain-style Plugin identity |
| `release_version` | Semantic version of this immutable Release |
| `artifacts[]` | Release-local ID, kind, digest, size, media type, Bundle path, and target constraints |
| `module_contributions[]` | Stable logical contribution ID, package ID, configuration-schema reference, provided/required Capability descriptors, and one or more implementation variants |
| `data_contributions[]` | Stable inert contribution ID, Artifact reference, media type, exact content-schema identity and digest, and Product Metadata reference |
| `features[]` | Named optional sets of Module contributions, Data contributions, Artifacts, and permission requests |
| `binding_templates[]` | Bindings only between contributions inside this Manifest; cross-Plugin bindings remain App Composition decisions |
| `permission_requests[]` | Stable ID, resource kind, required/optional status, requested scope, and explanation key |
| `product_metadata[]` | Namespace, schema identity, relative path, and digest of product-owned metadata |

Capability declarations reference exact Descriptor versions and canonical
Descriptor digests; the Manifest does not copy or reinterpret their Schemas.
Each Module implementation variant has a Release-local ID, exactly one
execution input, entrypoint, Execution Class, target constraints, required
Protocol Profiles, and support channel. An execution input is either an
Artifact reference or an exact built-in factory reference declared by the Host
Build Manifest; a built-in reference installs no new machine code. Variants of
one logical contribution expose the same Capability and configuration
Interface. They are alternatives, not
separately enabled implementations, and publishers do not assign runtime
preference.

Data contributions declare no entrypoint, Execution Class, Capability, or
permission request. They remain inert until an App-local product resolver binds
one to an explicitly selected interpreter Module Instance and named input slot.
Generic admission verifies bytes and schema identity; the product validates the
content schema and interpretation contract.

Package dependencies may acquire content, but runtime dependencies remain
Capability requirements. Manifest permission requests are not grants. Secret
values, credentials, download tokens, machine-local paths, and signatures are
absent.

Product metadata is a separately digested file. For example, Agent Harness may
own `lenso-agent-plugin.json` for Tool, Prompt, or Model curation without adding
those terms to the generic Manifest. Generic admission bounds the metadata file
and verifies its digest; the product validates its schema.

Publisher signatures and provenance use detached envelopes over the Manifest
digest and referenced Artifact digests. A signature never self-authorizes a
Release: the operator's admission policy produces the Admission Receipt.

### Plugin Set Lock: `lenso-plugin-set.lock.json`

The App resolver owns this canonical, App-local selection. Disabled and merely
installed Plugins are absent.

| Field | Meaning |
| --- | --- |
| `schema_version` | Exact lock schema version |
| `app_id` | App identity that owns the selection |
| `plugins[]` | Plugin ID, Release version, Manifest digest, selected Features, and Product Metadata digests |
| `instances[]` | Module contribution reference, final App-local Instance key, optional exact implementation-variant pin, non-secret configuration or secret references, and placement input |
| `data_mounts[]` | Data contribution reference, explicit interpreter Instance key, product-owned input slot, and interpretation-schema digest |
| `approved_grants[]` | Permission reference, narrowed approved scope, intended enforcement kind, and intended enforcer identity |

One lock selects at most one Release per Plugin ID. One contribution may produce
multiple keyed Instances. A variant pin is exact; absence delegates only to the
bound Host Execution Policy. Every Data contribution mount names one ordinary
Module Instance already selected in App Composition; it creates no hidden
Instance or binding. Plugin, Instance, Feature, mount, slot, and permission
references are unique and close over installed, admitted Manifests. An approved
scope is equal to or narrower than its request; a missing required request
fails resolution. `trust_review_only` remains visibly distinct from
enforcement.

### Admission Receipt

Admission produces an immutable receipt keyed by its own digest. It records the
policy identity, policy version or digest, Manifest and Artifact digests,
signature or explicit-local-trust provenance, decision, operator or security
principal, and bounded decision evidence. Credentials and secret material are
excluded. A local trust receipt is exact to one Release; it is not a wildcard
trust rule for future versions.

### Host Build Manifest: `lenso-host-build.json`

The product build owns this immutable account of non-Plugin execution inputs.
Its canonical digest prevents one Generation Spec from producing different
behavior when presented to different host binaries or Adapter catalogs.

| Field | Meaning |
| --- | --- |
| `schema_version` | Exact Host Build schema version |
| `app_id` | Product identity this host can run |
| `host_executable` | Content digest, size, target, and product build identity |
| `built_in_modules[]` | Package ID, product-build revision, factory identity, execution class, and Capability Descriptor digests |
| `fixed_artifacts[]` | Product-owned Process or static Artifact digest, size, target, and admitted entrypoint |
| `adapter_profiles[]` | Execution class, Adapter build identity, supported target constraints, and exact Protocol Profiles |

The manifest contains no machine-local paths or mutable discovery results. At
staging, the Supervisor hashes the running executable and any separately loaded
Adapter or fixed Artifact bytes, then constructs a catalog containing exactly
the declared factory and profile identities. A mismatch fails before Runner
startup. The final Plan still decides which available built-in inputs are used.

### Host Execution Policy: `lenso-execution-policy.json`

The product owner signs or otherwise admits this canonical policy independently
of Plugin publishers. It determines which installed execution mechanics may be
selected on one host target; it does not add an Execution Class to the host.

| Field | Meaning |
| --- | --- |
| `schema_version` | Exact policy schema version |
| `app_id` | Product identity governed by the policy |
| `host_build_manifest_digest` | Exact host and Adapter catalog to which the policy applies |
| `target` | Exact operating system, architecture, ABI, and host feature set |
| `classes[]` | Execution Class, allowed support channels, required isolation floor, allowed trust level, and exact Protocol Profiles |
| `preference[]` | Ordered Execution Class IDs used only when an Instance does not pin an implementation variant |
| `instance_overrides[]` | Optional exact Instance or contribution rule with a narrower allowed class set and deterministic preference |

Preference order is product policy, never publisher metadata and never an
implicit claim that one class is fastest. Resolution filters variants by exact
target, Host Build support, policy allowance, required Profiles, grants, and
support channel. It then honors an exact App-local variant pin or selects the
first class in the applicable policy preference containing exactly one valid
variant. Zero matches and ambiguity within one preference rank fail resolution.

The decision records the policy digest, chosen variant ID, chosen Artifact,
Execution Class, matched target, support channel, and selection reason. Staging
does not retry another variant when Artifact verification, Adapter preparation,
readiness, or execution fails. Another choice requires a new resolution,
Artifact Set, Generation Spec, and Transition.

### Resolved Artifact Set: `lenso-artifacts.lock.json`

Admission, Store, and resolution jointly produce the exact Artifact authority
for one Plugin Set Lock.

| Field | Meaning |
| --- | --- |
| `schema_version` | Exact Artifact Set schema version |
| `plugin_set_lock_digest` | Exact App-local Plugin selection |
| `host_execution_policy_digest` | Exact product policy used for variant selection |
| `releases[]` | Plugin ID, Release version, Manifest digest, and Admission Receipt digest |
| `artifacts[]` | Manifest and Artifact IDs, content digest, size, media type, and selected target |
| `instances[]` | Instance key, contribution and implementation-variant references, exact Artifact or built-in factory reference, entrypoint, Execution Class, target, support channel, selection reason, exact Profiles, and bounded per-instance limits |
| `data_mounts[]` | Data contribution and Artifact references, interpreter Instance key, input slot, content-schema digest, and product interpretation-schema digest |

Each selected Instance appears exactly once. Manifest, contribution, execution
input, variant, entrypoint, execution class, target, policy, and Admission
Receipt references must all close. An Artifact input closes over admitted Store
bytes; a built-in input closes over one exact Host Build factory. Every selected
Data contribution mount closes over one admitted Artifact and one Plan-owned
interpreter Instance; it does not create a Kernel endpoint or execution input
of its own. Absolute Store paths do not enter canonical authority. At staging time,
the host resolves each digest to a safe read-only directory handle or
materialized root and injects that non-serializable handle into the Adapter.
Adapters never receive a mutable Store handle.

### Effective Host Grant Set: `lenso-host-grants.lock.json`

This App-local document is the resolver's exact account of what can actually be
enforced for the selected Instances.

| Field | Meaning |
| --- | --- |
| `schema_version` | Exact grant schema version |
| `plugin_set_lock_digest` | Selection whose approvals are being realized |
| `grants[]` | Instance key, permission reference, narrowed scope, enforcement kind, enforcer identity, and bounded non-secret enforcement configuration |

Enforcement kinds are `capability`, `module`, `adapter`, `host`, and
`trust_review_only`. The last kind authorizes trusted execution but claims no
confinement. Every grant names its enforcer; a required request without an
effective grant fails staging. The document contains no secret values. Secret
references remain in App Composition and values are resolved through a Secrets
Capability owned by the consuming Module. The named enforcer derives
machine-local handles from the inline canonical configuration during staging;
there is no dangling configuration digest or mutable lookup.

### State Compatibility Receipt

This detached, product-owned receipt authorizes one specific replacement edge;
it is not a publisher self-assertion.

| Field | Meaning |
| --- | --- |
| `schema_version` | Exact receipt schema version |
| `app_id` | App whose replacement edge is authorized |
| `module_instance_key` | Stateful Instance governed by the receipt |
| `old_runtime_identity` | Exact Plugin Release and Artifact digests, or product-build identity |
| `new_runtime_identity` | Exact Plugin Release and Artifact digests, or product-build identity |
| `state_schema_id` | Product-owned durable-state schema identity |
| `compatibility` | Exact booleans for concurrent read, concurrent write, and old-code readability of new writes |
| `policy_digest` | Product compatibility policy used for the decision |
| `evidence_digest` | Immutable test, migration, or review evidence |
| `decision_authority` | Product authority that accepted the evidence |

Stateless Instances need no receipt. A receipt for one Release pair, schema, or
direction cannot authorize another. Only an accepted receipt is included in an
App Generation Transition Spec.

### App Generation Spec: `lenso-generation.json`

The Generation Supervisor owns this immutable node authority:

```json
{
  "schema_version": 1,
  "app_id": "agent-harness",
  "host_build_manifest_digest": "sha256:...",
  "host_execution_policy_digest": "sha256:...",
  "resolved_plan_digest": "sha256:...",
  "plugin_set_lock_digest": "sha256:...",
  "resolved_artifact_set_digest": "sha256:...",
  "effective_host_grant_set_digest": "sha256:..."
}
```

The SHA-256 of canonical Spec bytes is the Generation identity. Routing Epoch,
predecessor, and rollout policy are mutable or edge-specific state and are not
embedded in the Generation Spec.

### App Generation Transition Spec: `lenso-transition.json`

This immutable edge authority prevents compatibility evidence or rollout policy
from being reused with a different active Generation:

```json
{
  "schema_version": 1,
  "app_id": "agent-harness",
  "from_generation_spec_digest": "sha256:...",
  "to_generation_spec_digest": "sha256:...",
  "replacement_mode": "overlap",
  "state_compatibility_receipt_digests": [],
  "rollout_policy": {
    "ready_timeout_nanos": "30000000000",
    "drain_timeout_nanos": "30000000000",
    "rollback_window_nanos": "60000000000",
    "automatic_rollback_on_generation_failure": true
  }
}
```

For initial boot, `from_generation_spec_digest` is `null`, `replacement_mode` is
`initial`, receipts are empty, and automatic rollback is false. `overlap` keeps
the old Generation alive while staging and requires a receipt for every changed
stateful Instance. `maintenance` stops the old Generation before staging,
requires a zero rollback window, and cannot automatically roll back.

Staging loads every document by digest, validates it independently, and then
checks closure across documents:

1. App identities match across every document.
2. The running executable, built-in factories, fixed Artifacts, Adapter builds,
   targets, and Protocol Profiles exactly match the Host Build Manifest; the
   Host Execution Policy binds that exact Manifest and App.
3. Selected Features expand to one exact contribution, Artifact, permission,
   and Product Metadata set; unselected optional content cannot enter the Plan.
4. Every selected Module contribution maps to exactly one execution-input-backed
   Plan Instance, one implementation variant selected under the bound policy, and
   every Plugin-backed Plan Instance maps back. Every selected Data contribution
   maps to one admitted Artifact and explicit Plan-owned interpreter slot.
5. Package ID, variant, entrypoint, execution class, Capability Descriptor
   digests, canonical configuration and secret references, Instance keys,
   selected execution inputs, Data mounts, interpreter slots, and lane placement agree
   across the lock, Artifact Set, policy, and Plan.
6. Every selected intra-Plugin binding template maps to the same final explicit
   Plan binding; every cross-Plugin or built-in binding is an App Composition
   decision, and no undeclared binding appears.
7. Every Admission Receipt covers the Manifest and Artifact digests it admits.
8. Every required permission has an equal or narrower approved and effective
   grant with truthful enforcement.
9. Every non-Plugin Instance closes over an exact built-in factory or fixed
   first-party Artifact in the Host Build Manifest.
10. Plan validation, configuration schemas, explicit bindings,
    execution-class availability, and target matching all pass.
11. The host resolves each Artifact digest to a safe materialized root and
    builds a generation-local Adapter catalog.
12. The Transition `from` digest equals the durably active Generation under the
    fenced Routing Epoch; `to` equals the staged Generation. Every required
    pairwise State Compatibility Receipt closes over those exact runtime
    identities and the declared replacement mode.
13. All lanes prepare and become ready within the Transition deadline before
    the route can switch.

The existing Plan `package_revision` remains an opaque correlation value. It may
match a Release version or Artifact reference, but it is not treated as a
universal digest or Artifact authority.

The existing canonical Plan reader currently reconstructs the default `main`
lane and therefore cannot reload a non-default complete Lane Set faithfully.
Preserving and canonical-checking declared lanes is an implementation
prerequisite; the Supervisor must not work around it with per-lane partial
Plans.

## Store lifecycle

### Install

Installation validates the strict Manifest and Product Metadata, verifies every
path, size, and digest, evaluates signature or explicit local trust, writes an
Admission Receipt, and atomically commits bytes to a content-addressed Store.
It neither enables the Plugin nor changes an App Generation.

### Update and enable

V1 installs new Releases beside old Releases and performs no automatic update.
Enablement explicitly rewrites one App-local Plugin Set Lock, resolves a new
Plan and Generation Spec, then creates a Transition from the exact active
Generation before staging. Bundled and user-installed Releases obey the same
explicit selection; neither implicitly overrides the other.

### Disable, uninstall, and garbage collection

Disable removes a Plugin from a future App-local lock. Uninstall is refused
while any saved lock, staged, active, draining, standby, or rollback reference
uses the Release and reports all blockers. Once unreferenced, Store registration is
removed and the exact content-addressed directory becomes garbage-collection
eligible. Destructive cleanup first moves the resolved Store-owned target into
quarantine or trash; it never accepts arbitrary user paths, globs, unresolved
variables, or symlink targets.

## App Generation protocol

The forward lifecycle is `staged -> ready -> active -> draining -> standby ->
retired`. `standby` is omitted when the Transition has no rollback window. A
rollback may perform the explicit reverse edge `standby -> active` under the
rules below. Durable health is an orthogonal `healthy | failed` value, not a
lifecycle state. The control record binds lifecycle, health, Generation and
Transition Spec digests, activation direction, and Routing Epoch. Transitions
use compare-and-set under a fenced Supervisor lease.

A staging or readiness failure marks health `failed`, cleans up the candidate,
and moves `staged | ready -> retired` without changing the active route. A
failure after activation retains lifecycle long enough for fenced rollback or
drain; cleanup never requires a fictitious `failed` lifecycle edge.

One Generation includes the complete Plan Lane Set. Every lane must become ready
within the Transition deadline before one epoch advance atomically selects the
Generation. The active Generation must still equal the Transition `from` digest
at that compare-and-set. A router acquires a Generation Lease for a product work
unit at admission; work never migrates. Agent Harness maps one Turn to one Lease.

After switching, the old Generation receives no new Leases. Drain waits until
all existing work, nested Tool and Model calls, Streams, and final durable
commits terminate. At the configured deadline, the Supervisor cancels remaining
work and records explicit terminal outcomes. Once no Leases remain, a Generation
with a nonzero rollback window stays process-live and non-routable in `standby`;
otherwise the Supervisor requests Kernel shutdown immediately. At rollback
window expiry, it shuts down and retires the standby Generation. Uncertain calls
are not replayed.

Supervisor recovery reads the durable staged, ready, active, draining, standby,
and retired records plus their health before admission. A monotonically
increasing epoch fences old routers and Supervisors. Recovery reconciles actual
processes with durable state and never infers authority from the newest files in
the Store.

### Stateful Modules

Durable state remains Module-owned and is shared only when the exact Transition
Spec binds accepted old/new compatibility receipts. Agent Harness allows one
active Turn per Session across Generations, enforced by durable compare-and-set
or a fenced Session lease. Generation-local locks are insufficient.

Zero-downtime state evolution uses expand/contract. Destructive or one-way
migration uses maintenance restart and disables automatic rollback. Rollback
changes code routing only; it does not undo durable writes or replay work.
Within an overlap Transition's bounded window, an operator may roll back to the
standby Generation. V1 automatic rollback is narrower: it occurs only when the
candidate Generation itself becomes terminally failed and the Transition sets
`automatic_rollback_on_generation_failure`. It advances the Routing Epoch,
reactivates the complete standby Generation, and compare-and-sets the
candidate's lifecycle from `active` to `draining` while retaining its `failed`
health through cleanup and retirement.

Rollback is a restricted reverse operation of the original Transition, not a
new ordinary Transition. Its compare-and-set requires the same Transition
digest, active Generation equal to its original `to`, standby Generation equal
to its original `from`, an unexpired rollback window, accepted receipts proving
old-code readability of new writes, and the current fenced epoch. The durable
control record and product provenance set `activation_direction` to `rollback`.
Recovery may complete that exact reverse operation but may not synthesize a new
edge or reuse the receipts with another Generation pair.

## Generic Process Protocol V1

The language-neutral Process Protocol belongs with `lenso-protocols`; concrete
Execution Adapters own process, loopback transport, and host-failure mechanics.
The existing Bun wire is the semantic and conformance baseline, not the public
protocol source. The normative package must publish strict JSON Schemas,
canonical examples, idiomatic Rust and TypeScript types, and one locked
cross-language conformance gate that mechanically detects structural, proof,
and validation drift. Schema-to-language generation is permitted but is not an
end in itself when it produces a second validation authority. The legacy Bun
SDK wire is not V1-compatible.

The base profile is `lenso-process-jsonrpc-http-v1` and the portable value
profile is `lenso-json-value-v1`. Transport never negotiates or falls back to
stdio. `provide-request-v1` is the first mandatory interaction profile.
`consume-request-v1`, `stream-v1`, and `event-v1` are exact-selected optional
profiles and fail handshake until the SDK, Adapter, and bidirectional
conformance suite all support them.

One OS child process serves one Module Instance generation. A restart creates a
new process and session from the same admitted Artifact digest, Protocol
Profiles, limits, and Effective Host Grants. Any byte, profile, limit, or grant
change requires a new App Generation.

### HTTP and JSON-RPC envelope

The child exposes two HTTP/1.1 listeners on distinct ephemeral loopback ports:
data uses `POST /rpc`; handshake, cancel, and shutdown use `POST /control`. The
control listener has its own reserved accept, decode, admission, and handler
capacity that is never counted against data concurrency or queues. Data work
cannot delay admission of a control request.

Requests and responses use `Content-Type: application/json`; batches,
notifications, redirects, compression, cookies, and alternate paths are
forbidden. A valid JSON-RPC response uses HTTP 200. A conforming server returns
404 for another path, 405 for another method, 413 before decoding an oversized
body, and 415 for another media type. Parse and JSON-RPC envelope errors use the
standard `-32700`, `-32600`, `-32601`, and `-32602` codes. These responses make
malformed external traffic deterministic; if the Adapter receives any non-200
status or JSON-RPC error for its own authenticated request, it retires the
process generation for Protocol Violation. The same is a startup/auth failure
before authentication.

Every JSON-RPC request has this exact envelope shape:

```json
{
  "jsonrpc": "2.0",
  "id": "42",
  "method": "lenso.process.v1.request",
  "params": {}
}
```

Protocol schemas reject unknown and duplicate fields. IDs are non-negative
decimal strings with no leading zero except `"0"`. Params are one object, not a
one-element array. Every successful result echoes the authenticated `session`;
a response ID must equal its request ID. Application outcomes are result values,
never JSON-RPC errors.

| Method | Listener | Required params | Result |
| --- | --- | --- | --- |
| `lenso.process.v1.handshake` | control | Exact identities, profiles, peer limits, nonces, and host proof | Exact accepted identities, session, and child proof |
| `lenso.process.v1.request` | data | Session, correlation ID, Capability and operation identity, context, timeout, extensions, and payload | Session, correlation ID, and one success, Domain Error, or allowed Runtime Failure |
| `lenso.process.v1.cancel` | control | Session and correlation ID | Session and `accepted: true` |
| `lenso.process.v1.shutdown` | control | Session | Session and `accepted: true` after admission closes |

For `request`, the JSON-RPC ID must equal `correlation_id`. Control requests use
their own monotonically increasing decimal-string IDs. Correlation and control
IDs are never reused within one process session.

### Startup and bootstrap authentication

Before executing the entrypoint, the Adapter creates a dedicated process group
or platform job object. It then starts the exact admitted entrypoint without a
shell, with a sanitized environment, controlled working directory, and closed
inherited handles except explicitly selected bootstrap and readiness handles.
Stdout and stderr are bounded untrusted logs, never protocol channels.

The host generates a 256-bit bootstrap secret and 256-bit host nonce with a
cryptographic random source. The secret passes only through a one-way inherited
pipe or equivalent handle: never argv, environment, a filesystem path, the
readiness record, or logs. The child reads it once and closes the handle. On a
separate readiness handle it writes exactly one bounded UTF-8 JSON line with
`protocol`, decimal `data_port`, and decimal `control_port`, then closes that
handle.

Handshake params include exact Protocol and value profiles, Module Instance key,
Module generation, Generation Spec, Artifact, and Effective Host Grant Set
digests, canonical-sorted arrays of every provided Capability Descriptor and
operation, any selected outbound binding Descriptor, peer-confirmed limits, the
host nonce, and `host_proof`. A Module may provide more than one Capability.

Proof inputs use the protocol package's exact `jcs-rfc8785-v1` byte codec, with
floating-point values forbidden by schema. `handshake_params_digest` is the raw
32-byte SHA-256 of RFC 8785 canonical UTF-8 params with `host_proof` omitted.
The nonce and session fields encode exactly 32 raw bytes as base64url without
padding.

`host_proof` encodes the 32 raw output bytes of:

```text
HMAC-SHA-256(
  secret,
  ASCII("lenso-process-host-v1") || 0x00 || handshake_params_digest
)
```

The child verifies it, generates a session, and returns the exact accepted
fields plus `child_proof`, which encodes:

```text
HMAC-SHA-256(
  secret,
  ASCII("lenso-process-child-v1") || 0x00
    || handshake_params_digest || raw_session_bytes
)
```

There is no textual concatenation or platform newline. Both sides erase the
bootstrap secret after the first handshake attempt. Failure, a second attempt,
timeout, or process exit kills and retires the process generation;
authentication never retries in place. Normative proof vectors are a required
conformance gate.

Compatibility lint runs during admission and update analysis. Runtime handshake
still requires exact Descriptor content and sorted operation tables; it never
selects a nearby compatible version. The session is attached to every later
params and result object and remains valid only for that process generation.

### Bounded resource policy

The Resolved Artifact Set selects one `process_limits` object per Instance. A
publisher may request resources, but product admission narrows them and the
profile hard maxima cannot be exceeded.

| Limit | V1 default | V1 hard maximum | Authority |
| --- | ---: | ---: | --- |
| `max_http_body_bytes` | 65536 | 1048576 | peer-confirmed |
| `max_control_http_body_bytes` | 16384 | 65536 | peer-confirmed |
| `max_concurrent_requests` | 32 | 256 | peer-confirmed |
| `child_request_queue_capacity` | 32 | 1024 | peer-confirmed |
| `max_retired_correlation_ids` | 65536 | 1048576 | peer-confirmed |
| `host_dispatch_queue_capacity` | 64 | 4096 | host-only |
| `control_queue_capacity` | 32 | 256 | peer-confirmed |
| `readiness_record_bytes` | 4096 | 65536 | host-only |
| `stdout_capture_bytes` | 1048576 | 16777216 | host-only |
| `stderr_capture_bytes` | 1048576 | 16777216 | host-only |
| `startup_timeout_nanos` | `"30000000000"` | `"300000000000"` | host-only |
| `cancel_ack_timeout_nanos` | `"1000000000"` | `"10000000000"` | host-only |
| `shutdown_ack_timeout_nanos` | `"5000000000"` | `"30000000000"` | host-only |
| `shutdown_exit_timeout_nanos` | `"5000000000"` | `"30000000000"` | host-only |
| `term_grace_nanos` | `"5000000000"` | `"30000000000"` | host-only |
| `kill_reap_timeout_nanos` | `"5000000000"` | `"30000000000"` | host-only |

Counts and byte sizes are JSON integers; nanosecond values are decimal strings.
The handshake confirms every peer-confirmed value exactly and never negotiates.
The host enforces all values, including peer-confirmed ones, and rejects a child
that advertises another value. `max_http_body_bytes` applies independently to
encoded data requests and responses; the control limit applies independently in
both directions. The Adapter rejects an oversized outbound data invocation
locally as `ResourceExhausted` without sending it. An oversized response, an
oversized outbound control message, or any 413 received for an Adapter request
is a generation-fatal Protocol Violation.

Host dispatch saturation returns `ResourceExhausted` for that invocation. Child
request saturation may return the same allowed Runtime Failure. The independent
control listener always reserves at least one handler and its full declared
queue even when data admission is saturated. Control queue saturation or a
missing cancel acknowledgement at `cancel_ack_timeout_nanos` prevents truthful
cancellation and therefore retires the process generation.

Both peers retain completed correlation IDs to reject reuse. Before retaining
one more ID would exceed `max_retired_correlation_ids`, they close data
admission and retire the process generation; IDs are never silently evicted or
reused. The Adapter reports a stable `retired_id_capacity` host diagnostic and
exposes `ModuleFailure` so Kernel supervision may create a fresh process
generation.

### Invocation, deadlines, and cancellation

Request params carry the session, opaque decimal-string correlation ID,
Capability ID, Descriptor version and digest, operation, interaction kind
`request`, caller Module Instance, required `remaining_timeout_nanos`, portable
invocation extensions, and payload. `remaining_timeout_nanos` is either `null`
for no deadline or a decimal string. The Adapter derives it immediately before
writing the HTTP request, after all host queueing, from the Driver-monotonic
deadline. If the derived value is `"0"`, it commits `DeadlineExceeded` locally
and sends no request. It does not queue a nonzero request again after that
calculation. The child starts a cooperative timer before admitting the request
to its own queue, but timer expiry only marks its local Invocation Context
cancelled and asks Module work to stop; it does not create a wire outcome. The
SDK holds that data response until the matching host cancel arrives or a valid
application result had already completed. After cancel, the host ignores any
data-channel completion for the retired ID. Kernel keeps the authoritative host
timer because process clocks do not share an epoch.

The Adapter validates extension structure, size, uniqueness, sealed bytes,
declared audience, and transport binding only. Product-owned consumers validate
issuer, proof, freshness, and domain meaning; wire validation never asserts that
an extension is semantically trustworthy.

One serialized host terminal arbiter owns the only commit point for each
invocation. Before committing any response it re-reads cancellation and the
Driver-monotonic deadline in that same critical section. Cancellation wins over
deadline, and deadline wins over the response. No transport task may publish a
terminal outcome directly.

Once cancellation or deadline commits, the Adapter retires the ID, sends
idempotent cancel on the independent control listener, waits only up to
`cancel_ack_timeout_nanos`, and ignores a later response for that known retired
ID. A cancel for an active, retired, or unknown ID returns `accepted: true` and
leaks no existence information. An unknown response, a reused ID, a second
response for an active ID, a malformed result, or a wrong session is a
generation-fatal Protocol Violation. The child Runtime Failure schema excludes
`cancelled` and `deadline_exceeded`; receiving either is a Protocol Violation.

Process exit fails every in-flight invocation exactly once. Request, Stream, and
Event interactions are never replayed automatically. Optional Stream and Event
profiles, when eventually selected, must retain bounded credit or per-binding
FIFO admission, strict sequencing, explicit partial or terminal outcomes,
half-close rules, late-frame rejection, volatility, and no replay or redelivery.

### Failure ownership

| Source | Exact class | Scope and mapping |
| --- | --- | --- |
| Child outcome | `resource_exhausted` | One invocation; validated and returned as Kernel `ResourceExhausted` |
| Child outcome | `module_failure` | Retire the process generation; expose `ModuleFailure` to Kernel supervision |
| Adapter detection | `protocol_violation` | Retire the process generation; record the host class and expose `ModuleFailure` to Kernel supervision |
| Adapter process boundary | crash, refusal, startup/auth timeout, forced termination | Retire the process generation; stable host diagnostic plus `ModuleFailure` |
| Host Invocation Context | `cancelled`, `deadline_exceeded` | One invocation; child may observe but cannot originate authority |
| Host resolution or supervision | unavailable binding, invalid Plan, unavailable execution class, missing Artifact or factory, admission failure, restart exhausted | Never accepted from child wire |

Success and Domain Error payloads are validated against the exact selected
Capability schema. A child `unknown_operation` is a Protocol Violation because
the Adapter validates the canonical operation table before sending. A child
`unavailable` is also a Protocol Violation because Kernel `Unavailable` means
the consumer lacks a resolved binding, which is a host Plan fact. Plugin
diagnostic detail is bounded, treated as untrusted text, and cannot choose a
host failure discriminant. Only the `ModuleFailure` mapping enters the existing
Kernel automatic restart path; invocation-local failures do not restart the
Module.

### Shutdown

Shutdown first closes child admission and terminally cancels host invocations.
The Adapter sends cancellation over the reserved control path, sends
`lenso.process.v1.shutdown`, and waits up to `shutdown_ack_timeout_nanos`. After
acknowledgement it waits `shutdown_exit_timeout_nanos` for the complete process
group to exit. It then sends graceful group termination and waits
`term_grace_nanos`, followed by forced group kill and bounded reap for
`kill_reap_timeout_nanos`.

Refused shutdown, missing acknowledgement, acknowledgement without exit, TERM
timeout, forced kill, reap timeout, and surviving descendants have distinct
host-owned diagnostic classes. They never become child-selected Runtime
Failures. The current Bun Adapter's direct-kill path is not conformant until it
implements this sequence and the public Generic V1 schema.

## Permission truthfulness

| Layer | Authority | Security meaning |
| --- | --- | --- |
| Capability requirement and binding | Resolved App Plan and Kernel | Enforced access to declared Lenso Interfaces |
| Final operation authorization | Target Module | Enforced product and resource policy |
| Plugin permission request | Plugin Manifest | Publisher request only |
| Approved grant | Plugin Set Lock | App-owner intent only |
| Effective Host Grant | Host Grant Set plus named enforcer | Enforced only to the stated scope |
| `trust_review_only` | Admission Receipt | Trusted-code decision with no confinement claim |
| Secret reference | App Composition and Secrets Capability | Secret value stays outside every Plugin authority document |

V1 Process isolation is a failure boundary. The Adapter can truthfully enforce a
sanitized environment, explicit argv without a shell, controlled handles,
bounded I/O, and process lifecycle. Filesystem and network requests remain
trusted-code declarations unless a selected host policy supplies real
confinement. Product UI states the enforcement kind beside every grant.

## First vertical proof: trusted local Agent Tool Plugin

The first Plugin contains one Process Module providing the existing portable
`lenso.agent.tool-provider@1` Capability. It exposes `workspace.read_text` for
the useful Agent turn, `fixture.wait` for cancellation and drain evidence, and
`fixture.fail_generation` for supervised rollback evidence. The temporary
workspace root is an explicit local-trust grant, with final path authorization
owned by the Tool Provider; the proof makes no sandbox claim.

Before the proof, these one-time platform and product prerequisites must be
complete; none is per-Plugin recompilation:

- normative Generic V1 schemas, canonical vectors, and cross-language base
  profile conformance in `lenso-protocols`;
- a Generic V1-compatible public Bun SDK and Process Adapter, including string
  IDs, remaining-time semantics, bootstrap authentication, failure mapping, and
  graceful shutdown;
- a canonical Resolved App Plan reader that preserves the complete Lane Set;
- Host Build, Plugin Store, resolver, Generation and Transition authority,
  Supervisor, fenced router, and durable recovery; and
- durable cross-generation Agent Session fencing plus the generated Tool
  Provider host codec.

### Acceptance sequence

1. **Freeze the host.** Emit and verify the Host Build Manifest, record the
   Harness executable SHA-256 and clean Cargo files, and preserve them
   byte-for-byte through every later step.
2. **Install.** Admit a locally trusted Bundle into a temporary Store. Prove
   strict Manifest, path, Descriptor, size, and digest validation; detached
   local-trust provenance; atomic content-addressed commit; and no enablement.
3. **Resolve.** Select the Plugin Release, its required Feature set and grants;
   bind `tools -> process-tool-provider`; emit canonical locks, final Plan, and
   Generation and Transition Specs from the exact active built-in Generation;
   prove full closure and exact digests.
4. **Stage.** Reverify Store bytes, construct native plus Process Adapters, start
   all lanes, authenticate the exact handshake, call the Tool catalog during
   Tool Runtime activation, reject duplicate names, and open readiness only
   after the complete App is ready.
5. **Invoke.** Atomically switch to the Plugin Generation and complete a real
   Agent Turn through `Agent Loop -> Tool Runtime -> explicit Provider binding
   -> Process wire -> Plugin`. Persist Generation, Plugin, Provider Instance,
   Host Build, Transition, Artifact, Tool-call, and terminal provenance from
   host-resolved facts.
6. **Cancel.** Start `fixture.wait`, cancel it from the controlling surface,
   propagate cancellation to the child, persist `turn_cancelled`, prove one
   terminal outcome and no replay, and show no pending request leak.
7. **Switch and drain.** While a Turn in the Plugin Generation holds a Lease,
   stage a Generation selecting the built-in Provider, atomically switch new
   work, reject a concurrent Turn for the same Session, let another Session use
   the new Generation, then release or cancel the old Lease. Prove the old
   Generation enters standby, accepts no work, and shuts down only when its
   rollback window expires.
8. **Reject before switch.** Stage a Bundle with an invalid handshake or
   duplicate Tool catalog. Prove the candidate never becomes ready, all
   candidate resources are cleaned up, the active epoch and last-known-good
   Spec stay unchanged, and Supervisor restart recovers that committed state.
9. **Rollback after switch.** From the built-in Generation, stage a valid Plugin
   Generation with restart attempts exhausted by `fixture.fail_generation`.
   Switch, trigger the terminal Generation failure, and prove automatic rollback
   advances the epoch, reactivates the complete built-in standby Generation,
   drains the failed candidate, preserves durable writes, and replays nothing.
10. **Disable and remove.** Prove disable is a new Generation, removal is blocked
   while any staged, active, draining, saved, or rollback reference exists, and
   zero-reference cleanup moves only the exact Store-owned content address to
   quarantine or trash before removing its record.
11. **Recheck the host.** The Harness executable SHA-256 and Cargo inputs still
    match step 1; the Plugin process is absent; the built-in Provider works; all
    canonical documents and durable provenance read back successfully.

The proof is complete only when every step has executable evidence, including
negative cases. It does not establish marketplace distribution, automatic
updates, hostile-code isolation, Wasm, arbitrary Capability hot loading,
state-copy migration, or rollback of durable writes.

## Deferred branches

- implementation and conformance of the reviewed Wasm Component, QuickJS, and
  native dylib branches in [Plugin execution classes](plugin-execution-classes.md);
- a marketplace, search, ratings, remote registry, and automatic updates;
- any stable Rust library ABI or safe live dylib unloading claim;
- Process consume, Stream, or Event Profiles lacking public SDK and two-way
  conformance evidence;
- zero-downtime destructive state migration;
- cross-host or distributed Generation coordination; and
- repository or crate names before Interfaces and conformance justify them.
