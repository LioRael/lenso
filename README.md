# Lenso Backend Platform

[![CI](https://github.com/LioRael/lenso/actions/workflows/ci.yml/badge.svg)](https://github.com/LioRael/lenso/actions/workflows/ci.yml)

Rust-first service-ready modular monolith scaffold with generated contracts.

The platform starts as one deployable backend system with clear module boundaries. Modules own product capabilities, platform crates provide shared service-kit foundations, the runtime handles durable background work, and contracts produce stable API/event artifacts. The Runtime Console lives in the sibling `lenso-runtime-console` repository and consumes this backend API.

## Repository Set

- Backend platform: this repository owns Rust services, platform crates, modules, migrations, and contracts.
- Runtime Console: [`LioRael/lenso-runtime-console`](https://github.com/LioRael/lenso-runtime-console) owns the frontend workspace that consumes the admin APIs from this repository.
- CLI: [`LioRael/lenso-cli`](https://github.com/LioRael/lenso-cli) owns the standalone `lenso` command and host starter template.

Keep the relevant repositories checked out as siblings when working across
backend, Console, or CLI boundaries:

```text
framework/
  lenso/
  lenso-cli/
  lenso-runtime-console/
```

Repository operations notes, including branch protection and cross-repo CI
wiring, live in [docs/repository-operations.md](docs/repository-operations.md).

## Published Packages

The first public Rust authoring surface is published on crates.io:

```sh
cargo add lenso@0.1.0
```

The `lenso` crate is the public Rust facade for module-authoring declarations
and manifest lints. With its `host` feature enabled, it also exposes the narrow
host boot facade used by generated host applications.

Runtime Console and JavaScript module packages are owned outside this backend
repository. Runnable examples live in
[`LioRael/lenso-examples`](https://github.com/LioRael/lenso-examples).

A transitional host starter lives in
[`LioRael/lenso-cli`](https://github.com/LioRael/lenso-cli) and is scaffolded
with the `lenso` CLI (`lenso host init <dir>`). It shows the current API,
worker, migration, and local Postgres shape. New starters should use
`lenso = { features = ["host"] }`; the `lenso-host` crate remains as a
compatibility re-export for existing generated hosts.

## Architecture Overview

- Modular monolith first: modules run in-process today and can later be extracted behind HTTP, gRPC, or event boundaries.
- Rust first: API, worker, migrations, platform crates, modules, contract generators, and architecture checks are Rust workspace members.
- Explicit SQL and Postgres: no custom ORM, no hidden database magic.
- Transactional outbox: module writes and emitted events commit atomically.
- In-process outbox relay: worker claims outbox rows, dispatches registered handlers, and marks delivery state.
- Contract layer: Rust-authored OpenAPI and JSON Schema artifacts are committed.

More detail lives in [docs/architecture/overview.md](docs/architecture/overview.md). Hard rules live in [docs/architecture/rules.md](docs/architecture/rules.md).

First-time local setup lives in [docs/getting-started.md](docs/getting-started.md).

## Repository Layout

- `crates/`
  - `lenso-contracts`: shared declaration contracts re-exported by `lenso` and consumed by platform crates.
  - `lenso`: public Rust facade crate for serializable module-authoring declarations and manifest lints.
  - `lenso-api`: Axum HTTP API app.
  - `lenso-worker`: background worker and outbox relay app.
  - `lenso-migrate`: deterministic migration runner.
  - `lenso-bootstrap`: composition root listing the concrete modules; both `lenso-api` and `lenso-worker` wire their module set from here.
  - `lenso-host`: compatibility re-export for existing starter hosts.
  - `platform-core`: config, errors, context, DB, migrations, events, outbox, health, telemetry primitives.
  - `platform-http`: Axum adapters, request context middleware, JSON extractor, error responses, health routes, and the `OpenApiRouter` re-exports for single-source OpenAPI.
  - `platform-runtime`: embedded runtime primitives for functions, triggers, queues, flows, retries, and store traits.
  - `platform-module`: behavior seams and compatibility re-exports for module loading, linked bindings, and schema-admin data/action sources.
  - `platform-admin`: runtime-observability backend for the Runtime Console (`/admin/runtime/*`); reads platform/runtime tables only.
  - `platform-admin-data`: schema-admin backend for generic module data (`/admin/data/*`).
  - `platform-testing`: shared test database helpers.
- `modules/`
  - `auth`: host-owned authentication anchor and development session routes.
  - `auth-password`: first-party password provider for the auth anchor.
  - `story`: platform-owned Runtime Console story surface.
- `fixtures/`
  - `remote-module`: internal remote-module fixture for integration and protocol checks.
- `contracts/`
  - Generated and curated OpenAPI, JSON Schema, event, error, and runtime contracts.
- `tools/`
  - `generate-contracts`: writes committed contract artifacts from Rust sources.
  - `arch-check`: lightweight architecture rule checker.
- `infrastructure/local/`
  - Local Postgres and optional OpenTelemetry collector config.

Runtime Console source lives in the sibling `../lenso-runtime-console` repository. This backend repository still owns the `/admin/runtime/*`, `/admin/data/*`, module manifest, and contract APIs that the Console consumes.

## Local Development

Prerequisites:

- Rust toolchain compatible with the workspace (`rust-version = 1.94`).
- `just`.
- Docker if you want local Postgres via `just db-up`.
- The sibling `../lenso-runtime-console` checkout if you want to work on the Runtime Console.

Create local environment config:

```sh
cp .env.example .env
```

Typical loop:

```sh
just db-up
just migrate
just api
```

Worker:

```sh
just worker
```

OpenTelemetry collector for local span export:

```sh
just observability-up
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 just api
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 just worker
```

The local collector receives OTLP over gRPC on `localhost:4317` and OTLP over
HTTP on `localhost:4318`. The Rust exporter is configured for gRPC, so use:

```sh
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
```

To verify the local loop without starting the API and worker, run:

```sh
just otel-smoke
```

User-facing examples live in
[LioRael/lenso-examples](https://github.com/LioRael/lenso-examples).

The smoke command starts the collector, emits one outbox-style span and one
function-style span, and checks collector debug logs for
`lenso.correlation_id`, `lenso.story_id`, `lenso.execution.kind`,
`lenso.outbox_event_id`, and `lenso.function_run_id`.

Common local collector failures:

- Docker is not running: `just observability-up` fails during the Docker daemon
  preflight.
- The observability profile is not selected: start the collector through
  `just observability-up` or use
  `docker compose -f infrastructure/local/docker-compose.yml --profile observability ...`.
- Ports `4317` or `4318` are already occupied: stop the conflicting process or
  update both the compose ports and `OTEL_EXPORTER_OTLP_ENDPOINT`.
- The collector config path is wrong: `just observability-up` validates
  `infrastructure/local/docker-compose.yml` before startup; the expected mount is
  `infrastructure/local/otel-collector.yaml` to `/etc/otelcol/config.yaml`.
- First startup needs an image pull: the recipe uses visible Compose output and a
  45 second service wait timeout so failures are easier to see.

Regenerate contracts after changing Rust/OpenAPI sources:

```sh
just generate
```

## Common Commands

- `just`: list available recipes.
- `just fmt`: format Rust code.
- `just fmt-check`: check Rust formatting.
- `just check`: run the default local quality gate without slow smoke checks.
- `just test`: run Rust workspace tests with the locked dependency graph.
- `just rust-check`: run `cargo check` for the whole workspace.
- `just arch-check`: run architecture guardrails.
- `just generate`: generate OpenAPI and JSON Schema artifacts.
- `just generated-check`: regenerate committed artifacts and fail if they differ from git.
- `just release-check`: run the local release gate.
- `just ci`: run the local CI script.

## Quality Gates

`just ci` runs the same gates as GitHub Actions:

- Check Rust formatting, compile every Rust workspace target, and run Rust tests.
- Regenerate contracts, then fail if committed artifacts changed.
- Run architecture guardrails.

The architecture checker also fails on:

- DDD/Clean Architecture folders inside modules: `api`, `application`, `domain`, `infrastructure`.
- Cross-module imports inside module source code.
- Missing OpenAPI artifacts.
- Stale contract artifacts.
- Missing event payload contracts for current events.

Generated files are source-controlled artifacts, but they are not hand-edited. Update Rust/OpenAPI sources, then regenerate.

## Release Readiness

Use `just release-check` before cutting a release branch or tag. It runs the
backend quality gate. Runtime Console release checks live in the sibling
`lenso-runtime-console` repository. The release scope and manual smoke checklist live in
[docs/release-readiness.md](docs/release-readiness.md).

Release packaging and tagging steps live in
[docs/release-process.md](docs/release-process.md).
