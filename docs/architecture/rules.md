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
- Ordinary Module uninstall must preserve any containing Service installation.
  Service uninstall is a distinct `service.manage` operation with its own
  reviewed plan and receipt.
- System Plane Capability Providers may observe, predict effects, and execute
  idempotently, but common plan validation, durable acceptance, operation
  identity, terminal evidence, and feed semantics belong in
  `platform-system-plane`. Providers must not create a parallel operation
  protocol. Their exact capability plan payload and digest must be persisted in
  the reviewed Plan Receipt and execution must consume that persisted payload;
  never replan from the original Management Intent during execution.
- A Management Plan is not acceptance, an acknowledgement is not completion,
  terminal evidence is not a fresh observation, and unreachable or stale state
  must never be rewritten as failed or drifted.
- System Plane crash continuation is Service-owned. Persist a completion
  checkpoint after an idempotent Provider effect and before terminal evidence
  or Operation projection. Startup must resume accepted and running Operations
  deterministically; if the checkpoint exists, finalize it without invoking the
  Provider again. A missing or invalid checkpoint must never be guessed from a
  Console acknowledgement.
- Operation Evidence sequence, watermark, and cursor continuity are scoped to
  one capability, not shared across the Service. Feed reads must reverify the
  persisted payload digest, deterministic evidence identity, Service identity,
  terminal outcome, and uniqueness of capability sequence. Return bounded
  pages and an exact last-verified cursor for a sequence gap; corrupted state is
  an internal failure, never drift or a synthesized failed Operation.
- System Plane HTTP transport must supply a trusted live transport binding. The
  System Plane middleware may consume a server-injected verified Workload
  Identity or verify a short-lived bearer credential against that binding;
  caller-authored headers alone are never transport proof. Routes fail closed
  without explicit runtime and identity composition. The remote router must
  never expose Enrollment activation, transfer, or local revocation.
- A System Plane runtime is scoped to exactly one Router through explicit
  composition. Do not add a process-global runtime registry or hidden default
  identity provider. `lenso-bootstrap` enumerates concrete Host capabilities;
  the embedding Service supplies its production identity and transport adapters.
- Provider endpoint and credential resolution is target-owned composition.
  Select endpoint adapters by the exact resolver source ID and credential
  adapters by the exact trust profile; missing adapters fail startup. Resolved
  endpoint candidates remain constrained by the Provider Runtime Plan's allowed
  bindings and locked identities. Credential references are opaque, raw secrets
  never enter the plan, and config, proxy, diagnostics, and Debug output must not
  expose resolved credentials.
- Production Hosts should use `run_production_system_plane` so composition,
  Router construction, SPIFFE mTLS, request binding, graceful shutdown, and
  Workload API source shutdown remain one lifecycle. The lower-level
  `compose_host_production_system_plane_runtime` result must otherwise remain
  alive while serving. Only the framework mTLS adapter may inject
  `AuthenticatedTransportBinding`, and it must derive the value from the
  verified peer certificate. Its dedicated listener exposes only
  `/system-plane/v1/*`. The ordinary API listener never exposes System Plane
  routes. The embedding Service owns the distinct production listener and must
  validate the full production identity configuration before serving traffic;
  its bind address must remain distinct from the ordinary API listener. No
  forwarded or caller-authored header may substitute for verified mTLS state.
- System Plane authorization is an intersection: stable Console Service
  Principal, active Service-owned Enrollment, negotiated capability major,
  capability/operation ceiling, current authorization epoch, Console Module
  permission grant, exact request binding, Delegated Actor verification, and
  Service-local policy. Management wire envelopes must carry the signed Actor
  Context and optional Tenant Context needed for that verification. Discovery
  and plans never widen the Enrollment Grant.
- Enrollment activation stays local to the Service owner. An environment-driven
  bootstrap may import an already verified Receipt and its verification
  evidence digest, but must bind the Receipt to the exact configured Service
  identity, SPIFFE trust domain, and locally installed delegated-context keys
  before atomically persisting it. Exact startup replay is idempotent; changed
  evidence or authority requires a newer Receipt or explicit local revocation.
  Never expose activation, transfer, or revocation on the remote System Plane.
- Target Services depend only on `DelegatedContextVerifier`; they must never
  receive Console signing keys or require a signing-capable adapter. Production
  Actor and Tenant Context proofs use canonical, domain-separated Ed25519
  payloads and resolve keys by the exact issuer and verification-method pair.
  Rotation may overlap old and active public keys, but retirement must remove
  the old pair explicitly.

## Module Registration

- The concrete module set is enumerated only in `crates/lenso-bootstrap`. Apps must not hand-wire individual modules.
- A new Module is registered through the `lenso-bootstrap` entry points it needs: `modules` (runtime functions, event handlers, runtime config), `module_manifests` (context-free metadata), and `merge_linked_http` (Linked business HTTP routes).
- Each module exposes module data as `ModuleManifest` from the public `lenso` facade and source-specific behavior through `ModuleBinding` from `platform-module`; do not recreate descriptor types per module.
- Keep data and behavior split. Serializable declarations belong in `ModuleManifest`; behavior belongs behind narrow traits such as `ModuleBinding`.
- Module install, update, uninstall, restore, approval, crash-resume, and repair semantics belong in `lenso-module-management`. CLI and Console adapters must call this shared engine; they must not duplicate lifecycle rules or shell out to one another.
- Persist the desired composition, exact lock, immutable change plan, operation journal, effect receipts, and repair evidence as versioned target-owned contracts. Every mutating adapter must use revision compare-and-set plus the active lease fencing token, and reconciliation requires a separately succeeded repair operation bound to the exact repair plan.
- Every install, update, uninstall, optional-requirement change, and delivery switch must re-resolve the complete Module graph through `ModuleGraphResolver`. Preserve a currently locked release only while it remains eligible and satisfies every accumulated constraint; otherwise prefer Linked delivery for a new unconstrained selection, then highest SemVer, then release digest. Exact pins constrain selection but never bypass trust, lifecycle, provenance, compatibility, capability, or delivery policy.
- Optional requirements enter the graph only when explicitly selected in desired composition. Missing capabilities, incompatible version or delivery constraints, dependency cycles, blocked candidates, and duplicate release identities fail before planning with deterministic dependency paths and eligible alternatives. Uninstall must re-resolve and report orphaned transitive Modules rather than deleting only the requested root.
- Linked Cargo resolution must run in an isolated workspace materialized from the exact plan read set plus reviewed candidate files. The resulting `Cargo.lock` candidate must expose its package and feature diff, match each selected Linked release's package/version/archive checksum, and reject changes outside the selected package dependency closure before any workspace mutation.
- The trusted catalog adapter materializes `.lenso/module-planning-context.json`; management clients submit business-level changes only through negotiated target-owned System Plane Capability Contracts. The target must read target-owned desired/lock state and call the shared Plan Builder. Adapters must not accept caller-authored resolved locks, Cargo files, effect lists, or shell commands.
- Writable targets materialize `.lenso/module-environment-policy.json`. Starting an operation submits the reviewed backend plan, but the backend must rebuild it from target-owned state and reject any byte-level difference before journaling it.
- Exact plans, operation state, approvals, effect receipts, backups, fencing leases, and hash-chained journals live under `.lenso/module-management`; System Plane callers provide revisions and business decisions, never state transitions or receipts.
- The Host effect adapter may execute only backend-produced validation commands or target-owned Service deployment actions already bound into the immutable plan. Local and Kubernetes actions use argv execution without a shell and a workspace-confined working directory; externally managed targets provide a content-addressed deployment receipt. Missing actions produce a durable `blocked` operation with a retry action; they are never successful no-ops.
- Linked Module migrations bind the release artifact locator and SHA-256 digest into the immutable plan. The Host adapter accepts only workspace-confined artifacts for the `host` Store, applies them transactionally, and records migration identity plus artifact digest so retries are idempotent and same-name drift is rejected.
- Cancellation is allowed only before target mutation. A blocked post-mutation operation must be retried after its adapter is configured or repaired through a reviewed Repair Plan.
- Crash resume evidence is observed and constructed by the backend from the bound plan, journaled receipts, and current target bytes. Console may request resume with the expected revision but must not author completed-effect or idempotence evidence.
- Plan preview is read-only: it may use an isolated temporary Cargo workspace but must not write the target workspace or start an operation. Service Modules sharing one exact Service Release form one install/remove cohort, and plans emit effects only for changed releases or local-override content rather than replaying every existing migration or Service install.
- Linked Host integration uses `.lenso/linked-composition-seam.json`, one fixed `lenso-linked-composition` path dependency, and one `HostBuilder::linked_modules(lenso_linked_composition::linked_modules())` call. Per-Module operations may replace only the seam-owned generated crate and reviewed composition/lock artifacts; a missing or changed seam requires an explicit scaffold/repair plan.
- Workspace apply must validate the entire plan read set before journaling exact bytes and modes, reject symlinks and path escape, use same-filesystem replacement, preserve unrelated dirty files, and restore every touched path when a pre-migration file effect fails. It must never edit `.env` or mutate Git state.
- Module manifest lint rules belong in the public `lenso` facade and are evaluated before a Module Release enters Console composition. Console screens may render results but must not duplicate the rules in TypeScript.
- Console frontend contributions must use `ModuleManifest.console` plus the digest-bound `console_ui_esm` `ConsoleUiArtifact` in the same Module Release. The generated `lenso.console-module.v1` manifest, independent `hostApi`/`consoleUi` ranges, entry table, style assets, and digests are release-validated; the retired `lenso.console-bridge.v1` shape is rejected.
- A Module is delivered either as Linked code or as part of a Service release; `remote` is not a Module kind or public delivery contract. A Service still contains and exports Modules, and Provider or Autonomous responsibility describes the Service boundary rather than replacing Module identity and capabilities.
- Service Capability Tiers are exact: Provider Services use
  `lenso.service.v1` and may be authored in Rust or TypeScript; Autonomous
  Services use `lenso.service.v2` and are Rust-only. The TypeScript Service Kit
  must not claim Autonomous parity until it implements the complete
  Service-owned runtime, storage, identity, context, and Data Plane boundary.
- Provider Services are out-of-process providers for Modules, not peer Host runtimes. Follow `docs/architecture/service-module-boundary.md`: keep auth, queues, retries, outbox claims, Runtime Story records, and Technical Operations host-owned. Do not add service discovery, gateways, service mesh, distributed transactions, schema registry, or orchestration without a real extracted-Module need.
- Autonomous Services declared with `lenso.service.v2` use the separate `lenso-autonomous-service` runtime boundary. Keep their Store, health, migration, shutdown, and local Story Segment state Service-owned; do not route this profile through Host or Provider startup. Story Segment Feed reads must fail closed through Workload Identity, an exact audience, explicit tenant authorization, and a durable opaque cursor; collection must never acknowledge or mutate workflow execution. Business routes and migrations must still come from Modules rather than platform-owned business handlers.
- Managed-Service System Plane routes must use `platform-system-plane` admission: authenticated transport, audience-bound Workload Identity, and the current Service-owned Enrollment Grant are all required. Production enrollment must enter through the signed bilateral ceremony: verify the expiring Console Offer, locally choose a non-widening capability/policy Grant, sign the Service Receipt, then atomically persist Grant, Receipt, and append-only audit evidence. Possession of an Offer grants no authority; unsigned Store bootstrap is forbidden outside System Sandbox. One active Console Service Principal, exact contract/schema/feature scope, expiry, revocation, and monotonic authorization epochs are enforced from the Service Store. Capability Providers own their observations and operations, advertise the exact contract/schema digest through Core, remain independent of Console state and UI vocabulary, and keep read-only snapshots out of Story mutation. Observation recovery must start from a snapshot watermark, use a Service-owned opaque cursor, and surface invalid cursors, Service or schema revision changes, and retention loss as an explicit Evidence Gap that requires a fresh snapshot. A mutation must bind a fresh target revision and exact negotiated contract to an immutable Management Intent, return a side-effect-free expiring Plan Receipt, revalidate the unchanged plan and deployment-owned authority at submission, persist the accepted operation, authorization evidence, and acknowledgement before its effect, and expose Service-owned terminal Operation Evidence. Lost responses are resolved by operation identity and idempotency keys; callers must never blindly replay a mutation.
- Extraction readiness rules and the versioned report contract belong in the public `lenso-service` surface. CLI-owned analyzers may collect repository evidence and explicitly read-only live Store observations, but they must call the shared evaluator and render its report; they must not duplicate classification rules. Table and migration ownership must resolve to one Module, direct cross-Module table access and cross-boundary transactions block extraction, and large volume or missing cursors remain explicit planning risk. Readiness analysis is read-only and must never write repository files, start Workloads, move data, or change authority. Provider v1 and System v1 semantics remain unchanged.
- Extraction Plan artifact types, deterministic ordering, content addressing, phase vocabulary, dry-run effects, and stale-input validation belong in the public `lenso-service` surface. Pin the exact readiness, Module, Contract Version, System graph, analyzer, data mapping, evidence, and expected authority inputs; reject plan-integrity or input drift before mutation. CLI-owned orchestration may consume this artifact but must not reinterpret its digests, reorder phases, bypass `commit-extraction-authority`, or treat repository write access as Cutover authority.
- Extraction Scaffold artifacts and safety validation belong in the public `lenso-service` surface. Generate the candidate only from an integrity-valid plan, its exact Module declaration, and digest-matching authoritative Contract artifacts. Preserve the complete Module identity, derive HTTP/gRPC/Event bindings and Service Clients through the existing public Contract generators, and keep all remaining Module behavior local. Dry-run must expose the exact deterministic patch; apply must reject stale plans, changed targets, symlink traversal, and unrecognized target files before overwriting anything. Scaffold apply may create candidate files only: it must not start Workloads, move data, change authority, or modify Provider v1 files.
- Destination expansion state, operation ordering, receipt validation, and the `lenso.extraction-run.v1` artifact belong in the public `lenso-service` surface. Bind every migration to its plan-pinned source path and SQL digest, accept only conservative expand-first Postgres statements, and target only the isolated candidate Store. Advance at most one operation per persisted Run revision and inspect the public Workload receipt before execution so restarts never repeat a completed effect. Candidate Migration and API health must pass through public Workload behavior. This phase must not expose source mutation credentials, copy Service Data, modify linked behavior, change authority, or perform destructive cleanup.
- Federated Runtime Story aggregation belongs to the Story observability boundary, never an Autonomous Service execution path. Keep per-source cursors and collected Segment revisions in the aggregator Store, preserve source and tenant identity, represent source failures as typed gaps, and treat OpenTelemetry as optional node enrichment only. Aggregation availability must not affect Service-local capture, Workflow state, Inbox, Outbox, timers, or dispatch.
- Ecosystem Modules choose Linked or Service delivery explicitly. Linked is the primary in-process Module experience and Service delivery is the extraction or independent-responsibility path; provenance and policy decide whether a particular release is eligible, not whether its publisher is first-party.
- Custom admin UI must keep host-rendered declarations and module-owned embedded UI separate: use `DeclarativeCustom` for trusted Console rendering, and `EmbeddedCustom` for iframe/Wasm/other sandboxed module-owned UI. Do not model both as one generic `Custom` surface.
- Embedded custom admin surfaces must not receive host bearer tokens or ad hoc bridge access. Any bridge must be a versioned protocol with explicit manifest permissions and host enforcement.

## Contracts

- No HTTP API without OpenAPI schema coverage.
- HTTP handlers carry their own `#[utoipa::path]` annotation and are registered via `utoipa-axum`'s `OpenApiRouter` (`routes!`), so each route's path and parameters are authored once. Do not add detached `#[utoipa::path]` stub functions.
- `crates/lenso-api/src/openapi.rs` holds only document-level metadata (info, tags); it must not re-declare path or schema lists that the annotated handlers already provide.
- No event payload without a JSON Schema contract under `contracts/events/`.
- No runtime function without a JSON Schema contract under `contracts/runtime/functions/`.
- Error responses must use the standard error shape.
- Generated contract artifacts must be regenerated with
  `cargo run --locked -p lenso-api-contracts --bin generate-contracts`.
- Generated contract artifacts must not be manually patched.
- OpenAPI route invariants belong to the `lenso-api` integration tests;
  handwritten contract artifacts must still parse and use names that match their
  path and title.
- The architecture checker rejects root `tools/`, `scripts/`, or task-runner
  files. Development tooling belongs to its owning crate, package, or workflow.

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
cargo test --locked -p lenso-api-contracts --test architecture
```

The owner tests and CI fail on root tooling drift, forbidden module folders,
forbidden cross-module imports inside module source code, malformed contract
JSON/YAML, missing event contracts referenced by source code, event contract
name/path mismatches, missing runtime function contracts for registered module
runtime functions, and runtime function contract name/path mismatches. OpenAPI
route invariants are tested by `lenso-api`; generated byte freshness is tested
by `generated_artifacts`. Console source guardrails live in the sibling frontend
repository.
