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

use platform_core::{RuntimeConfigRegistry, StoryDisplayDescriptor};
use platform_http::{ApiOpenApiRouter, OpenApiRouter, routes};
use platform_module::ModuleSource;
use std::sync::{OnceLock, RwLock};

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
static STORY_DISPLAY: OnceLock<Vec<StoryDisplayDescriptor>> = OnceLock::new();
static RUNTIME_FUNCTION_DECLARATIONS: OnceLock<
    RwLock<Vec<AdminRuntimeFunctionDeclarationMetadata>>,
> = OnceLock::new();

/// Install the aggregated story-display descriptors from every domain.
///
/// Called once by the composition root before the router serves traffic. Story
/// display names are domain-owned metadata; injecting them keeps this crate
/// from depending on the domains or the composition root. Idempotent: later
/// calls are ignored.
pub fn install_story_display(catalog: Vec<StoryDisplayDescriptor>) {
    let _ = STORY_DISPLAY.set(catalog);
}

/// Runtime function declarations from every loaded module, injected by the
/// composition root. Later calls replace the catalog so module refreshes and
/// tests can update declaration metadata without restarting the process.
pub fn install_runtime_function_declarations(
    declarations: Vec<AdminRuntimeFunctionDeclarationMetadata>,
) {
    let catalog = RUNTIME_FUNCTION_DECLARATIONS.get_or_init(|| RwLock::new(Vec::new()));
    *catalog
        .write()
        .expect("runtime function declaration catalog lock poisoned") = declarations;
}

/// Install declarations only when the runtime-admin catalog has not yet been
/// initialized. Useful for context-free router/OpenAPI assembly; runtime
/// startup should call [`install_runtime_function_declarations`] with the full
/// linked + remote catalog.
pub fn install_default_runtime_function_declarations(
    declarations: Vec<AdminRuntimeFunctionDeclarationMetadata>,
) {
    if RUNTIME_FUNCTION_DECLARATIONS.get().is_none() {
        install_runtime_function_declarations(declarations);
    }
}

/// Project module manifests into the declaration catalog consumed by runtime
/// admin handlers.
#[must_use]
pub fn runtime_function_declarations_from_modules(
    modules: Vec<(
        String,
        ModuleSource,
        Option<platform_module::RuntimeSurface>,
    )>,
) -> Vec<AdminRuntimeFunctionDeclarationMetadata> {
    modules
        .into_iter()
        .flat_map(|(module_name, module_source, runtime)| {
            runtime
                .map(|surface| surface.functions)
                .unwrap_or_default()
                .into_iter()
                .map(move |function| AdminRuntimeFunctionDeclarationMetadata {
                    module_name: module_name.clone(),
                    module_source,
                    name: function.name,
                    version: function.version,
                    queue: function.queue,
                    input_schema: function.input_schema,
                    retry_policy: function.retry_policy,
                })
        })
        .collect()
}

static RUNTIME_CONFIG_REGISTRY: OnceLock<RuntimeConfigRegistry> = OnceLock::new();

/// Install the aggregated settings registry from the composition root. Idempotent.
pub fn install_runtime_config_registry(registry: RuntimeConfigRegistry) {
    let _ = RUNTIME_CONFIG_REGISTRY.set(registry);
}

/// The installed registry, or an empty one if none was installed.
fn runtime_config_registry() -> &'static RuntimeConfigRegistry {
    static EMPTY: OnceLock<RuntimeConfigRegistry> = OnceLock::new();
    RUNTIME_CONFIG_REGISTRY
        .get()
        .unwrap_or_else(|| EMPTY.get_or_init(RuntimeConfigRegistry::default))
}

fn runtime_function_declaration(
    function_name: &str,
) -> Option<AdminRuntimeFunctionDeclarationMetadata> {
    RUNTIME_FUNCTION_DECLARATIONS.get().and_then(|catalog| {
        catalog
            .read()
            .expect("runtime function declaration catalog lock poisoned")
            .iter()
            .find(|declaration| declaration.name == function_name)
            .cloned()
    })
}

fn enrich_function_run(mut run: AdminFunctionRun) -> AdminFunctionRun {
    run.runtime_declaration = runtime_function_declaration(&run.function_name);
    run
}

fn enrich_function_run_detail(mut run: AdminFunctionRunDetail) -> AdminFunctionRunDetail {
    run.runtime_declaration = runtime_function_declaration(&run.function_name);
    run
}

pub fn router() -> ApiOpenApiRouter {
    OpenApiRouter::new()
        .routes(routes!(get_summary))
        .routes(routes!(get_heatmap))
        .routes(routes!(list_stories))
        .routes(routes!(get_story))
        .routes(routes!(get_story_heatmap))
        .routes(routes!(get_story_technical_operations))
        .routes(routes!(get_execution_technical_operations))
        .routes(routes!(get_execution_payload))
        .routes(routes!(get_execution_logs))
        .routes(routes!(list_remote_proxy_calls))
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
