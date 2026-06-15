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

- The concrete module set is enumerated only in `crates/app-bootstrap`. Apps must not hand-wire individual modules.
- A new module is registered through the `app-bootstrap` entry points it needs: `modules` (runtime functions, event handlers, runtime config, admin data), `module_manifests` (context-free metadata), `merge_linked_http` (Linked HTTP routes), and `story_display_descriptors` (runtime console metadata).
- Each module exposes module data as `ModuleManifest` from the public `lenso` facade and source-specific behavior through `ModuleBinding` from `platform-module`; do not recreate descriptor types per module.
- Keep data and behavior split. Serializable declarations belong in `ModuleManifest`; behavior belongs behind narrow traits such as `ModuleBinding` and `AdminDataSource`.
- `platform-admin` and `platform-admin-data` must not depend on concrete module crates. Use composition-root injection and platform-module seams.
- Module manifest lint rules belong in the public `lenso` facade and are exposed through backend metadata endpoints such as `/admin/data/modules`. Runtime Console screens may filter, group, and render lint results, but must not duplicate backend manifest lint rules in TypeScript. Keep `docs/architecture/module-manifest-lints.md` current when adding or changing lint subjects.
- Runtime Console frontend contributions must use `ModuleManifest.console` and the host console module registry described in `docs/architecture/module-console-surfaces.md`. Console modules must import host services through a narrow host facade, not by deep-importing host pages, hooks, components, or data internals.
- Remote modules may provide manifests, declared HTTP route metadata, schema-admin read data, runtime functions, and event handlers through `platform-module-remote`. Declared remote HTTP routes are mounted only through the host policy in `docs/architecture/module-remote-http-proxy.md`. Remote runtime functions and event handlers must follow `docs/architecture/module-remote-runtime.md`: the host owns queues, outbox claims, retries, stories, and worker execution.
- Third-party ecosystem modules should default to Remote packaging as described in `docs/architecture/third-party-modules.md`. Do not compile third-party code into the host by default; `Linked` is for first-party application modules, framework fixtures, and local project-owned modules.
- Custom admin UI must keep host-rendered declarations and module-owned embedded UI separate: use `DeclarativeCustom` for trusted Runtime Console rendering, and `EmbeddedCustom` for iframe/Wasm/other sandboxed module-owned UI. Do not model both as one generic `Custom` surface.
- Embedded custom admin surfaces must not receive host bearer tokens or ad hoc bridge access. Any bridge must be a versioned protocol with explicit manifest permissions and host enforcement.

## Contracts

- No HTTP API without OpenAPI schema coverage.
- HTTP handlers carry their own `#[utoipa::path]` annotation and are registered via `utoipa-axum`'s `OpenApiRouter` (`routes!`), so each route's path and parameters are authored once. Do not add detached `#[utoipa::path]` stub functions.
- `apps/api/src/openapi.rs` holds only document-level metadata (info, tags); it must not re-declare path or schema lists that the annotated handlers already provide.
- No event payload without a JSON Schema contract under `contracts/events/`.
- No runtime function without a JSON Schema contract under `contracts/runtime/functions/`.
- Error responses must use the standard error shape.
- Generated contract artifacts must be regenerated with `just generate-contracts`.
- Generated contract artifacts must not be manually patched.
- Handwritten contract artifacts must still parse and use names that match their path and title.

## SDK

- The TypeScript SDK is generated from `contracts/openapi/app-api.v1.yaml`.
- Do not hand-edit files under `packages/ts-sdk/src/generated/`.
- Handwritten SDK code belongs in `packages/ts-sdk/src/index.ts`.
- Regenerate the SDK with `just generate-ts-sdk` after changing OpenAPI.

## Runtime And Outbox

- The runtime must not own business logic.
- Module commands that write data and emit events must use the transactional outbox.
- Module event handlers may enqueue runtime functions, but function behavior stays in the owning module.
- Runtime function names must be stable, versioned, and documented under `contracts/runtime/functions/`.
- Do not add NATS, Kafka, service mesh, or Kubernetes complexity before there is a real extraction need.

## Enforcement

Run:

```sh
just arch-check
```

The checker fails on forbidden module folders, forbidden cross-module imports inside module source code, stale generated contracts, stale generated SDK files, missing OpenAPI artifacts, malformed contract JSON/YAML, missing event contracts referenced by source code, event contract name/path mismatches, missing runtime function contracts for registered module runtime functions, and runtime function contract name/path mismatches. Runtime Console source guardrails live in the sibling frontend repository.
