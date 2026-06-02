# Architecture Overview

Lenso is a Rust-first service-ready modular monolith with an embedded runtime, an Axum HTTP API, a background worker, a migration runner, a Vite/React Runtime Console, and a generated TypeScript SDK. The platform gives future projects reliable defaults without becoming a framework that hides the system.

## Modular Monolith

The deployable shape is one API app, one worker app, and one migration app. Business capabilities live under `domains/`, and each domain is a Rust crate with its own routes, DTOs, commands, queries, models, repositories, events, jobs, runtime registrations, migrations, and tests.

Domains run in-process today. Extraction later should be mechanical: preserve the public interface and contracts, move the tables, turn in-process calls into client calls, and keep event and function names stable.

## Domain Boundaries

Domains own their data and behavior. A domain may expose:

- HTTP routes through its `module.rs` descriptor.
- Stable in-process calls through `public.rs`.
- Events under `events/`.
- Runtime jobs/functions under `jobs/` and `runtime/`.
- SQL and migrations under `repositories/` and `migrations/`.

Domains must not query another domain's tables or import another domain's internal modules. Cross-domain async work goes through events and runtime function enqueueing.

Current domain examples:

- `identity` owns users, exposes identity HTTP routes, emits `identity.user_registered.v1`, and registers `identity.cleanup_expired_sessions.v1`.
- `notifications` handles identity registration events and registers `notifications.send_welcome_email.v1`.

## Platform Service Kit

The service kit is split into a few crates:

- `platform-core`: config, error model, request context, actor context, IDs, clock, DB pool, migrations, events, transactional outbox, relay primitives, health, shutdown, telemetry foundations, and telemetry query abstractions.
- `platform-http`: Axum request context middleware, auth extractors, standard JSON error responses, JSON extractor, response helpers, health routes, and OpenAPI helpers.
- `platform-runtime`: embedded runtime primitives for functions, triggers, queues, flows, retry policies, registry, worker execution, and store traits.
- `platform-testing`: shared test database utilities.

The service kit should stay stable and small. It exists to remove boilerplate, not to own business behavior.

## Runtime

The runtime is embedded beside the modular monolith. It manages functions, triggers, queues, flows, retry policies, function run persistence, and execution metadata. It does not own business logic.

Domains register runtime functions through their module descriptors. The worker app composes all domain runtime descriptors into a `FunctionRegistry`, registers domain event handlers, runs the transactional outbox relay, and runs the runtime worker loop.

Current flow from an identity event to runtime work:

1. `identity.create_user` inserts `identity.users`.
2. The same transaction inserts `identity.user_registered.v1` into `platform.outbox`.
3. The worker claims pending outbox rows with `FOR UPDATE SKIP LOCKED`.
4. The relay dispatches events through an in-process `EventHandlerRegistry`.
5. `notifications` handles `identity.user_registered.v1` and enqueues `notifications.send_welcome_email.v1`.
6. The runtime worker claims pending function runs and invokes registered function handlers.
7. Success marks outbox/function rows complete; failures retry or eventually mark `dead`.

No NATS, Kafka, service mesh, or external broker is part of the current architecture.

## Runtime Console

The Runtime Console is a Vite/React operator UI under `apps/runtime-console`. It can run with local mock data or against the API.

The API exposes admin runtime endpoints under `/admin/runtime/*` for summaries, timelines, stories, heatmaps, outbox events, function runs, retries, execution payloads, and technical operations. These endpoints use the same OpenAPI contract as the public identity API.

OpenTelemetry data is an enrichment layer for technical operations. See `docs/architecture/runtime-telemetry.md` for the boundary between runtime story semantics and telemetry span enrichment.

## Contract Layer

Rust is the authoring source for the OpenAPI document. `apps/api/src/openapi.rs` defines the committed API contract, including:

- `POST /v1/identity/users`
- `GET /v1/identity/me`
- `/admin/runtime/*` Runtime Console endpoints
- standard error responses and request/correlation headers

Committed contract artifacts live under `contracts/`:

- `contracts/openapi/app-api.v1.yaml`
- `contracts/errors/*`
- `contracts/schemas/common/*`
- `contracts/events/{domain}/*.schema.json`
- `contracts/runtime/functions/*.schema.json`

Generated contract artifacts are committed. The current generator writes the OpenAPI artifact, the standard error response schema, and the generated identity event schema:

```sh
just generate-contracts
```

Freshness and contract coverage are checked by Rust tests and `arch-check`. Handwritten contract files are still checked for parseability and naming consistency.

## TypeScript SDK Generation

The TypeScript SDK is generated from the committed OpenAPI artifact:

```sh
just generate-ts-sdk
```

Generated files live under `packages/ts-sdk/src/generated/`. The stable ergonomic wrapper lives in `packages/ts-sdk/src/index.ts` and currently exposes identity helpers while re-exporting generated API types.

Do not hand-edit generated SDK files. Change the Rust OpenAPI source, regenerate contracts, then regenerate the SDK.
