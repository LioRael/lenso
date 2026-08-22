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

- **App** — one composed runtime process or host instance.
- **Module** — an installable product capability with one authoring Interface.
- **Module Instance** — one keyed App-local instantiation of a Module.
- **Capability** — a versioned deep role Interface exposed or required by a
  Module.
- **Operation** — one request, stream, or event interaction in a Capability.
- **App Composition** — authoring-time choices for Instances, configuration, and
  explicit Capability bindings.
- **Resolved App Plan** — immutable execution input materialized before boot.
- **Kernel** — portable graph, lifecycle, invocation, cancellation, supervision,
  readiness, and diagnostic mechanisms.
- **Runtime Driver** — host scheduling and monotonic-time implementation.
- **Execution Adapter** — host-specific Module generation and endpoint
  implementation.
- **Execution Lane** — one single-owner Kernel replica on its own host thread
  together with the Module Instances placed on it.
- **Placement** — the Plan-declared assignment of Module Instances to
  Execution Lanes.

`Service`, `Provider`, `Console`, `Story`, `System Plane`, and `Plugin` are not
peer runtime types in vNext. A separately running program is a host or an
Execution Adapter concern; a future Wasm source must earn its own reviewed
seam.

## Hard invariants

- Kernel receives one already resolved Plan and does not discover, install,
  download, rebind, hot-reload, or mutate the graph during execution.
- Consumers receive only explicitly bound typed handles.
- Kernel is independent of Tokio, operating-system facilities, filesystems,
  networks, databases, process control, and product policy.
- Runtime Driver owns scheduling, clocks, cancellation lanes, and host
  shutdown translation.
- Execution Adapter owns Module generation, endpoint mechanics, isolation, and
  host-specific failure semantics.
- Stateful behavior, persistence, migrations, Auth, HTTP, Console, Story,
  Workflow, Outbox, telemetry, and secrets are optional Module or Adapter
  concerns, never Kernel features.
- The same portable Kernel must compile for native and supported WebAssembly
  targets without target-specific host services in its core state machine.
- Parallelism comes from placing Module Instances on more Execution Lanes;
  Kernel correctness never requires work stealing, runtime Instance
  migration, or thread-safe Module state.

## Workspace ownership

`lenso-app-plan` owns serializable plan data. `lenso-kernel` owns the portable
runtime and deterministic test Driver. `lenso-runtime-conformance` owns the
product-neutral executable test surface for Kernel Interfaces. Runtime Drivers,
Execution Adapters, Modules, authoring tools, and examples are outer owners
being extracted under ADR 0064. New concerns must first identify their
Capability, Module, Adapter, or authoring seam before adding a crate.

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

ADRs 0030–0064 are normative for vNext. The vNext architecture overview,
validation roadmap, and research notes are retained beside them. Removed
v0.3.x implementation docs are not recreated in this branch.
