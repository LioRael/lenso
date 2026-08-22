# Lenso

Lenso is a local-first, language-independent modular application runtime.
The `main` branch contains only the vNext runtime and its design evidence. The
final v0.3.x source remains available from the `lenso@0.3.47` tag and Git
history.

## Workspace

The workspace is in a staged repository extraction. Its durable core ownership
is intentionally small:

- `crates/lenso-app-plan` — immutable, language-independent execution input.
- `crates/lenso-kernel` — portable Kernel state machine and Runtime Driver
  interface, with a deterministic Driver for conformance tests.
- `crates/lenso-runtime-conformance` — product-neutral fixtures that make the
  Kernel Interface executable without a concrete Driver, Adapter, product
  Capability, or example App.

Runtime Drivers, Execution Adapters, protocol tooling, Capability packages,
optional Modules, authoring tools, and examples remain temporary workspace
members while they move to the owners defined by ADR 0064. Their physical
presence does not make them part of portable core ownership.

The Kernel has no Service, Provider, System Plane, Console, Story, Auth,
PostgreSQL, Outbox, Workflow, migration, release, or discovery implementation.
Those concerns can return only as ordinary Modules, Execution Adapters,
authoring tools, or separate repositories when a vNext decision assigns them an
owner.

## Quick start

```sh
cargo fmt --all -- --check
cargo xtask check-core-repository-boundary
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo run --locked -p lenso-runner
```

The portable Kernel and its host Drivers are compile-checked for
`wasm32-unknown-unknown` and `wasm32-wasip2` in CI. Browser and WASIp2 Runners
can install only Adapter packages their hosts can execute; Kernel rejects a
Plan before preparation when its immutable Runner-assembled Adapter catalog
does not provide a selected open execution-class identity.

## Architecture

- [`CONTEXT.md`](CONTEXT.md) is the canonical vocabulary and invariant set.
- [`docs/architecture/lenso-vnext.md`](docs/architecture/lenso-vnext.md) is the
  runtime overview.
- [`docs/architecture/lenso-authoring.md`](docs/architecture/lenso-authoring.md)
  documents project authoring and Plan resolution.
- [`docs/adr/README.md`](docs/adr/README.md) routes the normative ADRs 0030–0064.
- [`docs/roadmaps/lenso-vnext-validation.md`](docs/roadmaps/lenso-vnext-validation.md)
  records the evidence sequence.
- [`docs/research/`](docs/research/) contains supporting research, not runtime
  requirements.

## Branches

`main` is the vNext integration and release line. Work starts from
`origin/main` and pull requests target `main`; `next` is retained only as a
pre-cutover integration reference.
