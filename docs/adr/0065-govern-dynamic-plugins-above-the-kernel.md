# ADR 0065: Govern dynamic Plugins above the Kernel

- Status: proposed
- Date: 2026-08-25
- Extends: ADR 0030, ADR 0031, ADR 0045, ADR 0046, ADR 0055, ADR 0057,
  ADR 0064
- Depends on: [ADR 0066](0066-derive-module-descriptors-and-plans-from-source.md),
  [ADR 0067](0067-transition-between-immutable-plan-snapshots.md)
- Contracts:
  [`../architecture/plugin-authoring-and-resolution.md`](../architecture/plugin-authoring-and-resolution.md),
  [`../architecture/dynamic-plugins.md`](../architecture/dynamic-plugins.md),
  [`../architecture/plugin-execution-classes.md`](../architecture/plugin-execution-classes.md)

## Context

A precompiled Lenso App should be able to install, enable, disable, update,
and remove third-party extensions without editing its Cargo inputs or
recompiling the product. The existing authoring path and statically linked
Native Rust Adapter remain appropriate for built-in Modules, but they do not
provide that Plugin experience. Adding a mutable Plugin graph to Kernel would
create a second composition authority and weaken the immutable Resolved App
Plan.

An earlier draft of this decision introduced Plugin Definition, Plugin
Contribution, Product Extension Point, and Plugin Runtime Facet as a second
authoring vocabulary above Module. Review rejected that layering: it made
Plugin a shallow wrapper over Module, forced ordinary authors through two
concept systems, and left hand-authored Descriptors, bindings, and Composition
JSON as the real interface underneath. This revision depends on ADR 0066,
which makes Module the sole authoring abstraction with derived Descriptors,
and ADR 0067, which makes runtime change a transition between immutable Plan
Snapshots instead of only a whole-App replacement.

## Decision

**Plugin is the installable distribution role of a Module Package, not a peer
architecture, runtime type, Execution Class, or second authoring abstraction.**
A Plugin owns stable identity, immutable versioned Releases, acquisition,
verification, permission requests, configuration, and enablement above Kernel.
What executes is always ordinary Module Instances under ordinary Capability
bindings; Kernel has no Plugin registry, discovery path, or Plugin-specific
lifecycle branch.

Authors write Modules and data packages exactly as ADR 0066 defines, whether
the result ships built-in, bundled, or through a registry. Packaging for
distribution adds metadata and signatures, never a different programming
model. There is no Plugin Runtime Facet: runtime resources shared by one
package's Modules are an ordinary Module with ordinary lifecycle.

### Slots

Products attach installed behavior through **Slots**. A Slot is a
product-owned, versioned attachment point that fixes:

- its attachment kind: `add` collects compatible entries, `provide` offers a
  candidate for a replaceable role, `intercept` joins a typed ordered
  processing seam, and `mount` attaches one closed Module subgraph through one
  public root Capability;
- its cardinality: `one`, `optional`, `many`, or `keyed_many`;
- its Module Descriptor and execution constraints;
- deterministic selection and ordering rules; and
- product-language explanations for proposals and diagnostics.

The immutable Slot Catalog binds the product's Slots and rules for one
resolution; the App Generation Spec binds its digest. Publishers cannot assign
global priority, displace an active `one` selection, bind another Plugin's
private Module Instances, or broaden host grants. Replacing an active provider
and resolving ambiguous order are explicit App-owner decisions. A Slot Entry
is generated from Module source or a product builder call; product vocabulary
such as Tool, Model, Prompt, or Skill stays in product SDKs and the Slot
Catalog, never in generic schemas or Kernel.

A data-only package offers inert, schema-identified content to a Slot whose
interpreter is an explicitly selected ordinary Module. Data never gains
ambient execution, Capability access, or a hidden Module graph.

### Install experience

Installation is one user action with one confirmation:

```text
acquire -> verify and admit -> confirm requested permissions
  -> resolve Desired State into the next Plan Snapshot
  -> apply: hot Plan Transition, or App Generation swap when structural
```

Install-disabled is a secondary option. Under ADR 0067, adding or removing a
`many` entry, replacing a provider with an Interface-identical Release, and
configuration-only changes apply as hot Plan Transitions without restarting
the App. Structural changes stage a complete new App Generation and switch
atomically. Both paths present one experience; the mechanism is visible only
in `inspect` output and diagnostics.

The control plane exposes three logical operations. `install` acquires,
verifies, and admits an immutable Release into the Plugin Store without
enabling it. `propose` deterministically turns a Desired State change into a
Change Proposal that is `ready`, `needs_decision`, or `rejected`, without
touching the running App. `apply` accepts only an exact ready proposal digest
and executes it through the Reconciler. Enable, disable, update, configure,
and rollback are Desired State changes projected onto these operations, never
imperative mutations of a running graph.

### Store, Releases, and identity

Plugin identity is distinct from distribution and runtime inputs. A stable
Plugin identity has immutable versioned Releases; Cargo, npm, OCI, or local
packages are acquisition inputs, not runtime identities. Updates are
append-only: admission installs a new immutable Release beside existing ones
and never overwrites bytes identified by an existing digest. One Plan Snapshot
selects at most one Release per Plugin identity; multiple App-local
configurations use named Plugin Instances, each resolving to its own keyed
Module Instances and state identities.

Installation and enablement are separate authorities. A Store retains Releases
per operator or security principal; each App owns its Desired State and
grants. Disable changes only App-local Desired State. Uninstall is refused
while any saved selection, running or draining Generation, or retained
rollback target references the Release; garbage collection deletes only
unreferenced immutable Artifacts. V1 performs no automatic update and no
implicit highest-SemVer selection, and a user-installed Release never silently
overrides a bundled one.

### Runtime change authority

The Reconciler owns Desired State resolution, Plan Snapshot production, delta
classification, and application. Hot Plan Transitions execute inside the
running Kernel under ADR 0067's whitelist, staging rules, and quiescence
protocol. Structural changes use the Generation Supervisor: stage the complete
candidate Generation, wait for its Ready Gate, advance one fenced Routing
Epoch, drain the old Generation under product-work-unit Leases, and hold a
bounded rollback standby. The
[dynamic Plugin control-plane contract](../architecture/dynamic-plugins.md)
owns the exact authority documents, staging, routing, drain, rollback, and
recovery rules; this ADR fixes only their ownership.

Durable product state remains Module-owned and shared across compatible
Snapshots and Generations. Stateful replacement without a State Compatibility
Receipt falls back to maintenance replacement; destructive migration disables
automatic rollback. Rollback changes selection and code only; it never undoes
committed business data or replays uncertain work.

### Execution and trust

Plugin governance and execution topology are orthogonal. One logical Module
may declare several target-constrained implementation variants; resolution
selects exactly one per keyed Instance under the Host Build Manifest and Host
Execution Policy and records it immutably. Runtime never negotiates, falls
back, or substitutes a variant after staging starts. Native built-ins,
Process, Wasm Component, embedded QuickJS, and trusted native dynamic
libraries remain Execution Adapter branches under one governance model; a
dynamically installed third-party Rust implementation still requires an
admitted Wasm or Process variant, and a native factory selection can only name
a factory already linked into the exact Host Build Manifest.

A verified signature establishes provenance, not safety, and process isolation
is a failure boundary, not a security sandbox. Permission declarations must
not be presented as enforced sandboxing when no enforcement exists; untrusted
code requires a reviewed Wasm or isolated-process security boundary.

## Consequences

- One authoring path serves built-in, bundled, and installed behavior; the
  Plugin layer adds distribution, trust, and lifecycle governance without a
  second programming model or a Plugin wrapper type.
- Ordinary installs, updates of Interface-identical Releases, and
  configuration changes no longer restart the App; structural changes keep
  the staged all-or-nothing Generation guarantee.
- Slots replace per-Plugin code-level attachment profiles; adding a
  compatible Plugin must not require rebuilding or editing the product Host.
- Ambiguous `one` selection, provider replacement, new permissions, and
  state-transition choices surface as structured Change Proposal decisions,
  never as instructions to edit generated JSON.
- A data-only Plugin may resolve to zero Module Instances; one package may
  resolve to several Module Instances with distinct target, state, and
  failure semantics while remaining one item to the user.
- Generation reproducibility continues to cover the exact Plan Snapshot, host
  build, admitted bytes, protocol profiles, and effective authority; Plan
  Transitions extend that record with an auditable snapshot sequence.
- The retired draft vocabulary — Plugin Definition, Plugin Contribution,
  Product Extension Point, Plugin Runtime Facet, Desired Plugin Set,
  Composition Proposal — is replaced by Module authoring (ADR 0066), Slot,
  Slot Entry, Slot Catalog, Desired State, and Change Proposal.
- The control plane requires new host-layer mechanisms — Reconciler, durable
  Routing Epochs, Leases, recovery — that current Kernel supervision and
  shutdown do not provide; ADR 0067 adds the single Kernel mechanism they
  build on.
