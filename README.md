# Lenso Backend Platform

Rust-first service-ready modular monolith scaffold for reusable backend projects.

The platform starts as one deployable system with clear module boundaries. Domains own business capabilities, the platform crates provide shared service-kit foundations, the runtime handles durable background work, and contracts produce stable API/event/SDK artifacts.

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
- `crates/`
  - `platform-core`: config, errors, context, DB, migrations, events, outbox, health, telemetry primitives.
  - `platform-http`: Axum adapters, request context middleware, JSON extractor, error responses, health routes.
  - `platform-runtime`: embedded runtime primitives for functions, triggers, queues, flows, retries, and store traits.
  - `platform-testing`: shared test database helpers.
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

- Rust toolchain compatible with the workspace.
- `just`.
- Docker if you want local Postgres via `just db-up`.
- `pnpm` and `tsc` for the TypeScript SDK checks.

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

Regenerate contracts and SDK:

```sh
just generate-contracts
just generate-ts-sdk
```

## Common Commands

- `just fmt`: format Rust.
- `just check`: run Rust formatting, workspace check, tests, architecture checks, generation, and SDK typecheck.
- `just test`: run Rust workspace tests.
- `just arch-check`: run architecture guardrails.
- `just generate-contracts`: generate OpenAPI and JSON Schema artifacts.
- `just generate-ts-sdk`: generate TypeScript SDK files.
- `just ci`: run the local CI script.

## Quality Gates

The architecture checker fails on:

- DDD/Clean Architecture folders inside domains: `api`, `application`, `domain`, `infrastructure`.
- Cross-domain imports inside domain source code.
- Missing OpenAPI artifacts.
- Stale contract artifacts.
- Stale generated TypeScript SDK files.
- Missing event payload contracts for current events.

Generated files are source-controlled artifacts, but they are not hand-edited. Update Rust/OpenAPI sources, then regenerate.
