//! Composition root: the single place that knows which modules exist.
//!
//! Both the API and the worker assemble their module wiring from this crate, so
//! a module is registered here once rather than in scattered per-app edits.
//!
//! A module's contributions are split by how they are consumed:
//! - [`modules`]: context-bound bindings (runtime functions + event handlers)
//!   and runtime config (API + worker), demo-default for context-local callers.
//! - [`modules_for_config`] / [`load_modules`]: config-aware module loaders that
//!   honor profile entry lists and configured remote sources for runtime apps.
//! - [`module_manifests`]: context-free manifest data (no [`AppContext`]) for
//!   read-only / `OpenAPI` paths, with profile-aware variants for runtime use.
//! - [`merge_linked_http`]: context-free HTTP routes and their OpenAPI docs
//!   (API only), assembled without a live [`AppContext`].
//! - [`story_display_descriptors`]: console display metadata, sourced from the
//!   context-free [`module_manifests`].
//!
//! When adding a module, register it in the appropriate profile entry lists and
//! expose its config-aware loader contributions from this crate.

use platform_admin_data::{
    AdminModule, AdminModuleMetadata, AdminModuleSourceDiagnostics, AdminRemoteModuleDiagnostics,
};
use platform_core::error::ErrorDetail;
use platform_core::{
    ActorContext, AppContext, AppError, CorrelationId, ErrorCode, EventHandlerRegistry, Migration,
    PLATFORM_MIGRATIONS, RuntimeConfigDescriptor, RuntimeConfigGroupDescriptor, RuntimeConfigScope,
    RuntimeConfigType, StoryDisplayDescriptor, StoryDisplaySource, TraceContext,
};
use platform_http::ApiOpenApiRouter;
use platform_module::{
    AdminSchema, AdminSurface, EventHandlerRegistrationContext, LifecycleActivationRunPolicy,
    LifecycleStartupCheckKind, LinkedBinding, Module, ModuleHttpMethod, ModuleLoadStatus,
    ModuleManifest, ModuleSource,
};
use platform_module_remote::{RemoteHttpProxyRegistry, RemoteModuleConfig, RemoteModuleSource};
use platform_runtime::{
    EnqueueFunctionRequest, FunctionRegistry, RUNTIME_MIGRATIONS, RuntimeClient,
};
use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_MODULE_SERVICES_FILE: &str = ".lenso/module-services.json";
const DEFAULT_REMOTE_SERVICE_READY_TIMEOUT_MS: u64 = 10_000;
const REMOTE_SERVICE_TERMINATE_GRACE_MS: u64 = 800;
const AUTH_SESSION_CACHE_MAX_TTL: Duration = Duration::from_secs(12 * 60 * 60);

struct LinkedModuleEntry {
    module_name: &'static str,
    manifest: fn() -> ModuleManifest,
    load: fn(&AppContext) -> Module,
    http_binding: Option<fn() -> LinkedBinding>,
}

const MODULES_CONFIG_GROUP: RuntimeConfigGroupDescriptor = RuntimeConfigGroupDescriptor {
    id: "modules",
    label: "Modules",
    description: "Module load toggles applied on service startup.",
    order: 10,
};

#[derive(Debug, Clone, Copy)]
pub struct HostLinkedModule {
    pub module_name: &'static str,
    pub manifest: fn() -> ModuleManifest,
    pub load: Option<fn(&AppContext) -> Module>,
    pub http_binding: Option<fn() -> LinkedBinding>,
    pub migrations: &'static [Migration],
}

impl HostLinkedModule {
    #[must_use]
    pub const fn manifest_only(
        module_name: &'static str,
        manifest: fn() -> ModuleManifest,
        migrations: &'static [Migration],
    ) -> Self {
        Self {
            module_name,
            manifest,
            load: None,
            http_binding: None,
            migrations,
        }
    }

    #[must_use]
    pub const fn linked(
        module_name: &'static str,
        manifest: fn() -> ModuleManifest,
        load: fn(&AppContext) -> Module,
        migrations: &'static [Migration],
    ) -> Self {
        Self {
            module_name,
            manifest,
            load: Some(load),
            http_binding: None,
            migrations,
        }
    }

    #[must_use]
    pub const fn with_http_binding(mut self, http_binding: fn() -> LinkedBinding) -> Self {
        self.http_binding = Some(http_binding);
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct HostComposition {
    linked_modules: Vec<HostLinkedModule>,
}

impl HostComposition {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_linked_module(mut self, module: HostLinkedModule) -> Self {
        self.add_linked_module(module);
        self
    }

    pub fn add_linked_module(&mut self, module: HostLinkedModule) {
        self.linked_modules.push(module);
    }

    #[must_use]
    pub fn linked_modules(&self) -> &[HostLinkedModule] {
        &self.linked_modules
    }
}

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

const CORE_LINKED_MODULE_ENTRIES: &[LinkedModuleEntry] = &[LinkedModuleEntry {
    module_name: "platform-story",
    manifest: story::module::manifest,
    load: story::module::module,
    http_binding: Some(story::module::binding),
}];

const DEMO_LINKED_MODULE_ENTRIES: &[LinkedModuleEntry] = &[
    LinkedModuleEntry {
        module_name: "auth",
        manifest: auth::module::manifest,
        load: auth::module::module,
        http_binding: Some(auth::module::binding),
    },
    LinkedModuleEntry {
        module_name: "auth-password",
        manifest: auth_password::module::manifest,
        load: auth_password::module::module,
        http_binding: Some(auth_password::module::binding),
    },
    LinkedModuleEntry {
        module_name: "platform-story",
        manifest: story::module::manifest,
        load: story::module::module,
        http_binding: Some(story::module::binding),
    },
];

fn linked_module_entries(profile: CompositionProfile) -> &'static [LinkedModuleEntry] {
    match profile {
        CompositionProfile::Core => CORE_LINKED_MODULE_ENTRIES,
        CompositionProfile::Demo => DEMO_LINKED_MODULE_ENTRIES,
    }
}

#[must_use]
pub const fn auth_linked_module() -> HostLinkedModule {
    HostLinkedModule::linked(
        auth::module::MODULE_NAME,
        auth::module::manifest,
        auth::module::module,
        auth::migrations::AUTH_MIGRATIONS,
    )
    .with_http_binding(auth::module::binding)
}

#[must_use]
pub const fn auth_password_linked_module() -> HostLinkedModule {
    HostLinkedModule::linked(
        auth_password::module::MODULE_NAME,
        auth_password::module::manifest,
        auth_password::module::module,
        auth_password::migrations::AUTH_PASSWORD_MIGRATIONS,
    )
    .with_http_binding(auth_password::module::binding)
}

fn linked_module_enabled_from_config(config: &platform_core::AppConfig, module_name: &str) -> bool {
    config
        .modules
        .get(module_name)
        .is_none_or(platform_core::ModuleConfig::is_enabled)
}

fn module_enabled_config_key(module_name: &str) -> String {
    format!("modules.{module_name}.enabled")
}

fn linked_module_enabled(ctx: &AppContext, module_name: &str) -> bool {
    ctx.runtime_config
        .snapshot()
        .raw(&module_enabled_config_key(module_name))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or_else(|| linked_module_enabled_from_config(&ctx.config, module_name))
}

fn first_disabled_dependency(ctx: &AppContext, manifest: fn() -> ModuleManifest) -> Option<String> {
    (manifest)()
        .dependencies
        .into_iter()
        .find(|dependency| !linked_module_enabled(ctx, dependency))
}

fn first_disabled_dependency_from_config(
    config: &platform_core::AppConfig,
    manifest: fn() -> ModuleManifest,
) -> Option<String> {
    (manifest)()
        .dependencies
        .into_iter()
        .find(|dependency| !linked_module_enabled_from_config(config, dependency))
}

fn linked_module_with_dependencies_enabled(
    ctx: &AppContext,
    module_name: &str,
    manifest: fn() -> ModuleManifest,
) -> bool {
    linked_module_enabled(ctx, module_name) && first_disabled_dependency(ctx, manifest).is_none()
}

fn linked_module_with_dependencies_enabled_from_config(
    config: &platform_core::AppConfig,
    module_name: &str,
    manifest: fn() -> ModuleManifest,
) -> bool {
    linked_module_enabled_from_config(config, module_name)
        && first_disabled_dependency_from_config(config, manifest).is_none()
}

fn linked_module_disabled_reason(
    ctx: &AppContext,
    module_name: &str,
    manifest: fn() -> ModuleManifest,
) -> Option<String> {
    if !linked_module_enabled(ctx, module_name) {
        return Some("module disabled by configuration".to_owned());
    }
    if let Some(dependency) = first_disabled_dependency(ctx, manifest) {
        return Some(format!("module dependency disabled: {dependency}"));
    }
    None
}

fn remote_module_enabled_from_config(config: &platform_core::AppConfig, module_name: &str) -> bool {
    config
        .modules
        .get(module_name)
        .is_none_or(platform_core::ModuleConfig::is_enabled)
}

fn remote_module_enabled(ctx: &AppContext, module_name: &str) -> bool {
    ctx.runtime_config
        .snapshot()
        .raw(&module_enabled_config_key(module_name))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or_else(|| remote_module_enabled_from_config(&ctx.config, module_name))
}

pub fn auth_actor_resolver_for_context(
    ctx: &AppContext,
) -> platform_core::AppResult<Option<Arc<dyn platform_core::ActorResolver>>> {
    auth_actor_resolver_for_context_with_composition(ctx, &HostComposition::default())
}

pub fn auth_actor_resolver_for_context_with_composition(
    ctx: &AppContext,
    composition: &HostComposition,
) -> platform_core::AppResult<Option<Arc<dyn platform_core::ActorResolver>>> {
    let profile = CompositionProfile::from_config(&ctx.config)?;
    let auth_in_profile = linked_module_entries(profile)
        .iter()
        .any(|entry| entry.module_name == auth::module::MODULE_NAME);
    let auth_in_composition = composition
        .linked_modules()
        .iter()
        .any(|entry| entry.module_name == auth::module::MODULE_NAME);
    if (!auth_in_profile && !auth_in_composition)
        || !linked_module_enabled(ctx, auth::module::MODULE_NAME)
    {
        return Ok(None);
    }

    let auth_resolver: Arc<dyn platform_core::ActorResolver> =
        Arc::new(auth::resolver::AuthActorResolver::new_with_session_cache(
            ctx.db.clone(),
            ctx.actor_resolver.clone(),
            auth_session_cache(ctx)?,
        ));

    let auth_password_enabled = linked_module_with_dependencies_enabled(
        ctx,
        auth_password::module::MODULE_NAME,
        auth_password::module::manifest,
    );
    if auth_password_enabled {
        if let Some(jwt_resolver) =
            auth_password::module::jwt_actor_resolver(ctx, auth_resolver.clone())?
        {
            return Ok(Some(jwt_resolver));
        }
    }

    Ok(Some(auth_resolver))
}

fn auth_session_cache(
    ctx: &AppContext,
) -> platform_core::AppResult<Option<Arc<dyn auth::resolver::SessionCache>>> {
    match auth::config::AuthRuntimeConfig::from_context(ctx).session_cache {
        auth::config::SessionCacheMode::Database => Ok(None),
        auth::config::SessionCacheMode::Redis => {
            let Some(redis) = ctx.redis.clone() else {
                return Err(AppError::validation(
                    "Redis auth session cache is not configured",
                    vec![ErrorDetail {
                        field: Some("auth.session_cache".to_owned()),
                        reason: "set REDIS_URL when auth.session_cache is redis".to_owned(),
                    }],
                ));
            };
            Ok(Some(Arc::new(auth::redis_cache::RedisSessionCache::new(
                redis,
                AUTH_SESSION_CACHE_MAX_TTL,
            ))))
        }
    }
}

fn linked_module_entries_for_context(
    ctx: &AppContext,
) -> platform_core::AppResult<Vec<&'static LinkedModuleEntry>> {
    Ok(
        linked_module_entries(CompositionProfile::from_config(&ctx.config)?)
            .iter()
            .filter(|entry| {
                linked_module_with_dependencies_enabled(ctx, entry.module_name, entry.manifest)
            })
            .collect(),
    )
}

fn linked_module_entries_for_config(
    config: &platform_core::AppConfig,
) -> platform_core::AppResult<Vec<&'static LinkedModuleEntry>> {
    Ok(
        linked_module_entries(CompositionProfile::from_config(config)?)
            .iter()
            .filter(|entry| {
                linked_module_with_dependencies_enabled_from_config(
                    config,
                    entry.module_name,
                    entry.manifest,
                )
            })
            .collect(),
    )
}

fn disabled_linked_module_entries_for_context(
    ctx: &AppContext,
) -> platform_core::AppResult<Vec<&'static LinkedModuleEntry>> {
    Ok(
        linked_module_entries(CompositionProfile::from_config(&ctx.config)?)
            .iter()
            .filter(|entry| {
                linked_module_disabled_reason(ctx, entry.module_name, entry.manifest).is_some()
            })
            .collect(),
    )
}

fn host_linked_modules_for_config(
    config: &platform_core::AppConfig,
    composition: &HostComposition,
) -> Vec<HostLinkedModule> {
    composition
        .linked_modules()
        .iter()
        .copied()
        .filter(|entry| {
            linked_module_with_dependencies_enabled_from_config(
                config,
                entry.module_name,
                entry.manifest,
            )
        })
        .collect()
}

fn host_linked_modules_for_context(
    ctx: &AppContext,
    composition: &HostComposition,
) -> Vec<HostLinkedModule> {
    composition
        .linked_modules()
        .iter()
        .copied()
        .filter(|entry| {
            linked_module_with_dependencies_enabled(ctx, entry.module_name, entry.manifest)
        })
        .collect()
}

fn disabled_host_linked_modules_for_context(
    ctx: &AppContext,
    composition: &HostComposition,
) -> Vec<HostLinkedModule> {
    composition
        .linked_modules()
        .iter()
        .copied()
        .filter(|entry| {
            linked_module_disabled_reason(ctx, entry.module_name, entry.manifest).is_some()
        })
        .collect()
}

fn load_host_linked_module(ctx: &AppContext, entry: HostLinkedModule) -> Module {
    match entry.load {
        Some(load) => load(ctx),
        None => Module::linked((entry.manifest)(), LinkedBinding::builder().build()),
    }
}

/// Demo-default linked modules helper (context-bound: builds bindings).
///
/// Startup and config-aware paths should use [`modules_for_config`] or
/// [`load_modules`] so `module_sources.linked_profile` is honored.
#[must_use]
pub fn modules(ctx: &AppContext) -> Vec<Module> {
    modules_for_profile(ctx, CompositionProfile::default())
}

pub fn modules_for_config(ctx: &AppContext) -> platform_core::AppResult<Vec<Module>> {
    Ok(linked_module_entries_for_context(ctx)?
        .into_iter()
        .map(|entry| (entry.load)(ctx))
        .collect())
}

pub fn modules_for_config_with_composition(
    ctx: &AppContext,
    composition: &HostComposition,
) -> platform_core::AppResult<Vec<Module>> {
    let mut modules = modules_for_config(ctx)?;
    modules.extend(
        host_linked_modules_for_context(ctx, composition)
            .into_iter()
            .map(|entry| load_host_linked_module(ctx, entry)),
    );
    Ok(modules)
}

#[must_use]
pub fn modules_for_profile(ctx: &AppContext, profile: CompositionProfile) -> Vec<Module> {
    linked_module_entries(profile)
        .iter()
        .map(|entry| (entry.load)(ctx))
        .collect()
}

/// Load every configured module, including out-of-process remote modules.
///
/// The synchronous [`modules`] function remains Linked-only for call sites that
/// must stay context-local and infallible. Startup paths that can perform IO
/// should use this async loader.
pub async fn load_modules(ctx: &AppContext) -> platform_core::AppResult<Vec<Module>> {
    load_modules_with_composition(ctx, &HostComposition::default()).await
}

pub async fn load_modules_with_composition(
    ctx: &AppContext,
    composition: &HostComposition,
) -> platform_core::AppResult<Vec<Module>> {
    let mut loaded = modules_for_config_with_composition(ctx, composition)?;

    for remote in &ctx.config.module_sources.remote {
        if !remote_module_enabled(ctx, &remote.name) {
            continue;
        }
        let source = RemoteModuleSource::new(remote_module_config(remote))?;
        loaded.push(source.load().await?);
    }

    Ok(loaded)
}

pub fn migrations_for_config(
    config: &platform_core::AppConfig,
) -> platform_core::AppResult<Vec<Migration>> {
    migrations_for_config_with_composition(config, &HostComposition::default())
}

pub fn migrations_for_config_with_composition(
    config: &platform_core::AppConfig,
    composition: &HostComposition,
) -> platform_core::AppResult<Vec<Migration>> {
    let mut migrations = PLATFORM_MIGRATIONS
        .iter()
        .chain(RUNTIME_MIGRATIONS)
        .copied()
        .collect::<Vec<_>>();

    if CompositionProfile::from_config(config)? == CompositionProfile::Demo {
        if linked_module_enabled_from_config(config, "auth") {
            migrations.extend(auth::migrations::AUTH_MIGRATIONS.iter().copied());
        }
        if linked_module_with_dependencies_enabled_from_config(
            config,
            "auth-password",
            auth_password::module::manifest,
        ) {
            migrations.extend(
                auth_password::migrations::AUTH_PASSWORD_MIGRATIONS
                    .iter()
                    .copied(),
            );
        }
    }

    for module in host_linked_modules_for_config(config, composition) {
        migrations.extend(module.migrations.iter().copied());
    }

    Ok(migrations)
}

#[must_use]
pub fn migrations_for_profile(profile: CompositionProfile) -> Vec<Migration> {
    let mut migrations = PLATFORM_MIGRATIONS
        .iter()
        .chain(RUNTIME_MIGRATIONS)
        .copied()
        .collect::<Vec<_>>();

    if profile == CompositionProfile::Demo {
        migrations.extend(auth::migrations::AUTH_MIGRATIONS.iter().copied());
        migrations.extend(
            auth_password::migrations::AUTH_PASSWORD_MIGRATIONS
                .iter()
                .copied(),
        );
    }

    migrations
}

/// Context-free module manifests for read-only / OpenAPI paths that have no
/// [`AppContext`]. Kept in sync with [`modules`] by listing the same modules.
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

/// Runtime function declaration sources for context-free linked modules.
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
) -> Vec<(
    String,
    ModuleSource,
    Option<platform_module::RuntimeSurface>,
)> {
    module_manifests_for_profile(profile)
        .into_iter()
        .map(|manifest| (manifest.name, ModuleSource::Linked, manifest.runtime))
        .collect()
}

pub fn linked_runtime_function_declaration_sources_for_config(
    config: &platform_core::AppConfig,
) -> platform_core::AppResult<
    Vec<(
        String,
        ModuleSource,
        Option<platform_module::RuntimeSurface>,
    )>,
> {
    Ok(linked_module_entries_for_config(config)?
        .into_iter()
        .map(|entry| {
            let manifest = (entry.manifest)();
            (manifest.name, ModuleSource::Linked, manifest.runtime)
        })
        .collect())
}

pub fn linked_runtime_function_declaration_sources_for_context(
    ctx: &AppContext,
) -> platform_core::AppResult<
    Vec<(
        String,
        ModuleSource,
        Option<platform_module::RuntimeSurface>,
    )>,
> {
    Ok(linked_module_entries_for_context(ctx)?
        .into_iter()
        .map(|entry| {
            let manifest = (entry.manifest)();
            (manifest.name, ModuleSource::Linked, manifest.runtime)
        })
        .collect())
}

pub fn linked_runtime_function_declaration_sources_for_context_with_composition(
    ctx: &AppContext,
    composition: &HostComposition,
) -> platform_core::AppResult<
    Vec<(
        String,
        ModuleSource,
        Option<platform_module::RuntimeSurface>,
    )>,
> {
    let mut sources = linked_runtime_function_declaration_sources_for_context(ctx)?;
    sources.extend(
        host_linked_modules_for_context(ctx, composition)
            .into_iter()
            .map(|entry| {
                let manifest = (entry.manifest)();
                (manifest.name, ModuleSource::Linked, manifest.runtime)
            }),
    );
    Ok(sources)
}

/// Runtime function declaration sources from loaded module metadata, including
/// configured remote modules.
#[must_use]
pub fn runtime_function_declaration_sources_from_metadata(
    modules: &[AdminModuleMetadata],
) -> Vec<(
    String,
    ModuleSource,
    Option<platform_module::RuntimeSurface>,
)> {
    modules
        .iter()
        .filter(|module| matches!(module.load_status, ModuleLoadStatus::Loaded))
        .map(|module| {
            (
                module.module_name.clone(),
                module.source,
                module.runtime.clone(),
            )
        })
        .collect()
}

/// Public HTTP path ownership for linked modules.
///
/// Projected from context-free linked modules so OpenAPI guards and router
/// assembly consume the same source-specific binding data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedHttpRouteOwner {
    pub module_name: String,
    pub public_prefixes: &'static [&'static str],
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

/// Context-free linked modules that contribute Axum/OpenAPI HTTP routers.
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

pub fn linked_http_modules_for_config(
    config: &platform_core::AppConfig,
) -> platform_core::AppResult<Vec<Module>> {
    Ok(linked_module_entries_for_config(config)?
        .into_iter()
        .filter_map(|entry| {
            let http_binding = entry.http_binding?;
            Some(Module::linked((entry.manifest)(), http_binding()))
        })
        .collect())
}

pub fn linked_http_modules_for_context(ctx: &AppContext) -> platform_core::AppResult<Vec<Module>> {
    Ok(linked_module_entries_for_context(ctx)?
        .into_iter()
        .filter_map(|entry| {
            let http_binding = entry.http_binding?;
            Some(Module::linked((entry.manifest)(), http_binding()))
        })
        .collect())
}

pub fn linked_http_modules_for_context_with_composition(
    ctx: &AppContext,
    composition: &HostComposition,
) -> platform_core::AppResult<Vec<Module>> {
    let mut modules = linked_http_modules_for_context(ctx)?;
    modules.extend(
        host_linked_modules_for_context(ctx, composition)
            .into_iter()
            .filter_map(|entry| {
                let http_binding = entry.http_binding?;
                Some(Module::linked((entry.manifest)(), http_binding()))
            }),
    );
    Ok(modules)
}

/// Aggregate admin-capable modules: those declaring an admin surface and
/// providing either an `AdminDataSource` or an `AdminActionSource`. Modules
/// without an admin behavior source are filtered out — "optional capability"
/// semantics.
#[must_use]
pub fn admin_modules(ctx: &AppContext) -> Vec<AdminModule> {
    admin_modules_from_modules(modules(ctx))
}

/// Load schema-admin capable modules, including configured remotes.
pub async fn load_admin_modules(ctx: &AppContext) -> platform_core::AppResult<Vec<AdminModule>> {
    load_admin_modules_with_composition(ctx, &HostComposition::default()).await
}

pub async fn load_admin_modules_with_composition(
    ctx: &AppContext,
    composition: &HostComposition,
) -> platform_core::AppResult<Vec<AdminModule>> {
    let mut admin_modules =
        admin_modules_from_modules(modules_for_config_with_composition(ctx, composition)?);

    for remote in &ctx.config.module_sources.remote {
        if !remote_module_enabled(ctx, &remote.name) {
            continue;
        }
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

/// Load registry metadata for every configured module, including modules with
/// no admin surface and custom surfaces not consumable by schema-admin
/// list/detail.
pub async fn load_admin_module_metadata(
    ctx: &AppContext,
) -> platform_core::AppResult<Vec<AdminModuleMetadata>> {
    load_admin_module_metadata_with_composition(ctx, &HostComposition::default()).await
}

pub async fn load_admin_module_metadata_with_composition(
    ctx: &AppContext,
    composition: &HostComposition,
) -> platform_core::AppResult<Vec<AdminModuleMetadata>> {
    let mut metadata =
        admin_metadata_from_modules(modules_for_config_with_composition(ctx, composition)?);
    metadata.extend(disabled_linked_admin_metadata(ctx)?);
    metadata.extend(disabled_host_linked_admin_metadata(ctx, composition));

    for remote in &ctx.config.module_sources.remote {
        let config = remote_module_config(remote);
        if !remote_module_enabled(ctx, &remote.name) {
            metadata.push(disabled_remote_admin_metadata(&config));
            continue;
        }
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

pub async fn load_remote_http_proxy_registry(
    ctx: &AppContext,
) -> platform_core::AppResult<RemoteHttpProxyRegistry> {
    let mut remote_modules = Vec::new();
    let mut remote_configs = Vec::new();

    for remote in &ctx.config.module_sources.remote {
        if !remote_module_enabled(ctx, &remote.name) {
            continue;
        }
        let config = remote_module_config(remote);
        let source = RemoteModuleSource::new(config.clone())?;
        if let Ok(module) = source.load().await {
            remote_modules.push(module);
            remote_configs.push(config);
        }
    }

    Ok(RemoteHttpProxyRegistry::from_modules(
        &remote_modules,
        &remote_configs,
    ))
}

fn admin_modules_from_modules(modules: Vec<Module>) -> Vec<AdminModule> {
    modules
        .into_iter()
        .filter_map(|module| {
            // `modules(ctx)` yields owned Modules — move the fields out.
            let data_source = module.admin_data;
            let action_source = module.admin_actions;
            let query_source = module.admin_queries;
            if data_source.is_none() && action_source.is_none() && query_source.is_none() {
                return None;
            }
            let ModuleManifest { name, admin, .. } = module.manifest;
            let admin = admin?;
            let (schema, listed_in_schema) = match &admin {
                AdminSurface::Schema(schema) => (schema.clone(), true),
                AdminSurface::DeclarativeCustom(surface) => (
                    surface.fallback_schema.clone().unwrap_or(AdminSchema {
                        entities: Vec::new(),
                    }),
                    false,
                ),
                AdminSurface::EmbeddedCustom(_) => return None,
                _ => return None,
            };
            Some(AdminModule {
                module_name: name,
                source: module.source,
                load_status: module.load_status,
                schema,
                admin: Some(admin),
                listed_in_schema,
                data_source,
                action_source,
                query_source,
            })
        })
        .collect()
}

fn admin_metadata_from_modules(modules: Vec<Module>) -> Vec<AdminModuleMetadata> {
    modules
        .into_iter()
        .map(|module| {
            let ModuleManifest {
                name,
                admin,
                http_routes,
                runtime,
                events,
                lifecycle,
                console,
                story_display,
                capabilities,
                dependencies,
                ..
            } = module.manifest;
            AdminModuleMetadata {
                module_name: name,
                source: module.source,
                load_status: module.load_status,
                http_routes,
                runtime,
                events,
                lifecycle,
                console,
                story_display,
                capabilities,
                dependencies,
                admin,
                source_diagnostics: None,
            }
        })
        .collect()
}

fn failed_remote_admin_module(name: String, message: String) -> AdminModule {
    AdminModule {
        module_name: name,
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Error { message },
        schema: AdminSchema {
            entities: Vec::new(),
        },
        admin: None,
        listed_in_schema: true,
        data_source: None,
        action_source: None,
        query_source: None,
    }
}

fn remote_admin_metadata_from_module(
    module: Module,
    config: &RemoteModuleConfig,
    checked_at: String,
    load_duration_ms: Option<u64>,
    load_error: Option<String>,
) -> Vec<AdminModuleMetadata> {
    admin_metadata_from_modules(vec![module])
        .into_iter()
        .map(|mut metadata| {
            metadata.source_diagnostics = Some(remote_source_diagnostics(
                config,
                Some(checked_at.clone()),
                load_duration_ms,
                load_error.clone(),
            ));
            metadata
        })
        .collect()
}

fn failed_remote_admin_metadata(
    config: &RemoteModuleConfig,
    checked_at: Option<String>,
    load_duration_ms: Option<u64>,
    message: String,
) -> AdminModuleMetadata {
    AdminModuleMetadata {
        module_name: config.name.clone(),
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Error {
            message: message.clone(),
        },
        http_routes: Vec::new(),
        runtime: None,
        events: None,
        lifecycle: None,
        console: Vec::new(),
        story_display: Vec::new(),
        capabilities: Vec::new(),
        dependencies: Vec::new(),
        admin: None,
        source_diagnostics: Some(remote_source_diagnostics(
            config,
            checked_at,
            load_duration_ms,
            Some(message),
        )),
    }
}

fn disabled_remote_admin_metadata(config: &RemoteModuleConfig) -> AdminModuleMetadata {
    AdminModuleMetadata {
        module_name: config.name.clone(),
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Error {
            message: "module disabled by configuration".to_owned(),
        },
        http_routes: Vec::new(),
        runtime: None,
        events: None,
        lifecycle: None,
        console: Vec::new(),
        story_display: Vec::new(),
        capabilities: Vec::new(),
        dependencies: Vec::new(),
        admin: None,
        source_diagnostics: Some(remote_source_diagnostics(config, None, None, None)),
    }
}

fn disabled_linked_admin_metadata(
    ctx: &AppContext,
) -> platform_core::AppResult<Vec<AdminModuleMetadata>> {
    Ok(disabled_linked_module_entries_for_context(ctx)?
        .into_iter()
        .map(|entry| {
            let ModuleManifest {
                name,
                admin,
                http_routes,
                runtime,
                events,
                lifecycle,
                console,
                story_display,
                capabilities,
                dependencies,
                ..
            } = (entry.manifest)();
            AdminModuleMetadata {
                module_name: name,
                source: ModuleSource::Linked,
                load_status: ModuleLoadStatus::Error {
                    message: linked_module_disabled_reason(ctx, entry.module_name, entry.manifest)
                        .unwrap_or_else(|| "module disabled by configuration".to_owned()),
                },
                http_routes,
                runtime,
                events,
                lifecycle,
                console,
                story_display,
                capabilities,
                dependencies,
                admin,
                source_diagnostics: None,
            }
        })
        .collect())
}

fn disabled_host_linked_admin_metadata(
    ctx: &AppContext,
    composition: &HostComposition,
) -> Vec<AdminModuleMetadata> {
    disabled_host_linked_modules_for_context(ctx, composition)
        .into_iter()
        .map(|entry| {
            let ModuleManifest {
                name,
                admin,
                http_routes,
                runtime,
                events,
                lifecycle,
                console,
                story_display,
                capabilities,
                dependencies,
                ..
            } = (entry.manifest)();
            AdminModuleMetadata {
                module_name: name,
                source: ModuleSource::Linked,
                load_status: ModuleLoadStatus::Error {
                    message: linked_module_disabled_reason(ctx, entry.module_name, entry.manifest)
                        .unwrap_or_else(|| "module disabled by configuration".to_owned()),
                },
                http_routes,
                runtime,
                events,
                lifecycle,
                console,
                story_display,
                capabilities,
                dependencies,
                admin,
                source_diagnostics: None,
            }
        })
        .collect()
}

fn remote_source_diagnostics(
    config: &RemoteModuleConfig,
    checked_at: Option<String>,
    load_duration_ms: Option<u64>,
    load_error: Option<String>,
) -> AdminModuleSourceDiagnostics {
    let (transport, manifest_url) = match config.transport {
        platform_module_remote::RemoteModuleTransport::HttpJson => {
            ("http_json", format!("{}/manifest", config.base_url))
        }
        platform_module_remote::RemoteModuleTransport::Grpc => (
            "grpc",
            format!(
                "{}#lenso.remote.v1.RemoteModule/GetManifest",
                config.base_url
            ),
        ),
    };
    AdminModuleSourceDiagnostics::Remote(AdminRemoteModuleDiagnostics {
        transport: transport.to_owned(),
        base_url: config.base_url.clone(),
        manifest_url,
        timeout_ms: config.timeout_ms,
        auth_configured: config.auth_token.is_some(),
        load_duration_ms,
        last_checked_at: checked_at,
        last_load_error: load_error,
    })
}

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

#[derive(Debug)]
pub struct RemoteModuleServiceSupervisor {
    services: Vec<RemoteModuleServiceHandle>,
}

impl RemoteModuleServiceSupervisor {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.services.is_empty()
    }
}

impl Drop for RemoteModuleServiceSupervisor {
    fn drop(&mut self) {
        for service in &mut self.services {
            terminate_remote_module_service(&mut service.child);
            release_remote_module_service_state(&service.lock_file_path, &service.pid_file_path);
        }
    }
}

#[derive(Debug)]
struct RemoteModuleServiceHandle {
    child: Child,
    lock_file_path: PathBuf,
    pid_file_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RemoteModuleServiceSpec {
    module_name: String,
    service_name: String,
    command: String,
    cwd: Option<PathBuf>,
    ready_url: String,
    ready_timeout_ms: u64,
    auto_start: bool,
}

pub async fn start_installed_remote_module_services(
    ctx: &AppContext,
) -> platform_core::AppResult<RemoteModuleServiceSupervisor> {
    start_installed_remote_module_services_from_path(ctx, Path::new(DEFAULT_MODULE_SERVICES_FILE))
        .await
}

pub async fn start_installed_remote_module_services_from_path(
    ctx: &AppContext,
    services_file_path: &Path,
) -> platform_core::AppResult<RemoteModuleServiceSupervisor> {
    let specs = read_remote_module_service_specs(services_file_path)?;
    let services_state_dir = services_file_path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(800))
        .build()
        .map_err(|source| {
            AppError::new(ErrorCode::Internal, "failed to build HTTP client").with_source(source)
        })?;
    let mut services = Vec::new();

    for spec in specs {
        if !spec.auto_start || !remote_service_module_enabled(ctx, &spec.module_name) {
            continue;
        }
        if remote_service_ready(&client, &spec.ready_url).await {
            tracing::info!(
                module = %spec.module_name,
                service = %spec.service_name,
                ready_url = %spec.ready_url,
                "remote module service already ready"
            );
            continue;
        }
        let lock_file_path = remote_module_service_state_path(services_state_dir, &spec, "lock");
        let pid_file_path = remote_module_service_state_path(services_state_dir, &spec, "pid");
        if !claim_remote_module_service_lock(&client, &spec, &lock_file_path, &pid_file_path)
            .await?
        {
            continue;
        }
        let mut child = match spawn_remote_module_service(&spec) {
            Ok(child) => child,
            Err(error) => {
                release_remote_module_service_state(&lock_file_path, &pid_file_path);
                return Err(error);
            }
        };
        if let Err(error) = write_remote_module_service_pid(&pid_file_path, child.id()) {
            terminate_remote_module_service(&mut child);
            release_remote_module_service_state(&lock_file_path, &pid_file_path);
            return Err(error);
        }
        if let Err(error) = wait_for_remote_module_service(&client, &spec, &mut child).await {
            terminate_remote_module_service(&mut child);
            release_remote_module_service_state(&lock_file_path, &pid_file_path);
            return Err(error);
        }
        tracing::info!(
            module = %spec.module_name,
            service = %spec.service_name,
            ready_url = %spec.ready_url,
            "started remote module service"
        );
        services.push(RemoteModuleServiceHandle {
            child,
            lock_file_path,
            pid_file_path,
        });
    }

    Ok(RemoteModuleServiceSupervisor { services })
}

fn remote_service_module_enabled(ctx: &AppContext, module_name: &str) -> bool {
    ctx.config
        .module_sources
        .remote
        .iter()
        .any(|remote| remote.name == module_name)
        && remote_module_enabled(ctx, module_name)
}

fn read_remote_module_service_specs(
    services_file_path: &Path,
) -> platform_core::AppResult<Vec<RemoteModuleServiceSpec>> {
    let source = match std::fs::read_to_string(services_file_path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(AppError::new(
                ErrorCode::ExternalDependency,
                format!("remote module services file could not be read: {source}"),
            ));
        }
    };
    let value = serde_json::from_str::<serde_json::Value>(&source).map_err(|source| {
        AppError::new(
            ErrorCode::Validation,
            format!("remote module services file could not be parsed: {source}"),
        )
    })?;
    parse_remote_module_service_specs(&value)
}

fn parse_remote_module_service_specs(
    value: &serde_json::Value,
) -> platform_core::AppResult<Vec<RemoteModuleServiceSpec>> {
    let modules = value
        .get("modules")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::Validation,
                "remote module services file modules must be an array",
            )
        })?;
    let mut specs = Vec::new();
    for module in modules {
        let module_name = json_string(module, "moduleName")?;
        let services = module
            .get("services")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::Validation,
                    format!("{module_name} services must be an array"),
                )
            })?;
        for service in services {
            let command = json_string(service, "command")?;
            let ready_url = json_string(service, "readyUrl")?;
            specs.push(RemoteModuleServiceSpec {
                module_name: module_name.clone(),
                service_name: service
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(&module_name)
                    .to_owned(),
                command,
                cwd: service
                    .get("cwd")
                    .and_then(serde_json::Value::as_str)
                    .map(PathBuf::from),
                ready_url,
                ready_timeout_ms: service
                    .get("readyTimeoutMs")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(DEFAULT_REMOTE_SERVICE_READY_TIMEOUT_MS),
                auto_start: service
                    .get("autoStart")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true),
            });
        }
    }
    Ok(specs)
}

fn json_string(value: &serde_json::Value, key: &str) -> platform_core::AppResult<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| AppError::new(ErrorCode::Validation, format!("{key} must be a string")))
}

fn spawn_remote_module_service(spec: &RemoteModuleServiceSpec) -> platform_core::AppResult<Child> {
    let cwd = spec
        .cwd
        .clone()
        .unwrap_or(std::env::current_dir().map_err(|source| {
            AppError::new(ErrorCode::Internal, "failed to resolve current directory")
                .with_source(source)
        })?);
    let mut command = shell_command(&spec.command);
    command.current_dir(cwd);
    configure_remote_module_service_process(&mut command);
    command.spawn().map_err(|source| {
        AppError::new(
            ErrorCode::ExternalDependency,
            format!(
                "failed to start remote module service {}: {}",
                spec.module_name, spec.service_name
            ),
        )
        .with_source(source)
    })
}

async fn wait_for_remote_module_service(
    client: &reqwest::Client,
    spec: &RemoteModuleServiceSpec,
    child: &mut Child,
) -> platform_core::AppResult<()> {
    let started = Instant::now();
    let timeout = Duration::from_millis(spec.ready_timeout_ms);
    loop {
        if remote_service_ready(client, &spec.ready_url).await {
            return Ok(());
        }
        if let Some(status) = child.try_wait().map_err(|source| {
            AppError::new(
                ErrorCode::ExternalDependency,
                format!(
                    "remote module service {} status could not be checked",
                    spec.service_name
                ),
            )
            .with_source(source)
        })? {
            return Err(AppError::new(
                ErrorCode::ExternalDependency,
                format!(
                    "remote module service {} exited before it became ready: {status}",
                    spec.service_name
                ),
            ));
        }
        if started.elapsed() >= timeout {
            return Err(AppError::new(
                ErrorCode::ExternalDependency,
                format!(
                    "remote module service {} did not become ready at {}",
                    spec.service_name, spec.ready_url
                ),
            ));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn remote_service_ready(client: &reqwest::Client, ready_url: &str) -> bool {
    client
        .get(ready_url)
        .send()
        .await
        .is_ok_and(|response| response.status().is_success())
}

async fn claim_remote_module_service_lock(
    client: &reqwest::Client,
    spec: &RemoteModuleServiceSpec,
    lock_file_path: &Path,
    pid_file_path: &Path,
) -> platform_core::AppResult<bool> {
    match create_remote_module_service_lock(lock_file_path) {
        Ok(()) => return Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(source) => {
            return Err(AppError::new(
                ErrorCode::ExternalDependency,
                format!(
                    "remote module service {} lock could not be created: {source}",
                    spec.service_name
                ),
            ));
        }
    }

    tracing::info!(
        module = %spec.module_name,
        service = %spec.service_name,
        ready_url = %spec.ready_url,
        "remote module service startup already claimed"
    );
    if wait_for_remote_module_service_ready(
        client,
        &spec.ready_url,
        Duration::from_millis(spec.ready_timeout_ms),
    )
    .await
    {
        return Ok(false);
    }

    tracing::warn!(
        module = %spec.module_name,
        service = %spec.service_name,
        lock_file = %lock_file_path.display(),
        "remote module service lock did not become ready before timeout; treating it as stale"
    );
    terminate_stale_remote_module_service(pid_file_path);
    release_remote_module_service_state(lock_file_path, pid_file_path);
    match create_remote_module_service_lock(lock_file_path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(source) => Err(AppError::new(
            ErrorCode::ExternalDependency,
            format!(
                "stale remote module service {} lock could not be replaced: {source}",
                spec.service_name
            ),
        )),
    }
}

fn create_remote_module_service_lock(lock_file_path: &Path) -> std::io::Result<()> {
    if let Some(parent) = lock_file_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(lock_file_path)?;
    writeln!(file, "owner_pid={}", std::process::id())?;
    Ok(())
}

fn write_remote_module_service_pid(
    pid_file_path: &Path,
    child_pid: u32,
) -> platform_core::AppResult<()> {
    if let Some(parent) = pid_file_path.parent() {
        fs::create_dir_all(parent).map_err(|source| {
            AppError::new(
                ErrorCode::ExternalDependency,
                format!("remote module service pid directory could not be created: {source}"),
            )
        })?;
    }
    fs::write(pid_file_path, format!("{child_pid}\n")).map_err(|source| {
        AppError::new(
            ErrorCode::ExternalDependency,
            format!("remote module service pid file could not be written: {source}"),
        )
    })
}

fn release_remote_module_service_state(lock_file_path: &Path, pid_file_path: &Path) {
    let _ = fs::remove_file(pid_file_path);
    let _ = fs::remove_file(lock_file_path);
}

#[cfg(unix)]
fn terminate_stale_remote_module_service(pid_file_path: &Path) {
    let Ok(source) = fs::read_to_string(pid_file_path) else {
        return;
    };
    let Ok(pid) = source.trim().parse::<u32>() else {
        return;
    };

    let _ = Command::new("kill")
        .arg("-TERM")
        .arg(format!("-{pid}"))
        .status();
    thread::sleep(Duration::from_millis(100));
}

#[cfg(not(unix))]
fn terminate_stale_remote_module_service(_pid_file_path: &Path) {}

async fn wait_for_remote_module_service_ready(
    client: &reqwest::Client,
    ready_url: &str,
    timeout: Duration,
) -> bool {
    let started = Instant::now();
    loop {
        if remote_service_ready(client, ready_url).await {
            return true;
        }
        if started.elapsed() >= timeout {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

fn remote_module_service_state_path(
    services_state_dir: &Path,
    spec: &RemoteModuleServiceSpec,
    extension: &str,
) -> PathBuf {
    services_state_dir.join(format!(
        "remote-{}-{}.{}",
        remote_module_service_state_segment(&spec.module_name),
        remote_module_service_state_segment(&spec.service_name),
        extension
    ))
}

fn remote_module_service_state_segment(value: &str) -> String {
    let mut segment = String::new();
    let mut previous_dash = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            segment.push(character.to_ascii_lowercase());
            previous_dash = false;
        } else if !segment.is_empty() && !previous_dash {
            segment.push('-');
            previous_dash = true;
        }
    }
    while segment.ends_with('-') {
        segment.pop();
    }
    if segment.is_empty() {
        "service".to_owned()
    } else {
        segment
    }
}

fn terminate_remote_module_service(child: &mut Child) {
    if matches!(child.try_wait(), Ok(Some(_))) {
        return;
    }

    #[cfg(unix)]
    {
        let process_group_id = child.id();
        let _ = Command::new("kill")
            .arg("-TERM")
            .arg(format!("-{process_group_id}"))
            .status();
        if wait_for_remote_module_service_exit(
            child,
            Duration::from_millis(REMOTE_SERVICE_TERMINATE_GRACE_MS),
        ) {
            return;
        }
    }

    let _ = child.kill();
    let _ = child.wait();
}

fn wait_for_remote_module_service_exit(child: &mut Child, timeout: Duration) -> bool {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) => {}
            Err(_) => return true,
        }
        if started.elapsed() >= timeout {
            return false;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(unix)]
fn configure_remote_module_service_process(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_remote_module_service_process(_command: &mut Command) {}

fn shell_command(command: &str) -> Command {
    if cfg!(windows) {
        let mut process = Command::new("cmd");
        process.arg("/C").arg(command);
        process
    } else {
        let mut process = Command::new("sh");
        process.arg("-c").arg(command);
        process
    }
}

fn current_timestamp() -> String {
    use platform_core::Clock;
    platform_core::SystemClock.now().to_rfc3339()
}

fn duration_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Build a [`FunctionRegistry`] from every module's binding.
#[must_use]
pub fn function_registry(modules: &[Module]) -> FunctionRegistry {
    let mut registry = FunctionRegistry::default();
    for module in modules {
        module.binding.register_functions(&mut registry);
    }
    registry
}

/// Validate and enqueue every startup activation job declared by loaded modules.
///
/// Lifecycle activation is host-owned: module manifests declare the work, and
/// the Lenso bootstrap validates those declarations against the runtime registry
/// before scheduling function runs.
pub async fn enqueue_lifecycle_activation_jobs(
    ctx: &AppContext,
    modules: &[Module],
    registry: &FunctionRegistry,
) -> platform_core::AppResult<Vec<String>> {
    validate_lifecycle_activation_jobs(modules, registry)?;

    let client = RuntimeClient::new(ctx.db.clone());
    let mut run_ids = Vec::new();

    for module in modules {
        let Some(lifecycle) = &module.manifest.lifecycle else {
            continue;
        };

        for job in &lifecycle.activation_jobs {
            if job.run_policy != LifecycleActivationRunPolicy::EveryStartup {
                continue;
            }
            if !module_declares_runtime_function(module, &job.function_name) {
                continue;
            }

            let Some(definition) = registry.get(&job.function_name) else {
                continue;
            };

            let enqueue_result = client
                .enqueue_function(EnqueueFunctionRequest {
                    function_name: job.function_name.clone(),
                    input_json: job.input.clone(),
                    correlation_id: CorrelationId::new(ctx.ids.new_id("corr_lifecycle")),
                    actor: ActorContext::Service {
                        service_id: "worker".to_owned(),
                        scopes: vec!["runtime.functions.enqueue".to_owned()],
                    },
                    trace: TraceContext::default(),
                    causation_id: Some(format!(
                        "module_lifecycle:{}:{}",
                        module.manifest.name, job.name
                    )),
                    max_attempts: Some(runtime_max_attempts_for_enqueue(
                        definition.retry_policy.max_attempts,
                    )),
                })
                .await;

            match enqueue_result {
                Ok(run_id) => run_ids.push(run_id),
                Err(error) if job.required => return Err(error),
                Err(error) => warn_optional_lifecycle_enqueue_failure(
                    &module.manifest.name,
                    &job.name,
                    &job.function_name,
                    &error,
                ),
            }
        }
    }

    Ok(run_ids)
}

fn validate_lifecycle_activation_jobs(
    modules: &[Module],
    registry: &FunctionRegistry,
) -> platform_core::AppResult<()> {
    for module in modules {
        let Some(lifecycle) = &module.manifest.lifecycle else {
            continue;
        };

        for check in &lifecycle.startup_checks {
            match &check.check {
                LifecycleStartupCheckKind::FunctionRegistered { function_name } => {
                    if !module_declares_runtime_function(module, function_name) {
                        let reason = format!(
                            "startup check `{}` references function `{}` not declared by module `{}`",
                            check.name, function_name, module.manifest.name
                        );
                        if !check.required {
                            warn_optional_lifecycle_skip(
                                &module.manifest.name,
                                "startup_checks",
                                &check.name,
                                &reason,
                            );
                            continue;
                        }
                        return Err(lifecycle_validation_error(
                            &module.manifest.name,
                            "startup_checks",
                            &check.name,
                            format!("required {reason}"),
                        ));
                    }
                    if registry.get(function_name).is_none() {
                        let reason = format!(
                            "startup check `{}` references missing function `{}`",
                            check.name, function_name
                        );
                        if !check.required {
                            warn_optional_lifecycle_skip(
                                &module.manifest.name,
                                "startup_checks",
                                &check.name,
                                &reason,
                            );
                            continue;
                        }
                        return Err(lifecycle_validation_error(
                            &module.manifest.name,
                            "startup_checks",
                            &check.name,
                            format!("required {reason}"),
                        ));
                    }
                }
                LifecycleStartupCheckKind::CapabilityDeclared { capability } => {
                    if !module.manifest.capabilities.contains(capability) {
                        let reason = format!(
                            "startup check `{}` references missing capability `{}`",
                            check.name, capability
                        );
                        if !check.required {
                            warn_optional_lifecycle_skip(
                                &module.manifest.name,
                                "startup_checks",
                                &check.name,
                                &reason,
                            );
                            continue;
                        }
                        return Err(lifecycle_validation_error(
                            &module.manifest.name,
                            "startup_checks",
                            &check.name,
                            format!("required {reason}"),
                        ));
                    }
                }
                _ => {
                    let reason = format!(
                        "startup check `{}` uses an unsupported lifecycle check kind",
                        check.name
                    );
                    if !check.required {
                        warn_optional_lifecycle_skip(
                            &module.manifest.name,
                            "startup_checks",
                            &check.name,
                            &reason,
                        );
                        continue;
                    }
                    return Err(lifecycle_validation_error(
                        &module.manifest.name,
                        "startup_checks",
                        &check.name,
                        format!("required {reason}"),
                    ));
                }
            }
        }

        for job in &lifecycle.activation_jobs {
            if job.run_policy != LifecycleActivationRunPolicy::EveryStartup {
                continue;
            }

            if !module_declares_runtime_function(module, &job.function_name) {
                let reason = format!(
                    "activation job `{}` references function `{}` not declared by module `{}`",
                    job.name, job.function_name, module.manifest.name
                );
                if !job.required {
                    warn_optional_lifecycle_skip(
                        &module.manifest.name,
                        "activation_jobs",
                        &job.name,
                        &reason,
                    );
                    continue;
                }
                return Err(lifecycle_validation_error(
                    &module.manifest.name,
                    "activation_jobs",
                    &job.name,
                    format!("required {reason}"),
                ));
            }
            if registry.get(&job.function_name).is_none() {
                let reason = format!(
                    "activation job `{}` references missing function `{}`",
                    job.name, job.function_name
                );
                if !job.required {
                    warn_optional_lifecycle_skip(
                        &module.manifest.name,
                        "activation_jobs",
                        &job.name,
                        &reason,
                    );
                    continue;
                }
                return Err(lifecycle_validation_error(
                    &module.manifest.name,
                    "activation_jobs",
                    &job.name,
                    format!("required {reason}"),
                ));
            }
        }
    }

    Ok(())
}

fn module_declares_runtime_function(module: &Module, function_name: &str) -> bool {
    module.manifest.runtime.as_ref().is_some_and(|runtime| {
        runtime
            .functions
            .iter()
            .any(|function| function.name == function_name)
    })
}

fn lifecycle_validation_error(
    module_name: &str,
    collection: &str,
    item_name: &str,
    reason: String,
) -> AppError {
    AppError::validation(
        "Module lifecycle declaration failed validation",
        vec![ErrorDetail {
            field: Some(format!(
                "module.{module_name}.lifecycle.{collection}.{item_name}"
            )),
            reason,
        }],
    )
}

fn warn_optional_lifecycle_skip(
    module_name: &str,
    collection: &str,
    item_name: &str,
    reason: &str,
) {
    tracing::warn!(
        module_name = %module_name,
        lifecycle_collection = %collection,
        lifecycle_item = %item_name,
        reason = %reason,
        "optional module lifecycle declaration skipped"
    );
}

fn warn_optional_lifecycle_enqueue_failure(
    module_name: &str,
    job_name: &str,
    function_name: &str,
    error: &AppError,
) {
    tracing::warn!(
        module_name = %module_name,
        lifecycle_collection = "activation_jobs",
        lifecycle_item = %job_name,
        function_name = %function_name,
        error_code = %error.code.as_str(),
        error_message = %error.public_message,
        "optional module lifecycle activation enqueue failed"
    );
}

fn runtime_max_attempts_for_enqueue(max_attempts: u32) -> i32 {
    i32::try_from(max_attempts).unwrap_or(i32::MAX)
}

/// Build an [`EventHandlerRegistry`] from every module's binding.
#[must_use]
pub fn event_handlers(modules: &[Module]) -> EventHandlerRegistry {
    event_handlers_with_context(modules, &EventHandlerRegistrationContext::empty())
}

/// Build an [`EventHandlerRegistry`] with host runtime actions enabled for
/// remote event-handler result actions.
#[must_use]
pub fn event_handlers_with_runtime_actions(
    ctx: &AppContext,
    modules: &[Module],
    function_registry: Arc<FunctionRegistry>,
) -> EventHandlerRegistry {
    let context = EventHandlerRegistrationContext::with_runtime(
        RuntimeClient::new(ctx.db.clone()),
        function_registry,
    );
    event_handlers_with_context(modules, &context)
}

fn event_handlers_with_context(
    modules: &[Module],
    context: &EventHandlerRegistrationContext,
) -> EventHandlerRegistry {
    let mut registry = EventHandlerRegistry::new();
    for module in modules {
        module
            .binding
            .register_event_handlers(&mut registry, context);
    }
    registry
}

/// Merge every linked module's HTTP routes (and their `OpenAPI` docs) onto `base`.
///
/// Linked route builders are context-free, so this assembles the HTTP surface
/// without constructing the full module set (which requires an [`AppContext`])
/// — usable both for serving and for standalone `OpenAPI` document assembly.
/// This is the single source for linked API routes until HTTP joins the
/// [`platform_module::ModuleBinding`] seam.
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

pub fn merge_linked_http_for_config(
    base: ApiOpenApiRouter,
    config: &platform_core::AppConfig,
) -> platform_core::AppResult<ApiOpenApiRouter> {
    Ok(linked_http_modules_for_config(config)?
        .into_iter()
        .filter_map(|module| module.linked_http)
        .fold(base, |router, contribution| (contribution.merge)(router)))
}

pub fn merge_linked_http_for_context(
    base: ApiOpenApiRouter,
    ctx: &AppContext,
) -> platform_core::AppResult<ApiOpenApiRouter> {
    Ok(linked_http_modules_for_context(ctx)?
        .into_iter()
        .filter_map(|module| module.linked_http)
        .fold(base, |router, contribution| (contribution.merge)(router)))
}

pub fn merge_linked_http_for_context_with_composition(
    base: ApiOpenApiRouter,
    ctx: &AppContext,
    composition: &HostComposition,
) -> platform_core::AppResult<ApiOpenApiRouter> {
    Ok(
        linked_http_modules_for_context_with_composition(ctx, composition)?
            .into_iter()
            .filter_map(|module| module.linked_http)
            .fold(base, |router, contribution| (contribution.merge)(router)),
    )
}

/// Story-display descriptors for every module. Sourced from context-free
/// manifests so the `OpenAPI` path stays pure (no [`AppContext`]).
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
        .flat_map(story_display_descriptors_from_manifest)
        .collect()
}

pub fn story_display_descriptors_for_config(
    config: &platform_core::AppConfig,
) -> platform_core::AppResult<Vec<StoryDisplayDescriptor>> {
    Ok(linked_module_entries_for_config(config)?
        .into_iter()
        .flat_map(|entry| story_display_descriptors_from_manifest((entry.manifest)()))
        .collect())
}

pub fn story_display_descriptors_for_context(
    ctx: &AppContext,
) -> platform_core::AppResult<Vec<StoryDisplayDescriptor>> {
    Ok(linked_module_entries_for_context(ctx)?
        .into_iter()
        .flat_map(|entry| story_display_descriptors_from_manifest((entry.manifest)()))
        .collect())
}

pub fn install_default_story_display_catalog(ctx: &AppContext) -> platform_core::AppResult<()> {
    install_default_story_display_catalog_with_composition(ctx, &HostComposition::default())
}

pub fn install_default_story_display_catalog_with_composition(
    ctx: &AppContext,
    composition: &HostComposition,
) -> platform_core::AppResult<()> {
    if !linked_module_enabled(ctx, story::module::MODULE_NAME) {
        story::backend::install_default_story_display(Vec::new());
        return Ok(());
    }
    let mut descriptors = story_display_descriptors_for_context(ctx)?;
    descriptors.extend(
        host_linked_modules_for_context(ctx, composition)
            .into_iter()
            .flat_map(|entry| story_display_descriptors_from_manifest((entry.manifest)())),
    );
    story::backend::install_default_story_display(descriptors);
    Ok(())
}

pub fn install_story_display_catalog(metadata: &[AdminModuleMetadata]) {
    story::backend::install_story_display(
        metadata
            .iter()
            .flat_map(story_display_descriptors_from_metadata)
            .collect(),
    );
}

fn story_display_descriptors_from_metadata(
    module: &AdminModuleMetadata,
) -> Vec<StoryDisplayDescriptor> {
    story_display_descriptors_from_manifest(
        ModuleManifest::builder(module.module_name.clone())
            .story_display(module.story_display.clone())
            .http_routes(module.http_routes.clone())
            .build(),
    )
}

fn story_display_descriptors_from_manifest(
    manifest: ModuleManifest,
) -> Vec<StoryDisplayDescriptor> {
    let mut descriptors = manifest.story_display;
    let existing_http = descriptors
        .iter()
        .filter_map(|descriptor| match &descriptor.source {
            StoryDisplaySource::HttpRequest { method, path } => {
                Some((method.clone(), path.clone()))
            }
            StoryDisplaySource::ExecutionName { .. } => None,
        })
        .collect::<Vec<_>>();

    descriptors.extend(manifest.http_routes.into_iter().filter_map(|route| {
        let display_name = route.display_name?;
        let method = http_method_label(route.method)?;
        if existing_http
            .iter()
            .any(|(existing_method, existing_path)| {
                existing_method == method && existing_path == &route.path
            })
        {
            return None;
        }
        Some(StoryDisplayDescriptor {
            source: StoryDisplaySource::HttpRequest {
                method: method.to_owned(),
                path: route.path,
            },
            display_name,
            story_title: route.story_title,
        })
    }));
    descriptors
}

fn http_method_label(method: ModuleHttpMethod) -> Option<&'static str> {
    Some(match method {
        ModuleHttpMethod::Get => "GET",
        ModuleHttpMethod::Post => "POST",
        ModuleHttpMethod::Put => "PUT",
        ModuleHttpMethod::Patch => "PATCH",
        ModuleHttpMethod::Delete => "DELETE",
        _ => return None,
    })
}

/// Every module's setting descriptors.
///
/// The single source for the editable configuration registry. Apps build a
/// `RuntimeConfigRegistry` from this list at startup.
pub fn runtime_config_descriptors(
    ctx: &AppContext,
) -> platform_core::AppResult<Vec<RuntimeConfigDescriptor>> {
    runtime_config_descriptors_with_composition(ctx, &HostComposition::default())
}

pub fn runtime_config_descriptors_with_composition(
    ctx: &AppContext,
    composition: &HostComposition,
) -> platform_core::AppResult<Vec<RuntimeConfigDescriptor>> {
    let profile = CompositionProfile::from_config(&ctx.config)?;
    let module_enabled_descriptors =
        linked_module_entries(profile)
            .iter()
            .map(|entry| RuntimeConfigDescriptor {
                key: module_enabled_config_key(entry.module_name),
                scope: RuntimeConfigScope::Shared,
                group: Some("modules"),
                section: None,
                order: 10,
                visible_when: None,
                generated: None,
                value_type: RuntimeConfigType::Bool,
                default: serde_json::json!(linked_module_enabled_from_config(
                    &ctx.config,
                    entry.module_name
                )),
                editable: true,
                restart_only: true,
                description: "Whether this linked module is loaded on service startup.",
            });
    let host_module_enabled_descriptors =
        composition
            .linked_modules()
            .iter()
            .map(|entry| RuntimeConfigDescriptor {
                key: module_enabled_config_key(entry.module_name),
                scope: RuntimeConfigScope::Shared,
                group: Some("modules"),
                section: None,
                order: 10,
                visible_when: None,
                generated: None,
                value_type: RuntimeConfigType::Bool,
                default: serde_json::json!(linked_module_enabled_from_config(
                    &ctx.config,
                    entry.module_name
                )),
                editable: true,
                restart_only: true,
                description: "Whether this host linked module is loaded on service startup.",
            });
    let remote_module_enabled_descriptors =
        ctx.config
            .module_sources
            .remote
            .iter()
            .map(|source| RuntimeConfigDescriptor {
                key: module_enabled_config_key(&source.name),
                scope: RuntimeConfigScope::Shared,
                group: Some("modules"),
                section: None,
                order: 10,
                visible_when: None,
                generated: None,
                value_type: RuntimeConfigType::Bool,
                default: serde_json::json!(remote_module_enabled_from_config(
                    &ctx.config,
                    &source.name
                )),
                editable: true,
                restart_only: true,
                description: "Whether this remote module is loaded on service startup.",
            });
    let module_descriptors = linked_module_entries(profile)
        .iter()
        .filter(|entry| linked_module_enabled_from_config(&ctx.config, entry.module_name))
        .map(|entry| (entry.load)(ctx))
        .chain(
            host_linked_modules_for_config(&ctx.config, composition)
                .into_iter()
                .map(|entry| load_host_linked_module(ctx, entry)),
        )
        .flat_map(|module| module.runtime_config.iter().cloned())
        .collect::<Vec<_>>();
    // Platform-owned descriptors (e.g. worker knobs) plus every module's; keys
    // are globally unique, so chain order is presentation-only.
    Ok(platform_core::worker_runtime_config::RUNTIME_CONFIG
        .iter()
        .cloned()
        .chain(module_enabled_descriptors)
        .chain(host_module_enabled_descriptors)
        .chain(remote_module_enabled_descriptors)
        .chain(module_descriptors)
        .collect())
}

/// Every config presentation group known to the current composition.
pub fn runtime_config_group_descriptors(
    ctx: &AppContext,
) -> platform_core::AppResult<Vec<RuntimeConfigGroupDescriptor>> {
    runtime_config_group_descriptors_with_composition(ctx, &HostComposition::default())
}

pub fn runtime_config_group_descriptors_with_composition(
    ctx: &AppContext,
    composition: &HostComposition,
) -> platform_core::AppResult<Vec<RuntimeConfigGroupDescriptor>> {
    let profile = CompositionProfile::from_config(&ctx.config)?;
    let module_groups = linked_module_entries(profile)
        .iter()
        .filter(|entry| linked_module_enabled_from_config(&ctx.config, entry.module_name))
        .map(|entry| (entry.load)(ctx))
        .chain(
            host_linked_modules_for_config(&ctx.config, composition)
                .into_iter()
                .map(|entry| load_host_linked_module(ctx, entry)),
        )
        .flat_map(|module| module.runtime_config_groups.iter().cloned())
        .collect::<Vec<_>>();

    Ok(std::iter::once(MODULES_CONFIG_GROUP.clone())
        .chain(
            platform_core::worker_runtime_config::RUNTIME_CONFIG_GROUPS
                .iter()
                .cloned(),
        )
        .chain(module_groups)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use platform_core::{
        AppConfig, AuthConfig, DatabaseConfig, ErrorCode, ExecutionContext, HttpConfig,
        LoggingEventPublisher, ModuleConfig, ModuleSourcesConfig, PLATFORM_MIGRATIONS, RedisConfig,
        RemoteModuleSourceConfig, RuntimeConfigProvider, RuntimeConfigRegistry,
        RuntimeConfigSnapshot, ServiceConfig, TelemetryConfig, apply_migrations,
    };
    use platform_module::{
        ConsoleArea, LifecycleActivationJobDeclaration, LifecycleStartupCheckDeclaration,
        LifecycleSurface, ModuleManifestLintSeverity, RuntimeFunctionDeclaration, RuntimeSurface,
        lint_module_manifest,
    };
    use platform_runtime::{FunctionDefinition, FunctionHandler, RUNTIME_MIGRATIONS, RetryPolicy};
    use platform_testing::{SequentialIdGenerator, TestDatabase};
    use serde_json::{Value, json};
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Duration;

    #[derive(Debug)]
    struct TestRuntimeConfigProvider {
        snapshot: Arc<RuntimeConfigSnapshot>,
    }

    impl RuntimeConfigProvider for TestRuntimeConfigProvider {
        fn snapshot(&self) -> Arc<RuntimeConfigSnapshot> {
            Arc::clone(&self.snapshot)
        }
    }

    #[test]
    fn linked_module_entry_names_match_manifests() {
        for profile in [CompositionProfile::Core, CompositionProfile::Demo] {
            for entry in linked_module_entries(profile) {
                assert_eq!(
                    entry.module_name,
                    (entry.manifest)().name,
                    "linked module entry name must match ModuleManifest::name"
                );
            }
        }
    }

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

        assert_eq!(names, vec!["auth", "auth-password", "platform-story"]);
    }

    #[test]
    fn http_route_metadata_contributes_story_display_descriptors() {
        let descriptors = story_display_descriptors_for_profile(CompositionProfile::Demo);

        assert!(descriptors.iter().any(|descriptor| {
            matches!(
                &descriptor.source,
                StoryDisplaySource::HttpRequest { method, path }
                    if method == "POST" && path == "/v1/auth/dev/sessions"
            ) && descriptor.display_name == "Create Development Session"
        }));
    }

    #[test]
    fn core_profile_migrations_exclude_demo_module_migrations() {
        let names = migrations_for_profile(CompositionProfile::Core)
            .into_iter()
            .map(|migration| migration.name)
            .collect::<Vec<_>>();

        assert!(names.iter().any(|name| name.starts_with("platform/")));
        assert!(names.iter().any(|name| name.starts_with("runtime/")));
        assert!(!names.iter().any(|name| name.starts_with("auth/")));
        assert!(!names.iter().any(|name| name.starts_with("auth-password/")));
    }

    #[test]
    fn demo_profile_migrations_include_fixture_module_migrations() {
        let names = migrations_for_profile(CompositionProfile::Demo)
            .into_iter()
            .map(|migration| migration.name)
            .collect::<Vec<_>>();

        assert!(
            names
                .iter()
                .any(|name| name == &"auth/0001_create_auth_schema")
        );
        assert!(
            names
                .iter()
                .any(|name| name == &"auth-password/0001_create_auth_password_schema")
        );
    }

    #[test]
    fn host_composition_migrations_include_enabled_host_linked_modules() {
        let config = test_config_with_database_url("postgres://localhost/lenso_test");
        let composition = HostComposition::new().with_linked_module(test_host_linked_module());

        let names = migrations_for_config_with_composition(&config, &composition)
            .expect("host composition migrations should load")
            .into_iter()
            .map(|migration| migration.name)
            .collect::<Vec<_>>();

        assert!(names.iter().any(|name| name == &"billing/0001_init"));
    }

    #[test]
    fn host_composition_can_install_auth_modules() {
        let mut config = test_config_with_database_url("postgres://localhost/lenso_test");
        config.module_sources.linked_profile = "core".to_owned();
        let composition = HostComposition::new()
            .with_linked_module(auth_linked_module())
            .with_linked_module(auth_password_linked_module());

        let names = migrations_for_config_with_composition(&config, &composition)
            .expect("host composition migrations should load")
            .into_iter()
            .map(|migration| migration.name)
            .collect::<Vec<_>>();

        assert!(
            names
                .iter()
                .any(|name| name == &"auth/0001_create_auth_schema")
        );
        assert!(
            names
                .iter()
                .any(|name| name == &"auth-password/0001_create_auth_password_schema")
        );
    }

    #[tokio::test]
    async fn host_composition_runtime_config_includes_host_module_toggle() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let config = test_config_with_database_url("postgres://localhost/lenso_test");
        let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));
        let composition = HostComposition::new().with_linked_module(test_host_linked_module());

        let keys = runtime_config_descriptors_with_composition(&ctx, &composition)
            .expect("host composition descriptors should load")
            .into_iter()
            .map(|descriptor| descriptor.key)
            .collect::<Vec<_>>();

        assert!(keys.iter().any(|key| key == "modules.billing.enabled"));
    }

    #[tokio::test]
    async fn host_composition_modules_include_manifest_only_modules() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let config = test_config_with_database_url("postgres://localhost/lenso_test");
        let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));
        let composition = HostComposition::new().with_linked_module(test_host_linked_module());

        let names = modules_for_config_with_composition(&ctx, &composition)
            .expect("host composition modules should load")
            .into_iter()
            .map(|module| module.manifest.name)
            .collect::<Vec<_>>();

        assert!(names.iter().any(|name| name == "billing"));
    }

    #[test]
    fn demo_profile_includes_every_core_entry() {
        let demo_names = linked_module_entries(CompositionProfile::Demo)
            .iter()
            .map(|entry| entry.module_name)
            .collect::<Vec<_>>();

        for core_entry in linked_module_entries(CompositionProfile::Core) {
            assert!(
                demo_names.contains(&core_entry.module_name),
                "demo profile should include core linked module `{}`",
                core_entry.module_name
            );
        }
    }

    #[test]
    fn default_module_manifests_use_demo_profile() {
        let names = module_manifests()
            .into_iter()
            .map(|manifest| manifest.name)
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["auth", "auth-password", "platform-story"]);
    }

    #[test]
    fn linked_http_route_owners_are_profile_aware() {
        assert_eq!(
            linked_http_route_owners_for_profile(CompositionProfile::Core),
            vec![LinkedHttpRouteOwner {
                module_name: "platform-story".to_owned(),
                public_prefixes: &["/admin/runtime/stories"],
            }]
        );
        assert_eq!(
            linked_http_route_owners_for_profile(CompositionProfile::Demo),
            vec![
                LinkedHttpRouteOwner {
                    module_name: "auth".to_owned(),
                    public_prefixes: &["/v1/auth/dev/", "/v1/auth/sessions/"],
                },
                LinkedHttpRouteOwner {
                    module_name: "auth-password".to_owned(),
                    public_prefixes: &["/v1/auth/password/"],
                },
                LinkedHttpRouteOwner {
                    module_name: "platform-story".to_owned(),
                    public_prefixes: &["/admin/runtime/stories"],
                },
            ]
        );
    }

    #[tokio::test]
    async fn modules_for_config_uses_core_linked_profile() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let mut config = test_config_with_database_url("postgres://localhost/lenso_test");
        config.module_sources.linked_profile = "core".to_owned();
        let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));

        let names = modules_for_config(&ctx)
            .expect("core linked profile should parse")
            .into_iter()
            .map(|module| module.manifest.name)
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["platform-story"]);
    }

    #[tokio::test]
    async fn auth_actor_resolver_is_profile_and_composition_aware() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let demo_ctx = AppContext::new(
            test_config_with_database_url("postgres://localhost/lenso_test"),
            db.clone(),
            Arc::new(LoggingEventPublisher),
        );
        assert!(
            auth_actor_resolver_for_context(&demo_ctx)
                .expect("demo profile")
                .is_some()
        );

        let mut composition_config =
            test_config_with_database_url("postgres://localhost/lenso_test");
        composition_config.module_sources.linked_profile = "core".to_owned();
        let composition_ctx = AppContext::new(
            composition_config,
            db.clone(),
            Arc::new(LoggingEventPublisher),
        );
        let composition = HostComposition::new().with_linked_module(auth_linked_module());
        assert!(
            auth_actor_resolver_for_context_with_composition(&composition_ctx, &composition)
                .expect("auth composition")
                .is_some()
        );

        let mut core_config = test_config_with_database_url("postgres://localhost/lenso_test");
        core_config.module_sources.linked_profile = "core".to_owned();
        let core_ctx = AppContext::new(core_config, db, Arc::new(LoggingEventPublisher));
        assert!(
            auth_actor_resolver_for_context(&core_ctx)
                .expect("core profile")
                .is_none()
        );
    }

    #[tokio::test]
    async fn auth_actor_resolver_respects_disabled_auth_module() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let mut config = test_config_with_database_url("postgres://localhost/lenso_test");
        config.modules.insert(
            auth::module::MODULE_NAME.to_owned(),
            ModuleConfig {
                enabled: Some(false),
                values: BTreeMap::new(),
            },
        );
        let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));

        assert!(
            auth_actor_resolver_for_context(&ctx)
                .expect("demo profile")
                .is_none()
        );
    }

    #[tokio::test]
    async fn auth_password_requires_auth_module() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let mut config = test_config_with_database_url("postgres://localhost/lenso_test");
        config.modules.insert(
            auth::module::MODULE_NAME.to_owned(),
            ModuleConfig {
                enabled: Some(false),
                values: BTreeMap::new(),
            },
        );
        let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));

        let names = modules_for_config(&ctx)
            .expect("demo profile")
            .into_iter()
            .map(|module| module.manifest.name)
            .collect::<Vec<_>>();

        assert!(!names.iter().any(|name| name == "auth-password"));
    }

    #[tokio::test]
    async fn auth_password_dependency_status_is_visible_in_metadata() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let mut config = test_config_with_database_url("postgres://localhost/lenso_test");
        config.modules.insert(
            auth::module::MODULE_NAME.to_owned(),
            ModuleConfig {
                enabled: Some(false),
                values: BTreeMap::new(),
            },
        );
        let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));

        let metadata = load_admin_module_metadata(&ctx)
            .await
            .expect("module metadata should load");
        let auth_password = metadata
            .iter()
            .find(|module| module.module_name == "auth-password")
            .expect("dependency-disabled provider should remain visible in metadata");

        assert_eq!(
            auth_password.dependencies,
            vec![auth::module::MODULE_NAME.to_owned()]
        );
        assert!(matches!(
            &auth_password.load_status,
            ModuleLoadStatus::Error { message }
                if message == "module dependency disabled: auth"
        ));
    }

    #[tokio::test]
    async fn auth_actor_resolver_allows_jwt_strategy_without_secret() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let config = test_config_with_database_url("postgres://localhost/lenso_test");
        let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));
        let registry =
            RuntimeConfigRegistry::try_new(runtime_config_descriptors(&ctx).expect("descriptors"))
                .expect("registry");
        let mut stored = BTreeMap::new();
        stored.insert(
            ("*".to_owned(), "auth-password.token_strategy".to_owned()),
            json!("jwt"),
        );
        let snapshot = RuntimeConfigSnapshot::resolve(&registry, "api", &stored);
        let ctx = ctx.with_runtime_config_provider(Arc::new(TestRuntimeConfigProvider {
            snapshot: Arc::new(snapshot),
        }));

        assert!(
            auth_actor_resolver_for_context(&ctx)
                .expect("JWT resolver should be skipped until jwt_secret is configured")
                .is_some()
        );
    }

    #[tokio::test]
    async fn auth_actor_resolver_requires_redis_when_session_cache_is_redis() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let config = test_config_with_database_url("postgres://localhost/lenso_test");
        let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));
        let registry =
            RuntimeConfigRegistry::try_new(runtime_config_descriptors(&ctx).expect("descriptors"))
                .expect("registry");
        let mut stored = BTreeMap::new();
        stored.insert(
            ("*".to_owned(), "auth.session_cache".to_owned()),
            json!("redis"),
        );
        let snapshot = RuntimeConfigSnapshot::resolve(&registry, "api", &stored);
        let ctx = ctx.with_runtime_config_provider(Arc::new(TestRuntimeConfigProvider {
            snapshot: Arc::new(snapshot),
        }));

        let error =
            auth_actor_resolver_for_context(&ctx).expect_err("redis cache should require Redis");

        assert_eq!(error.code, ErrorCode::Validation);
    }

    #[tokio::test]
    async fn modules_for_config_skips_disabled_linked_modules() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let mut config = test_config_with_database_url("postgres://localhost/lenso_test");
        config.modules.insert(
            "auth-password".to_owned(),
            ModuleConfig {
                enabled: Some(false),
                values: BTreeMap::new(),
            },
        );
        let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));

        let names = modules_for_config(&ctx)
            .expect("demo linked profile should parse")
            .into_iter()
            .map(|module| module.manifest.name)
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["auth", "platform-story"]);
    }

    #[tokio::test]
    async fn modules_for_config_uses_runtime_config_enabled_flag() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let config = test_config_with_database_url("postgres://localhost/lenso_test");
        let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));
        let registry =
            RuntimeConfigRegistry::try_new(runtime_config_descriptors(&ctx).expect("descriptors"))
                .expect("registry");
        let mut stored = BTreeMap::new();
        stored.insert(
            ("*".to_owned(), "modules.auth-password.enabled".to_owned()),
            json!(false),
        );
        let snapshot = RuntimeConfigSnapshot::resolve(&registry, "api", &stored);
        let ctx = ctx.with_runtime_config_provider(Arc::new(TestRuntimeConfigProvider {
            snapshot: Arc::new(snapshot),
        }));

        let names = modules_for_config(&ctx)
            .expect("demo linked profile should parse")
            .into_iter()
            .map(|module| module.manifest.name)
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["auth", "platform-story"]);
        let linked_http_names = linked_http_modules_for_context(&ctx)
            .expect("linked HTTP modules should load")
            .into_iter()
            .map(|module| module.manifest.name)
            .collect::<Vec<_>>();

        assert_eq!(linked_http_names, vec!["auth", "platform-story"]);
    }

    #[tokio::test]
    async fn story_module_runtime_config_disables_backend_metadata() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let config = test_config_with_database_url("postgres://localhost/lenso_test");
        let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));
        let registry =
            RuntimeConfigRegistry::try_new(runtime_config_descriptors(&ctx).expect("descriptors"))
                .expect("registry");
        let mut stored = BTreeMap::new();
        stored.insert(
            ("*".to_owned(), "modules.platform-story.enabled".to_owned()),
            json!(false),
        );
        let snapshot = RuntimeConfigSnapshot::resolve(&registry, "api", &stored);
        let ctx = ctx.with_runtime_config_provider(Arc::new(TestRuntimeConfigProvider {
            snapshot: Arc::new(snapshot),
        }));

        let linked_http_names = linked_http_modules_for_context(&ctx)
            .expect("linked HTTP modules should load")
            .into_iter()
            .map(|module| module.manifest.name)
            .collect::<Vec<_>>();
        assert_eq!(linked_http_names, vec!["auth", "auth-password"]);

        let metadata = load_admin_module_metadata(&ctx)
            .await
            .expect("module metadata should load");
        let story = metadata
            .iter()
            .find(|module| module.module_name == "platform-story")
            .expect("disabled story module should remain visible in metadata");

        assert!(matches!(
            &story.load_status,
            ModuleLoadStatus::Error { message }
                if message == "module disabled by configuration"
        ));
        assert_eq!(story.console.len(), 1);
        assert_eq!(story.http_routes.len(), story::module::http_routes().len());
    }

    #[tokio::test]
    async fn runtime_config_descriptors_include_module_enabled_flags() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let config = test_config_with_database_url("postgres://localhost/lenso_test");
        let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));

        let keys = runtime_config_descriptors(&ctx)
            .expect("descriptors should load")
            .into_iter()
            .map(|descriptor| {
                (
                    descriptor.key,
                    descriptor.group,
                    descriptor.restart_only,
                    descriptor.default,
                )
            })
            .collect::<Vec<_>>();

        assert!(keys.iter().any(|(key, group, restart_only, default)| {
            key == "modules.auth.enabled"
                && *group == Some("modules")
                && *restart_only
                && default == &json!(true)
        }));
        assert!(keys.iter().any(|(key, group, restart_only, default)| {
            key == "modules.auth-password.enabled"
                && *group == Some("modules")
                && *restart_only
                && default == &json!(true)
        }));
        assert!(keys.iter().any(|(key, group, restart_only, default)| {
            key == "modules.platform-story.enabled"
                && *group == Some("modules")
                && *restart_only
                && default == &json!(true)
        }));
    }

    #[tokio::test]
    async fn runtime_config_groups_include_module_owned_groups() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let config = test_config_with_database_url("postgres://localhost/lenso_test");
        let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));

        let groups = runtime_config_group_descriptors(&ctx)
            .expect("groups should load")
            .into_iter()
            .map(|group| (group.id, group.label))
            .collect::<Vec<_>>();

        assert!(groups.contains(&("modules", "Modules")));
        assert!(groups.contains(&("auth-password.hashing", "Password Hashing")));
        assert!(groups.contains(&("auth-password.tokens", "Tokens")));
        assert!(!groups.iter().any(|(id, _)| *id == "auth-password.jwt"));
    }

    #[tokio::test]
    async fn runtime_config_descriptors_include_remote_module_enabled_flags() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let mut config = test_config_with_database_url("postgres://localhost/lenso_test");
        config.module_sources.remote.push(RemoteModuleSourceConfig {
            name: "remote-crm".to_owned(),
            base_url: "http://127.0.0.1:65535".to_owned(),
            auth_token_env: None,
            timeout_ms: 1,
        });
        config.modules.insert(
            "remote-crm".to_owned(),
            ModuleConfig {
                enabled: Some(false),
                values: BTreeMap::new(),
            },
        );
        let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));

        let keys = runtime_config_descriptors(&ctx)
            .expect("descriptors should load")
            .into_iter()
            .map(|descriptor| (descriptor.key, descriptor.restart_only, descriptor.default))
            .collect::<Vec<_>>();

        assert!(keys.iter().any(|(key, restart_only, default)| {
            key == "modules.remote-crm.enabled" && *restart_only && default == &json!(false)
        }));
    }

    #[tokio::test]
    async fn load_modules_skips_runtime_disabled_remote_modules() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let mut config = test_config_with_database_url("postgres://localhost/lenso_test");
        config.module_sources.remote.push(RemoteModuleSourceConfig {
            name: "remote-crm".to_owned(),
            base_url: "http://127.0.0.1:65535".to_owned(),
            auth_token_env: None,
            timeout_ms: 1,
        });
        let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));
        let registry =
            RuntimeConfigRegistry::try_new(runtime_config_descriptors(&ctx).expect("descriptors"))
                .expect("registry");
        let mut stored = BTreeMap::new();
        stored.insert(
            ("*".to_owned(), "modules.remote-crm.enabled".to_owned()),
            json!(false),
        );
        let snapshot = RuntimeConfigSnapshot::resolve(&registry, "api", &stored);
        let ctx = ctx.with_runtime_config_provider(Arc::new(TestRuntimeConfigProvider {
            snapshot: Arc::new(snapshot),
        }));

        let names = load_modules(&ctx)
            .await
            .expect("disabled remote should not be loaded")
            .into_iter()
            .map(|module| module.manifest.name)
            .collect::<Vec<_>>();

        assert!(!names.iter().any(|name| name == "remote-crm"));
    }

    #[tokio::test]
    async fn module_metadata_reports_disabled_remote_modules() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let mut config = test_config_with_database_url("postgres://localhost/lenso_test");
        config.module_sources.remote.push(RemoteModuleSourceConfig {
            name: "remote-grpc-crm".to_owned(),
            base_url: "grpc://127.0.0.1:65535".to_owned(),
            auth_token_env: None,
            timeout_ms: 1,
        });
        config.modules.insert(
            "remote-grpc-crm".to_owned(),
            ModuleConfig {
                enabled: Some(false),
                values: BTreeMap::new(),
            },
        );
        let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));

        let metadata = load_admin_module_metadata(&ctx)
            .await
            .expect("module metadata should load");
        let remote = metadata
            .iter()
            .find(|module| module.module_name == "remote-grpc-crm")
            .expect("disabled remote module should remain visible in metadata");

        assert_eq!(remote.source, ModuleSource::Remote);
        assert!(matches!(
            &remote.load_status,
            ModuleLoadStatus::Error { message }
                if message == "module disabled by configuration"
        ));
        assert!(matches!(
            &remote.source_diagnostics,
            Some(AdminModuleSourceDiagnostics::Remote(diagnostics))
                if diagnostics.transport == "grpc"
                    && diagnostics.base_url == "http://127.0.0.1:65535"
        ));
    }

    #[test]
    fn migrations_for_config_skip_disabled_linked_module_migrations() {
        let mut config = test_config_with_database_url("postgres://localhost/lenso_test");
        config.modules.insert(
            "auth-password".to_owned(),
            ModuleConfig {
                enabled: Some(false),
                values: BTreeMap::new(),
            },
        );

        let names = migrations_for_config(&config)
            .expect("demo linked profile should parse")
            .into_iter()
            .map(|migration| migration.name)
            .collect::<Vec<_>>();

        assert!(!names.iter().any(|name| name.starts_with("auth-password/")));
        assert!(
            names
                .iter()
                .any(|name| name == &"auth/0001_create_auth_schema")
        );
    }

    #[test]
    fn linked_http_modules_for_config_skip_disabled_linked_routes() {
        let mut config = test_config_with_database_url("postgres://localhost/lenso_test");
        config.modules.insert(
            "auth-password".to_owned(),
            ModuleConfig {
                enabled: Some(false),
                values: BTreeMap::new(),
            },
        );

        let names = linked_http_modules_for_config(&config)
            .expect("demo linked profile should parse")
            .into_iter()
            .map(|module| module.manifest.name)
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["auth", "platform-story"]);
    }

    #[test]
    fn linked_http_modules_for_config_skip_disabled_story_routes() {
        let mut config = test_config_with_database_url("postgres://localhost/lenso_test");
        config.modules.insert(
            "platform-story".to_owned(),
            ModuleConfig {
                enabled: Some(false),
                values: BTreeMap::new(),
            },
        );

        let names = linked_http_modules_for_config(&config)
            .expect("demo linked profile should parse")
            .into_iter()
            .map(|module| module.manifest.name)
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["auth", "auth-password"]);
    }

    #[tokio::test]
    async fn disabled_story_module_omits_default_story_display_catalog() {
        story::backend::reset_catalogs_for_test();
        let mut config = test_config_with_database_url("postgres://localhost/lenso_test");
        config.modules.insert(
            "platform-story".to_owned(),
            ModuleConfig {
                enabled: Some(false),
                values: BTreeMap::new(),
            },
        );
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));

        install_default_story_display_catalog(&ctx)
            .expect("story display catalog installation should succeed");

        assert!(story::backend::story_display_catalog_snapshot().is_empty());
    }

    #[tokio::test]
    async fn module_metadata_reports_disabled_linked_modules() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let mut config = test_config_with_database_url("postgres://localhost/lenso_test");
        config.modules.insert(
            "auth-password".to_owned(),
            ModuleConfig {
                enabled: Some(false),
                values: BTreeMap::new(),
            },
        );
        let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));

        let metadata = load_admin_module_metadata(&ctx)
            .await
            .expect("module metadata should load");
        let auth_password = metadata
            .iter()
            .find(|module| module.module_name == "auth-password")
            .expect("disabled module should remain visible in metadata");

        assert!(matches!(
            &auth_password.load_status,
            ModuleLoadStatus::Error { message }
                if message == "module disabled by configuration"
        ));
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

    #[test]
    fn linked_http_route_owners_are_projected_from_modules() {
        assert_eq!(
            linked_http_route_owners(),
            vec![
                LinkedHttpRouteOwner {
                    module_name: "auth".to_owned(),
                    public_prefixes: &["/v1/auth/dev/", "/v1/auth/sessions/"],
                },
                LinkedHttpRouteOwner {
                    module_name: "auth-password".to_owned(),
                    public_prefixes: &["/v1/auth/password/"],
                },
                LinkedHttpRouteOwner {
                    module_name: "platform-story".to_owned(),
                    public_prefixes: &["/admin/runtime/stories"],
                },
            ]
        );
    }

    #[test]
    fn linked_http_bindings_are_declared_in_manifests() {
        for module in linked_http_modules() {
            let http = module
                .linked_http
                .expect("linked HTTP module should carry HTTP contribution");
            assert!(
                !module.manifest.http_routes.is_empty(),
                "linked HTTP module `{}` must declare ModuleManifest::http_routes",
                module.manifest.name
            );
            for route in &module.manifest.http_routes {
                assert!(
                    http.public_prefixes
                        .iter()
                        .any(|prefix| route.path.starts_with(prefix)),
                    "linked HTTP module `{}` declares manifest route `{}` outside its public prefixes",
                    module.manifest.name,
                    route.path
                );
            }
        }
    }

    #[test]
    fn linked_http_modules_are_registered_modules() {
        let manifests = module_manifests();

        for module in linked_http_modules() {
            let registered_manifest = manifests
                .iter()
                .find(|manifest| manifest.name == module.manifest.name)
                .unwrap_or_else(|| {
                    panic!(
                        "linked HTTP module `{}` is missing from module_manifests",
                        module.manifest.name
                    )
                });
            assert_eq!(
                registered_manifest, &module.manifest,
                "linked HTTP module `{}` must use the registered ModuleManifest",
                module.manifest.name
            );
        }
    }

    #[test]
    fn linked_http_routes_include_story_module_routes() {
        let document = merge_linked_http(platform_http::OpenApiRouter::new()).to_openapi();
        let value = serde_json::to_value(document).expect("OpenAPI document should serialize");
        let paths = value["paths"].as_object().expect("OpenAPI paths object");

        assert!(paths.contains_key("/admin/runtime/stories"));
        assert!(paths.contains_key("/admin/runtime/stories/{correlation_id}"));
        assert!(paths.contains_key("/admin/runtime/stories/{correlation_id}/heatmap"));
        assert!(paths.contains_key("/admin/runtime/stories/{correlation_id}/technical-operations"));
    }

    #[test]
    fn platform_story_manifest_declares_story_console_surface() {
        let manifest = module_manifests()
            .into_iter()
            .find(|manifest| manifest.name == "platform-story")
            .expect("platform-story manifest should be registered");
        let console_surface_contract: Value = serde_json::from_str(include_str!(
            "../../../modules/story/console/console-surface.json"
        ))
        .expect("story console surface contract should be valid json");

        assert_eq!(manifest.admin, None);
        assert_eq!(manifest.console.len(), 1);
        let surface = &manifest.console[0];
        let surface_json =
            serde_json::to_value(surface).expect("platform-story console surface should serialize");

        assert_eq!(
            manifest.capabilities,
            required_capabilities_from_contract(&console_surface_contract)
        );
        assert_eq!(manifest.name, console_surface_contract["id"]);
        assert_eq!(surface.name, console_surface_contract["surfaceName"]);
        assert_eq!(surface.label, console_surface_contract["label"]);
        assert_eq!(surface.area, ConsoleArea::Runtime);
        assert_eq!(surface_json["area"], console_surface_contract["area"]);
        assert_eq!(surface.route, console_surface_contract["route"]);
        assert_eq!(
            surface.package.name,
            console_surface_contract["packageName"]
        );
        assert_eq!(
            surface.package.export,
            console_surface_contract["exportName"]
        );
        assert_eq!(surface_json["icon"], console_surface_contract["icon"]);
        assert_eq!(surface.navigation, None);
        assert!(console_surface_contract.get("navigation").is_none());
        assert_eq!(
            surface.required_capabilities,
            required_capabilities_from_contract(&console_surface_contract)
        );

        let lints = lint_module_manifest(ModuleSource::Linked, &manifest);
        assert!(
            lints
                .iter()
                .all(|lint| lint.severity == ModuleManifestLintSeverity::Ok),
            "platform-story manifest should not have warning/error lints: {lints:?}"
        );
    }

    fn required_capabilities_from_contract(contract: &Value) -> Vec<String> {
        contract["requiredCapabilities"]
            .as_array()
            .expect("requiredCapabilities should be an array")
            .iter()
            .map(|capability| {
                capability
                    .as_str()
                    .expect("requiredCapabilities should contain strings")
                    .to_owned()
            })
            .collect()
    }

    #[tokio::test]
    async fn lifecycle_activation_enqueue_creates_function_run() {
        let Some(db) = TestDatabase::create().await else {
            return;
        };
        apply_runtime_stack_migrations(&db).await;

        let mut ctx = AppContext::new(
            test_config(&db),
            db.pool.clone(),
            Arc::new(LoggingEventPublisher),
        );
        ctx.ids = Arc::new(SequentialIdGenerator::default());
        let modules = vec![
            test_lifecycle_module(lifecycle_activation_job(true, json!({ "warm": "cache" })))
                .into(),
        ];
        let registry = registry_with_lifecycle_function(7);

        let run_ids = enqueue_lifecycle_activation_jobs(&ctx, &modules, &registry)
            .await
            .expect("lifecycle activation job should enqueue");

        assert_eq!(run_ids.len(), 1);
        let row = sqlx::query_as::<_, (String, Value, i32, String, Value)>(
            r#"
            select function_name, input_json, max_attempts, correlation_id, actor
            from runtime.function_runs
            where id = $1
            "#,
        )
        .bind(&run_ids[0])
        .fetch_one(&db.pool)
        .await
        .expect("function run should exist");

        assert_eq!(row.0, LIFECYCLE_FUNCTION_NAME);
        assert_eq!(row.1["warm"], "cache");
        assert_eq!(
            row.1["_lenso_runtime"]["correlation_id"],
            "corr_lifecycle_1"
        );
        assert_eq!(
            row.1["_lenso_runtime"]["causation_id"],
            "module_lifecycle:test-module:warm cache"
        );
        assert_eq!(row.2, 7);
        assert_eq!(row.3, "corr_lifecycle_1");
        assert_eq!(row.4["kind"], "service");
        assert_eq!(row.4["service_id"], "worker");
        assert_eq!(row.4["scopes"][0], "runtime.functions.enqueue");

        db.cleanup().await;
    }

    #[test]
    fn lifecycle_activation_validation_rejects_required_missing_function() {
        let modules =
            vec![test_lifecycle_module(lifecycle_activation_job(true, Value::Null)).into()];
        let registry = FunctionRegistry::default();

        let error = validate_lifecycle_activation_jobs(&modules, &registry)
            .expect_err("required missing activation function should fail validation");

        assert_eq!(error.code, ErrorCode::Validation);
        assert_eq!(
            error.details[0].field.as_deref(),
            Some("module.test-module.lifecycle.activation_jobs.warm cache")
        );
        assert!(
            error.details[0].reason.contains("missing function"),
            "validation detail should name the missing registry function"
        );
    }

    #[test]
    fn lifecycle_activation_validation_rejects_required_startup_check_missing_function() {
        let modules = vec![test_lifecycle_module_with_lifecycle(
            LifecycleSurface {
                startup_checks: vec![LifecycleStartupCheckDeclaration {
                    name: "function registered".to_owned(),
                    required: true,
                    check: LifecycleStartupCheckKind::FunctionRegistered {
                        function_name: LIFECYCLE_FUNCTION_NAME.to_owned(),
                    },
                }],
                activation_jobs: Vec::new(),
            },
            true,
            Vec::new(),
        )];
        let registry = FunctionRegistry::default();

        let error = validate_lifecycle_activation_jobs(&modules, &registry)
            .expect_err("required startup check should fail when function is missing");

        assert_eq!(error.code, ErrorCode::Validation);
        assert_eq!(
            error.details[0].field.as_deref(),
            Some("module.test-module.lifecycle.startup_checks.function registered")
        );
        assert!(
            error.details[0].reason.contains("missing function"),
            "validation detail should name the missing registry function"
        );
    }

    #[test]
    fn lifecycle_activation_validation_rejects_required_startup_check_function_not_declared() {
        let modules = vec![test_lifecycle_module_with_lifecycle(
            LifecycleSurface {
                startup_checks: vec![LifecycleStartupCheckDeclaration {
                    name: "function registered".to_owned(),
                    required: true,
                    check: LifecycleStartupCheckKind::FunctionRegistered {
                        function_name: LIFECYCLE_FUNCTION_NAME.to_owned(),
                    },
                }],
                activation_jobs: Vec::new(),
            },
            false,
            Vec::new(),
        )];
        let registry = registry_with_lifecycle_function(3);

        let error = validate_lifecycle_activation_jobs(&modules, &registry)
            .expect_err("required startup check should fail when manifest does not declare it");

        assert_eq!(error.code, ErrorCode::Validation);
        assert_eq!(
            error.details[0].field.as_deref(),
            Some("module.test-module.lifecycle.startup_checks.function registered")
        );
        assert!(
            error.details[0].reason.contains("not declared"),
            "validation detail should name the missing module runtime declaration"
        );
    }

    #[test]
    fn lifecycle_activation_validation_rejects_required_startup_check_missing_capability() {
        let modules = vec![test_lifecycle_module_with_lifecycle(
            LifecycleSurface {
                startup_checks: vec![LifecycleStartupCheckDeclaration {
                    name: "capability declared".to_owned(),
                    required: true,
                    check: LifecycleStartupCheckKind::CapabilityDeclared {
                        capability: "test.cache.warm".to_owned(),
                    },
                }],
                activation_jobs: Vec::new(),
            },
            false,
            Vec::new(),
        )];
        let registry = FunctionRegistry::default();

        let error = validate_lifecycle_activation_jobs(&modules, &registry)
            .expect_err("required startup check should fail when capability is missing");

        assert_eq!(error.code, ErrorCode::Validation);
        assert_eq!(
            error.details[0].field.as_deref(),
            Some("module.test-module.lifecycle.startup_checks.capability declared")
        );
        assert!(
            error.details[0].reason.contains("missing capability"),
            "validation detail should name the missing capability"
        );
    }

    #[test]
    fn lifecycle_activation_optional_startup_checks_do_not_fail_validation() {
        let modules = vec![test_lifecycle_module_with_lifecycle(
            LifecycleSurface {
                startup_checks: vec![
                    LifecycleStartupCheckDeclaration {
                        name: "optional function".to_owned(),
                        required: false,
                        check: LifecycleStartupCheckKind::FunctionRegistered {
                            function_name: LIFECYCLE_FUNCTION_NAME.to_owned(),
                        },
                    },
                    LifecycleStartupCheckDeclaration {
                        name: "optional capability".to_owned(),
                        required: false,
                        check: LifecycleStartupCheckKind::CapabilityDeclared {
                            capability: "test.cache.warm".to_owned(),
                        },
                    },
                ],
                activation_jobs: Vec::new(),
            },
            false,
            Vec::new(),
        )];
        let registry = FunctionRegistry::default();

        validate_lifecycle_activation_jobs(&modules, &registry)
            .expect("optional startup checks should not fail validation");
    }

    #[test]
    fn lifecycle_activation_validation_rejects_required_job_not_declared_by_module() {
        let modules = vec![
            test_lifecycle_module(lifecycle_activation_job(true, Value::Null))
                .without_runtime_declaration()
                .into(),
        ];
        let registry = registry_with_lifecycle_function(3);

        let error = validate_lifecycle_activation_jobs(&modules, &registry)
            .expect_err("required activation job should fail when manifest does not declare it");

        assert_eq!(error.code, ErrorCode::Validation);
        assert_eq!(
            error.details[0].field.as_deref(),
            Some("module.test-module.lifecycle.activation_jobs.warm cache")
        );
        assert!(
            error.details[0].reason.contains("not declared"),
            "validation detail should name the missing module runtime declaration"
        );
    }

    #[tokio::test]
    async fn optional_missing_lifecycle_activation_is_skipped() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let ctx = AppContext::new(
            test_config_with_database_url("postgres://localhost/lenso_test"),
            db,
            Arc::new(LoggingEventPublisher),
        );
        let modules =
            vec![test_lifecycle_module(lifecycle_activation_job(false, Value::Null)).into()];
        let registry = FunctionRegistry::default();

        let run_ids = enqueue_lifecycle_activation_jobs(&ctx, &modules, &registry)
            .await
            .expect("optional missing activation function should be skipped");

        assert!(run_ids.is_empty());
    }

    #[tokio::test]
    async fn lifecycle_activation_optional_job_not_declared_is_skipped() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let ctx = AppContext::new(
            test_config_with_database_url("postgres://localhost/lenso_test"),
            db,
            Arc::new(LoggingEventPublisher),
        );
        let modules = vec![
            test_lifecycle_module(lifecycle_activation_job(false, Value::Null))
                .without_runtime_declaration()
                .into(),
        ];
        let registry = registry_with_lifecycle_function(3);

        let run_ids = enqueue_lifecycle_activation_jobs(&ctx, &modules, &registry)
            .await
            .expect("optional undeclared activation function should be skipped");

        assert!(run_ids.is_empty());
    }

    #[tokio::test]
    async fn lifecycle_activation_optional_enqueue_failure_is_skipped() {
        let db = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_millis(50))
            .connect_lazy_with(
                PgConnectOptions::new()
                    .host("127.0.0.1")
                    .port(1)
                    .username("postgres")
                    .database("lenso_test"),
            );
        let ctx = AppContext::new(
            test_config_with_database_url("postgres://localhost:1/lenso_test"),
            db,
            Arc::new(LoggingEventPublisher),
        );
        let modules =
            vec![test_lifecycle_module(lifecycle_activation_job(false, Value::Null)).into()];
        let registry = registry_with_lifecycle_function(3);

        let run_ids = enqueue_lifecycle_activation_jobs(&ctx, &modules, &registry)
            .await
            .expect("optional enqueue failure should be skipped");

        assert!(run_ids.is_empty());
    }

    #[test]
    fn lifecycle_activation_max_attempts_conversion_saturates() {
        assert_eq!(runtime_max_attempts_for_enqueue(7), 7);
        assert_eq!(runtime_max_attempts_for_enqueue(u32::MAX), i32::MAX);
    }

    const LIFECYCLE_FUNCTION_NAME: &str = "test.warm_cache.v1";

    #[derive(Debug)]
    struct NoopFunctionHandler;

    #[async_trait]
    impl FunctionHandler for NoopFunctionHandler {
        async fn call(
            &self,
            _ctx: ExecutionContext,
            _input: Value,
        ) -> platform_core::AppResult<Value> {
            Ok(Value::Null)
        }
    }

    fn lifecycle_activation_job(required: bool, input: Value) -> LifecycleActivationJobDeclaration {
        LifecycleActivationJobDeclaration {
            name: "warm cache".to_owned(),
            function_name: LIFECYCLE_FUNCTION_NAME.to_owned(),
            run_policy: LifecycleActivationRunPolicy::EveryStartup,
            input,
            required,
        }
    }

    struct TestLifecycleModuleBuilder {
        lifecycle: LifecycleSurface,
        declare_runtime_function: bool,
        capabilities: Vec<String>,
    }

    impl TestLifecycleModuleBuilder {
        fn without_runtime_declaration(mut self) -> Self {
            self.declare_runtime_function = false;
            self
        }
    }

    impl From<TestLifecycleModuleBuilder> for Module {
        fn from(builder: TestLifecycleModuleBuilder) -> Self {
            let mut manifest = ModuleManifest::builder("test-module").lifecycle(builder.lifecycle);
            if builder.declare_runtime_function {
                manifest = manifest.runtime(RuntimeSurface {
                    functions: vec![RuntimeFunctionDeclaration {
                        name: LIFECYCLE_FUNCTION_NAME.to_owned(),
                        version: 1,
                        queue: "test".to_owned(),
                        input_schema: None,
                        retry_policy: None,
                    }],
                });
            }
            if !builder.capabilities.is_empty() {
                manifest = manifest.capabilities(builder.capabilities);
            }
            Module::linked(manifest.build(), LinkedBinding::builder().build())
        }
    }

    fn test_lifecycle_module(job: LifecycleActivationJobDeclaration) -> TestLifecycleModuleBuilder {
        TestLifecycleModuleBuilder {
            lifecycle: LifecycleSurface {
                startup_checks: Vec::new(),
                activation_jobs: vec![job],
            },
            declare_runtime_function: true,
            capabilities: Vec::new(),
        }
    }

    fn test_lifecycle_module_with_lifecycle(
        lifecycle: LifecycleSurface,
        declare_runtime_function: bool,
        capabilities: Vec<String>,
    ) -> Module {
        TestLifecycleModuleBuilder {
            lifecycle,
            declare_runtime_function,
            capabilities,
        }
        .into()
    }

    fn registry_with_lifecycle_function(max_attempts: u32) -> FunctionRegistry {
        let mut registry = FunctionRegistry::default();
        registry.register(FunctionDefinition {
            name: LIFECYCLE_FUNCTION_NAME.to_owned(),
            version: 1,
            queue: "test".to_owned(),
            retry_policy: RetryPolicy::fixed(max_attempts, Duration::ZERO),
            handler: Arc::new(NoopFunctionHandler),
        });
        registry
    }

    #[test]
    fn remote_module_service_specs_parse() {
        let specs = parse_remote_module_service_specs(&serde_json::json!({
            "modules": [
                {
                    "moduleName": "crm",
                    "services": [
                        {
                            "name": "crm-api",
                            "command": "pnpm dev",
                            "cwd": "../crm",
                            "readyUrl": "http://127.0.0.1:4100/lenso/module/v1/manifest",
                            "readyTimeoutMs": 12000,
                            "autoStart": true
                        }
                    ]
                }
            ],
            "version": 1
        }))
        .expect("service specs parse");

        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].module_name, "crm");
        assert_eq!(specs[0].service_name, "crm-api");
        assert_eq!(specs[0].ready_timeout_ms, 12000);
    }

    #[test]
    fn remote_module_service_state_path_sanitizes_names() {
        let spec = RemoteModuleServiceSpec {
            module_name: "CRM Module".to_owned(),
            service_name: "API Worker!".to_owned(),
            command: "pnpm dev".to_owned(),
            cwd: None,
            ready_url: "http://127.0.0.1:4100/lenso/module/v1/manifest".to_owned(),
            ready_timeout_ms: 12000,
            auto_start: true,
        };

        let path = remote_module_service_state_path(Path::new(".lenso"), &spec, "lock");

        assert_eq!(
            path,
            PathBuf::from(".lenso/remote-crm-module-api-worker.lock")
        );
    }

    #[test]
    fn remote_module_service_lock_is_exclusive_and_released() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after Unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "lenso-bootstrap-service-lock-{}-{unique}",
            std::process::id()
        ));
        let lock_file_path = dir.join("service.lock");
        let pid_file_path = dir.join("service.pid");

        let _ = std::fs::remove_dir_all(&dir);
        create_remote_module_service_lock(&lock_file_path)
            .expect("first lock claim should create the lock");
        let second_claim = create_remote_module_service_lock(&lock_file_path)
            .expect_err("second lock claim should fail while the file exists");
        assert_eq!(second_claim.kind(), std::io::ErrorKind::AlreadyExists);
        std::fs::write(&pid_file_path, "123\n").expect("pid file should write");

        release_remote_module_service_state(&lock_file_path, &pid_file_path);

        assert!(!lock_file_path.exists());
        assert!(!pid_file_path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    const TEST_HOST_MIGRATIONS: &[Migration] = &[Migration {
        name: "billing/0001_init",
        sql: "select 1;",
    }];

    fn test_host_manifest() -> ModuleManifest {
        ModuleManifest::builder("billing").build()
    }

    fn test_host_linked_module() -> HostLinkedModule {
        HostLinkedModule::manifest_only("billing", test_host_manifest, TEST_HOST_MIGRATIONS)
    }

    fn test_config(db: &TestDatabase) -> AppConfig {
        test_config_with_database_url(db.url.clone())
    }

    fn test_config_with_database_url(database_url: impl Into<String>) -> AppConfig {
        AppConfig {
            service: ServiceConfig::default(),
            database: DatabaseConfig {
                url: database_url.into(),
                max_connections: 5,
            },
            redis: RedisConfig::default(),
            http: HttpConfig::default(),
            telemetry: TelemetryConfig::default(),
            auth: AuthConfig::default(),
            console: Default::default(),
            module_sources: ModuleSourcesConfig::default(),
            modules: BTreeMap::new(),
        }
    }

    async fn apply_runtime_stack_migrations(db: &TestDatabase) {
        let migrations = PLATFORM_MIGRATIONS
            .iter()
            .chain(RUNTIME_MIGRATIONS)
            .copied()
            .collect::<Vec<_>>();
        apply_migrations(&db.pool, &migrations)
            .await
            .expect("platform and runtime migrations should apply");
    }
}
