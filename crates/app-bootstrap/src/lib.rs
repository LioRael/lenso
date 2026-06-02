//! Composition root: the single place that knows which domains exist.
//!
//! Both the API and the worker assemble their domain wiring from this crate, so
//! a domain is registered here once rather than in scattered per-app edits.
//!
//! A domain's contributions are split by how they are consumed:
//! - [`domains`]: context-bound runtime functions and event handlers (API +
//!   worker). The authoritative descriptor list.
//! - [`merge_domain_http`]: context-free HTTP routes and their OpenAPI docs
//!   (API only), assembled without a live [`AppContext`].
//! - [`story_display_descriptors`]: console display metadata (read-only queries
//!   without an [`AppContext`]).
//!
//! When adding a domain, register it in [`domains`] and — if it has them — in
//! [`merge_domain_http`] and [`story_display_descriptors`].

use platform_core::{AppContext, EventHandlerRegistry, SettingDescriptor, StoryDisplayDescriptor};
use platform_domain::DomainDescriptor;
use platform_http::ApiOpenApiRouter;
use platform_runtime::FunctionRegistry;

/// The authoritative list of domains wired into the platform.
///
/// This is the only function that enumerates concrete domains. Every app and
/// the runtime console story metadata derive their domain set from here.
#[must_use]
pub fn domains(ctx: &AppContext) -> Vec<DomainDescriptor> {
    vec![identity::domain(ctx), notifications::module::domain(ctx)]
}

/// Build a [`FunctionRegistry`] from every domain's runtime descriptor.
#[must_use]
pub fn function_registry(domains: &[DomainDescriptor]) -> FunctionRegistry {
    let mut registry = FunctionRegistry::default();
    for domain in domains {
        domain.runtime.register_into(&mut registry);
    }
    registry
}

/// Build an [`EventHandlerRegistry`] from every domain's event handlers.
#[must_use]
pub fn event_handlers(domains: &[DomainDescriptor]) -> EventHandlerRegistry {
    let mut registry = EventHandlerRegistry::new();
    for domain in domains {
        registry.register_all(domain.event_handlers.clone());
    }
    registry
}

/// Merge every domain's HTTP routes (and their `OpenAPI` docs) onto `base`.
///
/// Domain route builders are context-free, so this assembles the HTTP surface
/// without constructing the full descriptor set (which requires an
/// [`AppContext`]) — usable both for serving and for standalone `OpenAPI`
/// document assembly. This is the single source for the API's domain routes;
/// kept in sync with [`domains`] by listing the same domains.
pub fn merge_domain_http(base: ApiOpenApiRouter) -> ApiOpenApiRouter {
    base.merge(identity::routes::router())
}

/// Story-display descriptors for every domain.
///
/// Replaces hard-coded per-domain chains: read-only console queries (which have
/// no [`AppContext`]) iterate this instead of enumerating domains themselves.
pub fn story_display_descriptors() -> impl Iterator<Item = &'static StoryDisplayDescriptor> {
    identity::module::STORY_DISPLAY
        .iter()
        .chain(notifications::module::STORY_DISPLAY.iter())
}

/// Every domain's setting descriptors, plus any platform-owned descriptors.
///
/// The single source for the editable configuration registry. Apps build a
/// `SettingsRegistry` from this list at startup.
#[must_use]
pub fn setting_descriptors(ctx: &AppContext) -> Vec<SettingDescriptor> {
    domains(ctx)
        .iter()
        .flat_map(|domain| domain.settings.iter().cloned())
        .collect()
}
