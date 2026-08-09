# Lenso

[![CI](https://github.com/LioRael/lenso/actions/workflows/ci.yml/badge.svg)](https://github.com/LioRael/lenso/actions/workflows/ci.yml)

Agent-ready Rust business systems.

Lenso is a Rust-first modular app framework for composing real product shapes,
verifying every change, and evolving stable boundaries into services. Start
with a runnable host, services, modules, local processes, contracts, migrations,
and Console already connected instead of assembling the surrounding
system one library at a time.

Humans and coding agents work from the same explicit model: product blueprints,
module and service manifests, reviewable change plans, generated contracts,
checks, App Proof, and runtime evidence.

Build modular first. Keep one deployable app while boundaries are changing,
then move selected capabilities into independently delivered services when
those boundaries are ready.

[Read the documentation](https://lenso.dev/docs) ·
[Follow the quickstart](https://lenso.dev/docs/quickstart) ·
[Explore the examples](https://github.com/LioRael/lenso-examples)

## Quickstart

Install the CLI, compose a support application from a product blueprint, and
start the generated system:

```sh
npm install -g @lenso/cli

lenso app compose ./acme-support \
  --blueprint support-desk \
  --apply
cd acme-support
lenso dev up
```

The generated application includes:

- a Rust host with API, worker, migrations, and Postgres integration;
- TypeScript or Rust services registered in a local service workspace;
- explicit module, service, system, and contract declarations;
- Launchpad state for services, modules, addons, and next actions;
- Doctor, App Proof, and App Change Plan evidence;
- Console at `/console`.

Use `cargo install lenso-cli` instead when you prefer the Rust distribution of
the same CLI.

## From product blueprint to running system

1. **Compose a real app.** Begin with a product blueprint that connects the
   host, services, modules, local processes, and Launchpad state.
2. **Extend explicit capabilities.** Add modules, services, addons, or reusable
   capability packs through a reviewable App Change Plan.
3. **Verify the generated system.** Doctor, App Proof, contract checks, smoke
   checks, and Console turn generated state into reviewable evidence.
4. **Evolve stable boundaries.** Keep the product modular, then move selected
   capabilities across process or service boundaries without rewriting the
   product model.

## Why Lenso instead of assembling the rails yourself?

Axum remains the HTTP layer. Lenso adds the business-system lifecycle around
it:

- a runnable host with API, worker, migrations, Postgres, and hosted Console;
- manifests for routes, data, actions, events, lifecycle, dependencies, and
  operator surfaces;
- product blueprints, capability packs, and safe generated-state change plans;
- Runtime Stories that correlate requests, functions, events, outbox work, and
  service activity;
- generated contracts, manifest lints, architecture checks, smoke checks, and
  release gates;
- linked modules today and service-backed modules when a boundary is ready to
  leave the host.

## Lenso Console

Lenso Console is the separately installed operator service for one Lenso
System. It projects managed-Service state through the System Plane so humans and
coding agents can review the same system without putting Console code in a
business Host.

[![Console App Lifecycle view showing services, modules, Doctor checks, App Proof, and next actions](https://lenso.dev/lenso-assets/console/app-lifecycle.png)](https://lenso.dev/lenso-assets/console/app-lifecycle.png)

_App Lifecycle shows the generated services and modules alongside readiness,
Doctor checks, App Proof, change plans, and the next safe command._

[![Runtime Stories execution graph showing a request fan-out across functions, events, and services](https://lenso.dev/lenso-assets/console/runtime-story-graph.png)](https://lenso.dev/lenso-assets/console/runtime-story-graph.png)

_Runtime Stories follows one business flow across requests, functions, events,
and services without losing its correlation._

[![Runtime Overview showing queue pressure, recent activity, failures, and dead letters](https://lenso.dev/lenso-assets/console/runtime-overview.png)](https://lenso.dev/lenso-assets/console/runtime-overview.png)

_Runtime Overview brings queue pressure, active work, recent activity, failures,
and dead letters into one operator workspace._

These screenshots use the seeded demo dataset so the workflows are reproducible.
Read the [Console System Plane architecture](docs/architecture/lenso-console-system-plane.md)
for service boundaries, access controls, evidence, and Module UI isolation.

## Agent-ready development

The public proof point is intentionally concrete:

```text
Build a support ticket module for a Lenso app.
```

The result should be a bounded change with generated code, passing checks, App
Proof, and visible evidence in `/console`, not just a scaffold that compiles.
The working loop is:

```text
product brief -> app compose -> change plan -> implementation -> checks + App Proof -> Lenso Console
```

Install the public skill pack directly from this repository:

```sh
npx skills add LioRael/lenso
```

The pack covers business planning, app composition, host setup, linked Module
authoring, Service authoring, Console Surface authoring, API clients,
Autonomous Services, Contract evolution, Durable Workflows, Module extraction,
incident recovery, and reviewed releases. `lenso-start` is the human-invoked
router; the other skills have narrow task descriptions so agents can discover
the right workflow without loading unrelated instructions.

See the [public skill catalog](skills/README.md). Manifests, contracts, current
CLI help, repository checks, and Console evidence remain the inspectable source
of truth for each workflow.

See the [agent-ready module demo](docs/agent-ready-module-demo.md). Runnable
support-ticket and account-profile examples are guarded in
[`LioRael/lenso-examples`](https://github.com/LioRael/lenso-examples) by module
smoke checks and real host API smokes.

## Packages and repositories

| Surface | Role |
| --- | --- |
| [`lenso`](https://crates.io/crates/lenso) | Public Rust facade for module declarations, manifest lints, and the narrow host boot API. |
| [`@lenso/cli`](https://www.npmjs.com/package/@lenso/cli) / [`lenso-cli`](https://crates.io/crates/lenso-cli) | Compose apps, manage generated state, author capabilities, run local systems, and operate modules and services. |
| [`LioRael/lenso`](https://github.com/LioRael/lenso) | This repository: backend platform crates, built-in Modules, System Plane contracts, framework SDKs, migrations, and architecture checks. |
| [`LioRael/lenso-console`](https://github.com/LioRael/lenso-console) | Independent Console Service, web shell, composition Store, and reviewed same-realm ESM Module UI host. |
| [`LioRael/lenso-examples`](https://github.com/LioRael/lenso-examples) | Runnable product, module, service, and integration examples. |
| [`lenso.dev`](https://lenso.dev) | Product documentation, guides, API reference, and agent-readable docs. |

Add the Rust authoring surface directly when building a module or custom host:

```sh
cargo add lenso
```

Generated hosts enable the crate's `host` feature. Lenso Console is installed
and operated as a separate Service; managed hosts never serve Console assets.

Keep `lenso`, `lenso-cli`, and `lenso-console` checked out as siblings
when changing behavior across backend, CLI, and Console boundaries. Repository
operations notes live in
[docs/repository-operations.md](docs/repository-operations.md).

## Architecture Overview

- Modular monolith first: linked modules run in-process and can later be
  extracted behind independently running services over HTTP, gRPC, or event boundaries
  ([guide](docs/architecture/linked-to-service-module.md)).
- Services: independently running backends that provide one or more modules and
  are observed by the host.
- Service Workspace: local development control plane in `lenso.workspace.json`;
  `lenso service create` registers Rust/TS providers there, and
  `lenso service dev` starts them before the host. Workspace tools also check
  provider readiness and read older `.lenso/services.json` files during
  migration.
- Modules: business capabilities installed through `lenso module install`.
  A module may resolve to linked Rust code, a service-provided module, a
  bundled host capability, or a future sandbox package.
- Rust first: API, worker, migrations, platform crates, modules, contract generators, and architecture checks are Rust workspace members.
- Explicit SQL and Postgres: no custom ORM, no hidden database magic.
- Transactional outbox: module writes and emitted events commit atomically.
- In-process outbox relay: worker claims outbox rows, dispatches registered handlers, and marks delivery state.
- Contract layer: Rust-authored OpenAPI and JSON Schema artifacts are committed.

More detail lives in [docs/architecture/overview.md](docs/architecture/overview.md). Hard rules live in [docs/architecture/rules.md](docs/architecture/rules.md).

`lenso module install` is the primary business-capability entrypoint. It may
enable linked code or resolve a `lenso.module-release.json` to a provider
service. `lenso service install` remains the lower-level provider/process
operation for operators who want to connect a service before enabling one of
its modules.

A small first-party proof path is `audit-log`: install it by name from the
official catalog, refresh the hosted Console, then inspect the module in
`/console/modules?module=audit-log` and its generic admin data in `/console/data`.
The module declares the `audit_log.events.read` data capability and surfaces
Audit Events through the Console's Data Surfaces panel.

First-time local setup lives in [docs/getting-started.md](docs/getting-started.md).

## Repository Layout

- `crates/`
  - `lenso-contracts`: shared declaration contracts re-exported by `lenso` and consumed by platform crates.
  - `lenso`: public Rust facade crate for serializable module-authoring declarations and manifest lints.
  - `lenso-api`: Axum HTTP API app.
  - `lenso-api-contracts`: owner-local contract generator and architecture checks.
  - `lenso-worker`: background worker and outbox relay app.
  - `lenso-migrate`: deterministic migration runner.
  - `lenso-bootstrap`: composition root listing the concrete modules; both `lenso-api` and `lenso-worker` wire their module set from here.
  - `platform-core`: config, errors, context, DB, migrations, events, outbox, health, telemetry primitives.
  - `platform-http`: Axum adapters, request context middleware, JSON extractor, error responses, health routes, and the `OpenApiRouter` re-exports for single-source OpenAPI.
  - `platform-runtime`: embedded runtime primitives for functions, triggers, queues, flows, retries, and store traits.
  - `platform-module`: behavior seams and compatibility re-exports for Module loading and Linked bindings.
  - `platform-system-plane`: capability-neutral managed-Service management kernel mounted only on the dedicated System Plane listener.
  - `platform-testing`: shared test database helpers.
- `modules/`
  - `auth`: host-owned authentication anchor and development session routes.
    Session resolution defaults to Postgres and can opt into Redis by enabling
    the auth module's `redis` feature, setting `REDIS_URL`, and setting runtime
    config `auth.session_cache=redis`.
  - `auth-oauth`: reusable OAuth client flow substrate for authentication adapters.
  - `auth-anonymous`: first-party anonymous provider for guest sessions.
  - `auth-password`: first-party password provider for the auth anchor.
  - `auth-phone`: first-party phone OTP and phone password provider for the auth anchor.
  - `auth-github`: first-party GitHub OAuth provider built on `auth-oauth`.
  - `auth-google`: first-party Google OAuth/OIDC provider built on `auth-oauth`.
- `fixtures/`
  - `provider`: internal provider fixture for integration and protocol checks.
- `contracts/`
- Generated and curated OpenAPI, JSON Schema, and error contracts.
- `infrastructure/local/`
  - Local Postgres and optional OpenTelemetry collector config.

Lenso Console source, its deployable Service backend, and the Runtime Stories
Console module live in the sibling `../lenso-console` repository. This framework
repository owns public contracts, Module release declarations, and System Plane
capability contracts consumed by the Console.

## Local Development

Prerequisites:

- Rust toolchain compatible with the workspace (`rust-version = 1.94`).
- Cargo and Docker Compose for development commands.
- Docker if you want local Postgres.
- The sibling `../lenso-console` checkout if you want to work on the Console.

Create local environment config:

```sh
cp .env.example .env
```

Module-local config belongs in env/static host config, not runtime-config DB
overrides. Use `LENSO_MODULE_<MODULE>__<KEY>=<json-or-string>` for local values;
for example `LENSO_MODULE_AUTH_PASSWORD__JWT_ISSUER=acme` is available to linked
module code through `ctx.config.module_local_config("auth-password")`. Module
load toggles remain `LENSO_MODULE_<MODULE>_ENABLED=false` and are also surfaced
as restart-only runtime config for operator overrides.

`auth-phone` also keeps OTP secrets in module-local config. Set
`LENSO_MODULE_AUTH_PHONE__OTP_SECRET=<secret>` outside local development; the
secret is intentionally not exposed as editable runtime config.

`REDIS_URL` is optional for the platform itself. The first-party auth module uses
Redis only when its dependency is built with the `redis` feature and runtime
config sets `auth.session_cache=redis`; otherwise session resolution reads
Postgres directly.

Generated hosts can install that auth profile with:

```sh
lenso module install auth --profile redis-session-cache
```

The CLI applies the module descriptor profile, enabling the auth dependency's
`redis` Cargo feature, writing `REDIS_URL=redis://localhost:6379/0` to `.env`,
and recording the runtime default `auth.session_cache=redis` in
`.lenso/runtime-config-defaults.json`. Provide a Redis service separately; the
starter Docker Compose file only starts Postgres by default.

Typical loop:

```sh
docker compose -f infrastructure/local/docker-compose.yml up -d postgres
cargo run --locked -p lenso-migrate
cargo run --locked -p lenso-api
```

Worker:

```sh
cargo run --locked -p lenso-worker
```

Console Service and CLI development shortcuts:

```sh
# Run the complete local Console Service from its own repository.
cd ../lenso-console
pnpm run service:serve

# Serve a generated host through the local lenso-cli checkout.
cargo run --locked --manifest-path ../lenso-cli/Cargo.toml -- serve --repo-root <host-root>
```

Use an absolute or relative path for `<host-root>`; it does not need to be a
sibling directory. Managed Services never host Console web assets.

Production Console access must use real auth, not development bearer tokens.
With `APP_ENV=production`, `dev-user:*` and `dev-service:*` tokens are ignored.
Browser users should sign in through password auth or OIDC, then receive
the Console Service's own operator grant through `lenso console operator bootstrap`.

OpenTelemetry collector for local span export:

```sh
docker compose -f infrastructure/local/docker-compose.yml --profile observability up --pull missing --wait --wait-timeout 45 postgres otel-collector
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 cargo run --locked -p lenso-api
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 cargo run --locked -p lenso-worker
```

The local collector receives OTLP over gRPC on `localhost:4317` and OTLP over
HTTP on `localhost:4318`. The Rust exporter is configured for gRPC, so use:

```sh
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
```

To verify the local loop without starting the API and worker, run:

```sh
docker compose -f infrastructure/local/docker-compose.yml --profile observability up --pull missing --wait --wait-timeout 45 otel-collector
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 cargo run --locked -p lenso-platform-core --example otel
```

User-facing examples live in
[LioRael/lenso-examples](https://github.com/LioRael/lenso-examples).

The platform example emits one outbox-style span and one function-style span.
Inspect collector debug logs for
`lenso.correlation_id`, `lenso.story_id`, `lenso.execution.kind`,
`lenso.outbox_event_id`, and `lenso.function_run_id`.

Common local collector failures:

- Docker is not running: the observability Compose command fails during the Docker daemon
  preflight.
- The observability profile is not selected: start the collector through
  the observability Compose command or use
  `docker compose -f infrastructure/local/docker-compose.yml --profile observability ...`.
- Ports `4317` or `4318` are already occupied: stop the conflicting process or
  update both the compose ports and `OTEL_EXPORTER_OTLP_ENDPOINT`.
- The collector config path is wrong: the observability Compose command validates
  `infrastructure/local/docker-compose.yml` before startup; the expected mount is
  `infrastructure/local/otel-collector.yaml` to `/etc/otelcol/config.yaml`.
- First startup needs an image pull: the recipe uses visible Compose output and a
  45 second service wait timeout so failures are easier to see.

Regenerate contracts after changing Rust/OpenAPI sources:

```sh
cargo run --locked -p lenso-api-contracts --bin generate-contracts
```

## Common Commands

- `cargo fmt --all -- --check`: check Rust formatting.
- `cargo test --locked --workspace`: run Rust workspace tests.
- `cargo check --locked --workspace --all-targets`: compile the whole workspace.
- `cargo test --locked -p lenso-api-contracts --test architecture`: run architecture guardrails.
- `cargo test --locked -p lenso-api-contracts --test generated_artifacts`: verify committed contract bytes.
- `cargo run --locked -p lenso-api-contracts --bin generate-contracts`: generate OpenAPI and JSON Schema artifacts.
- `.github/workflows/ci.yml`: the explicit CI quality gate.

## Quality Gates

The CI quality gate runs:

- Check Rust formatting, compile every Rust workspace target, and run Rust tests.
- Regenerate contracts, then fail if committed artifacts changed.
- Run architecture guardrails.

The owner integration tests also fail on:

- A root `tools/`, `scripts/`, or task-runner file.
- DDD/Clean Architecture folders inside modules: `api`, `application`, `domain`, `infrastructure`.
- Cross-module imports inside module source code.
- OpenAPI route invariants in the API owner test.
- Stale contract artifacts in the generated-artifact test.
- Missing event payload contracts for current events.

Generated files are source-controlled artifacts, but they are not hand-edited. Update Rust/OpenAPI sources, then regenerate.

## Release Readiness

Run the explicit quality commands from `.github/workflows/ci.yml` before a
Release-plz or Changesets release pull request. Cargo and npm publish
independently from this repository; Console checks live in the sibling
`lenso-console` repository. The release scope and package checklist live in
[docs/release-readiness.md](docs/release-readiness.md).

Release packaging and tagging steps live in
[docs/release-process.md](docs/release-process.md).
