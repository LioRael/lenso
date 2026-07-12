# Architecture Overview

Lenso is a Rust-first backend framework and service-ready modular monolith with
an embedded runtime, an Axum HTTP API, a background worker, a migration runner,
committed contracts, and a sibling Runtime Console. The platform gives future
projects reliable defaults without hiding the system. The public package surface
is defined in
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

Current linked module fixtures:

- `story` owns the `platform-story` Runtime Console module manifest and keeps
  Story visible as a first-class linked module while the compatible
  `/admin/runtime/*` backend remains mounted through `platform-admin`.
- `auth` owns the authentication anchor, session tables, development session
  routes, and host actor resolver. See [`auth-module.md`](auth-module.md).
- `auth-password` exercises a first-party linked password provider over the auth
  public interface.

These modules are demo fixtures, not product defaults. `lenso-bootstrap` selects a
linked composition profile: `core` keeps only platform-owned linked surfaces such
as `platform-story`, while `demo` adds `auth` and `auth-password` for local
development, examples, contracts, and integration tests. Product hosts should
use `core` and explicitly install first-party auth modules through their host
composition. Local development may default to `demo`; non-local environments
must set `LENSO_COMPOSITION_PROFILE=core` or `LENSO_COMPOSITION_PROFILE=demo`
explicitly.

## Platform Service Kit

The service kit is split into a few crates:

- `platform-core`: config, error model, request context, actor context, IDs, clock, DB pool, migrations, events, transactional outbox, relay primitives, health, shutdown, telemetry foundations, and telemetry query abstractions.
- `lenso-contracts`: shared serializable declaration contracts for module manifests, admin surfaces, HTTP route metadata, runtime/event/lifecycle declarations, Runtime Console surfaces, story display metadata, and manifest lints.
- `lenso`: the public Rust facade crate. Its default surface re-exports declaration contracts; its `host` feature exposes the narrow API, worker, migration, and linked HTTP host boot facade.
- `platform-http`: Axum request context middleware, auth extractors, standard JSON error responses, JSON extractor, response helpers, health routes, and the `OpenApiRouter` re-exports used for single-source OpenAPI.
- `platform-runtime`: embedded runtime primitives for functions, triggers, queues, flows, retry policies, registry, worker execution, and store traits.
- `platform-module`: internal module behavior seams and compatibility re-exports. `ModuleBinding` is the narrow behavior seam; `LinkedBinding` is the current compile-time source; `AdminDataSource` and `AdminActionSource` support generic schema-admin reads and manifest-declared action execution. It re-exports `lenso-contracts` declaration types for backend workspace compatibility.
- `platform-admin`: the compatibility runtime-observability backend for the Runtime Console. It only reads platform/runtime tables (`platform.outbox`, `platform.story_events`, `runtime.function_runs`) to observe every module's activity, and exposes one router the API app mounts under `/admin/runtime/*`. Story module metadata is owned by `modules/story`; the backend route implementation is being extracted from this platform crate in slices.
- `platform-admin-data`: the schema-admin backend for module business data. It exposes generic `/admin/data/*` endpoints over injected `AdminSurface::Schema` manifests and `AdminDataSource` implementations, without depending on concrete modules.
- `platform-testing`: shared test database utilities.

A thin composition root, `lenso-bootstrap`, sits above the service kit. It is the single place that enumerates the concrete modules, and both the API and the worker derive their module set from it. It pairs manifests, bindings, runtime config descriptors, story-display metadata, and admin data sources from concrete modules. It depends on the module crates, so it lives outside `platform-*` (those crates must not depend on concrete modules).

Configured services are loaded at startup through the Remote source in
`platform-module-remote`. The microservice-facing shape is named in
[`service-module-boundary.md`](service-module-boundary.md): a service is an
out-of-process provider for one or more modules while the host keeps auth,
runtime, retries, stories, and operator visibility. The current Remote slices
support manifest loading, declared HTTP route metadata, schema-admin reads,
admin surface metadata, host-owned HTTP proxying for declared GET, POST, PUT,
PATCH, and DELETE routes, remote runtime functions, and remote event handlers.
Third-party module packaging and ecosystem boundaries are specified in
`docs/architecture/third-party-modules.md`; V9 service packages add a small
`lenso.service-package.v1` artifact around `lenso.service.json` for release and
handoff tooling, V10 module releases add a `lenso.module-release.v1`
business-module entrypoint, and V11 adds a `lenso.module.v1` module contract so
linked, bundled, and service-provided modules share the same product-level
contract language. `lenso module install` remains the main module install
surface; `lenso service install` is the lower-level provider/process surface.
V18 adds a system-level graph in
[`service-system-plane.md`](service-system-plane.md): `lenso.system.json`
connects legacy Providers, modules, environments, and capability dependencies
without turning Kubernetes into a hard requirement. The `lenso.service.v1` and
`lenso.system.v1` protocols keep this Host-managed Provider meaning; they are
not Autonomous Service declarations.
The separate `lenso.service.v2` protocol is the Autonomous Service boundary. It
gives a logical Service a stable `serviceId` independent
of its Workload count or deployment topology, and declares its API, Worker,
Migration, or extension Workloads alongside owned Modules, logical Service
Stores, Tenancy Mode, and Operating Regions. Its authoritative fixture and
packaged schema live in `crates/lenso-service`; `just generate` publishes the
matching committed schema under `contracts/services/`.
`crates/lenso-autonomous-service` supplies the first Host-independent runtime
profile for definitions containing one API and one Migration Workload. It
validates Service, Workload, Store, and declared configuration coherence before
startup; applies platform, module, and Service-local Story Segment migrations
to the explicitly injected Service Store; mounts Service-owned health and local
evidence surfaces; and performs deterministic shutdown phase transitions.
Business routes and migrations remain injected Module contributions. This
runtime does not call the Host or Provider boot paths and does not reinterpret
Provider v1 artifacts.
Its versioned Service, Event, Config, and Reliability Contract declarations are
specified in [`autonomous-service-contract-artifacts.md`](autonomous-service-contract-artifacts.md).
The separate [`lenso.context.v1`](common-context-contracts.md) envelope
publishes Story, trace, identity, tenant, deadline, idempotency, causation, and
region declarations without adding runtime propagation or enforcement.
Route proxying is specified
separately in `docs/architecture/module-remote-http-proxy.md`. Remote runtime
execution and event-handler dispatch are scoped in
`docs/architecture/module-remote-runtime.md`, with native gRPC transport scoped
in `docs/architecture/module-remote-grpc.md`. Module install trust is
operator-owned: the CLI accepts explicit manifest URLs, and official catalogs
are curated at publication time without adding a separate host-side trust
protocol. Linked modules that have hardened boundaries can follow
[`linked-to-service-module.md`](linked-to-service-module.md) to preserve the
manifest contract while moving implementation into a service process.

The current Provider checkpoint is intentionally narrow but complete for
operator-visible HTTP proxying. Authentication, proxy policy, retries, runtime
queues, Outbox delivery, and Story evidence remain Host-owned:

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
  and per-module OpenAPI fragments remain deferred.

The service kit should stay stable and small. It exists to remove boilerplate, not to own business behavior.

## Runtime

The runtime is embedded beside the modular monolith. It manages functions, triggers, queues, flows, retry policies, function run persistence, and execution metadata. It does not own business logic.

Modules register runtime functions through their `ModuleBinding`. Modules may
also declare UTC cron schedules for those functions in `ModuleManifest`.
The worker app gets the module set from `lenso-bootstrap`, composes their
runtime descriptors into a `FunctionRegistry`, schedules due function runs
through host-owned runtime state, registers module event handlers, runs the
transactional outbox relay, and runs the runtime worker loop.

Current flow from a module event to runtime work:

1. A module command writes its own tables.
2. The same transaction inserts a versioned event into `platform.outbox`.
3. The worker claims pending outbox rows with `FOR UPDATE SKIP LOCKED`.
4. The relay dispatches events through an in-process `EventHandlerRegistry`.
5. Event handlers may enqueue versioned runtime functions.
6. The runtime worker claims pending function runs and invokes registered function handlers.
7. Success marks outbox/function rows complete; failures retry or eventually mark `dead`.

No NATS, Kafka, service mesh, or external broker is part of the current architecture.

## Runtime Console

The Runtime Console is a Vite/React operator UI developed in the sibling
`lenso-runtime-console` repository. It can run with local mock data or against
this backend API.

The API exposes admin runtime endpoints under `/admin/runtime/*` for summaries, stories, story timeline items, heatmaps, outbox events, function runs, retries, execution payloads, and technical operations. Story timeline data is returned by the Runtime Story detail endpoint rather than a standalone timeline endpoint. These are served by the compatible `platform-admin` backend while Story ownership moves into `modules/story`; they use the same OpenAPI contract as the public linked-module APIs. Story display names are module-owned, so the composition root injects the aggregated catalog into `platform-admin` (via `install_story_display`) rather than having it depend on concrete modules.

The API also exposes schema-admin endpoints under `/admin/data/*`. These are served by `platform-admin-data`, which reads module schemas and data through the injected `AdminSurface::Schema` + `AdminDataSource` registry. The demo profile uses the auth User anchor to exercise the framework; Lenso does not prescribe product-default business modules. Writes, richer RBAC, and custom module UI are later module-framework steps.

The module registry endpoint under `/admin/data/modules` is also the source of truth for module manifest health and Runtime Console frontend contributions. `platform-admin-data` derives manifest lint results from the public `lenso` facade helpers, including HTTP route and console surface declaration checks, and returns those lint results with the module metadata. The Runtime Console renders these `manifest_lints` as Manifest Lints; it must not reimplement the lint rules locally. See `docs/architecture/module-manifest-lints.md` for the current lint catalog and UI category contract. Module-owned Runtime Console pages are declared through `ConsoleSurface` and loaded through the host's console module registry; see `docs/architecture/module-console-surfaces.md`.

OpenTelemetry data is an enrichment layer for technical operations. See `docs/architecture/runtime-telemetry.md` for the boundary between runtime story semantics and telemetry span enrichment.

## Contract Layer

Rust is the authoring source for the OpenAPI document. Each HTTP handler carries its own `#[utoipa::path]` annotation and is registered through `utoipa-axum`'s `OpenApiRouter`, so routes and their documentation share a single source. `crates/lenso-api/src/openapi.rs` holds only the document-level metadata (title, version, tags) and assembles the linked-module and admin routers into the committed contract, including:

- `POST /v1/auth/dev/sessions`
- `POST /v1/auth/sessions/revoke`
- `POST /v1/auth/password/register`
- `POST /v1/auth/password/login`
- `/admin/runtime/*` Runtime Console endpoints
- `/admin/data/*` schema-admin endpoints
- standard error responses and request/correlation headers

Paths and component schemas are collected automatically from the annotated handlers; `openapi.rs` declares no path or schema lists of its own.

Committed contract artifacts live under `contracts/`:

- `contracts/openapi/app-api.v1.yaml`
- `contracts/errors/error-response.v1.schema.json`
- `contracts/grpc/lenso/remote/v1/remote_module.proto`

When modules add emitted event payloads or registered runtime functions, their
JSON Schema contracts belong under `contracts/events/{module}/` and
`contracts/runtime/functions/` respectively.

Generated contract artifacts are committed. The current generator writes the OpenAPI artifact and the standard error response schema:

```sh
just generate-contracts
```

Freshness and contract coverage are checked by Rust tests and `arch-check`. Handwritten contract files are still checked for parseability and naming consistency.
