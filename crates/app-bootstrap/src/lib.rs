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
    ActorContext, AppContext, AppError, CorrelationId, EventHandlerRegistry, Migration,
    PLATFORM_MIGRATIONS, RuntimeConfigDescriptor, StoryDisplayDescriptor, TraceContext,
};
use platform_http::ApiOpenApiRouter;
use platform_module::{
    AdminSchema, AdminSurface, ConsoleArea, ConsolePackage, ConsoleSurface,
    LifecycleActivationRunPolicy, LifecycleStartupCheckKind, LinkedBinding, Module,
    ModuleLoadStatus, ModuleManifest, ModuleSource,
};
use platform_module_remote::{RemoteHttpProxyRegistry, RemoteModuleConfig, RemoteModuleSource};
use platform_runtime::{
    EnqueueFunctionRequest, FunctionRegistry, RUNTIME_MIGRATIONS, RuntimeClient,
};
use std::time::Instant;

struct LinkedModuleEntry {
    module_name: &'static str,
    manifest: fn() -> ModuleManifest,
    load: fn(&AppContext) -> Module,
    http_binding: Option<fn() -> LinkedBinding>,
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

fn linked_module_enabled(config: &platform_core::AppConfig, module_name: &str) -> bool {
    config
        .modules
        .get(module_name)
        .is_none_or(platform_core::ModuleConfig::is_enabled)
}

fn linked_module_entries_for_config(
    config: &platform_core::AppConfig,
) -> platform_core::AppResult<Vec<&'static LinkedModuleEntry>> {
    Ok(
        linked_module_entries(CompositionProfile::from_config(config)?)
            .iter()
            .filter(|entry| linked_module_enabled(config, entry.module_name))
            .collect(),
    )
}

fn disabled_linked_module_entries_for_config(
    config: &platform_core::AppConfig,
) -> platform_core::AppResult<Vec<&'static LinkedModuleEntry>> {
    Ok(
        linked_module_entries(CompositionProfile::from_config(config)?)
            .iter()
            .filter(|entry| !linked_module_enabled(config, entry.module_name))
            .collect(),
    )
}

const STORY_CONSOLE_CAPABILITY: &str = "runtime.stories.read";

fn platform_story_manifest() -> ModuleManifest {
    ModuleManifest::builder("platform-story")
        .capabilities(vec![STORY_CONSOLE_CAPABILITY.to_owned()])
        .console(vec![ConsoleSurface {
            name: "stories".to_owned(),
            label: "Stories".to_owned(),
            area: ConsoleArea::Runtime,
            route: "/runtime/stories".to_owned(),
            package: ConsolePackage {
                name: "@lenso/story-console".to_owned(),
                export: "storyConsoleModule".to_owned(),
            },
            icon: Some("workflow".to_owned()),
            required_capabilities: vec![STORY_CONSOLE_CAPABILITY.to_owned()],
            navigation: None,
        }])
        .build()
}

fn platform_story_module(_ctx: &AppContext) -> Module {
    Module::linked(platform_story_manifest(), LinkedBinding::builder().build())
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
    Ok(linked_module_entries_for_config(&ctx.config)?
        .into_iter()
        .map(|entry| (entry.load)(ctx))
        .collect())
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
    let mut loaded = modules_for_config(ctx)?;

    for remote in &ctx.config.module_sources.remote {
        let source = RemoteModuleSource::new(remote_module_config(remote))?;
        loaded.push(source.load().await?);
    }

    Ok(loaded)
}

pub fn migrations_for_config(
    config: &platform_core::AppConfig,
) -> platform_core::AppResult<Vec<Migration>> {
    let mut migrations = PLATFORM_MIGRATIONS
        .iter()
        .chain(RUNTIME_MIGRATIONS)
        .copied()
        .collect::<Vec<_>>();

    if CompositionProfile::from_config(config)? == CompositionProfile::Demo {
        if linked_module_enabled(config, "identity") {
            migrations.extend(identity::migrations::IDENTITY_MIGRATIONS.iter().copied());
        }
        if linked_module_enabled(config, "notifications") {
            migrations.extend(
                notifications::migrations::NOTIFICATIONS_MIGRATIONS
                    .iter()
                    .copied(),
            );
        }
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
        migrations.extend(identity::migrations::IDENTITY_MIGRATIONS.iter().copied());
        migrations.extend(
            notifications::migrations::NOTIFICATIONS_MIGRATIONS
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

/// Load registry metadata for every configured module, including modules with
/// no admin surface and custom surfaces not consumable by schema-admin
/// list/detail.
pub async fn load_admin_module_metadata(
    ctx: &AppContext,
) -> platform_core::AppResult<Vec<AdminModuleMetadata>> {
    let mut metadata = admin_metadata_from_modules(modules_for_config(ctx)?);
    metadata.extend(disabled_linked_admin_metadata(&ctx.config)?);

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

pub async fn load_remote_http_proxy_registry(
    ctx: &AppContext,
) -> platform_core::AppResult<RemoteHttpProxyRegistry> {
    let mut remote_modules = Vec::new();
    let mut remote_configs = Vec::new();

    for remote in &ctx.config.module_sources.remote {
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
            if data_source.is_none() && action_source.is_none() {
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
                lifecycle,
                console,
                story_display,
                capabilities,
                ..
            } = module.manifest;
            AdminModuleMetadata {
                module_name: name,
                source: module.source,
                load_status: module.load_status,
                http_routes,
                runtime,
                lifecycle,
                console,
                story_display,
                capabilities,
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
        lifecycle: None,
        console: Vec::new(),
        story_display: Vec::new(),
        capabilities: Vec::new(),
        admin: None,
        source_diagnostics: Some(remote_source_diagnostics(
            config,
            checked_at,
            load_duration_ms,
            Some(message),
        )),
    }
}

fn disabled_linked_admin_metadata(
    config: &platform_core::AppConfig,
) -> platform_core::AppResult<Vec<AdminModuleMetadata>> {
    Ok(disabled_linked_module_entries_for_config(config)?
        .into_iter()
        .map(|entry| {
            let ModuleManifest {
                name,
                admin,
                http_routes,
                runtime,
                lifecycle,
                console,
                story_display,
                capabilities,
                ..
            } = (entry.manifest)();
            AdminModuleMetadata {
                module_name: name,
                source: ModuleSource::Linked,
                load_status: ModuleLoadStatus::Error {
                    message: "module disabled by configuration".to_owned(),
                },
                http_routes,
                runtime,
                lifecycle,
                console,
                story_display,
                capabilities,
                admin,
                source_diagnostics: None,
            }
        })
        .collect())
}

fn remote_source_diagnostics(
    config: &RemoteModuleConfig,
    checked_at: Option<String>,
    load_duration_ms: Option<u64>,
    load_error: Option<String>,
) -> AdminModuleSourceDiagnostics {
    AdminModuleSourceDiagnostics::Remote(AdminRemoteModuleDiagnostics {
        base_url: config.base_url.clone(),
        manifest_url: format!("{}/manifest", config.base_url),
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
/// the app bootstrap validates those declarations against the runtime registry
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
    let mut registry = EventHandlerRegistry::new();
    for module in modules {
        module.binding.register_event_handlers(&mut registry);
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
        .flat_map(|manifest| manifest.story_display)
        .collect()
}

pub fn story_display_descriptors_for_config(
    config: &platform_core::AppConfig,
) -> platform_core::AppResult<Vec<StoryDisplayDescriptor>> {
    Ok(linked_module_entries_for_config(config)?
        .into_iter()
        .flat_map(|entry| (entry.manifest)().story_display)
        .collect())
}

/// Every module's setting descriptors.
///
/// The single source for the editable configuration registry. Apps build a
/// `RuntimeConfigRegistry` from this list at startup.
pub fn runtime_config_descriptors(
    ctx: &AppContext,
) -> platform_core::AppResult<Vec<RuntimeConfigDescriptor>> {
    let module_descriptors = modules_for_config(ctx)?
        .iter()
        .flat_map(|module| module.runtime_config.iter().cloned())
        .collect::<Vec<_>>();
    // Platform-owned descriptors (e.g. worker knobs) plus every module's; keys
    // are globally unique, so chain order is presentation-only.
    Ok(platform_core::worker_runtime_config::RUNTIME_CONFIG
        .iter()
        .cloned()
        .chain(module_descriptors)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use platform_core::{
        AppConfig, AuthConfig, DatabaseConfig, ErrorCode, ExecutionContext, HttpConfig,
        LoggingEventPublisher, ModuleConfig, ModuleSourcesConfig, PLATFORM_MIGRATIONS,
        ServiceConfig, TelemetryConfig, apply_migrations,
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

        assert_eq!(names, vec!["identity", "notifications", "platform-story"]);
    }

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

        assert!(
            names
                .iter()
                .any(|name| name == &"identity/0001_create_identity_schema")
        );
        assert!(
            names
                .iter()
                .any(|name| name == &"notifications/0001_create_notifications_schema")
        );
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
    async fn modules_for_config_skips_disabled_linked_modules() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let mut config = test_config_with_database_url("postgres://localhost/lenso_test");
        config.modules.insert(
            "identity".to_owned(),
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

        assert_eq!(names, vec!["notifications", "platform-story"]);
    }

    #[test]
    fn migrations_for_config_skip_disabled_linked_module_migrations() {
        let mut config = test_config_with_database_url("postgres://localhost/lenso_test");
        config.modules.insert(
            "identity".to_owned(),
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

        assert!(!names.iter().any(|name| name.starts_with("identity/")));
        assert!(
            names
                .iter()
                .any(|name| name == &"notifications/0001_create_notifications_schema")
        );
    }

    #[test]
    fn linked_http_modules_for_config_skip_disabled_linked_routes() {
        let mut config = test_config_with_database_url("postgres://localhost/lenso_test");
        config.modules.insert(
            "identity".to_owned(),
            ModuleConfig {
                enabled: Some(false),
                values: BTreeMap::new(),
            },
        );

        assert!(
            linked_http_modules_for_config(&config)
                .expect("demo linked profile should parse")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn module_metadata_reports_disabled_linked_modules() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let mut config = test_config_with_database_url("postgres://localhost/lenso_test");
        config.modules.insert(
            "identity".to_owned(),
            ModuleConfig {
                enabled: Some(false),
                values: BTreeMap::new(),
            },
        );
        let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));

        let metadata = load_admin_module_metadata(&ctx)
            .await
            .expect("module metadata should load");
        let identity = metadata
            .iter()
            .find(|module| module.module_name == "identity")
            .expect("disabled module should remain visible in metadata");

        assert!(matches!(
            &identity.load_status,
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
            vec![LinkedHttpRouteOwner {
                module_name: "identity".to_owned(),
                public_prefixes: &["/v1/identity/"],
            }]
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
    fn platform_story_manifest_declares_story_console_surface() {
        let manifest = module_manifests()
            .into_iter()
            .find(|manifest| manifest.name == "platform-story")
            .expect("platform-story manifest should be registered");
        let console_surface_contract: Value = serde_json::from_str(include_str!(
            "../../../apps/runtime-console/packages/story-console/console-surface.json"
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
            http: HttpConfig::default(),
            telemetry: TelemetryConfig::default(),
            auth: AuthConfig::default(),
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
