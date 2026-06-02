//! Runtime-observability API backing the Runtime Console.
//!
//! This is a platform cross-cutting concern, not a business domain: it only
//! reads platform/runtime tables (`platform.outbox`, `platform.story_events`,
//! `runtime.function_runs`) to observe the activity of every domain. It exposes
//! a single [`router`] mounted by the API app under `/admin/runtime/*`.
//!
//! The crate is split by responsibility:
//! - [`dto`]: request query params and response DTOs (re-exported for `OpenAPI`).
//! - [`handlers`]: Axum route handlers.
//! - [`rows`]: SQL row tuples/structs and their `From` conversions to DTOs.
//! - [`fetch`]: shared data-access helpers used by multiple handlers.
//! - [`stories`]: story graph assembly and naming.
//! - [`spans`]: telemetry-span → technical-operation mapping and PII redaction.
//! - [`support`]: small cross-cutting helpers (errors, pagination, limits).
//!
//! Story display names are domain-owned, so they are injected by the
//! composition root via [`install_story_display`] rather than depended on
//! directly — keeping this crate free of any business-domain dependency.

use platform_core::SettingsRegistry;
use platform_core::StoryDisplayDescriptor;
use platform_http::{ApiOpenApiRouter, OpenApiRouter, routes};
use std::sync::OnceLock;

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 100;

mod config_dto;
mod config_handlers;
mod dto;
mod fetch;
mod handlers;
mod rows;
mod spans;
mod stories;
mod support;

pub use config_dto::*;
#[allow(clippy::wildcard_imports)]
use config_handlers::*;
pub use dto::*;
#[allow(clippy::wildcard_imports)]
use fetch::*;
#[allow(clippy::wildcard_imports)]
use handlers::*;
#[allow(clippy::wildcard_imports)]
use rows::*;
#[allow(clippy::wildcard_imports)]
use spans::*;
#[allow(clippy::wildcard_imports)]
use stories::*;
#[allow(clippy::wildcard_imports)]
use support::*;

/// Domain-provided story-display catalog, injected by the composition root.
static STORY_DISPLAY: OnceLock<Vec<&'static StoryDisplayDescriptor>> = OnceLock::new();

/// Install the aggregated story-display descriptors from every domain.
///
/// Called once by the composition root before the router serves traffic. Story
/// display names are domain-owned metadata; injecting them keeps this crate
/// from depending on the domains or the composition root. Idempotent: later
/// calls are ignored.
pub fn install_story_display(catalog: Vec<&'static StoryDisplayDescriptor>) {
    let _ = STORY_DISPLAY.set(catalog);
}

static SETTINGS_REGISTRY: OnceLock<SettingsRegistry> = OnceLock::new();

/// Install the aggregated settings registry from the composition root. Idempotent.
pub fn install_settings_registry(registry: SettingsRegistry) {
    let _ = SETTINGS_REGISTRY.set(registry);
}

/// The installed registry, or an empty one if none was installed.
fn settings_registry() -> &'static SettingsRegistry {
    static EMPTY: OnceLock<SettingsRegistry> = OnceLock::new();
    SETTINGS_REGISTRY
        .get()
        .unwrap_or_else(|| EMPTY.get_or_init(SettingsRegistry::default))
}

pub fn router() -> ApiOpenApiRouter {
    OpenApiRouter::new()
        .routes(routes!(get_summary))
        .routes(routes!(get_timeline))
        .routes(routes!(get_heatmap))
        .routes(routes!(list_stories))
        .routes(routes!(get_story))
        .routes(routes!(get_story_technical_operations))
        .routes(routes!(get_execution_technical_operations))
        .routes(routes!(get_execution_payload))
        .routes(routes!(get_execution_logs))
        .routes(routes!(list_outbox))
        .routes(routes!(get_outbox_event))
        .routes(routes!(retry_outbox_event))
        .routes(routes!(list_function_runs))
        .routes(routes!(get_function_run))
        .routes(routes!(retry_function_run))
        .routes(routes!(list_config_descriptors))
        .routes(routes!(list_config_values))
        .routes(routes!(put_config_value, delete_config_value))
        .routes(routes!(get_config_audit))
}
