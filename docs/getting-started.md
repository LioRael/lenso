# Getting Started

> **Legacy implementation guide:** these commands and artifacts describe the
> current Service-oriented release. The vNext architecture is specified in
> [`architecture/lenso-vnext.md`](architecture/lenso-vnext.md) and is not yet an
> implemented CLI workflow.

This guide follows the public application lifecycle for a local Lenso System.
It begins with one exact App Composition and uses the same public entrypoints as
the integrated Support Desk acceptance.

## Prerequisites

- Node.js and npm for the `@lenso/cli` distribution.
- Rust and Cargo when the App contains Linked Rust Modules.
- Docker when the selected local Store uses Postgres.
- A separately installed Lenso Console Service.

Install the CLI:

```sh
npm install -g @lenso/cli
```

Use `cargo install lenso-cli` instead when you prefer the Rust distribution of
the same binary.

## Public lifecycle

### Compose

Compose the Support Desk App and apply the reviewed result:

```sh
lenso app compose ./acme-support \
  --blueprint support-desk \
  --apply
```

The result is `./acme-support/lenso.app.json`. It is the only application
composition and lock: its revision, immutable Module release digests,
dependency selections, and Linked or Service implementation bindings must all
validate together. Blueprints and addons are authoring inputs, not parallel
runtime state.

The App Composition contains identities and bindings. It does not contain
process commands, bearer tokens, resolved secrets, or production deployment
instructions.

### Run locally

Preview and then run the exact App Composition:

```sh
lenso system dev \
  --system-file ./acme-support/lenso.app.json \
  --dry-run \
  --json

lenso system dev \
  --system-file ./acme-support/lenso.app.json
```

`lenso system dev` realizes Service-backed bindings through a persistent Local
Control Adapter and starts the declared local Workloads through their public
entrypoints. Adapter state records only credential references; credentials
remain in owner-only local files and are never copied into `lenso.app.json`.

Clean up only Adapter-owned local Workloads when the session ends:

```sh
lenso system dev \
  --system-file ./acme-support/lenso.app.json \
  --cleanup
```

### Connect

Start the separately installed Console Service, authenticate as an operator,
and submit the exact System topology and Management Binding through the Console
Service's Connect System API. The connection is bound to the App revision,
Module releases, Surface artifacts, Service identities, and declared Control
Adapter.

Connecting does not require an environment or deployment API. Console records
the connection and loads eligible receipt-bound `console_ui_esm` Surfaces; it
does not create, adopt, release, or deploy Workloads.

### Status

Open Console and inspect the System Connection. System, Service, Module,
Surface, and Workload objects report one of `connected`, `unavailable`,
`incompatible`, or `unmanaged`, always with a direct reason.

The Support Desk Surface should list, create, update, and close tickets through
its generated client and Surface Gateway. It must not receive direct Service
credentials or read the Service Store. A supported local Suspend/Resume or
Stop/Start operation is asynchronous: follow its Operation Record until Console
shows the final operational state.

If the Local Control Adapter is unavailable, Workload observation is unknown
and mutation is rejected without queueing or fallback. Console does not release or deploy
the System; production delivery remains repository- and operator-owned.

## Service capability tiers

Provider `lenso.service.v1` can be authored in Rust or TypeScript and relies on
Host-owned runtime coordination. Autonomous Service `lenso.service.v2` is
Rust-only and owns its runtime, Service Store, identity boundary, direct
HTTP/gRPC, Event Contracts, and Durable Workflows. See
[Service Capability Tiers](architecture/service-capability-tiers.md).

## Framework repository development

When changing the framework itself, use the owner-local commands rather than
the generated App commands:

```sh
docker compose -f infrastructure/local/docker-compose.yml up -d postgres
cargo run --locked -p lenso-migrate
cargo run --locked -p lenso-api
cargo run --locked -p lenso-worker
```

The sibling `lenso-console` repository owns Console Service startup and checks.
The sibling `lenso-examples` repository owns the integrated Support Desk
acceptance.

## Release checks

Before approving a repository-owned release pull request, run the explicit
commands from `.github/workflows/ci.yml` and follow
[the release process](release-process.md). Publication uses the repository's
Trusted Publisher workflow; local development authority does not grant
publication or production deployment authority.
