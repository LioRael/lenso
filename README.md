# Lenso Backend Platform

Rust-first service-ready modular monolith scaffold with a local Runtime Console and generated TypeScript SDK.

The platform starts as one deployable system with clear module boundaries. Domains own business capabilities, platform crates provide shared service-kit foundations, the runtime handles durable background work, contracts produce stable API/event/SDK artifacts, and the console gives those runtime primitives an operator-facing UI.

## Architecture Overview

- Modular monolith first: domains run in-process today and can later be extracted behind HTTP, gRPC, or event boundaries.
- Rust first: API, worker, migrations, platform crates, domains, contract generators, and architecture checks are Rust workspace members.
- Explicit SQL and Postgres: no custom ORM, no hidden database magic.
- Transactional outbox: domain writes and emitted events commit atomically.
- In-process outbox relay: worker claims outbox rows, dispatches registered handlers, and marks delivery state.
- Contract layer: Rust-authored OpenAPI and JSON Schema artifacts feed the TypeScript SDK.

More detail lives in [docs/architecture/overview.md](docs/architecture/overview.md). Hard rules live in [docs/architecture/rules.md](docs/architecture/rules.md).

## Repository Layout

- `apps/`
  - `api`: Axum HTTP API app.
  - `worker`: background worker and outbox relay app.
  - `migrate`: deterministic migration runner.
  - `runtime-console`: Vite/React console for runtime traces, queues, functions, events, and dead letters.
- `crates/`
  - `platform-core`: config, errors, context, DB, migrations, events, outbox, health, telemetry primitives.
  - `platform-http`: Axum adapters, request context middleware, JSON extractor, error responses, health routes, and the `OpenApiRouter` re-exports for single-source OpenAPI.
  - `platform-runtime`: embedded runtime primitives for functions, triggers, queues, flows, retries, and store traits.
  - `platform-domain`: the shared `DomainDescriptor` each domain exposes (runtime, event handlers, story display).
  - `platform-admin`: runtime-observability backend for the Runtime Console (`/admin/runtime/*`); reads platform/runtime tables only.
  - `platform-testing`: shared test database helpers.
  - `app-bootstrap`: composition root listing the concrete domains; both `api` and `worker` wire their domain set from here.
- `domains/`
  - `identity`: create-user vertical slice, user table, outbox event, HTTP route, repository, command tests.
  - `notifications`: in-process handler for `identity.user_registered.v1`.
- `contracts/`
  - Generated and curated OpenAPI, JSON Schema, event, error, and runtime contracts.
- `packages/`
  - `ts-sdk`: TypeScript SDK generated from `contracts/openapi/app-api.v1.yaml`.
- `tools/`
  - `generate-contracts`: writes committed contract artifacts from Rust sources.
  - `generate-ts-sdk`: writes committed TypeScript SDK generated files from OpenAPI.
  - `arch-check`: lightweight architecture rule checker.
- `infrastructure/local/`
  - Local Postgres and optional OpenTelemetry collector config.

## Local Development

Prerequisites:

- Rust toolchain compatible with the workspace (`rust-version = 1.94`).
- `just`.
- Docker if you want local Postgres via `just db-up`.
- Node 24 and `pnpm` for the SDK and Runtime Console checks.

Install frontend dependencies:

```sh
just install
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

Runtime Console with seeded data:

```sh
just console
```

Runtime Console pointed at the local API:

```sh
just console-api
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

Regenerate contracts and SDK after changing Rust/OpenAPI sources:

```sh
just generate
```

## Common Commands

- `just`: list available recipes.
- `just install`: install SDK and Runtime Console pnpm dependencies.
- `just fmt`: format Rust and Runtime Console code.
- `just fmt-check`: check Rust and Runtime Console formatting.
- `just check`: run the full local quality gate.
- `just test`: run Rust workspace tests with the locked dependency graph.
- `just rust-check`: run `cargo check` for the whole workspace.
- `just arch-check`: run architecture guardrails.
- `just generate`: generate OpenAPI, JSON Schema, and TypeScript SDK artifacts.
- `just generated-check`: regenerate committed artifacts and fail if they differ from git.
- `just sdk-check`: typecheck `packages/ts-sdk`.
- `just console-check`: format-check, lint, typecheck, and build `apps/runtime-console`.
- `just ci`: run the local CI script.

## Quality Gates

`just ci` runs the same gates as GitHub Actions:

- Install pnpm dependencies for `packages/ts-sdk` and `apps/runtime-console` with frozen lockfiles.
- Check Rust formatting, compile every Rust workspace target, and run Rust tests.
- Regenerate contracts and generated SDK files, then fail if committed artifacts changed.
- Run architecture guardrails.
- Typecheck the TypeScript SDK.
- Format-check, lint, typecheck, and build the Runtime Console.

The architecture checker also fails on:

- DDD/Clean Architecture folders inside domains: `api`, `application`, `domain`, `infrastructure`.
- Cross-domain imports inside domain source code.
- Missing OpenAPI artifacts.
- Stale contract artifacts.
- Stale generated TypeScript SDK files.
- Missing event payload contracts for current events.

Generated files are source-controlled artifacts, but they are not hand-edited. Update Rust/OpenAPI sources, then regenerate.
