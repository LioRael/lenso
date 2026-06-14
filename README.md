# Lenso Backend Platform

[![CI](https://github.com/LioRael/lenso/actions/workflows/ci.yml/badge.svg)](https://github.com/LioRael/lenso/actions/workflows/ci.yml)

Rust-first service-ready modular monolith scaffold with generated contracts and a TypeScript SDK.

The platform starts as one deployable backend system with clear module boundaries. Modules own product capabilities, platform crates provide shared service-kit foundations, the runtime handles durable background work, and contracts produce stable API/event/SDK artifacts. The Runtime Console lives in the sibling `lenso-runtime-console` repository and consumes this backend API.

## Repository Pair

- Backend platform: this repository owns Rust services, platform crates, modules, migrations, contracts, and the generated TypeScript SDK.
- Runtime Console: [`LioRael/lenso-runtime-console`](https://github.com/LioRael/lenso-runtime-console) owns the frontend workspace that consumes the admin APIs and SDK from this repository.

Keep both repositories checked out as siblings when working on Console-backed backend changes:

```text
framework/
  lenso/
  lenso-runtime-console/
```

Repository operations notes, including branch protection and cross-repo CI
wiring, live in [docs/repository-operations.md](docs/repository-operations.md).

## Architecture Overview

- Modular monolith first: modules run in-process today and can later be extracted behind HTTP, gRPC, or event boundaries.
- Rust first: API, worker, migrations, platform crates, modules, contract generators, and architecture checks are Rust workspace members.
- Explicit SQL and Postgres: no custom ORM, no hidden database magic.
- Transactional outbox: module writes and emitted events commit atomically.
- In-process outbox relay: worker claims outbox rows, dispatches registered handlers, and marks delivery state.
- Contract layer: Rust-authored OpenAPI and JSON Schema artifacts feed the TypeScript SDK.

More detail lives in [docs/architecture/overview.md](docs/architecture/overview.md). Hard rules live in [docs/architecture/rules.md](docs/architecture/rules.md).

First-time local setup lives in [docs/getting-started.md](docs/getting-started.md).

## Repository Layout

- `apps/`
  - `api`: Axum HTTP API app.
  - `worker`: background worker and outbox relay app.
  - `migrate`: deterministic migration runner.
- `crates/`
  - `platform-core`: config, errors, context, DB, migrations, events, outbox, health, telemetry primitives.
  - `platform-http`: Axum adapters, request context middleware, JSON extractor, error responses, health routes, and the `OpenApiRouter` re-exports for single-source OpenAPI.
  - `platform-runtime`: embedded runtime primitives for functions, triggers, queues, flows, retries, and store traits.
  - `platform-module`: module framework contracts for `ModuleManifest`, `ModuleBinding`, linked/remote sources, admin surfaces, and console surfaces.
  - `platform-admin`: runtime-observability backend for the Runtime Console (`/admin/runtime/*`); reads platform/runtime tables only.
  - `platform-admin-data`: schema-admin backend for generic module data (`/admin/data/*`).
  - `platform-testing`: shared test database helpers.
  - `app-bootstrap`: composition root listing the concrete modules; both `api` and `worker` wire their module set from here.
- `modules/`
  - `identity`: framework fixture for a create-user vertical slice, user table, outbox event, HTTP route, repository, command tests.
  - `notifications`: framework fixture for an in-process handler of `identity.user_registered.v1`.
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

Runtime Console source lives in the sibling `../lenso-runtime-console` repository. This backend repository still owns the `/admin/runtime/*`, `/admin/data/*`, module manifest, and contract APIs that the Console consumes.

## Local Development

Prerequisites:

- Rust toolchain compatible with the workspace (`rust-version = 1.94`).
- `just`.
- Docker if you want local Postgres via `just db-up`.
- Node 24 and `pnpm` for the SDK checks.
- The sibling `../lenso-runtime-console` checkout if you want to run the Runtime Console locally.

Install SDK dependencies:

```sh
just install
cp .env.example .env
```

Install Runtime Console dependencies from the sibling checkout when needed:

```sh
pnpm --dir ../lenso-runtime-console install
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

Embedded remote admin iframe demo:

```sh
just db-up
just migrate
just embedded-admin-demo
```

This starts the remote module example, the API with `remote-crm`,
`remote-crm-embedded`, and `remote-crm-declarative` configured, and the Runtime
Console in API mode. Open the Data page and select `remote-crm-embedded` to see
the sandboxed iframe surface, or `remote-crm-declarative` to see the host-rendered
declarative surface. The `remote-crm` fixture also declares a remote event
handler that can ask the host to enqueue its declared runtime function.
If the default ports are busy, override them with `REMOTE_MODULE_ADDR`,
`HTTP_PORT`, or `CONSOLE_PORT`.

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

Remote module release demo:

```sh
pnpm --dir ../lenso-runtime-console demo:release
```

This starts the internal `hello-action` fixture, reads its manifest, checks its
schema-admin, HTTP route, and runtime function endpoints, and verifies the short
install path: `lenso module add <manifest-url>`. It uses the sibling
`../lenso-runtime-console` checkout for local release validation.

User-facing examples that install the published npm packages live in
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

Regenerate contracts and SDK after changing Rust/OpenAPI sources:

```sh
just generate
```

## Common Commands

- `just`: list available recipes.
- `just install`: install SDK pnpm dependencies.
- `just fmt`: format Rust code.
- `just fmt-check`: check Rust formatting.
- `just check`: run the full local quality gate.
- `just test`: run Rust workspace tests with the locked dependency graph.
- `just rust-check`: run `cargo check` for the whole workspace.
- `just arch-check`: run architecture guardrails.
- `just generate`: generate OpenAPI, JSON Schema, and TypeScript SDK artifacts.
- `just generated-check`: regenerate committed artifacts and fail if they differ from git.
- `just sdk-check`: typecheck `packages/ts-sdk`.
- `just console-check`: run the sibling Runtime Console quality gate.
- `just demo-release`: convenience wrapper for the sibling Console repo's first-user remote module demo.
- `just remote-module-run-demo`: convenience wrapper for the sibling Console repo's installable remote module happy-path demo.
- `just release-check`: run the local release gate.
- `just ci`: run the local CI script.

## Quality Gates

`just ci` runs the same gates as GitHub Actions:

- Install pnpm dependencies for `packages/ts-sdk` with frozen lockfiles.
- Check Rust formatting, compile every Rust workspace target, and run Rust tests.
- Regenerate contracts and generated SDK files, then fail if committed artifacts changed.
- Run architecture guardrails.
- Typecheck the TypeScript SDK.

The architecture checker also fails on:

- DDD/Clean Architecture folders inside modules: `api`, `application`, `domain`, `infrastructure`.
- Cross-module imports inside module source code.
- Missing OpenAPI artifacts.
- Stale contract artifacts.
- Stale generated TypeScript SDK files.
- Missing event payload contracts for current events.

Generated files are source-controlled artifacts, but they are not hand-edited. Update Rust/OpenAPI sources, then regenerate.

## Release Readiness

Use `just release-check` before cutting a release branch or tag. It runs the
backend quality gate. Runtime Console release checks live in the sibling
`lenso-runtime-console` repository. The release scope and manual smoke checklist live in
[docs/release-readiness.md](docs/release-readiness.md).

Release packaging and tagging steps live in
[docs/release-process.md](docs/release-process.md).
