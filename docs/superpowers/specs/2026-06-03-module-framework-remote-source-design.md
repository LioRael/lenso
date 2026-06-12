# Module Framework Step 3 — Remote Module Source

**Date:** 2026-06-03
**Status:** Implemented initial slice
**Scope:** Step 3 of the module-framework evolution. Introduce an out-of-process module source without collapsing the manifest/behavior split.

---

## Context & Vision

Lenso is evolving into a module framework where business modules can eventually be installed from a market and managed in the Runtime Console.

The 4-step roadmap:

1. Manifest/Binding split — **DONE**.
2. `AdminSurface::Schema` + data protocol + capabilities seam — **DONE** as a read-only vertical slice.
3. `Remote` out-of-process module source. ← **THIS SPEC**
4. `AdminSurface::Custom` self-rendering modules + `Wasm` module source.

Step 1 made `ModuleManifest` owned and serializable so every loading source can produce the same data contract. Step 2 made schema-admin records cross the module boundary as `serde_json::Value`, which is also remote-friendly. Step 3 fills the first non-Linked loading source while keeping HTTP routing and custom UI deferred.

## Scope

This spec delivers a minimal Remote source slice:

- Load a `ModuleManifest` from an out-of-process module endpoint.
- Expose remote schema-admin read operations through the existing `AdminDataSource` seam.
- Keep the API and console generic: `/admin/data/*` continues to work through `platform-admin-data`.
- Provide one local test double remote module for contract and integration tests.

Everything else is deferred.

## Non-goals

- Remote modules contributing Axum HTTP routes or OpenAPI paths.
- Remote modules registering executable runtime functions.
- Remote event handlers or remote outbox dispatch.
- Write operations for schema-admin entities.
- `AdminSurface::Custom` or any plugin-owned frontend bundle.
- Wasm loading.
- Module marketplace installation, version negotiation, signature verification, or sandbox policy.

Remote runtime functions and event handlers are intentionally out of scope because they need execution, retries, idempotency, auth, and observability semantics. This first slice is data-plane only: manifest plus schema-admin reads.

## Key Decisions

| Decision | Choice | Why |
|----------|--------|-----|
| First Remote capability | Manifest + schema-admin read data | Reuses the existing serializable contracts and proves out-of-process modules without inventing execution semantics. |
| HTTP routing | Still Linked-only via `app-bootstrap::merge_domain_http` | Remote Axum/OpenAPI contribution is a larger protocol problem and should not block the first Remote slice. |
| Runtime/event behavior | Not supported for Remote in this step | The current `ModuleBinding` methods register in-process Rust handlers; remote execution needs a separate spec. |
| Remote contract | JSON over HTTP, versioned under a small module protocol | Boring transport, easy to test locally, future-compatible with market-installed modules. |
| Manifest source | `RemoteModuleSource` fetches `ModuleManifest` | Keeps manifest data independent from behavior and preserves Step 1's architecture. |
| Admin reads | `RemoteAdminDataSource` implements `AdminDataSource` | `platform-admin-data` stays generic and does not learn about remotes. |
| Crate placement | New `platform-module-remote` crate | Keeps transport/client code out of the core `platform-module` contracts. |
| Auth/security | Configurable shared-token header for dev/test only | Gives the seam a place for auth without pretending market-grade trust is solved. |

## Architecture

```
remote module process
  GET /lenso/module/v1/manifest
  GET /lenso/module/v1/admin/{entity}
  GET /lenso/module/v1/admin/{entity}/{id}
        ▲ JSON module protocol
platform-module-remote
  RemoteModuleSource -> ModuleManifest
  RemoteAdminDataSource: AdminDataSource
        ▲ produces Module { manifest, binding, admin_data }
app-bootstrap
  linked modules + configured remote modules
        ▲ injected registry
platform-admin-data
  unchanged /admin/data/* endpoints
runtime-console
  unchanged generic Data page
```

The remote module is not a Rust crate dependency of the host. It is configured as an endpoint. The host fetches its manifest and routes schema-admin reads through a transport-backed `AdminDataSource`.

## Protocol

Remote module endpoints are versioned by path:

```text
GET /lenso/module/v1/manifest
GET /lenso/module/v1/admin/{entity}?limit=50&cursor=...
GET /lenso/module/v1/admin/{entity}/{id}
```

Responses:

```json
// GET /manifest
{
  "name": "remote-crm",
  "story_display": [],
  "admin": {
    "kind": "schema",
    "entities": [
      {
        "name": "contacts",
        "label": "Contacts",
        "fields": [],
        "read_capability": "remote_crm.contacts.read"
      }
    ]
  },
  "capabilities": ["remote_crm.contacts.read"]
}
```

```json
// GET /admin/{entity}
{
  "records": [{ "id": "contact_1", "email": "sam@example.com" }],
  "next_cursor": null
}
```

```json
// GET /admin/{entity}/{id}
{
  "record": { "id": "contact_1", "email": "sam@example.com" }
}
```

Errors use the standard platform error shape where possible. The host maps remote transport failures, unsupported entities, and malformed responses into normal `AppError`s before they reach `platform-admin-data`.

## Contracts And Types

Add `crates/platform-module-remote`:

```rust
pub struct RemoteModuleConfig {
    pub name: String,
    pub base_url: String,
    pub auth_token: Option<String>,
    pub timeout_ms: u64,
}

pub struct RemoteModuleSource {
    client: reqwest::Client,
    config: RemoteModuleConfig,
}

impl RemoteModuleSource {
    pub async fn load(&self) -> AppResult<Module> {
        let manifest = self.fetch_manifest().await?;
        let has_schema_admin = matches!(manifest.admin, Some(AdminSurface::Schema(_)));
        let mut module = Module::remote(manifest, RemoteBinding::default());

        if has_schema_admin {
            module = module.with_admin_data(Arc::new(RemoteAdminDataSource::new(...)));
        }

        Ok(module)
    }
}
```

`RemoteBinding` is deliberately inert in this step:

```rust
#[derive(Debug, Default)]
pub struct RemoteBinding;

impl ModuleBinding for RemoteBinding {
    fn register_functions(&self, _registry: &mut FunctionRegistry) {}
    fn register_event_handlers(&self, _registry: &mut EventHandlerRegistry) {}
}
```

This makes a Remote module a first-class loaded `Module` while making the unsupported behavior explicit. A later remote-execution spec can replace the inert behavior with a real transport-backed execution model.

`RemoteAdminDataSource` implements the existing trait:

```rust
#[async_trait::async_trait]
impl AdminDataSource for RemoteAdminDataSource {
    async fn list(&self, entity: &str, query: &AdminListQuery) -> AppResult<AdminPage>;
    async fn get(&self, entity: &str, id: &str) -> AppResult<Option<Value>>;
}
```

## App Bootstrap

`app-bootstrap` remains the only place that enumerates modules. It should aggregate:

- Linked modules from Rust crates: identity, notifications.
- Remote modules from runtime configuration.

Because remote loading is async and fallible, avoid forcing the existing synchronous `modules(ctx) -> Vec<Module>` to become async everywhere in one step. Instead introduce a startup-loaded registry:

```rust
pub async fn load_modules(ctx: &AppContext) -> AppResult<Vec<Module>>;
```

Then keep the existing synchronous helpers for context-free paths:

- `module_manifests()` remains Linked-only for `openapi_document()` purity.
- `merge_domain_http()` remains Linked-only.
- Runtime startup paths that need remotes use `load_modules(ctx)`.

OpenAPI must stay pure and context-free. Remote module manifests do not contribute OpenAPI paths in this step, so they do not belong in `openapi_document()`.

## Configuration

Use runtime config or app config to declare remote module endpoints:

```toml
[[modules.remote]]
name = "remote-crm"
base_url = "http://localhost:4100/lenso/module/v1"
auth_token_env = "LENSO_REMOTE_CRM_TOKEN"
timeout_ms = 5000
```

Do not persist marketplace installation state yet. This is a local configured source, enough to validate the protocol and host integration.

## Tests

Targeted tests:

- `platform-module-remote`: manifest JSON round-trip and malformed manifest handling.
- `platform-module-remote`: `RemoteAdminDataSource::list/get` maps remote responses into `AdminPage`/`Value`.
- `app-bootstrap`: remote modules appear in loaded admin modules when configured.
- `platform-admin-data`: existing generic route tests pass unchanged with a remote-backed data source.
- `arch-check`: no concrete domain dependency from `platform-module-remote`, `platform-admin`, or `platform-admin-data`.

Validation:

```sh
cargo check --locked -p platform-module-remote --all-targets
cargo test --locked -p platform-module-remote
cargo test --locked -p platform-admin-data
just arch-check
```

If OpenAPI or SDK types change unexpectedly, treat that as a design smell for this step.

## Open Questions

- Should remote module endpoints use the host's standard error shape strictly, or should the host accept a looser remote error envelope and normalize it?
- Should `RemoteModuleConfig` live in app config only, or should it be editable through runtime config after the first slice lands?
- Should a remote module's manifest be cached for the process lifetime, or refreshed with an explicit admin action?
- Should the host require remote manifest `name` to match the configured name?

## Implementation Order

1. Add `platform-module-remote` with protocol DTOs, config, and HTTP client.
2. Add `RemoteAdminDataSource` unit tests against a local test server.
3. Add inert `RemoteBinding` and a `Module::remote` constructor.
4. Add async `app_bootstrap::load_modules(ctx)` that includes configured remote modules.
5. Wire runtime/admin-data startup paths to the loaded module set without changing pure OpenAPI assembly.
6. Add arch-check coverage for remote/platform boundaries.
