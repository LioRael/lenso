# Lenso

Lenso is a local-first, language-independent modular application runtime.
The `main` branch contains only the vNext runtime and its design evidence. The
final v0.3.x source remains available from the `lenso@0.3.47` tag and Git
history.

## Workspace

The repository extraction is complete. Its durable product ownership is:

- `crates/lenso-app-plan` — immutable, language-independent Plan Snapshots and
  validated Plan Transition data.
- `crates/lenso-kernel` — portable Kernel state machine and Runtime Driver
  interface, with a deterministic Driver for conformance tests.
- `crates/lenso-runtime-conformance` — product-neutral fixtures that make the
  Kernel Interface executable without a concrete Driver, Adapter, product
  Capability, or example App.

Runtime Drivers, Execution Adapters, protocol tooling, Capability packages,
optional Plugins, authoring tools, and examples live in the owner repositories
defined by ADR 0064 and are consumed through versioned dependencies.

- [lenso-protocols](https://github.com/LioRael/lenso-protocols) owns portable
  contract tooling and conformance vectors.
- [lenso-runtime-rust](https://github.com/LioRael/lenso-runtime-rust) and
  [lenso-bun-adapter](https://github.com/LioRael/lenso-bun-adapter) own host
  runtimes and Execution Adapters.
- The observability and authentication owner repositories provide optional
  Plugin contracts and implementations; their current repository names are
  compatibility-era names, not public runtime concepts.
- [lenso-cli](https://github.com/LioRael/lenso-cli) owns authoring, while
  [lenso-examples](https://github.com/LioRael/lenso-examples) owns example
  Capabilities and executable fixtures.

The Kernel has no Service, Provider, System Plane, Console, Story, Auth,
PostgreSQL, Outbox, Workflow, migration, release, or discovery implementation.
Those concerns can return only as ordinary Plugins, Execution Adapters,
authoring tools, or separate repositories when a vNext decision assigns them an
owner.

## Agent skills

The [project skill pack](skills/README.md) turns the vNext architecture into
cross-repository planning, Capability, Plugin, App configuration, and runtime
workflows without relocating implementation ownership. List the six workflows
with:

```sh
npx skills add LioRael/lenso --list
```

Start with `lenso-start` when the owning seam is not yet clear.
The [Agents and skills guide](docs/agents/skills.md) documents invocation,
installation, progressive disclosure, contributor validation, and behavioral
forward testing.

## Quick start

```sh
cargo fmt --all -- --check
cargo xtask check-core-repository-boundary
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
```

The portable Plan, Kernel, and conformance Interface are compile-checked for
`wasm32-unknown-unknown` and `wasm32-wasip2` in CI. Host Driver and Adapter
repositories own their target-specific checks against released core packages.

## Architecture

- [`CONTEXT.md`](CONTEXT.md) is the canonical vocabulary and invariant set.
- [`docs/architecture/lenso-vnext.md`](docs/architecture/lenso-vnext.md) is the
  runtime overview.
- [`docs/architecture/lenso-authoring.md`](docs/architecture/lenso-authoring.md)
  documents project authoring and Plan resolution.
- [`docs/adr/README.md`](docs/adr/README.md) routes the normative ADRs 0030–0067.
- [`docs/roadmaps/lenso-vnext-validation.md`](docs/roadmaps/lenso-vnext-validation.md)
  records the evidence sequence.
- [`docs/research/`](docs/research/) contains supporting research, not runtime
  requirements.

## Branches

`main` is the vNext integration and release line. Work starts from
`origin/main` and pull requests target `main`; `next` is retained only as a
pre-cutover integration reference.
