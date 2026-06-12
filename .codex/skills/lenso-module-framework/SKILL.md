---
name: lenso-module-framework
description: Use when changing or reasoning about Lenso's module framework, crates/platform-module, crates/platform-admin-data, crates/app-bootstrap module registration, AdminSurface/AdminDataSource, ModuleManifest/ModuleBinding, manifest lints, schema-admin, custom admin surfaces, or loading-source architecture.
---

# Lenso Module Framework

## Purpose

Use this skill whenever work touches module loading, module metadata, admin surfaces, schema-driven data pages, manifest lints, or composition-root registration. Treat it as a compact architecture guide, but verify live code and docs before extending older imported memory.

## Current Direction

Lenso is evolving from a fixed modular monolith into a module framework where linked and out-of-process modules can be registered, inspected, and managed through host-owned platform surfaces.

The evolution is staged, but several remote/custom slices now exist:

1. Split `DomainDescriptor` into `ModuleManifest` data plus `ModuleBinding` behavior. Done.
2. Add `AdminSurface::Schema`, a data protocol, and capabilities for schema-driven business CRUD. Read-only vertical slice done.
3. Add `Remote` out-of-process module source. Manifest loading, schema-admin reads, admin surface metadata, HTTP route metadata, host-owned HTTP proxying, and proxy-backed runtime functions are implemented slices.
4. Add custom admin surfaces. `EmbeddedCustom` iframe rendering and `DeclarativeCustom` trusted component rendering have first Runtime Console slices; action bridges and richer protocols are deferred.
5. Add `Wasm` and marketplace trust as future source/execution lanes.

Keep two axes separate:

- Loading source: `Linked` and configured `Remote` today, later `Wasm`.
- Admin rendering: `AdminSurface::Schema` for plain business-entity CRUD, `DeclarativeCustom` for host-rendered custom UI, and `EmbeddedCustom` for sandboxed module-owned UI.

Use `$lenso-remote-modules` for remote protocol, proxy, remote runtime, and remote/custom admin implementation details.

## Manifest And Binding

`crates/platform-module` is the source of truth.

- `ModuleManifest` is owned, serializable data. It carries declarations such as name, story display, capabilities, and optional admin surface.
- `ModuleBinding` is a narrow behavior trait. It abstracts only what varies by source, such as runtime function registration, event handler registration, and HTTP route behavior.
- `LinkedBinding` is the in-process source. Remote modules are loaded through `platform-module-remote` and app-bootstrap wiring.
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
- Current schema-admin scope is list/detail. Writes, multi-entity expansion, fine-grained RBAC, and richer action protocols are deferred. Remote modules may provide schema-admin reads through `platform-module-remote` when their manifest exposes `AdminSurface::Schema` or a custom surface with `fallback_schema`.

Important type-shape rule:

- Do not re-add `#[non_exhaustive]` to `AdminSchema`, `EntitySchema`, `FieldSchema`, or `AdminPage`; producer crates need struct literal construction.
- Keep `#[non_exhaustive]` on consumer-matched enums such as `FieldType` and `AdminSurface`.

## Platform Boundaries

`platform-admin` and `platform-admin-data` are separate on purpose.

- `platform-admin` is the Runtime Console observability backend under `/admin/runtime/*`. It reads platform/runtime tables and has zero business-domain dependencies.
- `platform-admin-data` is the schema-admin backend under `/admin/data/*`. It works through injected `AdminModule` registry entries, `AdminSurface::Schema`, and `AdminDataSource`; it must not depend on concrete domains.
- `crates/app-bootstrap` is the composition root. It pairs manifests, bindings, story display descriptors, and data sources from concrete modules.

Manifest lint rules belong in `platform-module`. `platform-admin-data` exposes lint results through module metadata, and Runtime Console screens render them without duplicating lint logic locally.

## Custom Admin Surfaces

Custom admin UI has two distinct lanes, documented in
`docs/architecture/module-custom-admin-surfaces.md`.

- `DeclarativeCustom`: host-rendered custom UI. Modules declare pages, components, data bindings, actions, and optional `fallback_schema` as serializable manifest data. The Runtime Console renders trusted components and does not execute module-provided frontend code.
- `EmbeddedCustom`: module-owned UI behind a sandbox boundary. The first Runtime Console lane is a sandboxed iframe with explicit origin allowlists and no host bridge. Wasm and JS bundles are reserved for later specs.

Do not collapse these into one generic `Custom` variant. Their fields, security
models, and implementation order are different. Embedded surfaces must not get
host bearer tokens or ad hoc `postMessage` access; any bridge needs a versioned
protocol and manifest-declared permissions.

## Remote Modules

Configured remote modules are loaded through `platform-module-remote` and composed in `crates/app-bootstrap`.

Current remote slices include:

- Remote manifest loading into the same `ModuleManifest` data contract used by linked modules.
- Remote schema-admin reads through protocol-backed `AdminDataSource` behavior.
- Remote admin metadata for schema, declarative custom, and embedded custom surfaces.
- Declared host-owned HTTP proxy routes under `/modules/{module}/http/{*path}`.
- Persisted remote proxy call history, Runtime Story nodes, and Technical Operations rows.
- Proxy-backed runtime function handlers registered into the host `FunctionRegistry`.

Keep the host responsible for auth, capability checks, request/response limits, error normalization, persisted visibility, queues, retries, and story semantics. Do not let remote modules claim runtime rows, consume outbox rows, receive caller bearer tokens, or contribute arbitrary host bridges.

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

For remote-module implementation, also use `$lenso-remote-modules`. For OpenAPI or SDK-affecting changes, also use `$lenso-contracts-sdk`.
