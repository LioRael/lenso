# Architecture Rules

These rules are hard guardrails for future agent-driven development.

## Module Structure

Modules must use the flat Rust-friendly structure:

```text
modules/{module}/
  migrations/
  src/
    lib.rs
    config.rs
    module.rs
    public.rs
    routes/
    dto/
    commands/
    queries/
    models/
    repositories/
    events/
    jobs/
    runtime/
    tests/
```

Do not create DDD or Clean Architecture folders:

- `api`
- `application`
- `domain`
- `infrastructure`

## Module Boundaries

- A module must not directly access another module's tables.
- A module must not import another module's internal implementation.
- Cross-module synchronous calls must go through stable public interfaces.
- Cross-module asynchronous work must use events.
- Shared platform behavior belongs in `crates/platform-*`, not in a concrete module.

## Module Registration

- The concrete module set is enumerated only in `crates/lenso-bootstrap`. Apps must not hand-wire individual modules.
- A new module is registered through the `lenso-bootstrap` entry points it needs: `modules` (runtime functions, event handlers, runtime config, admin data), `module_manifests` (context-free metadata), `merge_linked_http` (Linked HTTP routes), and `story_display_descriptors` (runtime console metadata).
- Each module exposes module data as `ModuleManifest` from the public `lenso` facade and source-specific behavior through `ModuleBinding` from `platform-module`; do not recreate descriptor types per module.
- Keep data and behavior split. Serializable declarations belong in `ModuleManifest`; behavior belongs behind narrow traits such as `ModuleBinding` and `AdminDataSource`.
- `platform-admin` and `platform-admin-data` must not depend on concrete module crates. Use composition-root injection and platform-module seams.
- Module manifest lint rules belong in the public `lenso` facade and are exposed through backend metadata endpoints such as `/admin/data/modules`. Runtime Console screens may filter, group, and render lint results, but must not duplicate backend manifest lint rules in TypeScript. Keep `docs/architecture/module-manifest-lints.md` current when adding or changing lint subjects.
- Runtime Console frontend contributions must use `ModuleManifest.console` and the host console module registry described in `docs/architecture/module-console-surfaces.md`. Console modules must import host services through a narrow host facade, not by deep-importing host pages, hooks, components, or data internals.
- Remote modules may provide manifests, declared HTTP route metadata, schema-admin read data, runtime functions, and event handlers through `platform-module-remote`. Declared remote HTTP routes are mounted only through the host policy in `docs/architecture/module-remote-http-proxy.md`. Remote runtime functions and event handlers must follow `docs/architecture/module-remote-runtime.md`: the host owns queues, outbox claims, retries, stories, and worker execution. Native gRPC remote transport is scoped in `docs/architecture/module-remote-grpc.md`.
- Services are out-of-process providers for modules, not peer runtimes. Follow `docs/architecture/service-module-boundary.md`: keep auth, queues, retries, outbox claims, Runtime Story records, and Technical Operations host-owned. Do not add service discovery, gateways, service mesh, distributed transactions, schema registry, or orchestration without a real extracted-module need.
- Autonomous Services declared with `lenso.service.v2` use the separate `lenso-autonomous-service` runtime boundary. Keep their Store, health, migration, shutdown, and local Story Segment state Service-owned; do not route this profile through Host or Provider startup. Story Segment Feed reads must fail closed through Workload Identity, an exact audience, explicit tenant authorization, and a durable opaque cursor; collection must never acknowledge or mutate workflow execution. Business routes and migrations must still come from Modules rather than platform-owned business handlers.
- Extraction readiness rules and the versioned report contract belong in the public `lenso-service` surface. CLI-owned analyzers may collect repository evidence and explicitly read-only live Store observations, but they must call the shared evaluator and render its report; they must not duplicate classification rules. Table and migration ownership must resolve to one Module, direct cross-Module table access and cross-boundary transactions block extraction, and large volume or missing cursors remain explicit planning risk. Readiness analysis is read-only and must never write repository files, start Workloads, move data, or change authority. Provider v1 and System v1 semantics remain unchanged.
- Extraction Plan artifact types, deterministic ordering, content addressing, phase vocabulary, dry-run effects, and stale-input validation belong in the public `lenso-service` surface. Pin the exact readiness, Module, Contract Version, System graph, analyzer, data mapping, evidence, and expected authority inputs; reject plan-integrity or input drift before mutation. CLI-owned orchestration may consume this artifact but must not reinterpret its digests, reorder phases, bypass `commit-extraction-authority`, or treat repository write access as Cutover authority.
- Extraction Scaffold artifacts and safety validation belong in the public `lenso-service` surface. Generate the candidate only from an integrity-valid plan, its exact Module declaration, and digest-matching authoritative Contract artifacts. Preserve the complete Module identity, derive HTTP/gRPC/Event bindings and Service Clients through the existing public Contract generators, and keep all remaining Module behavior local. Dry-run must expose the exact deterministic patch; apply must reject stale plans, changed targets, symlink traversal, and unrecognized target files before overwriting anything. Scaffold apply may create candidate files only: it must not start Workloads, move data, change authority, or modify Provider v1 files.
- Destination expansion state, operation ordering, receipt validation, and the `lenso.extraction-run.v1` artifact belong in the public `lenso-service` surface. Bind every migration to its plan-pinned source path and SQL digest, accept only conservative expand-first Postgres statements, and target only the isolated candidate Store. Advance at most one operation per persisted Run revision and inspect the public Workload receipt before execution so restarts never repeat a completed effect. Candidate Migration and API health must pass through public Workload behavior. This phase must not expose source mutation credentials, copy Service Data, modify linked behavior, change authority, or perform destructive cleanup.
- Federated Runtime Story aggregation belongs to the Story observability boundary, never an Autonomous Service execution path. Keep per-source cursors and collected Segment revisions in the aggregator Store, preserve source and tenant identity, represent source failures as typed gaps, and treat OpenTelemetry as optional node enrichment only. Aggregation availability must not affect Service-local capture, Workflow state, Inbox, Outbox, timers, or dispatch.
- Third-party ecosystem modules should default to Remote packaging as described in `docs/architecture/third-party-modules.md`. Do not compile third-party code into the host by default; `Linked` is for first-party application modules, framework fixtures, and local project-owned modules.
- Custom admin UI must keep host-rendered declarations and module-owned embedded UI separate: use `DeclarativeCustom` for trusted Runtime Console rendering, and `EmbeddedCustom` for iframe/Wasm/other sandboxed module-owned UI. Do not model both as one generic `Custom` surface.
- Embedded custom admin surfaces must not receive host bearer tokens or ad hoc bridge access. Any bridge must be a versioned protocol with explicit manifest permissions and host enforcement.

## Contracts

- No HTTP API without OpenAPI schema coverage.
- HTTP handlers carry their own `#[utoipa::path]` annotation and are registered via `utoipa-axum`'s `OpenApiRouter` (`routes!`), so each route's path and parameters are authored once. Do not add detached `#[utoipa::path]` stub functions.
- `crates/lenso-api/src/openapi.rs` holds only document-level metadata (info, tags); it must not re-declare path or schema lists that the annotated handlers already provide.
- No event payload without a JSON Schema contract under `contracts/events/`.
- No runtime function without a JSON Schema contract under `contracts/runtime/functions/`.
- Error responses must use the standard error shape.
- Generated contract artifacts must be regenerated with `just generate-contracts`.
- Generated contract artifacts must not be manually patched.
- Handwritten contract artifacts must still parse and use names that match their path and title.

## Runtime And Outbox

- The runtime must not own business logic.
- Module commands that write data and emit events must use the transactional outbox.
- Host-owned linked modules must use `lenso::host::transaction` when combining
  a caller idempotency key, business SQL, and Outbox publication. They must not
  import `lenso-platform-core` or write platform transaction tables directly.
- Module event handlers may enqueue runtime functions, but function behavior stays in the owning module.
- Runtime function names must be stable, versioned, and documented under `contracts/runtime/functions/`.
- Do not add NATS, Kafka, service mesh, or Kubernetes complexity before there is a real extraction need.

## Enforcement

Run:

```sh
just arch-check
```

The checker fails on forbidden module folders, forbidden cross-module imports inside module source code, stale generated contracts, missing OpenAPI artifacts, malformed contract JSON/YAML, missing event contracts referenced by source code, event contract name/path mismatches, missing runtime function contracts for registered module runtime functions, and runtime function contract name/path mismatches. Runtime Console source guardrails live in the sibling frontend repository.
