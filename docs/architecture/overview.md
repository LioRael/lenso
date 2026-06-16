# Architecture Overview

Lenso is a Rust-first backend framework and service-ready modular monolith with
an embedded runtime, an Axum HTTP API, a background worker, a migration runner, a
Vite/React Runtime Console, and a generated TypeScript SDK. The platform gives
future projects reliable defaults without hiding the system. The public package
surface is defined in
[`framework-public-surface.md`](framework-public-surface.md).

## Modular Monolith

The deployable shape is one API app, one worker app, and one migration app. Product capabilities live under `modules/`, and each module is a Rust crate with its own routes, DTOs, commands, queries, models, repositories, events, jobs, runtime registrations, migrations, and tests.

Linked modules run in-process today. Extraction later should be mechanical: preserve the public interface and contracts, move the tables, turn in-process calls into client calls, and keep event and function names stable.

## Module Boundaries

Modules own their data and behavior. A module may expose:

- HTTP routes through its `routes/` module, where each handler carries its own `#[utoipa::path]` annotation.
- Stable in-process calls through `public.rs`.
- Events under `events/`.
- Runtime jobs/functions under `jobs/` and `runtime/`.
- SQL and migrations under `repositories/` and `migrations/`.

A module exposes metadata and behavior through `module.rs`. Pure declarations such as the module name, story-display metadata, capabilities, and schema-admin surface live in a `ModuleManifest`; source-specific behavior such as runtime function and event-handler registration lives behind `ModuleBinding`.

Modules must not query another module's tables or import another module's internal modules. Cross-module async work goes through events and runtime function enqueueing.

Current demo module fixtures:

- `story` owns the `platform-story` Runtime Console module manifest and keeps
  Story visible as a first-class linked module while the compatible
  `/admin/runtime/*` backend remains mounted through `platform-admin`.
- `identity` exercises users, identity HTTP routes, `identity.user_registered.v1`,
  `identity.cleanup_expired_sessions.v1`, schema-admin reads, and a module-owned
  Runtime Console workspace.
- `notifications` exercises identity registration event handling and
  `notifications.send_welcome_email.v1`.

These modules are demo fixtures, not product defaults. `app-bootstrap` selects a
linked composition profile: `core` keeps only platform-owned linked surfaces such
as `platform-story`, while `demo` adds `identity` and `notifications` for local
development, examples, contracts, and integration tests. Local development may
default to `demo`; non-local environments must set
`LENSO_COMPOSITION_PROFILE=core` or `LENSO_COMPOSITION_PROFILE=demo` explicitly.

## Platform Service Kit

The service kit is split into a few crates:

- `platform-core`: config, error model, request context, actor context, IDs, clock, DB pool, migrations, events, transactional outbox, relay primitives, health, shutdown, telemetry foundations, and telemetry query abstractions.
- `lenso`: the public Rust facade crate for serializable module-authoring declarations: module manifests, admin surfaces, HTTP route metadata, runtime/event/lifecycle declarations, Runtime Console surfaces, story display metadata, and manifest lints.
- `lenso-host`: the internal transitional host boot facade for API, worker, and migration runner helpers. Starter templates depend on this crate while the future public `lenso` host feature is being validated.
- `platform-http`: Axum request context middleware, auth extractors, standard JSON error responses, JSON extractor, response helpers, health routes, and the `OpenApiRouter` re-exports used for single-source OpenAPI.
- `platform-runtime`: embedded runtime primitives for functions, triggers, queues, flows, retry policies, registry, worker execution, and store traits.
- `platform-module`: internal module behavior seams and compatibility re-exports. `ModuleBinding` is the narrow behavior seam; `LinkedBinding` is the current compile-time source; `AdminDataSource` and `AdminActionSource` support generic schema-admin reads and manifest-declared action execution. It re-exports `lenso` declaration types for backend workspace compatibility.
- `platform-admin`: the compatibility runtime-observability backend for the Runtime Console. It only reads platform/runtime tables (`platform.outbox`, `platform.story_events`, `runtime.function_runs`) to observe every module's activity, and exposes one router the API app mounts under `/admin/runtime/*`. Story module metadata is owned by `modules/story`; the backend route implementation is being extracted from this platform crate in slices.
- `platform-admin-data`: the schema-admin backend for module business data. It exposes generic `/admin/data/*` endpoints over injected `AdminSurface::Schema` manifests and `AdminDataSource` implementations, without depending on concrete modules.
- `platform-testing`: shared test database utilities.

A thin composition root, `app-bootstrap`, sits above the service kit. It is the single place that enumerates the concrete modules, and both the API and the worker derive their module set from it. It pairs manifests, bindings, runtime config descriptors, story-display metadata, and admin data sources from concrete modules. It depends on the module crates, so it lives outside `platform-*` (those crates must not depend on concrete modules).

Configured remote modules are loaded at startup through `platform-module-remote`. The current Remote slices support manifest loading, declared HTTP route metadata, schema-admin reads, admin surface metadata, host-owned HTTP proxying for declared GET, POST, PUT, PATCH, and DELETE routes, remote runtime functions, and remote event handlers. Third-party module packaging and ecosystem boundaries are specified in `docs/architecture/third-party-modules.md`. Route proxying is specified separately in `docs/architecture/module-remote-http-proxy.md`. Remote runtime execution and event-handler dispatch are scoped in `docs/architecture/module-remote-runtime.md`, with native gRPC transport scoped in `docs/architecture/module-remote-grpc.md`. Marketplace trust remains a separate future spec.

The current remote-module checkpoint is intentionally narrow but complete for
operator-visible HTTP proxying:

- Remote manifests are loaded as the same `ModuleManifest` data contract used by
  linked modules.
- Remote schema-admin data can be read through `/admin/data/*` when the module
  exposes `AdminSurface::Schema` and protocol-backed records.
- Remote admin metadata can expose schema, declarative custom, or embedded
  custom surfaces; the Runtime Console has read-only examples for schema,
  host-rendered declarative sections, and sandboxed iframe embedded surfaces.
- Declared remote HTTP routes are proxied under
  `/modules/{module}/http/{*path}` with host-owned auth, capability checks,
  request/response limits, header policy, error normalization, persisted call
  history, Runtime Story nodes, Technical Operations rows, and Remote Calls
  navigation.
- Remote runtime functions execute through host-owned worker queues, retry
  policy, Runtime Story data, and Technical Operations.
- Remote event handlers execute through host-owned outbox dispatch: the worker
  claims rows, invokes declared remote handlers, and keeps retry/dead-letter
  state in `platform.outbox`.
- Declarative admin actions invoke host-owned `/admin/data/{module}/actions/*`
  endpoints with manifest capability checks. Successful and failed action
  invocations are projected into Runtime Stories and Technical Operations.
- Embedded host bridges, JavaScript bundle loading, Wasm execution, streaming,
  per-module OpenAPI fragments, and marketplace install trust remain deferred.

The service kit should stay stable and small. It exists to remove boilerplate, not to own business behavior.

## Runtime

The runtime is embedded beside the modular monolith. It manages functions, triggers, queues, flows, retry policies, function run persistence, and execution metadata. It does not own business logic.

Modules register runtime functions through their `ModuleBinding`. The worker app gets the module set from `app-bootstrap`, composes their runtime descriptors into a `FunctionRegistry`, registers module event handlers, runs the transactional outbox relay, and runs the runtime worker loop.

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

The Runtime Console is a Vite/React operator UI developed in the sibling
`lenso-runtime-console` repository. It can run with local mock data or against
this backend API.

The API exposes admin runtime endpoints under `/admin/runtime/*` for summaries, stories, story timeline items, heatmaps, outbox events, function runs, retries, execution payloads, and technical operations. Story timeline data is returned by the Runtime Story detail endpoint rather than a standalone timeline endpoint. These are served by the compatible `platform-admin` backend while Story ownership moves into `modules/story`; they use the same OpenAPI contract as the public identity API. Story display names are module-owned, so the composition root injects the aggregated catalog into `platform-admin` (via `install_story_display`) rather than having it depend on concrete modules.

The API also exposes schema-admin endpoints under `/admin/data/*`. These are served by `platform-admin-data`, which reads module schemas and data through the injected `AdminSurface::Schema` + `AdminDataSource` registry. The demo profile uses a read-only identity User fixture to exercise the framework; Lenso does not prescribe product-default business modules. Writes, richer RBAC, and custom module UI are later module-framework steps.

The module registry endpoint under `/admin/data/modules` is also the source of truth for module manifest health and Runtime Console frontend contributions. `platform-admin-data` derives manifest lint results from the public `lenso` facade helpers, including HTTP route and console surface declaration checks, and returns those lint results with the module metadata. The Runtime Console renders these `manifest_lints` as Manifest Lints; it must not reimplement the lint rules locally. See `docs/architecture/module-manifest-lints.md` for the current lint catalog and UI category contract. Module-owned Runtime Console pages are declared through `ConsoleSurface` and loaded through the host's console module registry; see `docs/architecture/module-console-surfaces.md`.

OpenTelemetry data is an enrichment layer for technical operations. See `docs/architecture/runtime-telemetry.md` for the boundary between runtime story semantics and telemetry span enrichment.

## Contract Layer

Rust is the authoring source for the OpenAPI document. Each HTTP handler carries its own `#[utoipa::path]` annotation and is registered through `utoipa-axum`'s `OpenApiRouter`, so routes and their documentation share a single source. `apps/api/src/openapi.rs` holds only the document-level metadata (title, version, tags) and assembles the linked-module and admin routers into the committed contract, including:

- `POST /v1/identity/users`
- `GET /v1/identity/me`
- `/admin/runtime/*` Runtime Console endpoints
- `/admin/data/*` schema-admin endpoints
- standard error responses and request/correlation headers

Paths and component schemas are collected automatically from the annotated handlers; `openapi.rs` declares no path or schema lists of its own.

Committed contract artifacts live under `contracts/`:

- `contracts/openapi/app-api.v1.yaml`
- `contracts/errors/*`
- `contracts/schemas/common/*`
- `contracts/events/{module}/*.schema.json`
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
