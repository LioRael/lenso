# Demo Module Composition Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `identity` and `notifications` explicit demo linked modules while keeping platform-only `core` composition available.

**Architecture:** Add a shared config string for the linked composition profile, parse it in `app-bootstrap`, and route linked module enumeration through profile-aware helpers. Keep demo as the default contract/local profile so generated OpenAPI and SDK output stay stable, while runtime startup paths can select `core` and avoid demo routes, migrations, handlers, and console module metadata.

**Tech Stack:** Rust 2024, Axum, utoipa/utoipa-axum, platform-core config, app-bootstrap composition root, platform-module manifests, Vite Runtime Console checks.

---

## File Structure

- `crates/platform-core/src/config.rs` owns the raw string config field `ModuleSourcesConfig.linked_profile`, defaulting to `demo`.
- `crates/platform-core/src/lib.rs` re-exports `DEFAULT_LINKED_MODULE_PROFILE` so tests and docs can refer to the default without duplicating strings.
- `crates/app-bootstrap/src/lib.rs` owns `CompositionProfile`, profile parsing, profile-aware linked module lists, linked migration collection, and default demo wrappers for context-free contract generation.
- `apps/api/src/openapi.rs` gains `api_router_for_profile(profile)` while preserving `api_router()` and `openapi_document()` as demo-default context-free contract helpers.
- `apps/api/src/lib.rs` gains `try_build_router(ctx)` for startup/config-error paths and keeps `build_router(ctx)` as a test-friendly wrapper for valid config.
- `apps/api/src/main.rs` uses `try_build_router(ctx)` and handles profile-aware runtime config descriptor errors.
- `apps/worker/src/main.rs` handles profile-aware runtime config descriptor and module loading errors.
- `apps/migrate/src/main.rs` asks `app-bootstrap` for migrations for the selected profile.
- `apps/migrate/Cargo.toml` depends on `app-bootstrap` instead of directly depending on demo module crates.
- `apps/api/tests/openapi_contract.rs` covers demo-default OpenAPI and core-profile router behavior.
- `docs/architecture/overview.md` documents demo fixtures versus core platform composition.

## Task 1: Add Raw Linked Profile Config

**Files:**
- Modify: `crates/platform-core/src/config.rs`
- Modify: `crates/platform-core/src/lib.rs`

- [ ] **Step 1: Write the failing config tests**

Add these tests inside `#[cfg(test)] mod tests` in `crates/platform-core/src/config.rs`:

```rust
#[test]
fn module_sources_default_to_demo_linked_profile() {
    let config = ModuleSourcesConfig::default();

    assert_eq!(config.linked_profile, DEFAULT_LINKED_MODULE_PROFILE);
    assert!(config.remote.is_empty());
}

#[test]
fn linked_module_profile_from_env_value_trims_empty_to_default() {
    assert_eq!(
        linked_module_profile_from_env_value(None),
        DEFAULT_LINKED_MODULE_PROFILE
    );
    assert_eq!(
        linked_module_profile_from_env_value(Some("  ")),
        DEFAULT_LINKED_MODULE_PROFILE
    );
    assert_eq!(linked_module_profile_from_env_value(Some("core")), "core");
    assert_eq!(linked_module_profile_from_env_value(Some(" demo ")), "demo");
}
```

- [ ] **Step 2: Run platform-core config tests and confirm failure**

Run:

```sh
cargo test --locked -p platform-core profile
```

Expected: FAIL because `ModuleSourcesConfig` has no `linked_profile` field and the helper/default constant do not exist.

- [ ] **Step 3: Add the raw profile config field**

In `crates/platform-core/src/config.rs`, add a default constant near the imports:

```rust
pub const DEFAULT_LINKED_MODULE_PROFILE: &str = "demo";
```

Change `AppConfig::from_env()` so `module_sources` uses `ModuleSourcesConfig::from_env()`:

```rust
impl AppConfig {
    pub fn from_env() -> Self {
        Self {
            service: ServiceConfig::default(),
            database: DatabaseConfig::from_env(),
            http: HttpConfig::default(),
            telemetry: TelemetryConfig::default(),
            auth: AuthConfig::default(),
            module_sources: ModuleSourcesConfig::from_env(),
            modules: BTreeMap::new(),
        }
    }
}
```

Replace the derived default for `ModuleSourcesConfig` with an explicit struct and impl:

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModuleSourcesConfig {
    #[serde(default = "default_linked_module_profile")]
    pub linked_profile: String,
    #[serde(default)]
    pub remote: Vec<RemoteModuleSourceConfig>,
}

impl ModuleSourcesConfig {
    fn from_env() -> Self {
        Self {
            linked_profile: linked_module_profile_from_env_value(
                std::env::var("LENSO_COMPOSITION_PROFILE").ok().as_deref(),
            ),
            remote: remote_module_sources_from_env(),
        }
    }
}

impl Default for ModuleSourcesConfig {
    fn default() -> Self {
        Self {
            linked_profile: default_linked_module_profile(),
            remote: Vec::new(),
        }
    }
}

fn default_linked_module_profile() -> String {
    DEFAULT_LINKED_MODULE_PROFILE.to_owned()
}

fn linked_module_profile_from_env_value(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_LINKED_MODULE_PROFILE)
        .to_owned()
}
```

In `crates/platform-core/src/lib.rs`, add `DEFAULT_LINKED_MODULE_PROFILE` to the `pub use config::{ ... }` list:

```rust
pub use config::{
    AppConfig, AuthConfig, DEFAULT_LINKED_MODULE_PROFILE, DatabaseConfig, HttpConfig, LogFormat,
    ModuleConfig, ModuleSourcesConfig, RemoteModuleSourceConfig, ServiceConfig, TelemetryConfig,
    parse_cors_allowed_origins,
};
```

- [ ] **Step 4: Run platform-core config tests and confirm pass**

Run:

```sh
cargo test --locked -p platform-core profile
```

Expected: PASS.

- [ ] **Step 5: Commit Task 1**

Run:

```sh
git add crates/platform-core/src/config.rs crates/platform-core/src/lib.rs
git commit -m "feat(config): add linked module profile setting"
```

## Task 2: Add Profile-Aware Linked Module Enumeration

**Files:**
- Modify: `crates/app-bootstrap/src/lib.rs`

- [ ] **Step 1: Write failing app-bootstrap profile tests**

Add these tests near the existing app-bootstrap tests in `crates/app-bootstrap/src/lib.rs`:

```rust
#[test]
fn core_profile_excludes_demo_linked_modules() {
    let names = module_manifests_for_profile(CompositionProfile::Core)
        .into_iter()
        .map(|manifest| manifest.name)
        .collect::<Vec<_>>();

    assert_eq!(names, vec!["platform-story"]);
}

#[test]
fn demo_profile_includes_fixture_linked_modules() {
    let names = module_manifests_for_profile(CompositionProfile::Demo)
        .into_iter()
        .map(|manifest| manifest.name)
        .collect::<Vec<_>>();

    assert_eq!(names, vec!["identity", "notifications", "platform-story"]);
}

#[test]
fn default_module_manifests_use_demo_profile() {
    let names = module_manifests()
        .into_iter()
        .map(|manifest| manifest.name)
        .collect::<Vec<_>>();

    assert_eq!(names, vec!["identity", "notifications", "platform-story"]);
}

#[test]
fn linked_http_route_owners_are_profile_aware() {
    assert_eq!(
        linked_http_route_owners_for_profile(CompositionProfile::Core),
        Vec::<LinkedHttpRouteOwner>::new()
    );
    assert_eq!(
        linked_http_route_owners_for_profile(CompositionProfile::Demo),
        vec![LinkedHttpRouteOwner {
            module_name: "identity".to_owned(),
            public_prefixes: &["/v1/identity/"],
        }]
    );
}

#[test]
fn composition_profile_rejects_unknown_values() {
    let error = CompositionProfile::parse("fixture")
        .expect_err("fixture is not a supported linked module profile");

    assert_eq!(error.code, ErrorCode::Validation);
    assert!(
        error
            .details
            .iter()
            .any(|detail| detail.field.as_deref() == Some("module_sources.linked_profile"))
    );
}
```

- [ ] **Step 2: Run app-bootstrap tests and confirm failure**

Run:

```sh
cargo test --locked -p app-bootstrap profile
```

Expected: FAIL because `CompositionProfile` and profile-aware helper functions do not exist.

- [ ] **Step 3: Add `CompositionProfile` and profile-specific entry lists**

In `crates/app-bootstrap/src/lib.rs`, add this enum after `LinkedModuleEntry`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositionProfile {
    Core,
    Demo,
}

impl CompositionProfile {
    pub fn parse(value: &str) -> platform_core::AppResult<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "core" => Ok(Self::Core),
            "demo" => Ok(Self::Demo),
            other => Err(AppError::validation(
                "Invalid Lenso composition profile",
                vec![ErrorDetail {
                    field: Some("module_sources.linked_profile".to_owned()),
                    reason: format!("expected `core` or `demo`, got `{other}`"),
                }],
            )),
        }
    }

    pub fn from_config(config: &platform_core::AppConfig) -> platform_core::AppResult<Self> {
        Self::parse(&config.module_sources.linked_profile)
    }
}

impl Default for CompositionProfile {
    fn default() -> Self {
        Self::Demo
    }
}
```

Replace `LINKED_MODULE_ENTRIES` with two slices:

```rust
const CORE_LINKED_MODULE_ENTRIES: &[LinkedModuleEntry] = &[LinkedModuleEntry {
    module_name: "platform-story",
    manifest: platform_story_manifest,
    load: platform_story_module,
    http_binding: None,
}];

const DEMO_LINKED_MODULE_ENTRIES: &[LinkedModuleEntry] = &[
    LinkedModuleEntry {
        module_name: "identity",
        manifest: identity::module::manifest,
        load: identity::module::module,
        http_binding: Some(identity::module::binding),
    },
    LinkedModuleEntry {
        module_name: "notifications",
        manifest: notifications::module::manifest,
        load: notifications::module::module,
        http_binding: None,
    },
    LinkedModuleEntry {
        module_name: "platform-story",
        manifest: platform_story_manifest,
        load: platform_story_module,
        http_binding: None,
    },
];

fn linked_module_entries(profile: CompositionProfile) -> &'static [LinkedModuleEntry] {
    match profile {
        CompositionProfile::Core => CORE_LINKED_MODULE_ENTRIES,
        CompositionProfile::Demo => DEMO_LINKED_MODULE_ENTRIES,
    }
}
```

- [ ] **Step 4: Add profile-aware wrappers for manifests, modules, HTTP, runtime declarations, and story display**

Update the existing linked enumeration helpers in `crates/app-bootstrap/src/lib.rs` to use these exact shapes:

```rust
#[must_use]
pub fn modules(ctx: &AppContext) -> Vec<Module> {
    modules_for_profile(ctx, CompositionProfile::default())
}

pub fn modules_for_config(ctx: &AppContext) -> platform_core::AppResult<Vec<Module>> {
    Ok(modules_for_profile(ctx, CompositionProfile::from_config(&ctx.config)?))
}

#[must_use]
pub fn modules_for_profile(ctx: &AppContext, profile: CompositionProfile) -> Vec<Module> {
    linked_module_entries(profile)
        .iter()
        .map(|entry| (entry.load)(ctx))
        .collect()
}

#[must_use]
pub fn module_manifests() -> Vec<ModuleManifest> {
    module_manifests_for_profile(CompositionProfile::default())
}

#[must_use]
pub fn module_manifests_for_profile(profile: CompositionProfile) -> Vec<ModuleManifest> {
    linked_module_entries(profile)
        .iter()
        .map(|entry| (entry.manifest)())
        .collect()
}

#[must_use]
pub fn linked_runtime_function_declaration_sources() -> Vec<(
    String,
    ModuleSource,
    Option<platform_module::RuntimeSurface>,
)> {
    linked_runtime_function_declaration_sources_for_profile(CompositionProfile::default())
}

#[must_use]
pub fn linked_runtime_function_declaration_sources_for_profile(
    profile: CompositionProfile,
) -> Vec<(String, ModuleSource, Option<platform_module::RuntimeSurface>)> {
    module_manifests_for_profile(profile)
        .into_iter()
        .map(|manifest| (manifest.name, ModuleSource::Linked, manifest.runtime))
        .collect()
}

#[must_use]
pub fn linked_http_route_owners() -> Vec<LinkedHttpRouteOwner> {
    linked_http_route_owners_for_profile(CompositionProfile::default())
}

#[must_use]
pub fn linked_http_route_owners_for_profile(
    profile: CompositionProfile,
) -> Vec<LinkedHttpRouteOwner> {
    linked_module_entries(profile)
        .iter()
        .filter_map(|entry| {
            let http = entry.http_binding?().http?;
            Some(LinkedHttpRouteOwner {
                module_name: entry.module_name.to_owned(),
                public_prefixes: http.public_prefixes,
            })
        })
        .collect()
}

#[must_use]
pub fn linked_http_modules() -> Vec<Module> {
    linked_http_modules_for_profile(CompositionProfile::default())
}

#[must_use]
pub fn linked_http_modules_for_profile(profile: CompositionProfile) -> Vec<Module> {
    linked_module_entries(profile)
        .iter()
        .filter_map(|entry| {
            let http_binding = entry.http_binding?;
            Some(Module::linked((entry.manifest)(), http_binding()))
        })
        .collect()
}

pub fn merge_linked_http(base: ApiOpenApiRouter) -> ApiOpenApiRouter {
    merge_linked_http_for_profile(base, CompositionProfile::default())
}

pub fn merge_linked_http_for_profile(
    base: ApiOpenApiRouter,
    profile: CompositionProfile,
) -> ApiOpenApiRouter {
    linked_http_modules_for_profile(profile)
        .into_iter()
        .filter_map(|module| module.linked_http)
        .fold(base, |router, contribution| (contribution.merge)(router))
}

#[must_use]
pub fn story_display_descriptors() -> Vec<StoryDisplayDescriptor> {
    story_display_descriptors_for_profile(CompositionProfile::default())
}

#[must_use]
pub fn story_display_descriptors_for_profile(
    profile: CompositionProfile,
) -> Vec<StoryDisplayDescriptor> {
    module_manifests_for_profile(profile)
        .into_iter()
        .flat_map(|manifest| manifest.story_display)
        .collect()
}
```

- [ ] **Step 5: Update `load_modules`, admin metadata loading, and runtime config descriptors**

Change the existing functions in `crates/app-bootstrap/src/lib.rs` to parse from `ctx.config` for runtime startup paths:

```rust
pub async fn load_modules(ctx: &AppContext) -> platform_core::AppResult<Vec<Module>> {
    let mut loaded = modules_for_config(ctx)?;

    for remote in &ctx.config.module_sources.remote {
        let source = RemoteModuleSource::new(remote_module_config(remote))?;
        loaded.push(source.load().await?);
    }

    Ok(loaded)
}

pub async fn load_admin_modules(ctx: &AppContext) -> platform_core::AppResult<Vec<AdminModule>> {
    let mut admin_modules = admin_modules_from_modules(modules_for_config(ctx)?);

    for remote in &ctx.config.module_sources.remote {
        let source = RemoteModuleSource::new(remote_module_config(remote))?;
        match source.load().await {
            Ok(module) => admin_modules.extend(admin_modules_from_modules(vec![module])),
            Err(error) => admin_modules.push(failed_remote_admin_module(
                remote.name.clone(),
                error.public_message,
            )),
        }
    }

    Ok(admin_modules)
}

pub async fn load_admin_module_metadata(
    ctx: &AppContext,
) -> platform_core::AppResult<Vec<AdminModuleMetadata>> {
    let mut metadata = admin_metadata_from_modules(modules_for_config(ctx)?);

    for remote in &ctx.config.module_sources.remote {
        let config = remote_module_config(remote);
        let checked_at = current_timestamp();
        let source = RemoteModuleSource::new(config.clone())?;
        let load_started = Instant::now();
        match source.load().await {
            Ok(module) => metadata.extend(remote_admin_metadata_from_module(
                module,
                &config,
                checked_at,
                Some(duration_ms(load_started)),
                None,
            )),
            Err(error) => metadata.push(failed_remote_admin_metadata(
                &config,
                Some(checked_at),
                Some(duration_ms(load_started)),
                error.public_message,
            )),
        }
    }

    Ok(metadata)
}

pub fn runtime_config_descriptors(
    ctx: &AppContext,
) -> platform_core::AppResult<Vec<RuntimeConfigDescriptor>> {
    let module_descriptors = modules_for_config(ctx)?
        .iter()
        .flat_map(|module| module.runtime_config.iter().cloned())
        .collect::<Vec<_>>();

    Ok(platform_core::worker_runtime_config::RUNTIME_CONFIG
        .iter()
        .cloned()
        .chain(module_descriptors)
        .collect())
}
```

Keep the existing `admin_modules(ctx) -> Vec<AdminModule>` as a demo-default test helper:

```rust
#[must_use]
pub fn admin_modules(ctx: &AppContext) -> Vec<AdminModule> {
    admin_modules_from_modules(modules(ctx))
}
```

- [ ] **Step 6: Run app-bootstrap tests and confirm pass**

Run:

```sh
cargo test --locked -p app-bootstrap profile
```

Expected: PASS.

- [ ] **Step 7: Commit Task 2**

Run:

```sh
git add crates/app-bootstrap/src/lib.rs
git commit -m "feat(app-bootstrap): add linked composition profiles"
```

## Task 3: Wire Profile Through API, Worker, OpenAPI, And Migrations

**Files:**
- Modify: `crates/app-bootstrap/src/lib.rs`
- Modify: `apps/api/src/openapi.rs`
- Modify: `apps/api/src/lib.rs`
- Modify: `apps/api/src/main.rs`
- Modify: `apps/worker/src/main.rs`
- Modify: `apps/migrate/src/main.rs`
- Modify: `apps/migrate/Cargo.toml`
- Test: `apps/api/tests/openapi_contract.rs`

- [ ] **Step 1: Write failing migration profile tests**

Add these tests to `crates/app-bootstrap/src/lib.rs`:

```rust
#[test]
fn core_profile_migrations_exclude_demo_module_migrations() {
    let names = migrations_for_profile(CompositionProfile::Core)
        .into_iter()
        .map(|migration| migration.name)
        .collect::<Vec<_>>();

    assert!(names.iter().any(|name| name.starts_with("platform/")));
    assert!(names.iter().any(|name| name.starts_with("runtime/")));
    assert!(!names.iter().any(|name| name.starts_with("identity/")));
    assert!(!names.iter().any(|name| name.starts_with("notifications/")));
}

#[test]
fn demo_profile_migrations_include_fixture_module_migrations() {
    let names = migrations_for_profile(CompositionProfile::Demo)
        .into_iter()
        .map(|migration| migration.name)
        .collect::<Vec<_>>();

    assert!(names.iter().any(|name| name == &"identity/0001_create_identity_schema"));
    assert!(names.iter().any(|name| name == &"notifications/0001_create_notifications_schema"));
}
```

- [ ] **Step 2: Write failing core router test**

Add this test to `apps/api/tests/openapi_contract.rs`:

```rust
#[tokio::test]
async fn core_profile_router_does_not_mount_identity_routes() {
    let mut config = AppConfig::from_env();
    config.module_sources.linked_profile = "core".to_owned();
    let ctx = AppContext::new(
        config,
        platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build"),
        Arc::new(LoggingEventPublisher),
    );
    let app = app_api::try_build_router(ctx).expect("core profile router should build");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/identity/users")
                .method("POST")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
```

- [ ] **Step 3: Run focused tests and confirm failure**

Run:

```sh
cargo test --locked -p app-bootstrap profile_migrations
cargo test --locked -p app-api core_profile_router_does_not_mount_identity_routes
```

Expected: FAIL because migration helpers and `try_build_router` do not exist.

- [ ] **Step 4: Add migration helpers to app-bootstrap**

In `crates/app-bootstrap/src/lib.rs`, add imports if not already present:

```rust
use platform_core::{Migration, PLATFORM_MIGRATIONS};
use platform_runtime::RUNTIME_MIGRATIONS;
```

Add these functions near the linked enumeration helpers:

```rust
pub fn migrations_for_config(
    config: &platform_core::AppConfig,
) -> platform_core::AppResult<Vec<Migration>> {
    Ok(migrations_for_profile(CompositionProfile::from_config(config)?))
}

#[must_use]
pub fn migrations_for_profile(profile: CompositionProfile) -> Vec<Migration> {
    let mut migrations = PLATFORM_MIGRATIONS
        .iter()
        .chain(RUNTIME_MIGRATIONS)
        .copied()
        .collect::<Vec<_>>();

    if profile == CompositionProfile::Demo {
        migrations.extend(identity::migrations::IDENTITY_MIGRATIONS.iter().copied());
        migrations.extend(
            notifications::migrations::NOTIFICATIONS_MIGRATIONS
                .iter()
                .copied(),
        );
    }

    migrations
}
```

- [ ] **Step 5: Update API OpenAPI assembly to accept a profile**

In `apps/api/src/openapi.rs`, import `CompositionProfile`:

```rust
use app_bootstrap::CompositionProfile;
```

Replace `api_router()` with profile-aware routing while preserving the demo default:

```rust
pub(crate) fn api_router() -> ApiOpenApiRouter {
    api_router_for_profile(CompositionProfile::default())
}

pub(crate) fn api_router_for_profile(profile: CompositionProfile) -> ApiOpenApiRouter {
    platform_admin::install_story_display(app_bootstrap::story_display_descriptors_for_profile(
        profile,
    ));
    platform_admin::install_default_runtime_function_declarations(
        platform_admin::runtime_function_declarations_from_modules(
            app_bootstrap::linked_runtime_function_declaration_sources_for_profile(profile),
        ),
    );

    let base = OpenApiRouter::with_openapi(ApiDoc::openapi()).merge(base_router());
    app_bootstrap::merge_linked_http_for_profile(base, profile)
        .merge(platform_admin::router())
        .merge(platform_admin_data::router())
        .merge(platform_module_remote::router())
}
```

Keep `openapi_document()` unchanged:

```rust
#[must_use]
pub fn openapi_document() -> utoipa::openapi::OpenApi {
    api_router().to_openapi()
}
```

- [ ] **Step 6: Add `try_build_router` and keep `build_router`**

In `apps/api/src/lib.rs`, change the router build entrypoint:

```rust
pub fn build_router(ctx: AppContext) -> Router {
    try_build_router(ctx).expect("Runtime API router should build with a valid composition profile")
}

pub fn try_build_router(ctx: AppContext) -> platform_core::AppResult<Router> {
    let profile = app_bootstrap::CompositionProfile::from_config(&ctx.config)?;
    let (router, document) = openapi::api_router_for_profile(profile).split_for_parts();
    let document = Arc::new(document);

    Ok(router
        .route("/docs", axum::routing::get(scalar_docs))
        .route("/openapi.json", axum::routing::get(serve_openapi))
        .layer(axum::Extension(document))
        .layer(middleware::from_fn_with_state(
            ctx.clone(),
            request_context_middleware,
        ))
        .layer(cors_layer(&ctx))
        .with_state(ctx))
}
```

- [ ] **Step 7: Update API and worker startup to handle profile-aware descriptors**

In `apps/api/src/main.rs`, change the runtime config descriptor call:

```rust
let descriptors = app_bootstrap::runtime_config_descriptors(&ctx)
    .context("failed to collect runtime-config descriptors")?;
```

Change the router build call:

```rust
let app = app_api::try_build_router(ctx.clone()).context("failed to build API router")?;
```

In `apps/worker/src/main.rs`, change the runtime config descriptor call:

```rust
let descriptors = app_bootstrap::runtime_config_descriptors(&ctx)
    .context("failed to collect runtime-config descriptors")?;
```

Leave `load_modules(&ctx).await.context("failed to load modules")?` in place; Task 2 already made it parse the selected profile.

- [ ] **Step 8: Update migrate app to use profile-aware migrations**

In `apps/migrate/Cargo.toml`, replace direct demo module dependencies with app-bootstrap:

```toml
[dependencies]
anyhow.workspace = true
app-bootstrap.workspace = true
platform-core.workspace = true
tokio.workspace = true
tracing.workspace = true
```

In `apps/migrate/src/main.rs`, replace `collect_migrations()` with config-aware collection:

```rust
fn collect_migrations(config: &AppConfig) -> platform_core::AppResult<Vec<platform_core::Migration>> {
    app_bootstrap::migrations_for_config(config)
}
```

Update `main()` to pass config:

```rust
let migrations = collect_migrations(&config)?;
```

Remove unused imports of `PLATFORM_MIGRATIONS` and `RUNTIME_MIGRATIONS` from `apps/migrate/src/main.rs`.

- [ ] **Step 9: Run focused tests and confirm pass**

Run:

```sh
cargo test --locked -p app-bootstrap profile_migrations
cargo test --locked -p app-api core_profile_router_does_not_mount_identity_routes
cargo test --locked -p app-api committed_openapi_artifact_matches_rust_source
```

Expected: PASS. The committed OpenAPI artifact remains demo-profile because `openapi_document()` still uses `CompositionProfile::default()`.

- [ ] **Step 10: Commit Task 3**

Run:

```sh
git add crates/app-bootstrap/src/lib.rs apps/api/src/openapi.rs apps/api/src/lib.rs apps/api/src/main.rs apps/worker/src/main.rs apps/migrate/src/main.rs apps/migrate/Cargo.toml apps/api/tests/openapi_contract.rs
git commit -m "feat(app-bootstrap): wire demo composition profile"
```

## Task 4: Document Demo Fixtures And Verify Whole Slice

**Files:**
- Modify: `docs/architecture/overview.md`
- Modify: `docs/superpowers/specs/2026-06-07-demo-module-composition-design.md`

- [ ] **Step 1: Update architecture overview wording**

In `docs/architecture/overview.md`, replace the `Current module fixtures:` paragraph with:

```markdown
Current demo module fixtures:

- `identity` exercises users, identity HTTP routes, `identity.user_registered.v1`,
  `identity.cleanup_expired_sessions.v1`, schema-admin reads, and a module-owned
  Runtime Console workspace.
- `notifications` exercises identity registration event handling and
  `notifications.send_welcome_email.v1`.

These modules are demo fixtures, not product defaults. `app-bootstrap` selects a
linked composition profile: `core` keeps only platform-owned linked surfaces such
as `platform-story`, while `demo` adds `identity` and `notifications` for local
development, examples, contracts, and integration tests.
```

In the Runtime Console section, replace:

```markdown
The first implementation uses a read-only identity User fixture to exercise the framework; Lenso does not prescribe product-default business modules.
```

with:

```markdown
The demo profile uses a read-only identity User fixture to exercise the
framework; Lenso does not prescribe product-default business modules.
```

- [ ] **Step 2: Mark the design spec as implemented**

In `docs/superpowers/specs/2026-06-07-demo-module-composition-design.md`, change the status line to:

```markdown
**Status:** Implemented in linked composition profile slice
```

- [ ] **Step 3: Run formatting**

Run:

```sh
just fmt
```

Expected: Rust formatting and Runtime Console formatting complete without source-breaking changes.

- [ ] **Step 4: Run focused and aggregate gates**

Run:

```sh
cargo test --locked -p platform-core
cargo test --locked -p app-bootstrap
cargo test --locked -p app-api --test openapi_contract
just generated-check
just arch-check
just console-check
just sdk-check
```

Expected: all commands pass. `just generated-check` should not produce committed contract or SDK diffs because the default OpenAPI document remains demo-profile.

- [ ] **Step 5: Inspect final diff**

Run:

```sh
git status --short
git diff --stat
git diff --check
```

Expected: only files touched by this plan are modified, and `git diff --check` reports no whitespace errors.

- [ ] **Step 6: Commit Task 4**

Run:

```sh
git add docs/architecture/overview.md docs/superpowers/specs/2026-06-07-demo-module-composition-design.md
git commit -m "docs: clarify demo module composition"
```

## Task 5: Final Review And Optional Broad Gate

**Files:**
- Review only unless earlier gates reveal an issue.

- [ ] **Step 1: Review branch commits**

Run:

```sh
git log --oneline -5
```

Expected: the latest commits include config, app-bootstrap wiring, docs, and the earlier design/plan commits.

- [ ] **Step 2: Run broad gate if Task 4 changed generated artifacts or many tests were touched**

Run this if `just generated-check` changed files or if Task 4 touched tests beyond the files named above:

```sh
just check
```

Expected: PASS.

- [ ] **Step 3: Confirm clean worktree**

Run:

```sh
git status --short --branch
```

Expected:

```text
## codex/module-console-workspaces
```

- [ ] **Step 4: Prepare final summary**

Include these facts in the final response:

```text
- identity and notifications remain in the repo as demo fixtures.
- core profile excludes demo linked modules, demo migrations, and identity HTTP routes.
- demo remains the default for local contracts and SDK generation.
- validation commands that passed.
- latest commit hashes.
```
