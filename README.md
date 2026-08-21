# Lenso

Lenso is a local-first, language-independent modular application runtime.
The `next` branch contains only the vNext runtime and its design evidence.
The maintained v0.3.x implementation remains on `main`.

## Workspace

The initial vNext workspace is intentionally small:

- `crates/lenso-app-plan` — immutable, language-independent execution input.
- `crates/lenso-authoring` — project Composition, lock validation, canonical
  Plan resolution, and the `lenso` authoring CLI.
- `crates/lenso-kernel` — portable Kernel state machine and Runtime Driver
  interface, with a deterministic Driver for conformance tests.
- `crates/lenso-bun-adapter` — Adapter-owned Bun child-process request
  dispatch, selected JSON-RPC loopback wire, framed prototype, and reverse
  Rust-provider bridge.
- `crates/lenso-browser-driver` — browser/JavaScript Driver using the host
  monotonic clock, timers, and local event loop.
- `crates/lenso-capability-ui-contribution` and
  `crates/lenso-capability-web-shell` — portable generated Interfaces for
  target-owned routes, navigation, assets, and declared browser clients.
- `crates/lenso-runner` — native Tokio Runtime Driver and the smallest host
  Runner.
- `crates/lenso-wasip2-driver` — WASI Preview 2 Driver with a host-pumped
  local scheduler and monotonic clock.
- `fixtures/vnext-web-ui` — target-owned Web Shell, Browser Adapter, Auth,
  business Module, and optional UI Contribution tracer bullet.

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
- [`docs/adr/README.md`](docs/adr/README.md) routes the normative ADRs 0030–0058.
- [`docs/roadmaps/lenso-vnext-validation.md`](docs/roadmaps/lenso-vnext-validation.md)
  records the evidence sequence.
- [`docs/research/`](docs/research/) contains supporting research, not runtime
  requirements.

## Branches

`main` is the v0.3.x maintenance and release line. `next` is the vNext
integration line. vNext work starts from `origin/next` and targets `next`.
