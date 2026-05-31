# Architecture Overview

Lenso is a Rust-first service-ready modular monolith. It is intentionally small: the platform gives future projects reliable defaults without becoming a framework that hides the system.

## Modular Monolith

The first deployable shape is one API app, one worker app, and one migration app. Business capabilities live under `domains/`, and each domain is a Rust crate with its own routes, DTOs, commands, queries, models, repositories, events, jobs, runtime registrations, migrations, and tests.

Domains run in-process today. Extraction later should be mechanical: preserve the public interface and contracts, move the tables, turn in-process calls into client calls, and keep event names stable.

## Domain Boundaries

Domains own their data and behavior. A domain may expose:

- HTTP routes through its `module.rs` descriptor.
- Stable in-process calls through `public.rs`.
- Events under `events/`.
- Runtime jobs/functions under `jobs/` and `runtime/`.
- SQL and migrations under `repositories/` and `migrations/`.

Domains must not query another domain's tables or import another domain's internal modules. Cross-domain async work goes through events.

## Platform Service Kit

The service kit is split into a few crates:

- `platform-core`: config, error model, request context, IDs, clock, DB pool, migrations, events, transactional outbox, relay primitives, health, shutdown, telemetry foundations.
- `platform-http`: Axum request context middleware, standard JSON error responses, JSON extractor, response helpers, health routes, OpenAPI helpers.
- `platform-runtime`: minimal embedded runtime primitives for functions, triggers, queues, flows, retry policies, and store traits.
- `platform-testing`: shared test database utilities.

The service kit should stay stable and small. It exists to remove boilerplate, not to own business behavior.

## Runtime

The runtime is embedded beside the modular monolith. It manages functions, triggers, queues, flows, agents, retries, and execution metadata. It does not own business logic.

Domains register runtime functions and jobs through their module descriptors. The worker app composes these registrations and runs the worker loop.

## Transactional Outbox And Relay

Domain commands that write business data and emit domain events write both inside the same Postgres transaction.

Current flow:

1. `identity.create_user` inserts `identity.users`.
2. The same transaction inserts `identity.user_registered.v1` into `platform.outbox`.
3. The worker claims pending outbox rows with `FOR UPDATE SKIP LOCKED`.
4. The relay dispatches events through an in-process `EventHandlerRegistry`.
5. Success marks the outbox row `published`; failures retry or eventually mark `dead`.

No NATS, Kafka, service mesh, or external broker is part of the MVP.

## Contract Layer

Rust is the authoring source for the current API contract. `apps/api/src/openapi.rs` defines the OpenAPI document for `POST /v1/identity/users`, including request/response schemas and standard error responses.

Generated artifacts live in:

- `contracts/openapi/app-api.v1.yaml`
- `contracts/errors/error-response.v1.schema.json`
- `contracts/events/identity/identity.user_registered.v1.schema.json`

Run:

```sh
just generate-contracts
```

Freshness is checked by Rust tests and `arch-check`.

## TypeScript SDK Generation

The TypeScript SDK is generated from the committed OpenAPI artifact:

```sh
just generate-ts-sdk
```

Generated files live under `packages/ts-sdk/src/generated/`. The stable ergonomic wrapper lives in `packages/ts-sdk/src/index.ts` and exposes:

```ts
client.identity.createUser({
  email,
  display_name,
});
```

Do not hand-edit generated SDK files. Change the Rust OpenAPI source, regenerate contracts, then regenerate the SDK.
