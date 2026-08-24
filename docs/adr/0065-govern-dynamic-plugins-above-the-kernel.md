# ADR 0065: Govern dynamic Plugins above the Kernel

- Status: proposed
- Date: 2026-08-24
- Extends: ADR 0030, ADR 0031, ADR 0045, ADR 0046, ADR 0055, ADR 0057,
  ADR 0064
- Contract: [`../architecture/dynamic-plugins.md`](../architecture/dynamic-plugins.md)

## Context

A precompiled Lenso App should be able to install, enable, disable, update, and
remove third-party extensions without editing its Cargo inputs or recompiling
the product. The existing authoring path and statically linked Native Rust
Adapter remain appropriate for built-in Modules, but they do not provide that
Plugin experience. Adding a mutable Plugin graph to Kernel would create a
second composition authority and weaken the immutable Resolved App Plan.

## Decision

Plugin is a user-facing installation and governance concept above Kernel, not a
peer runtime type or an Execution Class. One Plugin may contribute one or more
ordinary Modules, explicit Capability bindings, configuration, and static
assets. Enabled contributions are materialized into an immutable Resolved App
Plan and executed through ordinary Execution Adapters; Kernel has no Plugin
registry, discovery path, or Plugin-specific lifecycle branch.

The Lenso platform layer outside portable core owns generic installation,
verification, resolution, and App Generation replacement. Product owners such
as Agent Harness own their Capability contracts, Plugin categories, curation,
product defaults, explanations, and UI. The portable Kernel continues to own
only execution of one validated Plan. Exact implementation repositories remain
unassigned under ADR 0064.

Plugin identity is distinct from its distribution and runtime inputs. A stable
Plugin identity has immutable versioned Plugin Releases. A verifiable Plugin
Bundle carries Release metadata and one or more exact digest-identified
Artifacts. Cargo, npm, OCI, or local Packages are acquisition inputs and do not
become runtime identities. A Built-in Module is not called a Plugin merely
because it is optional or uses an external Execution Adapter. A Plugin Release
shipped with the product is a Bundled Plugin only when it remains governed by
the same manifest, Plugin Store, Plugin Set, and enablement lifecycle.

One Plugin Store may retain multiple immutable Releases of the same Plugin so
different Apps and overlapping old and new App Generations can remain pinned.
One App Generation selects at most one Release for each stable Plugin identity;
multiple configurations of that Release use keyed Module Instances rather than
loading conflicting Plugin versions into one Generation.

Installation and enablement are separate. Installation admits a Plugin into a
Plugin Store scoped to one operator or security principal without adding it to
a running App. Immutable Releases may be reused across Apps, but each App owns
its enabled Plugin Set and permission grants. An exact enabled Plugin Set
contributes to one new App Generation. A change stages and readies that
generation before routing new work to it and drains the previous generation
after a switch. Failure during staging leaves the active route unchanged;
post-switch rollback uses the fenced rule below. It never mutates the running
Kernel graph.

The control plane separates four logical responsibilities without requiring
four processes, crates, or repositories: admission and storage acquire, verify,
and retain Releases; resolution turns one App-local Plugin Set into exact
contributions and execution inputs; the Generation Supervisor stages, switches,
drains, and rolls back App Generations; Execution Adapters create the selected
Module generations and endpoints. The authoritative Generation Supervisor
lives above the generations it replaces rather than as an ordinary Module
inside one of them.

A Plugin Release declares one atomic required contribution set and may expose
named optional Plugin Features. Feature selection happens before Plan
resolution. Every selected contribution must prepare and become ready before
the App Generation activates; the control plane never publishes a partially
ready subset. Package and Bundle dependencies may acquire required content, but
V1 does not resolve implicit cross-Plugin installation dependencies. Runtime
collaboration is expressed only through Module Capability requirements and
explicit bindings. Neither Modules nor Plugins discover peers through a Plugin
registry.

The generic Plugin Manifest contains only platform meaning: Release identity,
Module contributions, Artifacts, Features, and requested permissions. Product
owners attach namespaced extensions or separate Product Plugin Metadata for
categories, curation, explanations, and UI such as Agent Tool, Prompt, or Model.
Product vocabulary does not enter the generic Lenso schema or Kernel.

One immutable App Generation Spec is the control-plane authority for a staged
or running App Generation. It atomically binds an immutable Host Build Manifest,
the Resolved App Plan, an exact Plugin Set Lock, a Resolved Artifact Set,
Admission Receipts, and Effective Host Grants. The Host Build Manifest accounts
for the host executable, built-in factories, fixed Artifacts, Adapters, and
Protocol Profiles. The components carry their own canonical digests and the
Generation Spec binds those digests directly or through the Artifact Set so a
Plan, host build, and Artifact Set cannot be crossed between resolutions. This
Spec remains above Kernel: `lenso-app-plan` continues to own the logical
execution graph rather than signatures, provenance, Artifact verification, or
host permission policy.

Replacement compatibility belongs to an edge, not either Generation node. One
immutable App Generation Transition Spec binds the active and candidate
Generation Spec digests, rollout policy, and every required State Compatibility
Receipt. An initial boot uses no predecessor and no compatibility receipts.

The Generation Supervisor verifies the complete Spec, proves that the current
host bytes and configured catalog match its Host Build Manifest, materializes
admitted Artifacts, and constructs Execution Adapters with generation-local
execution inputs before starting the Runner. Kernel still receives only its
Resolved App Plan and the already configured Adapter catalog. The existing
opaque `package_revision` may correlate a Module Instance with resolution
evidence but is not treated as a universal content digest or substitute for the
Artifact Set.

The proposed implementation contract defines strict canonical authority
documents for the Plugin Manifest, Plugin Set Lock, Admission Receipt, Host
Build Manifest, Resolved Artifact Set, Effective Host Grant Set, State
Compatibility Receipt, App Generation Spec, and App Generation Transition Spec.
Unknown or duplicate fields, ambiguous paths, unresolved references, digest
mismatches, broadened grants, and non-canonical ordering fail closed.
Machine-local materialization paths and secrets never enter canonical Plugin
authority.

The first Process Protocol Profile is exact-selected JSON-RPC over authenticated
loopback HTTP with distinct data and reserved control listeners. It uses one
child process per Module Instance generation, a one-use bootstrap secret with
byte-exact mutual HMAC proof, a generation-scoped session, exact Descriptor,
Artifact, Generation Spec, and grant digests, relative remaining timeouts,
bounded messages and queues, decimal-string correlation IDs, a host-owned
terminal arbiter, explicit cancellation, graceful shutdown before forced
termination, and no replay or stdio fallback. Request provision is mandatory;
request consumption, Stream, and Event Profiles remain disabled until public
SDK, Adapter, and bidirectional conformance evidence exist.

Plugin updates are append-only in V1. Admission installs a new immutable Release
beside existing Releases; it never overwrites bytes identified by an existing
Release or Artifact digest. An App changes Release only through an explicit
Plugin Set resolution and App Generation switch. V1 performs no automatic
update or implicit highest-SemVer selection. A user-installed Release does not
silently override a Bundled Plugin; the App selects an exact allowed Release
under product admission policy.

Disable, uninstall, and garbage collection are distinct. Disable changes only
an App-local Plugin Set. Uninstall is refused while a Release is referenced by
any saved App selection, current or draining App Generation, or retained
rollback target, and reports those references. Once no references remain,
uninstall removes the Store registration and garbage collection may delete the
unreferenced immutable Artifacts.

## App Generation routing

The Generation Supervisor owns a fenced route to one complete App Generation,
not to individual Execution Lanes. Staging verifies one Transition Spec, starts
the candidate's entire Lane Set, waits until every lane is ready, and then
advances one durable, monotonic Routing Epoch to select the new Generation
atomically. A lane cannot roll forward independently, and a terminal lane
failure makes the whole App Generation unhealthy.

Routing is product-work-unit pinned. The generic router atomically reads the
active Generation and acquires a Generation Lease before admitting work. That
Lease pins the selected Generation until the work reaches its terminal outcome;
the work never migrates between Generations. Agent Harness maps one Turn to one
Generation Lease: a Turn already in progress stays on its old Generation, while
a later Turn may use the new Generation even when both belong to one Session.

After the active route switches, the old Generation enters Generation Drain and
receives no new Leases. A Lease remains active through all nested Model and Tool
calls, Streams, and the final durable Session commit. The Supervisor waits for
the active count to reach zero until a bounded deadline and then cancels any
remaining work through generation-scoped cancellation. When the Transition
Spec has a nonzero rollback window, the drained Generation stays live but
non-routable in rollback standby until that window expires; only then does the
Supervisor request Kernel shutdown. Without a rollback window it shuts down
immediately after drain. Cancelled or uncertain work is never replayed
automatically. Current Kernel shutdown is cancel-and-join rather than this outer
drain and standby protocol, so the Supervisor and lease accounting are new
host-layer mechanisms.

The active Generation, staged Generation, retained rollback candidate, lifecycle
state, Generation and Transition Spec digests, and Routing Epoch are durable
control-plane records. Every transition uses compare-and-set under a fenced
Supervisor lease. Routers reject an obsolete epoch, and recovery reconciles
staged, ready, active, draining, standby, and retired lifecycle records plus
their orthogonal healthy or failed outcome before admitting new work. An old
Supervisor cannot regain authority after its lease or epoch is superseded.

Product owners persist Generation Provenance for durable work. Agent Harness
records at `turn_started` the Generation identity, Routing Epoch, Generation
and Transition Spec digests, Host Build Manifest digest, Plan digest, Plugin Set
Lock digest, Resolved Artifact Set digest, and contribution-manifest digest. It
also records whether activation followed the Transition forward or rolled it
back. It records digests and public identities, not secret values or the
contents of Effective Host Grants.

## Stateful replacement and rollback

Durable product state remains Module-owned and is shared across compatible App
Generations; the Generation Supervisor neither copies it nor invents a second
state authority. Agent Harness permits at most one active Turn per Session
across all Generations. The Session owner must enforce that rule with durable
compare-and-set or a fenced Session lease rather than generation-local memory.
Until its file Session Store provides cross-process atomicity or fencing, that
Store is ineligible for overlapping Generations and requires a maintenance
restart for Plugin Set changes.

Zero-downtime replacement requires a Transition-bound State Compatibility
Receipt proving that old and new Module Releases can safely share the current
state schema and that writes made by either remain readable by the rollback
candidate. Stateful evolution follows
expand/contract: expand first, run overlapping compatible code, switch, drain,
and contract only after rollback expires. A destructive or one-way migration
requires maintenance mode, stopping the old Generation before migration, and
disables automatic rollback to code that cannot read the new state.

Rollback changes only the routing selection and code Generation; it never
undoes committed Session or business data and never replays completed, failed,
or uncertain operations. Failure before the route switch discards the staged
Generation and leaves the old route unchanged. After a switch, automatic
rollback is allowed only during a bounded canary window while the old Generation
remains in rollback standby and the Transition Spec proves backward data
compatibility. Rollback advances the Routing Epoch, reactivates that complete
standby Generation, and moves the candidate lifecycle from active to draining
while retaining its failed health through retirement. It is a restricted
reverse operation of the same Transition: compare-and-set requires active equal
to the original `to`, standby equal to the original `from`, the same Transition
digest, an unexpired window, backward-read compatibility, and the current fenced
epoch; control state and provenance record the rollback direction. Once the old
Generation has shut down, rollback requires a new explicit restaging operation
and is not automatic. Outside those conditions the system fails closed and
requires operator recovery. A fatal failure in any lane applies to the complete
Generation rather than producing a mixed-lane runtime.

The first supported third-party path is an admitted, explicitly trusted
out-of-process Plugin. A verified signature establishes provenance, not safety.
Process isolation is a failure boundary, not a security sandbox. Explicit
Capability bindings and Module authorization remain enforced; raw filesystem,
network, environment, and subprocess access remains part of the V1 trust
decision unless a selected Execution Adapter or host policy provides and
declares real confinement. Plugin permission declarations must not be presented
as enforced sandboxing when no such enforcement exists. Untrusted code requires
a later reviewed Wasm or isolated-process security boundary.

## Consequences

- External Process and future Wasm execution remain Adapter choices; neither
  makes a Module a Plugin by itself.
- A fixed first-party Process Module need not be a Plugin, while an installed
  Plugin still becomes ordinary Module Instances at runtime.
- Built-in Modules and Bundled Plugins remain distinct even when both ship in
  one product binary or installer.
- An installed Plugin may remain disabled and absent from every running Plan.
- Installing a new Release does not supersede the Release selected by any App
  Generation until that App resolves an explicit Plugin Set change.
- App Generation reproducibility covers the logical Plan, host build, exact
  admitted bytes, Protocol Profiles, and effective host authority used to
  execute it.
- Dynamic replacement requires new host-layer routing, lease accounting,
  durable epochs, and recovery; current Kernel supervision and shutdown do not
  already provide those mechanisms.
- Stateful Modules that cannot prove cross-generation concurrency and schema
  compatibility fall back to maintenance restart rather than weakening data
  ownership or rollback truthfulness.
- The existing private Bun wire requires a versioned public SDK, Adapter update,
  and cross-language Process Profile conformance before it can execute a V1
  Process Plugin.
- Dynamic replacement preserves Kernel, Plan, and Capability semantics instead
  of introducing in-place graph rebinding.
- The linked proposed implementation contract settles the authority schemas,
  Process Protocol Profile, shared-state gates, and exact V1 proof. Repository
  assignments and implementation remain future work.
