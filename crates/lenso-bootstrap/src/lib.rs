//! Composition root: the single place that knows which modules exist.
//!
//! Both the API and the worker assemble their module wiring from this crate, so
//! a module is registered here once rather than in scattered per-app edits.
//!
//! A module's contributions are split by how they are consumed:
//! - [`modules`]: context-bound bindings (runtime functions + event handlers)
//!   and runtime config (API + worker), demo-default for context-local callers.
//! - [`modules_for_config`]: config-aware Linked Module loader that honors the
//!   selected composition profile.
//! - [`module_manifests`]: context-free manifest data (no [`AppContext`]) for
//!   read-only / `OpenAPI` paths, with profile-aware variants for runtime use.
//! - [`merge_linked_http`]: context-free HTTP routes and their OpenAPI docs
//!   (API only), assembled without a live [`AppContext`].
//! - [`story_display_descriptors`]: console display metadata, sourced from the
//!   context-free [`module_manifests`].
//!
//! When adding a module, register it in the appropriate profile entry lists and
//! expose its config-aware loader contributions from this crate.

use platform_core::error::ErrorDetail;
use platform_core::{
    ActorContext, AppContext, AppError, CorrelationId, ErrorCode, EventHandlerRegistry, Migration,
    PLATFORM_MIGRATIONS, RuntimeConfigDescriptor, RuntimeConfigGroupDescriptor, RuntimeConfigScope,
    RuntimeConfigType, StoryDisplayDescriptor, StoryDisplaySource, TraceContext,
};
use platform_http::ApiOpenApiRouter;
use platform_module::CronSchedule;
pub use platform_module::HostLinkedModule;
use platform_module::{
    EventHandlerRegistrationContext, LifecycleActivationRunPolicy, LifecycleStartupCheckKind,
    LinkedBinding, Module, ModuleHttpMethod, ModuleLoadStatus, ModuleManifest, ModuleSource,
};
use platform_provider::{ProviderRuntimeAdapter, ProviderRuntimeAdapters};
use platform_runtime::{
    EnqueueFunctionRequest, FunctionRegistry, RUNTIME_MIGRATIONS, RuntimeClient,
    ScheduledFunctionDefinition,
};
use std::path::Path;
use std::sync::Arc;

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

#[derive(Debug, Clone)]
pub struct HostComposition {
    linked_modules: Vec<HostLinkedModule>,
    provider_runtime_adapters: ProviderRuntimeAdapters,
}

impl Default for HostComposition {
    fn default() -> Self {
        Self {
            linked_modules: Vec::new(),
            provider_runtime_adapters: ProviderRuntimeAdapters::production_defaults(),
        }
    }
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

    #[must_use]
    pub fn with_provider_runtime_adapters(mut self, adapters: ProviderRuntimeAdapters) -> Self {
        self.provider_runtime_adapters = adapters;
        self
    }

    #[must_use]
    pub fn provider_runtime_adapters(&self) -> &ProviderRuntimeAdapters {
        &self.provider_runtime_adapters
    }
}

#[derive(Debug, Clone)]
pub struct HostWiring {
    auth_session_policy: auth::session_policy::AuthSessionPolicyHandle,
}

impl HostWiring {
    #[must_use]
    pub fn auth_session_policy(&self) -> auth::session_policy::AuthSessionPolicyHandle {
        self.auth_session_policy.clone()
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
    http_binding: None,
}];

const DEMO_LINKED_MODULE_ENTRIES: &[LinkedModuleEntry] = &[
    LinkedModuleEntry {
        module_name: "auth",
        manifest: auth::module::manifest,
        load: auth::module::module,
        http_binding: Some(auth::module::binding),
    },
    LinkedModuleEntry {
        module_name: "auth-anonymous",
        manifest: auth_anonymous::module::manifest,
        load: auth_anonymous::module::module,
        http_binding: Some(auth_anonymous::module::binding),
    },
    LinkedModuleEntry {
        module_name: "auth-oauth",
        manifest: auth_oauth::module::manifest,
        load: auth_oauth::module::module,
        http_binding: None,
    },
    LinkedModuleEntry {
        module_name: "auth-password",
        manifest: auth_password::module::manifest,
        load: auth_password::module::module,
        http_binding: Some(auth_password::module::binding),
    },
    LinkedModuleEntry {
        module_name: "auth-phone",
        manifest: auth_phone::module::manifest,
        load: auth_phone::module::module,
        http_binding: Some(auth_phone::module::binding),
    },
    LinkedModuleEntry {
        module_name: "auth-github",
        manifest: auth_github::module::manifest,
        load: auth_github::module::module,
        http_binding: Some(auth_github::module::binding),
    },
    LinkedModuleEntry {
        module_name: "auth-google",
        manifest: auth_google::module::manifest,
        load: auth_google::module::module,
        http_binding: Some(auth_google::module::binding),
    },
    LinkedModuleEntry {
        module_name: "auth-oidc",
        manifest: auth_oidc::module::manifest,
        load: auth_oidc::module::module,
        http_binding: Some(auth_oidc::module::binding),
    },
    LinkedModuleEntry {
        module_name: "platform-story",
        manifest: story::module::manifest,
        load: story::module::module,
        http_binding: None,
    },
];

fn linked_module_entries(profile: CompositionProfile) -> &'static [LinkedModuleEntry] {
    match profile {
        CompositionProfile::Core => CORE_LINKED_MODULE_ENTRIES,
        CompositionProfile::Demo => DEMO_LINKED_MODULE_ENTRIES,
    }
}

#[must_use]
pub fn auth_linked_module() -> HostLinkedModule {
    HostLinkedModule::linked(
        auth::module::MODULE_NAME,
        auth::module::manifest,
        auth::module::module,
        auth::migrations::AUTH_MIGRATIONS,
    )
    .with_http_binding(auth::module::binding)
}

#[must_use]
pub fn auth_anonymous_linked_module() -> HostLinkedModule {
    HostLinkedModule::linked(
        auth_anonymous::module::MODULE_NAME,
        auth_anonymous::module::manifest,
        auth_anonymous::module::module,
        auth_anonymous::migrations::AUTH_ANONYMOUS_MIGRATIONS,
    )
    .with_http_binding(auth_anonymous::module::binding)
}

#[must_use]
pub fn auth_password_linked_module() -> HostLinkedModule {
    HostLinkedModule::linked(
        auth_password::module::MODULE_NAME,
        auth_password::module::manifest,
        auth_password::module::module,
        auth_password::migrations::AUTH_PASSWORD_MIGRATIONS,
    )
    .with_http_binding(auth_password::module::binding)
}

#[must_use]
pub fn auth_phone_linked_module() -> HostLinkedModule {
    HostLinkedModule::linked(
        auth_phone::module::MODULE_NAME,
        auth_phone::module::manifest,
        auth_phone::module::module,
        auth_phone::migrations::AUTH_PHONE_MIGRATIONS,
    )
    .with_http_binding(auth_phone::module::binding)
}

#[must_use]
pub fn auth_oauth_linked_module() -> HostLinkedModule {
    HostLinkedModule::linked(
        auth_oauth::module::MODULE_NAME,
        auth_oauth::module::manifest,
        auth_oauth::module::module,
        auth_oauth::migrations::AUTH_OAUTH_MIGRATIONS,
    )
}

#[must_use]
pub fn auth_github_linked_module() -> HostLinkedModule {
    HostLinkedModule::linked(
        auth_github::module::MODULE_NAME,
        auth_github::module::manifest,
        auth_github::module::module,
        auth_github::migrations::AUTH_GITHUB_MIGRATIONS,
    )
    .with_http_binding(auth_github::module::binding)
}

#[must_use]
pub fn auth_google_linked_module() -> HostLinkedModule {
    HostLinkedModule::linked(
        auth_google::module::MODULE_NAME,
        auth_google::module::manifest,
        auth_google::module::module,
        auth_google::migrations::AUTH_GOOGLE_MIGRATIONS,
    )
    .with_http_binding(auth_google::module::binding)
}

#[must_use]
pub fn auth_oidc_linked_module() -> HostLinkedModule {
    HostLinkedModule::linked(
        auth_oidc::module::MODULE_NAME,
        auth_oidc::module::manifest,
        auth_oidc::module::module,
        auth_oidc::migrations::AUTH_OIDC_MIGRATIONS,
    )
    .with_http_binding(auth_oidc::module::binding)
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
        .requires
        .into_iter()
        .map(|requirement| requirement.module_id)
        .find(|module_id| {
            !linked_module_enabled(ctx, module_id.rsplit('/').next().unwrap_or(module_id))
        })
}

fn first_disabled_dependency_from_config(
    config: &platform_core::AppConfig,
    manifest: fn() -> ModuleManifest,
) -> Option<String> {
    (manifest)()
        .requires
        .into_iter()
        .map(|requirement| requirement.module_id)
        .find(|module_id| {
            !linked_module_enabled_from_config(
                config,
                module_id.rsplit('/').next().unwrap_or(module_id),
            )
        })
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

    let auth_config = auth::config::AuthRuntimeConfig::from_context(ctx);
    if auth_config.session_cache == auth::config::SessionCacheMode::Redis && ctx.redis.is_none() {
        return Err(AppError::validation(
            "Redis auth session cache is not configured",
            vec![ErrorDetail {
                field: Some("auth.session_cache".to_owned()),
                reason: "set REDIS_URL when auth.session_cache is redis".to_owned(),
            }],
        ));
    }
    let auth_resolver: Arc<dyn platform_core::ActorResolver> = Arc::new(
        auth::resolver::AuthActorResolver::new_with_session_cache(
            ctx.db.clone(),
            ctx.actor_resolver.clone(),
            auth::redis_cache::session_cache_from_context(ctx),
        )
        .with_user_scopes(auth_config.console_admin_user_scopes),
    );

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

fn linked_profile_has_module(profile: CompositionProfile, module_name: &str) -> bool {
    linked_module_entries(profile)
        .iter()
        .any(|entry| entry.module_name == module_name)
}

fn host_linked_modules_not_in_profile(
    composition: &HostComposition,
    profile: CompositionProfile,
) -> impl Iterator<Item = HostLinkedModule> + '_ {
    composition
        .linked_modules()
        .iter()
        .cloned()
        .filter(move |entry| !linked_profile_has_module(profile, entry.module_name))
}

fn host_linked_modules_for_config(
    config: &platform_core::AppConfig,
    composition: &HostComposition,
    profile: CompositionProfile,
) -> Vec<HostLinkedModule> {
    host_linked_modules_not_in_profile(composition, profile)
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
    profile: CompositionProfile,
) -> Vec<HostLinkedModule> {
    host_linked_modules_not_in_profile(composition, profile)
        .filter(|entry| {
            linked_module_with_dependencies_enabled(ctx, entry.module_name, entry.manifest)
        })
        .collect()
}

pub fn host_wiring_for_context(ctx: &AppContext) -> platform_core::AppResult<HostWiring> {
    host_wiring_for_context_with_composition(ctx, &HostComposition::default())
}

pub fn host_wiring_for_context_with_composition(
    ctx: &AppContext,
    composition: &HostComposition,
) -> platform_core::AppResult<HostWiring> {
    let profile = CompositionProfile::from_config(&ctx.config)?;
    let mut session_policies = Vec::new();
    for module in host_linked_modules_for_context(ctx, composition, profile) {
        for extension in module.contributions::<auth::session_policy::AuthHostExtension>() {
            if let Some(factory) = extension.session_policy_factory() {
                session_policies.push(factory(ctx));
            }
        }
    }

    Ok(HostWiring {
        auth_session_policy: auth::session_policy::AuthSessionPolicyChain::handle(session_policies),
    })
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
    let profile = CompositionProfile::from_config(&ctx.config)?;
    let mut modules = modules_for_config(ctx)?;
    modules.extend(
        host_linked_modules_for_context(ctx, composition, profile)
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

/// Loads a target-owned Provider Runtime Plan when Module management artifacts
/// exist. A workspace with neither artifact is a Linked-only Host; a partial
/// or inconsistent management state fails closed.
pub fn provider_runtime_plan_from_workspace(
    root: impl AsRef<Path>,
) -> platform_core::AppResult<Option<lenso_module_management::ProviderRuntimePlan>> {
    let root = root.as_ref();
    let lock = root.join("lenso.modules.lock.json");
    let planning = root.join(".lenso/module-planning-context.json");
    if !lock.exists() && !planning.exists() {
        return Ok(None);
    }
    lenso_module_management::WorkspaceModuleManagement::new(root)
        .provider_runtime_plan()
        .map(Some)
        .map_err(|error| {
            AppError::new(
                ErrorCode::Validation,
                format!("Provider runtime workspace is invalid: {error}"),
            )
        })
}

pub async fn load_modules_with_composition_and_provider_plan(
    ctx: &AppContext,
    composition: &HostComposition,
    plan: Option<&lenso_module_management::ProviderRuntimePlan>,
) -> platform_core::AppResult<Vec<Module>> {
    let mut loaded = modules_for_config_with_composition(ctx, composition)?;
    if let Some(runtime) = load_provider_runtime_with_composition(ctx, composition, plan).await? {
        loaded.extend(runtime.into_modules());
    }
    Ok(loaded)
}

pub async fn load_provider_runtime_with_composition(
    ctx: &AppContext,
    composition: &HostComposition,
    plan: Option<&lenso_module_management::ProviderRuntimePlan>,
) -> platform_core::AppResult<Option<platform_provider::LoadedProviderRuntime>> {
    let Some(plan) = plan else {
        return Ok(None);
    };
    ProviderRuntimeAdapter::with_adapters(
        plan.clone(),
        composition.provider_runtime_adapters.clone(),
    )?
    .with_effect_coordinator(platform_provider::ProviderHostEffectCoordinator::new(
        ctx.db.clone(),
    ))
    .load_verified()
    .await
    .map(Some)
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

    let profile = CompositionProfile::from_config(config)?;
    if linked_module_enabled_from_config(config, story::module::MODULE_NAME) {
        migrations.extend(story::migrations::STORY_MIGRATIONS.iter().copied());
    }
    if profile == CompositionProfile::Demo {
        if linked_module_enabled_from_config(config, "auth") {
            migrations.extend(auth::migrations::AUTH_MIGRATIONS.iter().copied());
        }
        if linked_module_with_dependencies_enabled_from_config(
            config,
            "auth-oauth",
            auth_oauth::module::manifest,
        ) {
            migrations.extend(
                auth_oauth::migrations::AUTH_OAUTH_MIGRATIONS
                    .iter()
                    .copied(),
            );
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
        if linked_module_with_dependencies_enabled_from_config(
            config,
            "auth-phone",
            auth_phone::module::manifest,
        ) {
            migrations.extend(
                auth_phone::migrations::AUTH_PHONE_MIGRATIONS
                    .iter()
                    .copied(),
            );
        }
        if linked_module_with_dependencies_enabled_from_config(
            config,
            "auth-github",
            auth_github::module::manifest,
        ) {
            migrations.extend(
                auth_github::migrations::AUTH_GITHUB_MIGRATIONS
                    .iter()
                    .copied(),
            );
        }
        if linked_module_with_dependencies_enabled_from_config(
            config,
            "auth-google",
            auth_google::module::manifest,
        ) {
            migrations.extend(
                auth_google::migrations::AUTH_GOOGLE_MIGRATIONS
                    .iter()
                    .copied(),
            );
        }
        if linked_module_with_dependencies_enabled_from_config(
            config,
            "auth-oidc",
            auth_oidc::module::manifest,
        ) {
            migrations.extend(auth_oidc::migrations::AUTH_OIDC_MIGRATIONS.iter().copied());
        }
    }

    for module in host_linked_modules_for_config(config, composition, profile) {
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

    migrations.extend(story::migrations::STORY_MIGRATIONS.iter().copied());

    if profile == CompositionProfile::Demo {
        migrations.extend(auth::migrations::AUTH_MIGRATIONS.iter().copied());
        migrations.extend(
            auth_oauth::migrations::AUTH_OAUTH_MIGRATIONS
                .iter()
                .copied(),
        );
        migrations.extend(
            auth_password::migrations::AUTH_PASSWORD_MIGRATIONS
                .iter()
                .copied(),
        );
        migrations.extend(
            auth_phone::migrations::AUTH_PHONE_MIGRATIONS
                .iter()
                .copied(),
        );
        migrations.extend(
            auth_github::migrations::AUTH_GITHUB_MIGRATIONS
                .iter()
                .copied(),
        );
        migrations.extend(
            auth_google::migrations::AUTH_GOOGLE_MIGRATIONS
                .iter()
                .copied(),
        );
        migrations.extend(auth_oidc::migrations::AUTH_OIDC_MIGRATIONS.iter().copied());
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
        .map(|manifest| (manifest.module_id, ModuleSource::Linked, manifest.runtime))
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
            (manifest.module_id, ModuleSource::Linked, manifest.runtime)
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
            (manifest.module_id, ModuleSource::Linked, manifest.runtime)
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
    let profile = CompositionProfile::from_config(&ctx.config)?;
    let mut sources = linked_runtime_function_declaration_sources_for_context(ctx)?;
    sources.extend(
        host_linked_modules_for_context(ctx, composition, profile)
            .into_iter()
            .map(|entry| {
                let manifest = (entry.manifest)();
                (manifest.module_id, ModuleSource::Linked, manifest.runtime)
            }),
    );
    Ok(sources)
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
                module_name: (entry.manifest)().module_id,
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
    let profile = CompositionProfile::from_config(&ctx.config)?;
    let mut modules = linked_http_modules_for_context(ctx)?;
    modules.extend(
        host_linked_modules_for_context(ctx, composition, profile)
            .into_iter()
            .filter_map(|entry| {
                let http_binding = entry.http_binding?;
                Some(Module::linked((entry.manifest)(), http_binding()))
            }),
    );
    Ok(modules)
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
                    tenant_id: None,
                    tenancy_mode: platform_runtime::FunctionTenancyMode::None,
                    trace: TraceContext::default(),
                    causation_id: Some(format!(
                        "module_lifecycle:{}:{}",
                        module.manifest.module_id, job.name
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
                    &module.manifest.module_id,
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
                            check.name, function_name, module.manifest.module_id
                        );
                        if !check.required {
                            warn_optional_lifecycle_skip(
                                &module.manifest.module_id,
                                "startup_checks",
                                &check.name,
                                &reason,
                            );
                            continue;
                        }
                        return Err(lifecycle_validation_error(
                            &module.manifest.module_id,
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
                                &module.manifest.module_id,
                                "startup_checks",
                                &check.name,
                                &reason,
                            );
                            continue;
                        }
                        return Err(lifecycle_validation_error(
                            &module.manifest.module_id,
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
                                &module.manifest.module_id,
                                "startup_checks",
                                &check.name,
                                &reason,
                            );
                            continue;
                        }
                        return Err(lifecycle_validation_error(
                            &module.manifest.module_id,
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
                            &module.manifest.module_id,
                            "startup_checks",
                            &check.name,
                            &reason,
                        );
                        continue;
                    }
                    return Err(lifecycle_validation_error(
                        &module.manifest.module_id,
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
                    job.name, job.function_name, module.manifest.module_id
                );
                if !job.required {
                    warn_optional_lifecycle_skip(
                        &module.manifest.module_id,
                        "activation_jobs",
                        &job.name,
                        &reason,
                    );
                    continue;
                }
                return Err(lifecycle_validation_error(
                    &module.manifest.module_id,
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
                        &module.manifest.module_id,
                        "activation_jobs",
                        &job.name,
                        &reason,
                    );
                    continue;
                }
                return Err(lifecycle_validation_error(
                    &module.manifest.module_id,
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

/// Build host-owned runtime schedules declared by loaded modules.
pub fn scheduled_functions(
    modules: &[Module],
    registry: &FunctionRegistry,
) -> platform_core::AppResult<Vec<ScheduledFunctionDefinition>> {
    let mut schedules = Vec::new();

    for module in modules {
        if !matches!(module.load_status, ModuleLoadStatus::Loaded) {
            continue;
        }
        let Some(runtime) = &module.manifest.runtime else {
            continue;
        };

        for schedule in &runtime.schedules {
            if schedule.name.trim().is_empty() {
                return Err(AppError::new(
                    ErrorCode::Validation,
                    format!(
                        "scheduled runtime function for module {} is missing a name",
                        module.manifest.module_id
                    ),
                ));
            }
            if !module_declares_runtime_function(module, &schedule.function_name) {
                return Err(AppError::new(
                    ErrorCode::Validation,
                    format!(
                        "scheduled runtime function {}:{} references function {} not declared by module {}",
                        module.manifest.module_id,
                        schedule.name,
                        schedule.function_name,
                        module.manifest.module_id
                    ),
                ));
            }
            let Some(function) = registry.get(&schedule.function_name) else {
                return Err(AppError::new(
                    ErrorCode::Validation,
                    format!(
                        "scheduled runtime function {}:{} references missing function {}",
                        module.manifest.module_id, schedule.name, schedule.function_name
                    ),
                ));
            };
            let parsed_schedule = CronSchedule::parse(&schedule.cron).map_err(|error| {
                AppError::new(
                    ErrorCode::Validation,
                    format!(
                        "scheduled runtime function {}:{} has invalid cron expression: {error}",
                        module.manifest.module_id, schedule.name
                    ),
                )
            })?;
            schedules.push(ScheduledFunctionDefinition {
                schedule_key: format!("{}:{}", module.manifest.module_id, schedule.name),
                module_name: module.manifest.module_id.clone(),
                schedule_name: schedule.name.clone(),
                function_name: schedule.function_name.clone(),
                cron: schedule.cron.clone(),
                schedule: parsed_schedule,
                input_json: schedule.input.clone(),
                max_attempts: runtime_max_attempts_for_enqueue(function.retry_policy.max_attempts),
            });
        }
    }

    Ok(schedules)
}

/// Build an [`EventHandlerRegistry`] from every module's binding.
#[must_use]
pub fn event_handlers(modules: &[Module]) -> EventHandlerRegistry {
    event_handlers_with_context(modules, &EventHandlerRegistrationContext::empty())
}

/// Build an [`EventHandlerRegistry`] with host runtime actions enabled for
/// provider event-handler result actions.
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
    let profile = CompositionProfile::from_config(&ctx.config)?;
    if !linked_module_enabled(ctx, story::module::MODULE_NAME) {
        story::backend::install_default_story_display(Vec::new());
        return Ok(());
    }
    let mut descriptors = story_display_descriptors_for_context(ctx)?;
    descriptors.extend(
        host_linked_modules_for_context(ctx, composition, profile)
            .into_iter()
            .flat_map(|entry| story_display_descriptors_from_manifest((entry.manifest)())),
    );
    story::backend::install_default_story_display(descriptors);
    Ok(())
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
    let host_module_enabled_descriptors = host_linked_modules_not_in_profile(composition, profile)
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
    let module_descriptors = linked_module_entries(profile)
        .iter()
        .filter(|entry| linked_module_enabled_from_config(&ctx.config, entry.module_name))
        .map(|entry| (entry.load)(ctx))
        .chain(
            host_linked_modules_for_config(&ctx.config, composition, profile)
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
            host_linked_modules_for_config(&ctx.config, composition, profile)
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
    use auth::models::AuthUserId;
    use auth::session_policy::{
        AuthHostExtension, AuthSessionPolicy, SessionCreateDecision, SessionCreateInput,
    };
    use platform_core::{
        AppConfig, AuthConfig, DatabaseConfig, ErrorCode, ExecutionContext, HttpConfig,
        LoggingEventPublisher, ModuleConfig, ModuleSourcesConfig, PLATFORM_MIGRATIONS, RedisConfig,
        RuntimeConfigProvider, RuntimeConfigRegistry, RuntimeConfigSnapshot, ServiceConfig,
        TelemetryConfig, apply_migrations,
    };
    use platform_module::{
        LifecycleActivationJobDeclaration, LifecycleStartupCheckDeclaration, LifecycleSurface,
        ModuleManifestLintSeverity, RuntimeFunctionDeclaration, RuntimeSurface,
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
                    Some(entry.module_name),
                    (entry.manifest)().module_id.rsplit('/').next(),
                    "linked module entry slug must match the local ModuleManifest ID segment"
                );
            }
        }
    }

    #[test]
    fn core_profile_excludes_demo_linked_modules() {
        let names = module_manifests_for_profile(CompositionProfile::Core)
            .into_iter()
            .map(|manifest| manifest.module_id)
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["lenso/platform-story"]);
    }

    #[test]
    fn demo_profile_includes_fixture_linked_modules() {
        let names = module_manifests_for_profile(CompositionProfile::Demo)
            .into_iter()
            .map(|manifest| manifest.module_id)
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "lenso/auth",
                "lenso/auth-anonymous",
                "lenso/auth-oauth",
                "lenso/auth-password",
                "lenso/auth-phone",
                "lenso/auth-github",
                "lenso/auth-google",
                "lenso/auth-oidc",
                "lenso/platform-story",
            ]
        );
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
        assert!(names.iter().any(|name| name.starts_with("story/")));
        assert!(!names.iter().any(|name| name.starts_with("auth/")));
        assert!(!names.iter().any(|name| name.starts_with("auth-oauth/")));
        assert!(!names.iter().any(|name| name.starts_with("auth-github/")));
        assert!(!names.iter().any(|name| name.starts_with("auth-google/")));
        assert!(!names.iter().any(|name| name.starts_with("auth-password/")));
        assert!(!names.iter().any(|name| name.starts_with("auth-phone/")));
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
                .any(|name| name == &"auth-oauth/0001_create_auth_oauth_schema")
        );
        assert!(
            names
                .iter()
                .any(|name| name == &"auth-password/0001_create_auth_password_schema")
        );
        assert!(
            names
                .iter()
                .any(|name| name == &"auth-phone/0001_create_auth_phone_schema")
        );
        assert!(
            names
                .iter()
                .any(|name| name == &"auth-github/0001_create_auth_github_schema")
        );
        assert!(
            names
                .iter()
                .any(|name| name == &"auth-google/0001_create_auth_google_schema")
        );
        assert!(
            names
                .iter()
                .any(|name| name == &"auth-oidc/0001_create_auth_oidc_schema")
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
            .with_linked_module(auth_oauth_linked_module())
            .with_linked_module(auth_password_linked_module())
            .with_linked_module(auth_phone_linked_module())
            .with_linked_module(auth_github_linked_module())
            .with_linked_module(auth_google_linked_module())
            .with_linked_module(auth_oidc_linked_module());

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
                .any(|name| name == &"auth-oauth/0001_create_auth_oauth_schema")
        );
        assert!(
            names
                .iter()
                .any(|name| name == &"auth-password/0001_create_auth_password_schema")
        );
        assert!(
            names
                .iter()
                .any(|name| name == &"auth-phone/0001_create_auth_phone_schema")
        );
        assert!(
            names
                .iter()
                .any(|name| name == &"auth-github/0001_create_auth_github_schema")
        );
        assert!(
            names
                .iter()
                .any(|name| name == &"auth-google/0001_create_auth_google_schema")
        );
        assert!(
            names
                .iter()
                .any(|name| name == &"auth-oidc/0001_create_auth_oidc_schema")
        );
    }

    #[tokio::test]
    async fn auth_phone_linked_module_declares_routes_runtime_config_and_migrations() {
        let linked = auth_phone_linked_module();
        let manifest = (linked.manifest)();
        let binding = linked
            .http_binding
            .expect("auth-phone should expose HTTP binding")();
        let module =
            (linked
                .load
                .expect("auth-phone should load as linked module"))(&AppContext::new(
                test_config_with_database_url("postgres://localhost/lenso_test"),
                platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
                    .expect("lazy pool should build"),
                Arc::new(LoggingEventPublisher),
            ));

        assert_eq!(linked.module_name, auth_phone::module::MODULE_NAME);
        assert_eq!(manifest.module_id, "lenso/auth-phone");
        assert_eq!(
            manifest
                .requires
                .iter()
                .map(|requirement| requirement.module_id.as_str())
                .collect::<Vec<_>>(),
            vec!["lenso/auth", "lenso/auth-password",]
        );
        assert!(
            manifest
                .http_routes
                .iter()
                .any(|route| route.path == "/v1/auth/phone/otp/start")
        );
        assert!(
            manifest
                .http_routes
                .iter()
                .any(|route| route.path == "/v1/auth/phone/password/login")
        );
        assert_eq!(
            binding
                .http
                .expect("auth-phone HTTP contribution")
                .public_prefixes,
            &["/v1/auth/phone/"]
        );
        assert!(
            linked
                .migrations
                .iter()
                .any(|migration| migration.name == "auth-phone/0001_create_auth_phone_schema")
        );
        assert!(
            module
                .runtime_config
                .iter()
                .any(|descriptor| descriptor.key == "auth-phone.otp_code_length")
        );
        assert!(
            module
                .runtime_config_groups
                .iter()
                .any(|group| group.id == "auth-phone.otp")
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
    async fn host_composition_skips_modules_already_in_linked_profile() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let config = test_config_with_database_url("postgres://localhost/lenso_test");
        let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));
        let composition = HostComposition::new().with_linked_module(auth_linked_module());

        let descriptors = runtime_config_descriptors_with_composition(&ctx, &composition)
            .expect("host composition descriptors should load");
        let auth_toggle_count = descriptors
            .iter()
            .filter(|descriptor| descriptor.key == "modules.auth.enabled")
            .count();

        assert_eq!(auth_toggle_count, 1);
        RuntimeConfigRegistry::try_new(descriptors).expect("descriptors should be unique");
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
            .map(|module| module.manifest.module_id)
            .collect::<Vec<_>>();

        assert!(names.iter().any(|name| name == "fixture/billing"));
    }

    #[tokio::test]
    async fn host_wiring_collects_auth_session_policy_contributions() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let config = test_config_with_database_url("postgres://localhost/lenso_test");
        let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));
        let composition = HostComposition::new().with_linked_module(
            test_host_linked_module()
                .with_contribution(AuthHostExtension::session_policy(test_session_policy)),
        );

        let wiring = host_wiring_for_context_with_composition(&ctx, &composition)
            .expect("host wiring should compose");
        let now = ctx.clock.now();
        let decision = wiring
            .auth_session_policy()
            .policy()
            .before_session_create(&SessionCreateInput {
                user_id: AuthUserId("usr_wiring".to_owned()),
                session_id: "sess_wiring".to_owned(),
                proposed_device_id: Some("device_wiring".to_owned()),
                created_at: now,
                expires_at: now,
                client: Default::default(),
            })
            .await
            .expect("wired policy should allow session");

        assert_eq!(decision.device_id.as_deref(), Some("device_from_wiring"));
    }

    fn test_session_policy(_ctx: &AppContext) -> Arc<dyn AuthSessionPolicy> {
        Arc::new(TestSessionPolicy)
    }

    #[derive(Debug)]
    struct TestSessionPolicy;

    #[async_trait]
    impl AuthSessionPolicy for TestSessionPolicy {
        async fn before_session_create(
            &self,
            input: &SessionCreateInput,
        ) -> platform_core::AppResult<SessionCreateDecision> {
            assert_eq!(input.proposed_device_id.as_deref(), Some("device_wiring"));
            Ok(SessionCreateDecision {
                device_id: Some("device_from_wiring".to_owned()),
            })
        }
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
            .map(|manifest| manifest.module_id)
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "lenso/auth",
                "lenso/auth-anonymous",
                "lenso/auth-oauth",
                "lenso/auth-password",
                "lenso/auth-phone",
                "lenso/auth-github",
                "lenso/auth-google",
                "lenso/auth-oidc",
                "lenso/platform-story",
            ]
        );
    }

    #[test]
    fn linked_http_route_owners_are_profile_aware() {
        assert!(linked_http_route_owners_for_profile(CompositionProfile::Core).is_empty());
        assert_eq!(
            linked_http_route_owners_for_profile(CompositionProfile::Demo),
            vec![
                LinkedHttpRouteOwner {
                    module_name: "lenso/auth".to_owned(),
                    public_prefixes: &["/v1/auth/dev/", "/v1/auth/sessions/"],
                },
                LinkedHttpRouteOwner {
                    module_name: "lenso/auth-anonymous".to_owned(),
                    public_prefixes: &["/v1/auth/anonymous/"],
                },
                LinkedHttpRouteOwner {
                    module_name: "lenso/auth-password".to_owned(),
                    public_prefixes: &["/v1/auth/password/"],
                },
                LinkedHttpRouteOwner {
                    module_name: "lenso/auth-phone".to_owned(),
                    public_prefixes: &["/v1/auth/phone/"],
                },
                LinkedHttpRouteOwner {
                    module_name: "lenso/auth-github".to_owned(),
                    public_prefixes: &["/v1/auth/github/"],
                },
                LinkedHttpRouteOwner {
                    module_name: "lenso/auth-google".to_owned(),
                    public_prefixes: &["/v1/auth/google/"],
                },
                LinkedHttpRouteOwner {
                    module_name: "lenso/auth-oidc".to_owned(),
                    public_prefixes: &["/.well-known/", "/oauth/"],
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
            .map(|module| module.manifest.module_id)
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["lenso/platform-story"]);
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
    async fn auth_linked_providers_require_auth_module() {
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
            .map(|module| module.manifest.module_id)
            .collect::<Vec<_>>();

        assert!(!names.iter().any(|name| name == "lenso/auth-oauth"));
        assert!(!names.iter().any(|name| name == "lenso/auth-anonymous"));
        assert!(!names.iter().any(|name| name == "lenso/auth-password"));
        assert!(!names.iter().any(|name| name == "lenso/auth-phone"));
        assert!(!names.iter().any(|name| name == "lenso/auth-github"));
        assert!(!names.iter().any(|name| name == "lenso/auth-google"));
        assert!(!names.iter().any(|name| name == "lenso/auth-oidc"));
    }

    #[tokio::test]
    async fn auth_github_requires_oauth_substrate() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let mut config = test_config_with_database_url("postgres://localhost/lenso_test");
        config.modules.insert(
            auth_oauth::module::MODULE_NAME.to_owned(),
            ModuleConfig {
                enabled: Some(false),
                values: BTreeMap::new(),
            },
        );
        let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));

        let names = modules_for_config(&ctx)
            .expect("demo profile")
            .into_iter()
            .map(|module| module.manifest.module_id)
            .collect::<Vec<_>>();

        assert!(!names.iter().any(|name| name == "lenso/auth-oauth"));
        assert!(!names.iter().any(|name| name == "lenso/auth-github"));
        assert!(!names.iter().any(|name| name == "lenso/auth-google"));
        assert!(names.iter().any(|name| name == "lenso/auth-password"));
        assert!(names.iter().any(|name| name == "lenso/auth-phone"));
        assert!(names.iter().any(|name| name == "lenso/auth-oidc"));
    }

    #[tokio::test]
    async fn auth_google_requires_oauth_substrate() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let mut config = test_config_with_database_url("postgres://localhost/lenso_test");
        config.modules.insert(
            auth_oauth::module::MODULE_NAME.to_owned(),
            ModuleConfig {
                enabled: Some(false),
                values: BTreeMap::new(),
            },
        );
        let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));

        let names = modules_for_config(&ctx)
            .expect("demo profile")
            .into_iter()
            .map(|module| module.manifest.module_id)
            .collect::<Vec<_>>();

        assert!(!names.iter().any(|name| name == "lenso/auth-oauth"));
        assert!(!names.iter().any(|name| name == "lenso/auth-github"));
        assert!(!names.iter().any(|name| name == "lenso/auth-google"));
        assert!(names.iter().any(|name| name == "lenso/auth-password"));
        assert!(names.iter().any(|name| name == "lenso/auth-phone"));
        assert!(names.iter().any(|name| name == "lenso/auth-oidc"));
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
    async fn auth_session_cache_factory_returns_no_cache_in_database_mode() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let config = test_config_with_database_url("postgres://localhost/lenso_test");
        let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));

        assert!(auth::redis_cache::session_cache_from_context(&ctx).is_none());
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
            .map(|module| module.manifest.module_id)
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "lenso/auth",
                "lenso/auth-anonymous",
                "lenso/auth-oauth",
                "lenso/auth-github",
                "lenso/auth-google",
                "lenso/auth-oidc",
                "lenso/platform-story",
            ]
        );
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
            .map(|module| module.manifest.module_id)
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "lenso/auth",
                "lenso/auth-anonymous",
                "lenso/auth-oauth",
                "lenso/auth-github",
                "lenso/auth-google",
                "lenso/auth-oidc",
                "lenso/platform-story",
            ]
        );
        let linked_http_names = linked_http_modules_for_context(&ctx)
            .expect("linked HTTP modules should load")
            .into_iter()
            .map(|module| module.manifest.module_id)
            .collect::<Vec<_>>();

        assert_eq!(
            linked_http_names,
            vec![
                "lenso/auth",
                "lenso/auth-anonymous",
                "lenso/auth-github",
                "lenso/auth-google",
                "lenso/auth-oidc",
            ]
        );
    }

    #[tokio::test]
    async fn story_module_runtime_config_disables_linked_http() {
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
            .map(|module| module.manifest.module_id)
            .collect::<Vec<_>>();
        assert_eq!(
            linked_http_names,
            vec![
                "lenso/auth",
                "lenso/auth-anonymous",
                "lenso/auth-password",
                "lenso/auth-phone",
                "lenso/auth-github",
                "lenso/auth-google",
                "lenso/auth-oidc",
            ]
        );
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
            key == "modules.auth-anonymous.enabled"
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
            key == "modules.auth-phone.enabled"
                && *group == Some("modules")
                && *restart_only
                && default == &json!(true)
        }));
        assert!(keys.iter().any(|(key, group, restart_only, default)| {
            key == "modules.auth-oauth.enabled"
                && *group == Some("modules")
                && *restart_only
                && default == &json!(true)
        }));
        assert!(keys.iter().any(|(key, group, restart_only, default)| {
            key == "modules.auth-github.enabled"
                && *group == Some("modules")
                && *restart_only
                && default == &json!(true)
        }));
        assert!(keys.iter().any(|(key, group, restart_only, default)| {
            key == "modules.auth-google.enabled"
                && *group == Some("modules")
                && *restart_only
                && default == &json!(true)
        }));
        assert!(keys.iter().any(|(key, group, restart_only, default)| {
            key == "modules.auth-oidc.enabled"
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
        assert!(groups.contains(&("auth-phone.otp", "Phone OTP")));
        assert!(!groups.iter().any(|(id, _)| *id == "auth-phone.password"));
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
        assert!(!names.iter().any(|name| name.starts_with("auth-phone/")));
        assert!(
            names
                .iter()
                .any(|name| name == &"auth/0001_create_auth_schema")
        );
        assert!(
            names
                .iter()
                .any(|name| name == &"auth-oauth/0001_create_auth_oauth_schema")
        );
        assert!(
            names
                .iter()
                .any(|name| name == &"auth-github/0001_create_auth_github_schema")
        );
        assert!(
            names
                .iter()
                .any(|name| name == &"auth-google/0001_create_auth_google_schema")
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
            .map(|module| module.manifest.module_id)
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "lenso/auth",
                "lenso/auth-anonymous",
                "lenso/auth-github",
                "lenso/auth-google",
                "lenso/auth-oidc",
            ]
        );
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
            .map(|module| module.manifest.module_id)
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "lenso/auth",
                "lenso/auth-anonymous",
                "lenso/auth-password",
                "lenso/auth-phone",
                "lenso/auth-github",
                "lenso/auth-google",
                "lenso/auth-oidc",
            ]
        );

        let migration_names = migrations_for_config(&config)
            .expect("demo linked profile should parse")
            .into_iter()
            .map(|migration| migration.name)
            .collect::<Vec<_>>();
        assert!(
            !migration_names
                .iter()
                .any(|name| name.starts_with("story/")),
            "disabled Story module must not install aggregation tables"
        );
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
                    module_name: "lenso/auth".to_owned(),
                    public_prefixes: &["/v1/auth/dev/", "/v1/auth/sessions/"],
                },
                LinkedHttpRouteOwner {
                    module_name: "lenso/auth-anonymous".to_owned(),
                    public_prefixes: &["/v1/auth/anonymous/"],
                },
                LinkedHttpRouteOwner {
                    module_name: "lenso/auth-password".to_owned(),
                    public_prefixes: &["/v1/auth/password/"],
                },
                LinkedHttpRouteOwner {
                    module_name: "lenso/auth-phone".to_owned(),
                    public_prefixes: &["/v1/auth/phone/"],
                },
                LinkedHttpRouteOwner {
                    module_name: "lenso/auth-github".to_owned(),
                    public_prefixes: &["/v1/auth/github/"],
                },
                LinkedHttpRouteOwner {
                    module_name: "lenso/auth-google".to_owned(),
                    public_prefixes: &["/v1/auth/google/"],
                },
                LinkedHttpRouteOwner {
                    module_name: "lenso/auth-oidc".to_owned(),
                    public_prefixes: &["/.well-known/", "/oauth/"],
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
                module.manifest.module_id
            );
            for route in &module.manifest.http_routes {
                assert!(
                    http.public_prefixes
                        .iter()
                        .any(|prefix| route.path.starts_with(prefix)),
                    "linked HTTP module `{}` declares manifest route `{}` outside its public prefixes",
                    module.manifest.module_id,
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
                .find(|manifest| manifest.module_id == module.manifest.module_id)
                .unwrap_or_else(|| {
                    panic!(
                        "linked HTTP module `{}` is missing from module_manifests",
                        module.manifest.module_id
                    )
                });
            assert_eq!(
                registered_manifest, &module.manifest,
                "linked HTTP module `{}` must use the registered ModuleManifest",
                module.manifest.module_id
            );
        }
    }

    #[test]
    fn linked_http_routes_exclude_retired_story_admin_routes() {
        let document = merge_linked_http(platform_http::OpenApiRouter::new()).to_openapi();
        let value = serde_json::to_value(document).expect("OpenAPI document should serialize");
        let paths = value["paths"].as_object().expect("OpenAPI paths object");

        assert!(paths.keys().all(|path| !path.starts_with("/admin/")));
    }

    #[test]
    fn platform_story_manifest_declares_story_console_surface() {
        let manifest = module_manifests()
            .into_iter()
            .find(|manifest| manifest.module_id == "lenso/platform-story")
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
        assert_eq!(manifest.module_id, console_surface_contract["id"]);
        assert_eq!(surface.name, console_surface_contract["surfaceName"]);
        assert_eq!(surface.label, console_surface_contract["label"]);
        assert_eq!(surface.route, console_surface_contract["route"]);
        assert_eq!(
            surface_json["presentation"],
            console_surface_contract["presentation"]
        );
        assert_eq!(surface_json["icon"], console_surface_contract["icon"]);
        assert_eq!(surface.navigation, None);
        assert!(console_surface_contract.get("navigation").is_none());
        assert_eq!(
            surface.required_capabilities,
            required_capabilities_from_contract(&console_surface_contract)
        );

        let lints = lint_module_manifest(&manifest);
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
            "module_lifecycle:fixture/test-module:warm cache"
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
            Some("module.fixture/test-module.lifecycle.activation_jobs.warm cache")
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
            Some("module.fixture/test-module.lifecycle.startup_checks.function registered")
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
            Some("module.fixture/test-module.lifecycle.startup_checks.function registered")
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
            Some("module.fixture/test-module.lifecycle.startup_checks.capability declared")
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
            Some("module.fixture/test-module.lifecycle.activation_jobs.warm cache")
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
            let mut manifest =
                ModuleManifest::builder("fixture/test-module").lifecycle(builder.lifecycle);
            if builder.declare_runtime_function {
                manifest = manifest.runtime(RuntimeSurface {
                    functions: vec![RuntimeFunctionDeclaration {
                        name: LIFECYCLE_FUNCTION_NAME.to_owned(),
                        version: 1,
                        queue: "test".to_owned(),
                        input_schema: None,
                        retry_policy: None,
                        operation: None,
                    }],
                    schedules: vec![],
                    workflows: vec![],
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

    const TEST_HOST_MIGRATIONS: &[Migration] = &[Migration {
        name: "billing/0001_init",
        sql: "select 1;",
    }];

    fn test_host_manifest() -> ModuleManifest {
        ModuleManifest::builder("fixture/billing").build()
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
