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

use platform_core::{
    AppContext, EventHandlerRegistry, RuntimeConfigDescriptor, StoryDisplayDescriptor,
};
use platform_http::ApiOpenApiRouter;
use platform_module::{Module, ModuleManifest};
use platform_runtime::FunctionRegistry;

/// The authoritative list of loaded modules (context-bound: builds bindings).
///
/// The only function that enumerates concrete modules for the running apps.
#[must_use]
pub fn modules(ctx: &AppContext) -> Vec<Module> {
    vec![identity::module::module(ctx), notifications::module::module(ctx)]
}

/// Context-free module manifests for read-only / OpenAPI paths that have no
/// [`AppContext`]. Kept in sync with [`modules`] by listing the same modules.
#[must_use]
pub fn module_manifests() -> Vec<ModuleManifest> {
    vec![identity::module::manifest(), notifications::module::manifest()]
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
/// This is the single source for the API's domain routes; kept in sync with
/// [`modules`] manually (it still hardcodes `identity::routes::router()` — HTTP
/// is deferred).
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
