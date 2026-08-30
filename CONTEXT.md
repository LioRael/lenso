# Lenso vNext context

## Status

`main` is the vNext implementation and release line. It is a clean local-first
runtime, not a compatibility workspace for v0.3.x. The final v0.3.x source is
retained by the `lenso@0.3.47` tag and Git history; `next` is a pre-cutover
integration reference rather than a delivery target.

The vNext reset keeps the accepted design evidence in Git while removing the
old implementation from this branch. Git history remains the migration and
forensics source; no `legacy/` directory is part of the vNext workspace.

## Canonical vocabulary

- **Host** — one runnable product boundary with an immutable catalog of root
  Slots, embedded Plugin Releases, default Plugin Instances, and replacement
  policy.
- **Plugin Root** — the App-owner-owned collection of explicit Plugin
  Instances, configuration, disabled markers, and exact non-embedded Releases
  offered to one Host. It expresses differences from Host defaults rather than
  restating a complete application.
- **App** — the valid resolved result of combining one Host with one Plugin
  Root. It is runtime output, not a hand-authored manifest.
- **Plugin** — the sole application-behavior abstraction: product behavior
  authored in source with explicit execution, lifecycle, failure, state, and
  placement semantics. A Plugin may be embedded in a Host, bundled with a
  product, or installed later without becoming a different kind of thing.
- **Plugin Descriptor** — the generated, locked declaration of a Plugin's
  configuration Schema, safe defaults, provided and required Capabilities, and
  optional durable state Interface. It is derived from Plugin source and is
  never hand-authored.
- **Plugin Instance** — one App-local instantiation of an exact Plugin Release,
  identified by its Plugin ID and Instance key together.
- **Capability** — a versioned deep role Interface exposed or required by a
  Plugin; authored as source types whose canonical Schema is generated and
  locked.
- **Operation** — one request, stream, or event interaction in a Capability.
- **Port** — one typed Capability requirement declared in Plugin source; its
  cardinality and Descriptor requirement are derived, not restated.
- **Slot** — a product-owned, versioned attachment point with cardinality,
  replacement policy, and deterministic ordering. Plugins are offered to
  Slots.
- **Slot Entry** — one generated manifest declaration offering a Plugin or
  data item to one Slot.
- **Slot Catalog** — the immutable product-owned catalog of Slots and their
  deterministic resolution rules.
- **Plugin Release** — one immutable version of a Plugin and its exact
  metadata and Artifacts. Embedded, bundled, and installed are distribution
  modes of the same Release model, not separate authoring abstractions.
- **Desired State** — the Host defaults plus the exact Plugin Root state and
  approved permission scopes proposed for the next App.
- **Change Proposal** — a deterministic explanation of one Desired State
  change that is ready, needs an App-owner decision, or is rejected; it is not
  runtime authority.
- **App Composition** — the derived exact logical graph of Plugin Instances,
  configuration, and explicit Capability bindings; resolver output, not an
  authoring surface.
- **Resolved App Plan** — one immutable, complete execution input; one Plan
  Snapshot in the App's totally ordered plan sequence.
- **Plan Transition** — the validated atomic delta between two adjacent Plan
  Snapshots.
- **Reconciler** — the control-plane component that resolves Desired State
  into the next Plan Snapshot, computes the Plan Transition, and applies it in
  place when it is hot-applicable or through an App Generation swap when it is
  structural.
- **App Generation** — one complete host-level selection of a Plan Snapshot,
  exact Plugin Releases, Artifacts, grants, and host inputs; the swap unit for
  structural change.
- **Kernel** — portable graph, lifecycle, invocation, cancellation,
  supervision, readiness, diagnostic, and Plan Transition mechanisms.
- **Runtime Driver** — host scheduling and monotonic-time implementation.
- **Execution Adapter** — host-specific Plugin generation and endpoint
  implementation.
- **Execution Lane** — one single-owner Kernel replica on its own host thread
  together with the Plugin Instances placed on it.
- **Placement** — the Plan-declared assignment of Plugin Instances to
  Execution Lanes.

`App Definition`, `Module`, `Service`, `Provider`, `Console`, `Story`, and
`System Plane` are not peer product or authoring types in vNext and must not
appear in an author, operator, or App-owner interface. `Plugin Contribution`, `Product Extension
Point`, `Plugin Runtime Facet`, `Desired Plugin Set`, and `Composition Proposal`
are retired draft terms: attachment is a Slot concern, shared runtime resources
are Plugins, and intent plus proposal are Desired State and Change Proposal. A
separately running program is a host or an Execution Adapter concern.

## Hard invariants

- Kernel executes only immutable Plan Snapshots. It may apply one validated
  Plan Transition between adjacent snapshots at a quiescent point, and it
  never discovers, installs, downloads, selects versions, rebinds outside a
  Transition, or applies product policy.
- Consumers receive only explicitly bound typed handles; binding sets change
  only at an atomic Transition or Generation switch.
- Descriptors, Schemas, manifests, Compositions, Plan Snapshots, and
  Transitions are generated, locked build or resolver artifacts; hand-editing
  them is not an authoring path.
- A Plugin Root is the only App-owner composition input. The resolver fills
  Host defaults and unique legal bindings, rejects ambiguity, and never
  exposes a hand-authored binding or placement escape hatch.
- Kernel is independent of Tokio, operating-system facilities, filesystems,
  networks, databases, process control, and product policy.
- Runtime Driver owns scheduling, clocks, cancellation lanes, and host
  shutdown translation.
- Execution Adapter owns Plugin generation, endpoint mechanics, isolation, and
  host-specific failure semantics.
- Stateful behavior, persistence, migrations, Auth, HTTP, Console, Story,
  Workflow, Outbox, telemetry, and secrets are optional Plugin or Adapter
  concerns, never Kernel features.
- The same portable Kernel must compile for native and supported WebAssembly
  targets without target-specific host services in its core state machine.
- Parallelism comes from placing Plugin Instances on more Execution Lanes;
  Kernel correctness never requires work stealing, runtime Instance
  migration, or thread-safe Plugin state.

## Workspace ownership

`lenso-app-plan` owns serializable plan data. `lenso-kernel` owns the portable
runtime and deterministic test Driver. `lenso-runtime-conformance` owns the
product-neutral executable test surface for Kernel Interfaces. Runtime Drivers,
Execution Adapters, Plugins, authoring tools, and examples have been extracted
to their ADR 0064 owners. New concerns must first identify their
Capability, Plugin, Adapter, or authoring seam before adding a crate.

## Delivery

Create worktrees from the latest `origin/main` with Worktrunk. Pull requests
target `main`. Do not publish or recreate v0.3.x artifacts from the vNext
workspace.

## Evidence

Use the smallest meaningful gate during development:

```sh
cargo fmt --all -- --check
cargo xtask check-core-repository-boundary
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
```

The CI workflow additionally compile-checks the portable plan and Kernel for
`wasm32-unknown-unknown` and `wasm32-wasip2`.

## Documentation routing

ADRs 0030–0072 are normative for vNext. The vNext architecture overview,
validation roadmap, and research notes are retained beside them. Accepted
architecture is not an implementation claim; each contract states its current
evidence and remaining delivery gates. Removed v0.3.x implementation docs are
not recreated in this branch.
