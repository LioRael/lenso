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
//! - [`merge_linked_http`]: context-free HTTP routes and their OpenAPI docs
//!   (API only), assembled without a live [`AppContext`].
//! - [`story_display_descriptors`]: console display metadata, sourced from the
//!   context-free [`module_manifests`].
//!
//! When adding a module, register it in [`modules`] and [`module_manifests`]
//! and — if it has them — in [`merge_linked_http`].

use platform_admin_data::{AdminModule, AdminModuleMetadata};
use platform_core::{
    AppContext, EventHandlerRegistry, RuntimeConfigDescriptor, StoryDisplayDescriptor,
};
use platform_http::ApiOpenApiRouter;
use platform_module::{
    AdminSchema, AdminSurface, LinkedBinding, Module, ModuleLoadStatus, ModuleManifest,
    ModuleSource,
};
use platform_module_remote::{RemoteHttpProxyRegistry, RemoteModuleConfig, RemoteModuleSource};
use platform_runtime::FunctionRegistry;

struct LinkedModuleEntry {
    module_name: &'static str,
    manifest: fn() -> ModuleManifest,
    load: fn(&AppContext) -> Module,
    http_binding: Option<fn() -> LinkedBinding>,
}

const LINKED_MODULE_ENTRIES: &[LinkedModuleEntry] = &[
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
];

/// The authoritative list of loaded modules (context-bound: builds bindings).
///
/// The only function that enumerates concrete modules for the running apps.
#[must_use]
pub fn modules(ctx: &AppContext) -> Vec<Module> {
    LINKED_MODULE_ENTRIES
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
    LINKED_MODULE_ENTRIES
        .iter()
        .map(|entry| (entry.manifest)())
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
    LINKED_MODULE_ENTRIES
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
    LINKED_MODULE_ENTRIES
        .iter()
        .filter_map(|entry| {
            let http_binding = entry.http_binding?;
            Some(Module::linked((entry.manifest)(), http_binding()))
        })
        .collect()
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

/// Load registry metadata for every configured module, including modules with
/// no admin surface and custom surfaces not consumable by schema-admin
/// list/detail.
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
        .map(|module| {
            let ModuleManifest {
                name,
                admin,
                http_routes,
                ..
            } = module.manifest;
            AdminModuleMetadata {
                module_name: name,
                source: module.source,
                load_status: module.load_status,
                http_routes,
                admin,
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

/// Merge every linked module's HTTP routes (and their `OpenAPI` docs) onto `base`.
///
/// Linked route builders are context-free, so this assembles the HTTP surface
/// without constructing the full module set (which requires an [`AppContext`])
/// — usable both for serving and for standalone `OpenAPI` document assembly.
/// This is the single source for linked API routes until HTTP joins the
/// [`platform_module::ModuleBinding`] seam.
pub fn merge_linked_http(base: ApiOpenApiRouter) -> ApiOpenApiRouter {
    linked_http_modules()
        .into_iter()
        .filter_map(|module| module.linked_http)
        .fold(base, |router, contribution| (contribution.merge)(router))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linked_module_entry_names_match_manifests() {
        for entry in LINKED_MODULE_ENTRIES {
            assert_eq!(
                entry.module_name,
                (entry.manifest)().name,
                "linked module entry name must match ModuleManifest::name"
            );
        }
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
}
