# Module Framework Step 3 - Remote Source Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the first out-of-process module source: configured remote modules can provide a `ModuleManifest` and schema-admin read data while existing API, worker, OpenAPI, and Runtime Console paths stay generic.

**Architecture:** `platform-module` remains the contract crate; a new `platform-module-remote` crate owns HTTP transport and implements `AdminDataSource` for remote schema-admin reads. Remote modules are loaded at startup through `app-bootstrap::load_modules(ctx)`, while `module_manifests()` and `merge_domain_http()` stay Linked-only so `openapi_document()` remains pure and context-free.

**Tech Stack:** Rust 2024, `reqwest`, `axum` test servers, `serde`, `serde_json`, `async-trait`, `tokio`, existing `platform-core`, `platform-module`, `platform-admin-data`, and `app-bootstrap`. Quality gate is `cargo check` and targeted tests, plus `just arch-check`.

**Spec:** `docs/superpowers/specs/2026-06-03-module-framework-remote-source-design.md`

---

## File Structure

**Created:**
- `crates/platform-module-remote/Cargo.toml` - remote module source crate manifest.
- `crates/platform-module-remote/src/lib.rs` - public exports and crate docs.
- `crates/platform-module-remote/src/config.rs` - `RemoteModuleConfig`.
- `crates/platform-module-remote/src/protocol.rs` - remote HTTP protocol DTOs.
- `crates/platform-module-remote/src/source.rs` - `RemoteModuleSource::load`.
- `crates/platform-module-remote/src/admin_data.rs` - `RemoteAdminDataSource`.
- `crates/platform-module-remote/src/binding.rs` - inert `RemoteBinding`.
- `crates/platform-module-remote/tests/remote_source.rs` - local HTTP-server integration tests.

**Modified:**
- `Cargo.toml` - add workspace member and dependency for `platform-module-remote`; add `reqwest` and test HTTP dependency if not already present.
- `crates/platform-module/src/module.rs` - add `Module::remote`.
- `crates/platform-module/src/lib.rs` - no new remote export; remote source lives in `platform-module-remote`.
- `crates/platform-core/src/config.rs` - add typed `RemoteModuleConfig` parsing from environment.
- `crates/platform-core/src/lib.rs` - export remote config type.
- `crates/app-bootstrap/Cargo.toml` - add `platform-module-remote`.
- `crates/app-bootstrap/src/lib.rs` - add `load_modules(ctx)` and `load_admin_modules(ctx)`.
- `apps/api/Cargo.toml` - add `platform-module-remote` only if API needs direct type references; prefer app-bootstrap only.
- `apps/api/src/main.rs` - install loaded admin modules after remote modules are loaded.
- `apps/worker/src/main.rs` - use `load_modules(ctx).await` for function/event registries.
- `tools/arch-check/src/lib.rs` - extend no-domain-dep guard to `platform-module-remote`.
- `docs/architecture/overview.md` and `docs/architecture/rules.md` - mention configured Remote source after implementation.

---

## Task 1: Remote Protocol Crate

Add `platform-module-remote` with protocol DTOs, client config, and tests for manifest/list/detail transport. Nothing in the host consumes it yet.

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/platform-module-remote/Cargo.toml`
- Create: `crates/platform-module-remote/src/lib.rs`
- Create: `crates/platform-module-remote/src/config.rs`
- Create: `crates/platform-module-remote/src/protocol.rs`
- Create: `crates/platform-module-remote/src/admin_data.rs`
- Create: `crates/platform-module-remote/src/binding.rs`
- Create: `crates/platform-module-remote/src/source.rs`

- [ ] **Step 1: Add workspace dependencies**

Modify root `Cargo.toml`:

```toml
members = [
    "apps/api",
    "apps/worker",
    "apps/migrate",
    "crates/app-bootstrap",
    "crates/platform-admin",
    "crates/platform-admin-data",
    "crates/platform-core",
    "crates/platform-module",
    "crates/platform-module-remote",
    "crates/platform-http",
    "crates/platform-runtime",
    "crates/platform-testing",
    "domains/identity",
    "domains/notifications",
    "tools/generate-contracts",
    "tools/generate-ts-sdk",
    "tools/arch-check",
    "tools/otel-smoke",
]
```

Add workspace deps:

```toml
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }

platform-module-remote = { path = "crates/platform-module-remote" }
```

- [ ] **Step 2: Create crate manifest**

Create `crates/platform-module-remote/Cargo.toml`:

```toml
[package]
name = "platform-module-remote"
version = "0.1.0"
edition.workspace = true
license.workspace = true
publish.workspace = true
rust-version.workspace = true

[dependencies]
async-trait.workspace = true
platform-core.workspace = true
platform-module.workspace = true
platform-runtime.workspace = true
reqwest.workspace = true
serde.workspace = true
serde_json.workspace = true
tracing.workspace = true

[dev-dependencies]
axum.workspace = true
tokio.workspace = true

[lints]
workspace = true
```

- [ ] **Step 3: Create public module exports**

Create `crates/platform-module-remote/src/lib.rs`:

```rust
//! HTTP-backed module source for out-of-process modules.
//!
//! This crate owns transport only. Core contracts stay in `platform-module`,
//! and host integration stays in `app-bootstrap`.

mod admin_data;
mod binding;
mod config;
mod protocol;
mod source;

pub use admin_data::RemoteAdminDataSource;
pub use binding::RemoteBinding;
pub use config::RemoteModuleConfig;
pub use source::RemoteModuleSource;
```

- [ ] **Step 4: Add remote config type**

Create `crates/platform-module-remote/src/config.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteModuleConfig {
    pub name: String,
    pub base_url: String,
    pub auth_token: Option<String>,
    pub timeout_ms: u64,
}

impl RemoteModuleConfig {
    #[must_use]
    pub fn new(name: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            auth_token: None,
            timeout_ms: 5_000,
        }
    }

    #[must_use]
    pub fn with_auth_token(mut self, token: impl Into<String>) -> Self {
        self.auth_token = Some(token.into());
        self
    }

    #[must_use]
    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }
}
```

- [ ] **Step 5: Add protocol DTOs**

Create `crates/platform-module-remote/src/protocol.rs`:

```rust
use platform_module::{AdminPage, ModuleManifest};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type RemoteManifestResponse = ModuleManifest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteListResponse {
    pub records: Vec<Value>,
    pub next_cursor: Option<String>,
}

impl From<RemoteListResponse> for AdminPage {
    fn from(value: RemoteListResponse) -> Self {
        Self {
            records: value.records,
            next_cursor: value.next_cursor,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteGetResponse {
    pub record: Option<Value>,
}
```

- [ ] **Step 6: Implement remote admin data source**

Create `crates/platform-module-remote/src/admin_data.rs`:

```rust
use crate::config::RemoteModuleConfig;
use crate::protocol::{RemoteGetResponse, RemoteListResponse};
use platform_core::{AppError, AppResult, ErrorCode};
use platform_module::{AdminDataSource, AdminListQuery, AdminPage};
use reqwest::StatusCode;
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct RemoteAdminDataSource {
    client: reqwest::Client,
    config: RemoteModuleConfig,
}

impl RemoteAdminDataSource {
    pub fn new(config: RemoteModuleConfig) -> AppResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to build remote module client: {error}"),
                )
            })?;
        Ok(Self { client, config })
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.config.base_url, path.trim_start_matches('/'))
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let request = self.client.request(method, self.url(path));
        match &self.config.auth_token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    async fn send_json<T: serde::de::DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
    ) -> AppResult<Option<T>> {
        let response = request
            .send()
            .await
            .map_err(|error| {
                AppError::new(
                    ErrorCode::ExternalDependency,
                    format!("remote module request failed: {error}"),
                )
                .retryable()
            })?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !response.status().is_success() {
            return Err(AppError::new(
                ErrorCode::ExternalDependency,
                format!(
                "remote module returned status {}",
                response.status()
            ))
            .retryable());
        }

        let body = response
            .json::<T>()
            .await
            .map_err(|error| {
                AppError::new(
                    ErrorCode::ExternalDependency,
                    format!("remote module response was invalid JSON: {error}"),
                )
            })?;
        Ok(Some(body))
    }
}

#[async_trait::async_trait]
impl AdminDataSource for RemoteAdminDataSource {
    async fn list(&self, entity: &str, query: &AdminListQuery) -> AppResult<AdminPage> {
        let mut request = self
            .request(reqwest::Method::GET, &format!("admin/{entity}"))
            .query(&[("limit", query.limit.to_string())]);
        if let Some(cursor) = &query.cursor {
            request = request.query(&[("cursor", cursor)]);
        }
        let response = self
            .send_json::<RemoteListResponse>(request)
            .await?
            .ok_or_else(|| AppError::new(ErrorCode::NotFound, "remote admin entity not found"))?;
        Ok(response.into())
    }

    async fn get(&self, entity: &str, id: &str) -> AppResult<Option<Value>> {
        let request = self.request(reqwest::Method::GET, &format!("admin/{entity}/{id}"));
        Ok(self
            .send_json::<RemoteGetResponse>(request)
            .await?
            .and_then(|response| response.record))
    }
}
```

- [ ] **Step 7: Add compile stubs for binding/source**

Create `crates/platform-module-remote/src/binding.rs`:

```rust
use platform_core::EventHandlerRegistry;
use platform_module::ModuleBinding;
use platform_runtime::FunctionRegistry;

#[derive(Debug, Default)]
pub struct RemoteBinding;

impl ModuleBinding for RemoteBinding {
    fn register_functions(&self, _registry: &mut FunctionRegistry) {}

    fn register_event_handlers(&self, _registry: &mut EventHandlerRegistry) {}
}
```

Create `crates/platform-module-remote/src/source.rs`:

```rust
use crate::config::RemoteModuleConfig;

#[derive(Debug, Clone)]
pub struct RemoteModuleSource {
    pub config: RemoteModuleConfig,
}

impl RemoteModuleSource {
    #[must_use]
    pub fn new(config: RemoteModuleConfig) -> Self {
        Self { config }
    }
}
```

- [ ] **Step 8: Run targeted check**

Run:

```sh
cargo check --locked -p platform-module-remote --all-targets
```

Expected: pass.

- [ ] **Step 9: Commit**

```sh
git add Cargo.toml crates/platform-module-remote
git commit -m "feat(platform-module): add remote source crate"
```

---

## Task 2: Remote Binding And Source Loader

Make a remote source load a manifest and return a first-class `Module` with inert behavior plus optional remote admin data.

**Files:**
- Modify: `crates/platform-module-remote/src/binding.rs`
- Modify: `crates/platform-module-remote/src/source.rs`
- Modify: `crates/platform-module/src/module.rs`

- [ ] **Step 1: Confirm inert remote binding**

Keep `crates/platform-module-remote/src/binding.rs` as:

```rust
use platform_core::EventHandlerRegistry;
use platform_module::ModuleBinding;
use platform_runtime::FunctionRegistry;

#[derive(Debug, Default)]
pub struct RemoteBinding;

impl ModuleBinding for RemoteBinding {
    fn register_functions(&self, _registry: &mut FunctionRegistry) {}

    fn register_event_handlers(&self, _registry: &mut EventHandlerRegistry) {}
}
```

- [ ] **Step 2: Add `Module::remote` constructor**

Modify `crates/platform-module/src/module.rs`:

```rust
impl Module {
    #[must_use]
    pub fn remote(manifest: ModuleManifest, binding: Arc<dyn ModuleBinding>) -> Self {
        Self {
            manifest,
            binding,
            runtime_config: &[],
            admin_data: None,
        }
    }
}
```

Keep the existing `linked`, `with_runtime_config`, and `with_admin_data` methods unchanged.

- [ ] **Step 3: Implement remote source loader**

Create `crates/platform-module-remote/src/source.rs`:

```rust
use crate::admin_data::RemoteAdminDataSource;
use crate::binding::RemoteBinding;
use crate::config::RemoteModuleConfig;
use crate::protocol::RemoteManifestResponse;
use platform_core::{AppError, AppResult, ErrorCode};
use platform_module::{AdminSurface, Module};
use reqwest::StatusCode;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct RemoteModuleSource {
    client: reqwest::Client,
    config: RemoteModuleConfig,
}

impl RemoteModuleSource {
    pub fn new(config: RemoteModuleConfig) -> AppResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to build remote module client: {error}"),
                )
            })?;
        Ok(Self { client, config })
    }

    pub async fn load(&self) -> AppResult<Module> {
        let manifest = self.fetch_manifest().await?;
        if manifest.name != self.config.name {
            return Err(AppError::new(
                ErrorCode::Internal,
                format!(
                "remote module manifest name '{}' does not match configured name '{}'",
                manifest.name, self.config.name
            )));
        }

        let has_schema_admin = matches!(manifest.admin, Some(AdminSurface::Schema(_)));
        let mut module = Module::remote(manifest, Arc::new(RemoteBinding));
        if has_schema_admin {
            module = module.with_admin_data(Arc::new(RemoteAdminDataSource::new(
                self.config.clone(),
            )?));
        }
        Ok(module)
    }

    async fn fetch_manifest(&self) -> AppResult<RemoteManifestResponse> {
        let request = self.client.get(format!("{}/manifest", self.config.base_url));
        let request = match &self.config.auth_token {
            Some(token) => request.bearer_auth(token),
            None => request,
        };
        let response = request
            .send()
            .await
            .map_err(|error| {
                AppError::new(
                    ErrorCode::ExternalDependency,
                    format!("remote manifest request failed: {error}"),
                )
                .retryable()
            })?;

        if response.status() == StatusCode::NOT_FOUND {
            return Err(AppError::new(ErrorCode::NotFound, "remote module manifest not found"));
        }
        if !response.status().is_success() {
            return Err(AppError::new(
                ErrorCode::ExternalDependency,
                format!(
                "remote manifest returned status {}",
                response.status()
            ))
            .retryable());
        }

        response
            .json::<RemoteManifestResponse>()
            .await
            .map_err(|error| {
                AppError::new(
                    ErrorCode::ExternalDependency,
                    format!("remote manifest response was invalid JSON: {error}"),
                )
            })
    }
}
```

- [ ] **Step 4: Add remote source integration test**

Create `crates/platform-module-remote/tests/remote_source.rs`:

```rust
use platform_module::{AdminListQuery, AdminSurface};
use platform_module_remote::{RemoteAdminDataSource, RemoteModuleConfig, RemoteModuleSource};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn loads_manifest_and_attaches_admin_data_source() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/manifest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "remote-crm",
            "story_display": [],
            "admin": {
                "kind": "schema",
                "entities": [{
                    "name": "contacts",
                    "label": "Contacts",
                    "fields": [],
                    "read_capability": "remote_crm.contacts.read"
                }]
            },
            "capabilities": ["remote_crm.contacts.read"]
        })))
        .mount(&server)
        .await;

    let config = RemoteModuleConfig::new("remote-crm", server.uri());
    let module = RemoteModuleSource::new(config).unwrap().load().await.unwrap();

    assert_eq!(module.manifest.name, "remote-crm");
    assert!(matches!(module.manifest.admin, Some(AdminSurface::Schema(_))));
    assert!(module.admin_data.is_some());
}

#[tokio::test]
async fn remote_admin_data_source_lists_records() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/admin/contacts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "records": [{ "id": "contact_1", "email": "sam@example.com" }],
            "next_cursor": null
        })))
        .mount(&server)
        .await;

    let source = RemoteAdminDataSource::new(RemoteModuleConfig::new("remote-crm", server.uri()))
        .unwrap();
    let page = source
        .list("contacts", &AdminListQuery::new(50, None))
        .await
        .unwrap();

    assert_eq!(page.records.len(), 1);
    assert_eq!(page.records[0]["email"], "sam@example.com");
    assert!(page.next_cursor.is_none());
}
```

- [ ] **Step 5: Run targeted tests**

```sh
cargo test --locked -p platform-module -p platform-module-remote
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```sh
git add crates/platform-module crates/platform-module-remote
git commit -m "feat(platform-module): load remote module manifests"
```

---

## Task 3: Remote Module App Config

Add typed app config for remote module endpoints. Use app config/env only in this step; do not make remote module installation editable through runtime config yet.

**Files:**
- Modify: `crates/platform-core/src/config.rs`
- Modify: `crates/platform-core/src/lib.rs`

- [ ] **Step 1: Add config structs**

Modify `crates/platform-core/src/config.rs`:

```rust
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ModuleSourcesConfig {
    #[serde(default)]
    pub remote: Vec<RemoteModuleSourceConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RemoteModuleSourceConfig {
    pub name: String,
    pub base_url: String,
    pub auth_token_env: Option<String>,
    pub timeout_ms: u64,
}
```

Change `AppConfig`:

```rust
pub struct AppConfig {
    pub service: ServiceConfig,
    pub database: DatabaseConfig,
    pub http: HttpConfig,
    pub telemetry: TelemetryConfig,
    pub auth: AuthConfig,
    #[serde(default)]
    pub module_sources: ModuleSourcesConfig,
    #[serde(default)]
    pub modules: BTreeMap<String, ModuleConfig>,
}
```

- [ ] **Step 2: Parse remotes from env**

Add helpers in `crates/platform-core/src/config.rs`:

```rust
fn remote_module_sources_from_env() -> Vec<RemoteModuleSourceConfig> {
    let Some(raw) = std::env::var("REMOTE_MODULES").ok() else {
        return Vec::new();
    };

    raw.split(',')
        .filter_map(|entry| parse_remote_module_source(entry.trim()))
        .collect()
}

fn parse_remote_module_source(entry: &str) -> Option<RemoteModuleSourceConfig> {
    if entry.is_empty() {
        return None;
    }
    let (name, base_url) = entry.split_once('=')?;
    let name = name.trim();
    let base_url = base_url.trim();
    if name.is_empty() || base_url.is_empty() {
        return None;
    }
    let env_prefix = name.replace('-', "_").to_ascii_uppercase();
    let token_env = format!("REMOTE_MODULE_{}_TOKEN", env_prefix);
    let timeout_env = format!("REMOTE_MODULE_{}_TIMEOUT_MS", env_prefix);
    Some(RemoteModuleSourceConfig {
        name: name.to_owned(),
        base_url: base_url.trim_end_matches('/').to_owned(),
        auth_token_env: Some(token_env),
        timeout_ms: std::env::var(timeout_env)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(5_000),
    })
}
```

Update `AppConfig::from_env()`:

```rust
module_sources: ModuleSourcesConfig {
    remote: remote_module_sources_from_env(),
},
modules: BTreeMap::new(),
```

- [ ] **Step 3: Add parser tests**

Add to `crates/platform-core/src/config.rs` tests:

```rust
#[test]
fn parses_remote_module_source_entry() {
    let config = parse_remote_module_source("remote-crm=http://localhost:4100/lenso/module/v1")
        .expect("parse remote source");
    assert_eq!(config.name, "remote-crm");
    assert_eq!(config.base_url, "http://localhost:4100/lenso/module/v1");
    assert_eq!(
        config.auth_token_env.as_deref(),
        Some("REMOTE_MODULE_REMOTE_CRM_TOKEN")
    );
    assert_eq!(config.timeout_ms, 5_000);
}

#[test]
fn ignores_malformed_remote_module_source_entry() {
    assert!(parse_remote_module_source("").is_none());
    assert!(parse_remote_module_source("missing-url").is_none());
    assert!(parse_remote_module_source("=http://localhost:4100").is_none());
}
```

- [ ] **Step 4: Export config types**

Modify `crates/platform-core/src/lib.rs` config exports:

```rust
pub use config::{
    AppConfig, AuthConfig, DatabaseConfig, HttpConfig, LogFormat, ModuleConfig,
    ModuleSourcesConfig, RemoteModuleSourceConfig, ServiceConfig,
};
```

- [ ] **Step 5: Run tests**

```sh
cargo test --locked -p platform-core config
```

Expected: parser tests pass.

- [ ] **Step 6: Commit**

```sh
git add crates/platform-core/src/config.rs crates/platform-core/src/lib.rs
git commit -m "feat(config): add remote module sources"
```

---

## Task 4: App Bootstrap Remote Loading

Add async loading functions in `app-bootstrap` while preserving pure synchronous helpers for OpenAPI and Linked metadata.

**Files:**
- Modify: `crates/app-bootstrap/Cargo.toml`
- Modify: `crates/app-bootstrap/src/lib.rs`

- [ ] **Step 1: Add dependency**

Modify `crates/app-bootstrap/Cargo.toml`:

```toml
platform-module-remote.workspace = true
```

- [ ] **Step 2: Add remote config conversion**

Add to `crates/app-bootstrap/src/lib.rs` imports:

```rust
use platform_module_remote::{RemoteModuleConfig, RemoteModuleSource};
```

Add helper:

```rust
fn remote_module_config(source: &platform_core::RemoteModuleSourceConfig) -> RemoteModuleConfig {
    let mut config = RemoteModuleConfig::new(source.name.clone(), source.base_url.clone())
        .with_timeout_ms(source.timeout_ms);

    if let Some(env_name) = &source.auth_token_env {
        if let Ok(token) = std::env::var(env_name) {
            config = config.with_auth_token(token);
        }
    }

    config
}
```

- [ ] **Step 3: Add async module loader**

Add to `crates/app-bootstrap/src/lib.rs`:

```rust
pub async fn load_modules(ctx: &AppContext) -> platform_core::AppResult<Vec<Module>> {
    let mut loaded = modules(ctx);

    for remote in &ctx.config.module_sources.remote {
        let source = RemoteModuleSource::new(remote_module_config(remote))?;
        loaded.push(source.load().await?);
    }

    Ok(loaded)
}
```

- [ ] **Step 4: Add async admin module loader**

Add to `crates/app-bootstrap/src/lib.rs`:

```rust
pub async fn load_admin_modules(
    ctx: &AppContext,
) -> platform_core::AppResult<Vec<AdminModule>> {
    Ok(admin_modules_from_modules(load_modules(ctx).await?))
}

fn admin_modules_from_modules(modules: Vec<Module>) -> Vec<AdminModule> {
    modules
        .into_iter()
        .filter_map(|module| {
            let data_source = module.admin_data?;
            let ModuleManifest { name, admin, .. } = module.manifest;
            let AdminSurface::Schema(schema) = admin? else {
                return None;
            };
            Some(AdminModule {
                module_name: name,
                schema,
                data_source,
            })
        })
        .collect()
}
```

Then rewrite existing `admin_modules(ctx)` to preserve synchronous Linked behavior:

```rust
pub fn admin_modules(ctx: &AppContext) -> Vec<AdminModule> {
    admin_modules_from_modules(modules(ctx))
}
```

- [ ] **Step 5: Run app-bootstrap check**

```sh
cargo check --locked -p app-bootstrap --all-targets
```

Expected: pass.

- [ ] **Step 6: Commit**

```sh
git add crates/app-bootstrap/Cargo.toml crates/app-bootstrap/src/lib.rs
git commit -m "feat(app-bootstrap): load configured remote modules"
```

---

## Task 5: API And Worker Startup Wiring

Use async loaded modules where startup can tolerate IO. Keep OpenAPI document assembly unchanged.

**Files:**
- Modify: `apps/api/src/main.rs`
- Modify: `apps/worker/src/main.rs`

- [ ] **Step 1: Wire API admin modules**

In `apps/api/src/main.rs`, replace:

```rust
platform_admin_data::install_admin_modules(app_bootstrap::admin_modules(&ctx));
```

with:

```rust
let admin_modules = app_bootstrap::load_admin_modules(&ctx)
    .await
    .context("failed to load admin modules")?;
platform_admin_data::install_admin_modules(admin_modules);
```

Do not change `app_api::openapi_document()` or `apps/api/src/openapi.rs`.

- [ ] **Step 2: Wire worker module registry**

In `apps/worker/src/main.rs`, replace:

```rust
let modules = app_bootstrap::modules(&ctx);
```

with:

```rust
let modules = app_bootstrap::load_modules(&ctx)
    .await
    .context("failed to load modules")?;
```

Keep `function_registry(&modules)` and `event_handlers(&modules)` unchanged.

- [ ] **Step 3: Run startup package checks**

```sh
cargo check --locked -p app-api -p app-worker --all-targets
```

Expected: pass.

- [ ] **Step 4: Commit**

```sh
git add apps/api/src/main.rs apps/worker/src/main.rs
git commit -m "feat(module): load remotes during startup"
```

---

## Task 6: Guardrails, Docs, And Final Verification

Extend architecture guardrails and docs so future work preserves the boundary.

**Files:**
- Modify: `tools/arch-check/src/lib.rs`
- Modify: `docs/architecture/overview.md`
- Modify: `docs/architecture/rules.md`
- Modify: `docs/superpowers/specs/2026-06-03-module-framework-remote-source-design.md`

- [ ] **Step 1: Extend no-domain-dep check**

In `tools/arch-check/src/lib.rs`, replace `check_admin_data_no_domain_deps` with a generalized helper:

```rust
fn check_crates_no_domain_deps(root: &Path, crates: &[&str]) -> anyhow::Result<()> {
    let domain_names = domain_names(root)?;
    let mut violations = Vec::new();

    for crate_name in crates {
        let manifest = root.join(format!("crates/{crate_name}/Cargo.toml"));
        let source = fs::read_to_string(&manifest)
            .with_context(|| format!("failed to read {}", manifest.display()))?;
        for domain in &domain_names {
            if source.contains(&format!("{domain}.workspace"))
                || source.contains(&format!("\"{domain}\""))
                || source.contains(&format!("{domain} ="))
            {
                violations.push(format!("{crate_name} depends on domain `{domain}`"));
            }
        }
    }

    ensure_empty(
        violations,
        "platform admin/remote crates must not depend on any domain crate",
    )
}
```

Then call it from `run()`:

```rust
collect_result(
    check_crates_no_domain_deps(
        &root,
        &["platform-admin-data", "platform-module-remote"],
    ),
    "platform remote/admin domain dependency",
    &mut failures,
);
```

- [ ] **Step 2: Update architecture docs**

In `docs/architecture/overview.md`, add one sentence to the module framework paragraph:

```markdown
Configured remote modules are loaded at startup through `platform-module-remote`; the first Remote slice supports manifest loading and schema-admin reads only, not remote HTTP routes or runtime execution.
```

In `docs/architecture/rules.md`, add:

```markdown
- Remote modules may provide manifests and schema-admin read data through `platform-module-remote`; they must not contribute HTTP routes, runtime functions, or event handlers until those protocols have their own specs.
```

- [ ] **Step 3: Mark spec approved if implementation matches**

In `docs/superpowers/specs/2026-06-03-module-framework-remote-source-design.md`, change:

```markdown
**Status:** Draft design
```

to:

```markdown
**Status:** Implemented initial slice
```

Only do this after all implementation checks pass.

- [ ] **Step 4: Run final checks**

```sh
cargo check --locked -p platform-module -p platform-module-remote -p app-bootstrap -p app-api -p app-worker --all-targets
cargo test --locked -p platform-module-remote
just arch-check
```

Expected: all pass.

- [ ] **Step 5: Commit**

```sh
git add tools/arch-check/src/lib.rs docs/architecture/overview.md docs/architecture/rules.md docs/superpowers/specs/2026-06-03-module-framework-remote-source-design.md
git commit -m "docs: document remote module source guardrails"
```

---

## Self-Review

Spec coverage:

- Manifest loading is covered by Tasks 1, 2, and 4.
- Schema-admin remote reads are covered by Tasks 1 and 2.
- API and console generic behavior is preserved by Task 5; no frontend change is planned.
- OpenAPI purity is preserved by keeping `module_manifests()` and `merge_domain_http()` synchronous and Linked-only.
- Deferred remote HTTP/runtime/event/custom UI/marketplace concerns are not implemented.

Placeholder scan:

- The plan intentionally avoids marketplace trust, runtime execution, and custom UI.
- All code snippets use concrete file paths and concrete types.
- The one implementation-dependent note is `AdminListQuery::new`; the plan includes the exact fallback implementation.

Type consistency:

- Remote crate uses `RemoteModuleConfig`, `RemoteModuleSource`, `RemoteAdminDataSource`, and `RemoteBinding`.
- `app-bootstrap` consumes remote config from `ctx.config.module_sources.remote`.
- Host routes continue through `platform-admin-data`; no OpenAPI route contribution is introduced.
