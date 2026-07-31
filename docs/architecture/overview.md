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

- `auth` owns the authentication anchor, session tables, development session
  routes, and host actor resolver. See [`auth-module.md`](auth-module.md).
- `auth-password` exercises a first-party linked password provider over the auth
  public interface.

These modules are demo fixtures, not product defaults. `lenso-bootstrap` selects a
linked composition profile: `core` keeps only platform-owned runtime services,
while `demo` adds `auth` and `auth-password` for local development, examples,
contracts, and integration tests. Product hosts should use `core` and explicitly
install first-party auth modules through their host composition. Local
development may default to `demo`; non-local environments must set
`LENSO_COMPOSITION_PROFILE=core` or `LENSO_COMPOSITION_PROFILE=demo` explicitly.

The Runtime Stories Console module is owned by the sibling `lenso-console`
repository. Managed Services expose its evidence through the Service-owned
System Plane observability provider and do not embed its UI declaration or
assets.

## Platform Service Kit

The service kit is split into a few crates:

- `platform-core`: config, error model, request context, actor context, IDs, clock, DB pool, migrations, events, transactional outbox, relay primitives, health, shutdown, telemetry foundations, and telemetry query abstractions.
- `lenso-contracts`: shared serializable declaration contracts for module manifests, admin surfaces, HTTP route metadata, runtime/event/lifecycle declarations, Runtime Console surfaces, story display metadata, and manifest lints.
- `lenso`: the public Rust facade crate. Its default surface re-exports declaration contracts; its `host` feature exposes the narrow API, worker, migration, and linked HTTP host boot facade.
- `lenso-module-management`: the shared lifecycle kernel. One deep Plan Builder owns graph resolution, isolated Cargo candidate resolution, immutable plans, approvals, operation journals, crash recovery, repair, and Linked workspace transactions for every caller.
- `platform-http`: Axum request context middleware, auth extractors, standard JSON error responses, JSON extractor, response helpers, health routes, and the `OpenApiRouter` re-exports used for single-source OpenAPI.
- `platform-runtime`: embedded runtime primitives for functions, triggers, queues, flows, retry policies, registry, worker execution, and store traits.
- `platform-system-plane`: capability-neutral Core discovery, capability negotiation, common request admission, and the Service side of bilateral enrollment. It verifies signed, expiring Enrollment Offers through an injected trust adapter, signs Receipts through the managed Service identity, and atomically persists the exact Receipt, capability/policy Grant, and append-only audit evidence. Production authorization reads the current Grant, contract/schema/feature scope, expiry, revocation state, and authorization epoch from the Service Store on every System Plane request; unsigned bootstrap is limited to its local/test-only System Sandbox adapter.
- `platform-runtime-observability`: the first Service-owned System Plane Capability Provider. It publishes the exact `lenso.system-plane.runtime-observability.v1` schema digest and serves revisioned, read-only Outbox and Function queue snapshots without Console workflow or UI concepts. Each snapshot includes a durable Service-owned change watermark; the recovery feed resumes from its opaque cursor and reports invalid, revision-mismatched, schema-mismatched, or retention-lost cursors as an explicit Evidence Gap requiring a fresh snapshot.
- `platform-runtime-operations`: the opt-in Service-owned mutation provider for Runtime Operations. It publishes the exact `lenso.system-plane.runtime-operations.v1` schema digest and implements revision-bound Function Run and Outbox Event retry through immutable Management Intents, expiring Plan Receipts, durable acknowledgements, Service-local authority verification, idempotent execution, and terminal Operation Evidence.
- `platform-module`: internal Module behavior seams and compatibility re-exports. `ModuleBinding` is the narrow behavior seam and `LinkedBinding` is the current compile-time source. It re-exports `lenso-contracts` declaration types for backend workspace compatibility.
- `platform-module-management`: the target-owned Module lifecycle adapter and `service-installations` System Plane Capability Provider. It delegates resolution, Cargo, workspace transactions, approvals, retries, journals, and Service installation CAS to `lenso-module-management`; it is not mounted as a Data Plane `/admin/*` API.
- `platform-system-plane`: the capability-neutral managed-Service kernel and HTTP adapter for the System Plane Core Protocol. Capability Providers expose observation, planning, and idempotent local execution through one seam; the kernel persists the provider's exact content-addressed capability plan and passes that plan back for execution, so execution never silently replans. The kernel owns durable acceptance before effects, idempotency conflicts, crash continuation, terminal Service-owned evidence, and rebuildable evidence feeds. Evidence sequence and cursor continuity are independent per capability; feed pages are bounded, every persisted payload digest and deterministic evidence identity is reverified on read, and corruption fails closed rather than appearing as business drift. Before invoking a Provider the kernel persists the Running transition; after invocation it persists an internal completion checkpoint before projecting terminal evidence and Operation state. Runtime composition deterministically resumes every accepted or running Operation before serving requests, and a recovered checkpoint is finalized without repeating the Provider effect. Its Service-owned Enrollment Registry admits only one active Console authority per Service environment, while its admission boundary intersects verified Workload Identity, the exact Enrollment ceiling and revision, authorization epoch, operation type, and verified signed Delegated Actor and optional Tenant Context. Production admission uses a verify-only Ed25519 adapter keyed by the exact `(issuer, verification method)` pair; signing remains with the Console authority, and overlapping public keys permit explicit rotation windows. The runtime builder composes persistence, enrollment, identity adapters, providers, and HTTP admission as one target-owned graph. That runtime is attached to one Router through an Axum extension; there is no process-global install slot that another Service or test can overwrite. `/system-plane/v1/*` middleware either accepts a server-injected verified Workload Identity or verifies a short-lived bearer credential against a trusted live transport binding; handlers contain no policy and fail closed without the composed runtime. The crate contains no Console state or concrete capability policy.
- `platform-testing`: shared test database utilities.

A thin composition root, `lenso-bootstrap`, sits above the service kit. It is the single place that enumerates concrete Modules and System Plane Capability Providers, and both the API and worker derive their Module set from it. Its Host System Plane composition registers the real `service-installations` provider and accepts identity adapters from the embedding Service. The production composition path connects SPIFFE Workload Identity and the dedicated mTLS System Plane listener; the ordinary Data Plane contains no Console or management routes.

The Module Ecosystem V1 contract has exactly two delivery forms: Linked and
Service. Linked is the primary in-process Module experience. A Service is an
out-of-process owner or provider that still contains and exports Modules; it is
not a separate peer capability model. The older `platform-module-provider`
runtime remains temporarily as an internal Provider compatibility
implementation, but `provider`, `source`, and `bundled` are not public Module
contract values. Runtime Console, backend automation, and CLI are peer adapters
over `lenso-module-management`; no adapter delegates lifecycle work to another.
The target-owned `lenso.service-installations.v1` document records desired
Service releases, exported Modules, Config bindings, Endpoint resolution, and
lifecycle bindings per environment. Immutable install plans use revision and
state-digest CAS; durable receipts distinguish desired-state application from
fresh runtime readiness. Module install and update may embed one exact Service
Installation subplan. Ordinary Module uninstall never removes the Service;
Service uninstall is a separate user-owned operation.
For Provider-profile Services, `lenso-module-management` compiles the exact
Application Module Lock, retained immutable Module Releases, and target Service
Installation Set into `lenso.provider-runtime-plan.v1`. This is the only input
to Provider transport adapters. A live Provider descriptor verifies the
running endpoint but never discovers Modules, replaces locked Manifests, or
enables sibling exports.
Production API and worker startup compile that plan before connecting Provider
behavior. They neither discover Providers from environment variables nor own
Provider process lifecycle. The internal
HTTP/gRPC adapter resolves candidates through the endpoint adapter selected by
the exact source ID, deterministically selects only an allowed endpoint, checks
the live descriptor against the locked Manifest, and constructs behavior from
the locked copy only. Static endpoints need no resolver. `HostComposition`
injects non-static endpoint and credential adapters; an unresolved source ID or
trust profile fails startup. Production defaults include only the explicit
`bearer_env` credential adapter, which accepts one opaque `env://NAME`
reference. Secrets never enter discovery or the Provider Runtime Plan and
transport Debug output reports only whether auth is configured.
The public `lenso-service::system_plane` contracts keep snapshot freshness,
Management Intent, time-bound Plan Receipt, durable Operation
Acknowledgement, terminal Operation Evidence, cursor feed, and reconciliation
states distinct. The Console may project these facts, but the managed Service
owns its operation lifecycle and evidence.
Enrollment is two-sided rather than provider self-registration. Public Offer,
Receipt, and Service-owned Record contracts bind stable Service Principals,
trust anchors, protocol compatibility, capability and operation ceilings,
delegation policy, nonce, revision, expiry, and signature evidence. The provider
System Plane router exposes no activation endpoint: only a local Service
administrative path may persist an already verified Receipt. Production Host
startup can perform that local import from an explicitly provisioned Receipt
file and verification-evidence digest; it binds Service identity, SPIFFE trust,
and delegated verification keys before an atomic, idempotent activation. Capability
discovery is filtered by that ceiling and never widens it. Workload Identity
alone may authorize only explicitly declared service-only observations; every
management plan and submission carries its signed authority context in the wire
envelope and requires request-pinned delegated authority. The Service stores the
Capability Provider's exact plan payload and digest in its Plan Receipt, then
executes that persisted payload rather than deriving a new plan from intent.
The microservice-facing responsibilities are described in
[`service-module-boundary.md`](service-module-boundary.md).
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
`crates/lenso-autonomous-service` supplies the Host-independent runtime profile
for definitions containing API, Migration, and Worker Workloads. It
validates Service, Workload, Store, and declared configuration coherence before
startup; applies platform, module, and Service-local Story Segment migrations
to the explicitly injected Service Store; runs Service-owned function queues
and transactional Outbox relay; mounts Service-owned health and local evidence
surfaces; and performs deterministic shutdown and claim release transitions.
Business routes and migrations remain injected Module contributions. This
runtime does not call the Host or Provider boot paths and does not reinterpret
Provider v1 artifacts.
System Plane Core is mounted at `/system-plane/v1` when composed, and the
optional Runtime Observability provider is mounted at
`/system-plane/v1/runtime-observability`. The optional Runtime Operations
provider is mounted at `/system-plane/v1/runtime-operations`; it supports
revision-bound retry of failed or dead Function Runs and Outbox Events. All providers
use the same authenticated
transport, Workload Identity, and active Enrollment Grant seam. Discovery and
snapshot reads are excluded from Service Story writes. Mutations additionally
require deployment-owned management-authority verification and persist the
accepted operation, authorization evidence, and acknowledgement before applying
the effect. Each accepted operation appends accepted and terminal evidence to a
Service-owned sequence; callers can page that sequence through opaque,
operation-scoped cursors and recover the original acknowledgement plus latest
evidence by idempotency key. Console therefore resolves lost responses from
authoritative state instead of blind replay. No compatibility `/admin/runtime/*`
backend remains.
Its Story evidence is exposed as an authenticated
`lenso.story-segment-feed.v1` append-only feed. Stable evidence revisions carry
Service, Workload, contract, tenant, causation, and Workflow identity; signed
opaque cursors survive API Workload restarts for the declared retention window.
Workload Identity audience verification and explicit reader-to-tenant policy
protect every read. Collection remains read-only and cannot acknowledge,
advance, or unblock workflow execution.
The Story module owns delayed federation outside the Service runtime. Its
PostgreSQL-backed aggregator keeps one durable cursor per source Service and
tenant partition, stores Segment revisions idempotently, and projects the
latest revision under a stable cross-Service node identity. Unreachable,
stale, unauthorized, truncated, and retention-expired feeds remain explicit
typed gaps. Late evidence enriches the same `lenso.federated-runtime-story.v1`
identity, while optional trace, metric, and log providers may annotate matched
nodes without determining Story identity or business completion.
The runtime admin backend now presents those collected Stories through the
existing tenant-scoped Stories API. It projects cross-Service graph and
timeline nodes, exact Workflow evidence states, typed Segment gaps, correlated
technical operations, and collected Reliability Reports for Runtime Console;
the frontend does not recompute federation or reliability rules.
Modules can now declare engine-neutral, versioned Durable Workflow definitions
under `ModuleManifest.runtime.workflows`. Autonomous Service composition
collects those definitions, validates that each owner is a Module owned by the
Service, and starts instances in the Service Store through
`POST /runtime/workflows/{owner}/{name}/instances`. The instance and its initial
step are committed together with the exact immutable definition artifact, its
SHA-256 digest, the selected version, Story Context, tenant scope, and
timestamps. The artifact migration fails closed while any legacy instance is
still running, so an upgrade cannot silently adopt a new worker definition as
old durable state. `GET /runtime/workflows/instances/{instance_id}`
reconstructs the same state from the Store after a runtime restart. Declared
Event Contract deliveries can also start an instance inside the existing Inbox
transaction. Module behavior advances a pending step with a stable transition
identity; the runtime completes that step, creates the next declared step, and
writes its declared cross-Service Event Contract work to the Service Outbox in
the same transaction. Duplicate delivery returns the committed transition and
preserves Inbox evidence without repeating the business effect. Story,
causation, Service Principal, delegated actor, tenant, deadline, and idempotency
context remain explicit across the step. Step declarations may also pin one
retry schedule and per-attempt timeout. The Service Store retains the original
step identity, explicit attempt history, terminal exhaustion, and durable retry
and timeout timers. Timer claims use worker leases so a restart can reclaim the
same transition identity; the System Sandbox may inject controlled time without
wall-clock sleeps. A pending parent step can also start a
version-pinned child instance in the same Service Store and wait durably for its
completion. The child retains distinct identity, explicit parent and causation
links, and validated inherited Story, delegated actor, tenant, deadline, and
idempotency context. A stable completion delivery resumes the parent exactly
once after either worker restarts; child failure or an unsupported pinned
version remains durable parent evidence with a stable next action. Retry and
timer claim transactions reject a worker whose registered definition is not
structurally identical to the pinned artifact, so a deployment cannot reuse a
version string to reinterpret existing state. Definition compatibility checks
return deterministic `safe`, `needs-attention`, `breaking`, or `blocked`
evidence with paths and next actions. The migration dry-run endpoint reports
affected in-flight instances, deterministic state mappings, the target version,
compatibility evidence, rollback constraints, and the explicit
`in_flight_workflow_migration` Approval Boundary without changing instance,
step, timer, attempt, or claim state. Protected operator controls use a separate
durable dispatch gate: deterministic dry-run plans can pause or resume an
instance, or reopen one selected exhausted step for a single authorized retry,
without changing step identity or erasing completed work, attempt history,
timers, claims, causation, or idempotency context. Stale plans fail closed and
every applied action records verified actor, authority, reason, prior and
resulting state, time, and next action. The same protected plan protocol can
cooperatively cancel an instance, strongly terminate it, or record a human
intervention. Cancel stops ordinary work and selects declared compensation
before reaching `cancelled`; terminate reaches `terminated` without reporting
implicit cleanup. Plans identify affected steps, timers, children,
compensations, irreversible effects, tenant scope, and expected terminal state,
and every apply requires a deployment-owned authority verifier plus the
action's explicit Approval Boundary. Completed steps may declare stable
compensation identity, deterministic order, a request Event Contract, and a
completion Event Contract. A controlled timeout records the completed effects
before selecting their compensations. Each request is published through the
owning Service Outbox with stable effect and compensation identity; the
Workflow remains `compensating` until the provider Service reverses the business
effect and confirms it through the declared completion Event. Restart and
redelivery preserve at-most-once business reversal through the Service-owned
Inbox, while a rejected compensation becomes the distinct durable
`compensation_failed` state with explicit intervention evidence. These slices
do not introduce a distributed transaction or reinterpret the existing
lightweight Host flow, Runtime Function, or Provider models.
Reliability Contracts now select reusable development, standard, or critical
profiles and resolve validated Service overrides into deterministic effective
limits. The Autonomous Service runtime evaluates local queue and Workflow
pressure plus Service-owned dependency and SLO observations, reports explicit
healthy, degraded, or unavailable state, and activates declared Degraded Modes.
Public readiness and liveness follow the resolved contract semantics. This M3
evidence is read-only and does not block promotion, execute canary policy, or
trigger rollback.
The first M4 extraction slice adds the versioned, read-only
`lenso.extraction-readiness-report.v1` contract. It evaluates one Host-owned
linked Module from its manifest, the mixed-topology System graph, and
CLI-supplied boundary, Contract, Consumer, and Postgres Service Data evidence.
Missing or ambiguous evidence fails closed; unresolved ownership, cross-Module
table access, and cross-boundary transactions block planning, while data volume
and cursor findings expose bounded-pause risk without turning size alone into a
failure. Optional live Store observations remain explicitly read-only and
distinct from static declarations. Declared runtime, admin, Console, Workflow,
and Story surfaces remain explicit preservation work. Evaluation never writes
files, starts Workloads, moves data, or changes authority.
Ready Modules can produce the versioned, content-addressed
`lenso.extraction-plan.v1` artifact. It pins readiness, Module, Contract,
System, analyzer, Postgres mapping, evidence, and expected authority inputs;
proposes API, Worker, and Migration Workloads, an isolated Store, Service
References, generated clients, and the System graph diff; and orders every
phase through terminal evidence. Dry-run is the exact zero-effect plan, and
integrity plus per-input freshness checks reject any stale plan before a later
CLI-owned mutating phase can begin. Final authority commit remains a human
Approval Boundary.
The scaffold phase turns that exact plan into the content-addressed
`lenso.extraction-scaffold.v1` artifact. It preserves the complete
`ModuleManifest`, emits API, Worker, and Migration validation entrypoints,
copies only pinned Contract artifacts, and derives HTTP, gRPC, Event, and
Service Client bindings through the public Contract generators. Dry-run is an
exact unified patch. Apply revalidates plan freshness and every generated file
before writing, treats matching files as an idempotent retry, and refuses any
changed or unrecognized target without touching unrelated repository files.
The candidate remains non-authoritative, does not start Workloads or move data,
and never changes the Provider v1 path.
Destination expansion is recorded as the versioned
`lenso.extraction-run.v1` artifact. The Run embeds the exact Extraction Plan,
expected linked authority, candidate Store and Workload identities, ordered
operations, content-addressed receipts, evidence, errors, and next actions.
Migration mappings pin the authoritative source path and SQL digest. The public
Workload boundary checks for a durable operation receipt before executing one
operation, so a restart after the Store effect but before Run persistence
recovers without repeating that effect. Only an isolated Postgres candidate
Store and conservative expand-first schema statements are accepted; source
data, linked behavior, backfill, authority, and destructive cleanup remain out
of scope. Migration and API health are verified through candidate Workload
behavior before the phase succeeds.
Its versioned Service, Event, Config, and Reliability Contract declarations are
specified in [`autonomous-service-contract-artifacts.md`](autonomous-service-contract-artifacts.md).
The separate [`lenso.context.v1`](common-context-contracts.md) envelope
publishes Story, trace, identity, tenant, deadline, idempotency, causation, and
region declarations without adding runtime propagation or enforcement.
Autonomous Service callers can execute versioned OpenAPI contracts through the
direct HTTP bindings or versioned Protobuf contracts through generated direct
gRPC bindings. Both resolve logical Service References, preserve one absolute
deadline and declared Idempotency Key semantics, and enforce the same explicit,
protocol-neutral Call Policy for safe attempts, circuit breaking, concurrency
isolation, overload evidence, and composition-supplied business fallback while
retaining native transport failures. Policy state stays inside the calling or
receiving Service and has no Runtime Console or System Plane dependency.
Declared JSON Schema Event Contracts generate transport-independent Event
Contract artifacts and `lenso.event-envelope.v1` envelopes. The envelope keeps
stable Service, Module, Contract, Story, causation, tenant, identity, region,
and content metadata, and has a lossless CloudEvents 1.0 structured
representation without broker vocabulary.
Autonomous Service receivers authenticate stable `service:<service-id>`
Principals through the public Workload Identity provider boundary. The local
System Sandbox provider is deterministic and development-only; production
composition uses the first production integration selected in
[`ADR 0024`](../adr/0024-select-spiffe-spire-as-the-first-production-workload-identity.md):
SPIFFE X.509-SVID mTLS plus an audience-limited JWT-SVID from the Workload API.
Lenso maps the authenticated peer SPIFFE ID to the stable Service Principal and
does not become a certificate authority. Direct HTTP, gRPC, and event admission
verify signed, expiring credentials plus authenticated transport binding before
business behavior, without a Runtime Console, Host, or System Plane lookup.
Route proxying is specified
separately in `docs/architecture/module-provider-http-proxy.md`. Provider runtime
execution and event-handler dispatch are scoped in
`docs/architecture/module-provider-runtime.md`, with native gRPC transport scoped
in `docs/architecture/module-provider-grpc.md`. Module install trust is
operator-owned: the CLI accepts explicit manifest URLs, and official catalogs
are curated at publication time without adding a separate host-side trust
protocol. Linked modules that have hardened boundaries can follow
[`linked-to-service-module.md`](linked-to-service-module.md) to preserve the
manifest contract while moving implementation into a service process.

The current Provider checkpoint is intentionally narrow but complete for
operator-visible HTTP proxying. Authentication, proxy policy, retries, runtime
queues, Outbox delivery, and Story evidence remain Host-owned:

- Provider manifests are loaded as the same `ModuleManifest` data contract used by
  linked modules.
- Provider schema-admin data can be read through `/admin/data/*` when the module
  exposes `AdminSurface::Schema` and protocol-backed records.
- Provider admin metadata can expose schema, declarative custom, or embedded
  custom surfaces; the Runtime Console has read-only examples for schema,
  host-rendered declarative sections, and sandboxed iframe embedded surfaces.
- Declared provider HTTP routes are proxied under
  `/modules/{module}/http/{*path}` with host-owned auth, capability checks,
  request/response limits, header policy, error normalization, persisted call
  history, Runtime Story nodes, Technical Operations rows, and Provider Calls
  navigation.
- Provider runtime functions execute through host-owned worker queues, retry
  policy, Runtime Story data, and Technical Operations.
- Provider event handlers execute through host-owned outbox dispatch: the worker
  claims rows, invokes declared provider handlers, and keeps retry/dead-letter
  state in `platform.outbox`.
- Declarative admin actions invoke host-owned `/admin/data/{module}/actions/*`
  endpoints with manifest capability checks. Successful and failed action
  invocations are projected into Runtime Stories and Technical Operations.
- Embedded host bridges, JavaScript bundle loading, Wasm execution, streaming,
  and per-module OpenAPI fragments remain deferred.

The service kit should stay stable and small. It exists to remove boilerplate, not to own business behavior.

Host-owned linked modules that combine a business write with an emitted event
use `lenso::host::transaction::LinkedTransaction`. It exposes the caller
transaction for app-owned SQL and keeps the scoped idempotency claim plus
Outbox publication atomic with that SQL. Modules never import
`lenso-platform-core` or write `platform.outbox` and
`platform.idempotency_claims` directly.

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

The dependency-free System Sandbox uses the local PostgreSQL Transport Adapter.
NATS JetStream is the first production Transport Adapter, selected in
[`ADR 0023`](../adr/0023-select-nats-jetstream-as-the-first-production-transport-adapter.md).
It remains optional and confined to Autonomous Service composition; linked
Modules, Event Contracts, the Host runtime, and the System Plane do not depend
on NATS, Kafka, a service mesh, or broker-specific types.

## Runtime Console

The Lenso Console product is developed in the sibling `lenso-console`
repository as an independent Service. It owns operator identity, its composition
Store and the web shell. Managed Services expose only authenticated
`/system-plane/v1/*` contracts on a dedicated listener; Console never reads a
managed Service Store or loads executable code from its Data Plane.

Module-owned Console pages are immutable `ConsoleUiArtifact`s bound to the same
Module Release. Isolated web artifacts communicate only through
`lenso.console-bridge.v1` and the exact composition grant.

OpenTelemetry data is an enrichment layer for technical operations. See `docs/architecture/runtime-telemetry.md` for the boundary between runtime story semantics and telemetry span enrichment.

## Contract Layer

Rust is the authoring source for the Data Plane OpenAPI document. Each HTTP handler carries its own `#[utoipa::path]` annotation and is registered through `utoipa-axum`'s `OpenApiRouter`, so routes and their documentation share a single source. `crates/lenso-api/src/openapi.rs` assembles only health and business Module routes, including:

- `POST /v1/auth/dev/sessions`
- `POST /v1/auth/sessions/revoke`
- `POST /v1/auth/password/register`
- `POST /v1/auth/password/login`
- standard error responses and request/correlation headers

Paths and component schemas are collected automatically from the annotated handlers; `openapi.rs` declares no path or schema lists of its own.

Committed contract artifacts live under `contracts/`:

- `contracts/openapi/app-api.v1.yaml`
- `contracts/errors/error-response.v1.schema.json`
- `contracts/grpc/lenso/provider/v1/provider.proto`

When modules add emitted event payloads or registered runtime functions, their
JSON Schema contracts belong under `contracts/events/{module}/` and
`contracts/runtime/functions/` respectively.

Generated contract artifacts are committed. The current generator writes the OpenAPI artifact and the standard error response schema:

```sh
just generate-contracts
```

Freshness and contract coverage are checked by Rust tests and `arch-check`. Handwritten contract files are still checked for parseability and naming consistency.
