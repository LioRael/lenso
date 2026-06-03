---
name: lenso-module-framework
description: Use when changing or reasoning about Lenso's module framework, crates/platform-module, crates/platform-admin-data, crates/app-bootstrap module registration, AdminSurface/AdminDataSource, module manifests/bindings, or future Remote/Wasm/custom admin module work. This is the imported Claude Code project memory for the module architecture.
---

# Lenso Module Framework

## Purpose

Use this skill whenever work touches module loading, module metadata, admin surfaces, schema-driven data pages, or composition-root registration. It captures Claude Code project memory imported on 2026-06-03 and should be treated as the current architecture reference unless the code has moved on.

## Current Direction

Lenso is evolving from a fixed modular monolith into a module framework where third-party business modules can eventually be installed from a market and managed in the Runtime Console.

The evolution is staged:

1. Split `DomainDescriptor` into `ModuleManifest` data plus `ModuleBinding` behavior. Done.
2. Add `AdminSurface::Schema`, a data protocol, and capabilities for schema-driven business CRUD. Read-only vertical slice done.
3. Add `Remote` out-of-process module source. Future.
4. Add custom admin surfaces and `Wasm` module source. Future.

Keep two axes separate:

- Loading source: `Linked` today, later `Remote` and `Wasm`.
- Admin rendering: `AdminSurface::Schema` for plain business-entity CRUD, later `DeclarativeCustom` for host-rendered custom UI and `EmbeddedCustom` for sandboxed module-owned UI.

## Manifest And Binding

`crates/platform-module` is the source of truth.

- `ModuleManifest` is owned, serializable data. It carries declarations such as name, story display, capabilities, and optional admin surface.
- `ModuleBinding` is a narrow behavior trait. It abstracts only what varies by source, such as runtime function registration, event handler registration, and HTTP route merging.
- `LinkedBinding` is the only implemented source today.
- Do not move pure data into `ModuleBinding`; upper layers should read manifest data directly.
- Do not replace the open loading-source seam with a closed enum. Future sources should be new binding/source implementations.

## Schema Admin

Step 2 implemented a minimal read-only vertical slice:

- `AdminSurface::Schema(AdminSchema)` is pure manifest data.
- `AdminDataSource` is the separate behavior seam for list/detail reads.
- `Module` holds optional `admin_data: Option<Arc<dyn AdminDataSource>>`.
- Records cross the module boundary as `serde_json::Value`; strong domain types stay inside domains and convert at the seam exit.
- Identity User is the first schema-admin entity.
- Pagination uses `limit` plus opaque `Option<String>` cursor.
- Current scope is list/detail. Writes, multi-entity expansion, fine-grained RBAC, remote data sources, and custom admin UI are deferred.

Important type-shape rule:

- Do not re-add `#[non_exhaustive]` to `AdminSchema`, `EntitySchema`, `FieldSchema`, or `AdminPage`; producer crates need struct literal construction.
- Keep `#[non_exhaustive]` on consumer-matched enums such as `FieldType` and `AdminSurface`.

## Platform Boundaries

`platform-admin` and `platform-admin-data` are separate on purpose.

- `platform-admin` is the Runtime Console observability backend under `/admin/runtime/*`. It reads platform/runtime tables and has zero business-domain dependencies.
- `platform-admin-data` is the schema-admin backend under `/admin/data/*`. It works through injected `AdminModule` registry entries, `AdminSurface::Schema`, and `AdminDataSource`; it must not depend on concrete domains.
- `crates/app-bootstrap` is the composition root. It pairs manifests, bindings, story display descriptors, and data sources from concrete modules.

The observability console may eventually become an installable module using an embedded custom surface. Do not build that now; just avoid shaping schema-admin as if every future admin surface were plain CRUD.

## Custom Admin Surfaces

Future custom admin UI has two distinct lanes, documented in
`docs/architecture/module-custom-admin-surfaces.md`.

- `DeclarativeCustom`: host-rendered custom UI. Modules declare pages,
  components, data bindings, and actions as serializable manifest data. The
  Runtime Console renders trusted components and does not execute
  module-provided frontend code.
- `EmbeddedCustom`: module-owned UI behind a sandbox boundary. First target is a
  sandboxed iframe with explicit origin allowlists and no host bridge. Wasm and
  JS bundles are reserved for later specs.

Do not collapse these into one generic `Custom` variant. Their fields, security
models, and implementation order are different. Embedded surfaces must not get
host bearer tokens or ad hoc `postMessage` access; any bridge needs a versioned
protocol and manifest-declared permissions.

## OpenAPI

OpenAPI is single-source through `utoipa-axum`.

- Put `#[utoipa::path(...)]` on real handlers.
- Register handlers with `OpenApiRouter::new().routes(routes!(handler))`.
- Keep document-level metadata and assembly in `apps/api/src/openapi.rs`.
- Do not add detached stub functions just to carry OpenAPI annotations.
- Keep `openapi_document()` pure and context-free; generators, arch checks, and sync tests call it outside a Tokio runtime.

## Validation

Use the narrowest meaningful check for the change:

```sh
cargo check --locked -p platform-module --all-targets
cargo test --locked -p platform-module
cargo check --locked -p platform-admin-data --all-targets
cargo test --locked -p platform-admin-data
just arch-check
```

For OpenAPI or SDK-affecting changes, also use `$lenso-contracts-sdk`.
