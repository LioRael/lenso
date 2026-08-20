# Lenso

Lenso is a local-first, language-independent modular application runtime.
The `next` branch contains only the vNext runtime and its design evidence.
The maintained v0.3.x implementation remains on `main`.

## Workspace

The initial vNext workspace is intentionally small:

- `crates/lenso-app-plan` — immutable, language-independent execution input.
- `crates/lenso-kernel` — portable Kernel state machine and Runtime Driver
  interface, with a deterministic Driver for conformance tests.
- `crates/lenso-runner` — native Tokio Runtime Driver and the smallest host
  Runner.

The Kernel has no Service, Provider, System Plane, Console, Story, Auth,
PostgreSQL, Outbox, Workflow, migration, release, or discovery implementation.
Those concerns can return only as ordinary Modules, Execution Adapters,
authoring tools, or separate repositories when a vNext decision assigns them an
owner.

## Quick start

```sh
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo run --locked -p lenso-runner
```

The portable Kernel is also compile-checked for `wasm32-unknown-unknown` and
`wasm32-wasip2` in CI.

## Architecture

- [`CONTEXT.md`](CONTEXT.md) is the canonical vocabulary and invariant set.
- [`docs/architecture/lenso-vnext.md`](docs/architecture/lenso-vnext.md) is the
  runtime overview.
- [`docs/adr/README.md`](docs/adr/README.md) routes the normative ADRs 0030–0057.
- [`docs/roadmaps/lenso-vnext-validation.md`](docs/roadmaps/lenso-vnext-validation.md)
  records the evidence sequence.
- [`docs/research/`](docs/research/) contains supporting research, not runtime
  requirements.

## Branches

`main` is the v0.3.x maintenance and release line. `next` is the vNext
integration line. vNext work starts from `origin/next` and targets `next`.
