//! Composition root: the single place that knows which modules exist.
//!
//! Both the API and the worker assemble their module wiring from this crate, so
//! a module is registered here once rather than in scattered per-app edits.
//!
//! A module's contributions are split by how they are consumed:
//! - [`modules`]: context-bound bindings (runtime functions + event handlers)
//!   and runtime config (API + worker). The authoritative module list.
//! - [`module_manifests`]: context-free manifest data (no [`AppContext`]) for
//!   read-only / `OpenAPI` paths.
//! - [`merge_domain_http`]: context-free HTTP routes and their OpenAPI docs
//!   (API only), assembled without a live [`AppContext`].
//! - [`story_display_descriptors`]: console display metadata, sourced from the
//!   context-free [`module_manifests`].
//!
//! When adding a module, register it in [`modules`] and [`module_manifests`]
//! and — if it has them — in [`merge_domain_http`].

use platform_admin_data::{AdminModule, AdminModuleMetadata};
use platform_core::{
    AppContext, EventHandlerRegistry, RuntimeConfigDescriptor, StoryDisplayDescriptor,
};
use platform_http::ApiOpenApiRouter;
use platform_module::{
    AdminSchema, AdminSurface, Module, ModuleLoadStatus, ModuleManifest, ModuleSource,
};
use platform_module_remote::{RemoteModuleConfig, RemoteModuleSource};
use platform_runtime::FunctionRegistry;

/// The authoritative list of loaded modules (context-bound: builds bindings).
///
/// The only function that enumerates concrete modules for the running apps.
#[must_use]
pub fn modules(ctx: &AppContext) -> Vec<Module> {
    vec![
        identity::module::module(ctx),
        notifications::module::module(ctx),
    ]
}

/// Load every configured module, including out-of-process remote modules.
///
/// The synchronous [`modules`] function remains Linked-only for call sites that
/// must stay context-local and infallible. Startup paths that can perform IO
/// should use this async loader.
pub async fn load_modules(ctx: &AppContext) -> platform_core::AppResult<Vec<Module>> {
    let mut loaded = modules(ctx);

    for remote in &ctx.config.module_sources.remote {
        let source = RemoteModuleSource::new(remote_module_config(remote))?;
        loaded.push(source.load().await?);
    }

    Ok(loaded)
}

/// Context-free module manifests for read-only / OpenAPI paths that have no
/// [`AppContext`]. Kept in sync with [`modules`] by listing the same modules.
#[must_use]
pub fn module_manifests() -> Vec<ModuleManifest> {
    vec![
        identity::module::manifest(),
        notifications::module::manifest(),
    ]
}

/// Aggregate schema-admin capable modules: those declaring an
/// `AdminSurface::Schema` AND providing an `AdminDataSource`. Modules without a
/// schema data source are filtered out — "optional capability" semantics.
#[must_use]
pub fn admin_modules(ctx: &AppContext) -> Vec<AdminModule> {
    admin_modules_from_modules(modules(ctx))
}

/// Load schema-admin capable modules, including configured remotes.
pub async fn load_admin_modules(ctx: &AppContext) -> platform_core::AppResult<Vec<AdminModule>> {
    let mut admin_modules = admin_modules_from_modules(modules(ctx));

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

/// Load admin-surface metadata for every module that declares an admin surface,
/// including custom surfaces that are not consumable by schema-admin list/detail.
pub async fn load_admin_module_metadata(
    ctx: &AppContext,
) -> platform_core::AppResult<Vec<AdminModuleMetadata>> {
    let mut metadata = admin_metadata_from_modules(modules(ctx));

    for remote in &ctx.config.module_sources.remote {
        let source = RemoteModuleSource::new(remote_module_config(remote))?;
        match source.load().await {
            Ok(module) => metadata.extend(admin_metadata_from_modules(vec![module])),
            Err(error) => metadata.push(failed_remote_admin_metadata(
                remote.name.clone(),
                error.public_message,
            )),
        }
    }

    Ok(metadata)
}

fn admin_modules_from_modules(modules: Vec<Module>) -> Vec<AdminModule> {
    modules
        .into_iter()
        .filter_map(|module| {
            // `modules(ctx)` yields owned Modules — move the fields out.
            let data_source = module.admin_data?;
            let ModuleManifest { name, admin, .. } = module.manifest;
            let (schema, listed_in_schema) = match admin? {
                AdminSurface::Schema(schema) => (schema, true),
                AdminSurface::DeclarativeCustom(surface) => (surface.fallback_schema?, false),
                AdminSurface::EmbeddedCustom(_) => return None,
                _ => return None,
            };
            Some(AdminModule {
                module_name: name,
                source: module.source,
                load_status: module.load_status,
                schema,
                listed_in_schema,
                data_source: Some(data_source),
            })
        })
        .collect()
}

fn admin_metadata_from_modules(modules: Vec<Module>) -> Vec<AdminModuleMetadata> {
    modules
        .into_iter()
        .filter_map(|module| {
            let ModuleManifest {
                name,
                admin,
                http_routes,
                ..
            } = module.manifest;
            admin.map(|surface| AdminModuleMetadata {
                module_name: name,
                source: module.source,
                load_status: module.load_status,
                http_routes,
                admin: Some(surface),
            })
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
        listed_in_schema: true,
        data_source: None,
    }
}

fn failed_remote_admin_metadata(name: String, message: String) -> AdminModuleMetadata {
    AdminModuleMetadata {
        module_name: name,
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Error { message },
        http_routes: Vec::new(),
        admin: None,
    }
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

/// Build a [`FunctionRegistry`] from every module's binding.
#[must_use]
pub fn function_registry(modules: &[Module]) -> FunctionRegistry {
    let mut registry = FunctionRegistry::default();
    for module in modules {
        module.binding.register_functions(&mut registry);
    }
    registry
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

/// Merge every domain's HTTP routes (and their `OpenAPI` docs) onto `base`.
///
/// Domain route builders are context-free, so this assembles the HTTP surface
/// without constructing the full module set (which requires an [`AppContext`])
/// — usable both for serving and for standalone `OpenAPI` document assembly.
/// This is the single source for the API's routes; kept in sync with
/// [`modules`] manually (it still hardcodes `identity::routes::router()` — HTTP
/// is deferred). The `domain` in the name is retained until HTTP joins the
/// [`platform_module::ModuleBinding`] seam; every other path here is unified on
/// "module".
pub fn merge_domain_http(base: ApiOpenApiRouter) -> ApiOpenApiRouter {
    base.merge(identity::routes::router())
}

/// Story-display descriptors for every module. Sourced from context-free
/// manifests so the `OpenAPI` path stays pure (no [`AppContext`]).
#[must_use]
pub fn story_display_descriptors() -> Vec<StoryDisplayDescriptor> {
    module_manifests()
        .into_iter()
        .flat_map(|manifest| manifest.story_display)
        .collect()
}

/// Every module's setting descriptors.
///
/// The single source for the editable configuration registry. Apps build a
/// `RuntimeConfigRegistry` from this list at startup.
#[must_use]
pub fn runtime_config_descriptors(ctx: &AppContext) -> Vec<RuntimeConfigDescriptor> {
    let module_descriptors = modules(ctx)
        .iter()
        .flat_map(|module| module.runtime_config.iter().cloned())
        .collect::<Vec<_>>();
    // Platform-owned descriptors (e.g. worker knobs) plus every module's; keys
    // are globally unique, so chain order is presentation-only.
    platform_core::worker_runtime_config::RUNTIME_CONFIG
        .iter()
        .cloned()
        .chain(module_descriptors)
        .collect()
}
