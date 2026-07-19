//! Runtime-observability API backing the Runtime Console.
//!
//! This is a platform cross-cutting concern, not a product module: it only
//! reads platform/runtime tables (`platform.outbox`, `platform.story_events`,
//! `runtime.function_runs`) to observe the activity of every module. It exposes
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
//! Story display names are module-owned, so they are injected by the
//! composition root via [`install_story_display`] rather than depended on
//! directly, keeping this crate free of any concrete-module dependency.

use platform_core::RuntimeConfigRegistry;
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
mod system_handlers;

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
#[allow(clippy::wildcard_imports)]
use system_handlers::*;

static RUNTIME_FUNCTION_DECLARATIONS: OnceLock<
    RwLock<InstalledCatalog<AdminRuntimeFunctionDeclarationMetadata>>,
> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CatalogMode {
    Default,
    Runtime,
}

#[derive(Debug)]
struct InstalledCatalog<T> {
    mode: CatalogMode,
    items: Vec<T>,
}

impl<T> Default for InstalledCatalog<T> {
    fn default() -> Self {
        Self {
            mode: CatalogMode::Default,
            items: Vec::new(),
        }
    }
}

fn install_catalog<T>(
    catalog: &OnceLock<RwLock<InstalledCatalog<T>>>,
    items: Vec<T>,
    mode: CatalogMode,
) {
    let catalog = catalog.get_or_init(|| RwLock::new(InstalledCatalog::default()));
    let mut catalog = catalog.write().expect("admin catalog lock poisoned");
    if mode == CatalogMode::Default && catalog.mode == CatalogMode::Runtime {
        return;
    }
    *catalog = InstalledCatalog { mode, items };
}

fn cloned_catalog<T: Clone>(catalog: &OnceLock<RwLock<InstalledCatalog<T>>>) -> Vec<T> {
    catalog
        .get()
        .map(|catalog| {
            catalog
                .read()
                .expect("admin catalog lock poisoned")
                .items
                .clone()
        })
        .unwrap_or_default()
}

/// Runtime function declarations from every loaded module, injected by the
/// composition root. Later calls replace the catalog so module refreshes and
/// tests can update declaration metadata without restarting the process.
pub fn install_runtime_function_declarations(
    declarations: Vec<AdminRuntimeFunctionDeclarationMetadata>,
) {
    install_catalog(
        &RUNTIME_FUNCTION_DECLARATIONS,
        declarations,
        CatalogMode::Runtime,
    );
}

/// Install context-free default runtime function declarations.
///
/// Default catalogs may replace earlier defaults from a different composition
/// profile. They do not replace the full runtime catalog installed from loaded
/// module metadata.
pub fn install_default_runtime_function_declarations(
    declarations: Vec<AdminRuntimeFunctionDeclarationMetadata>,
) {
    install_catalog(
        &RUNTIME_FUNCTION_DECLARATIONS,
        declarations,
        CatalogMode::Default,
    );
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
            .expect("admin catalog lock poisoned")
            .items
            .iter()
            .find(|declaration| declaration.name == function_name)
            .cloned()
    })
}

#[doc(hidden)]
#[cfg(debug_assertions)]
pub fn runtime_function_declaration_catalog_snapshot()
-> Vec<AdminRuntimeFunctionDeclarationMetadata> {
    cloned_catalog(&RUNTIME_FUNCTION_DECLARATIONS)
}

#[doc(hidden)]
#[cfg(debug_assertions)]
pub fn reset_catalogs_for_test() {
    reset_catalog_for_test(&RUNTIME_FUNCTION_DECLARATIONS);
}

#[cfg(debug_assertions)]
fn reset_catalog_for_test<T>(catalog: &OnceLock<RwLock<InstalledCatalog<T>>>) {
    if let Some(catalog) = catalog.get() {
        *catalog.write().expect("admin catalog lock poisoned") = InstalledCatalog::default();
    }
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
        .routes(routes!(get_admin_context))
        .routes(routes!(get_summary))
        .routes(routes!(get_extraction_console_projection))
        .routes(routes!(get_extraction_artifact))
        .routes(routes!(get_heatmap))
        .routes(routes!(get_execution_technical_operations))
        .routes(routes!(get_execution_payload))
        .routes(routes!(get_execution_logs))
        .routes(routes!(list_remote_proxy_calls))
        .routes(routes!(list_admin_action_invocations))
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
        .routes(routes!(restart_service))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_catalogs_replace_defaults_but_do_not_clobber_runtime_catalogs() {
        reset_catalogs_for_test();

        install_default_runtime_function_declarations(vec![runtime_declaration(
            "profile.default.demo",
        )]);
        install_default_runtime_function_declarations(vec![runtime_declaration(
            "profile.default.core",
        )]);
        assert!(runtime_function_declaration("profile.default.demo").is_none());
        assert!(runtime_function_declaration("profile.default.core").is_some());

        install_runtime_function_declarations(vec![runtime_declaration("profile.runtime.remote")]);
        install_default_runtime_function_declarations(vec![runtime_declaration(
            "profile.default.late",
        )]);
        assert!(runtime_function_declaration("profile.runtime.remote").is_some());
        assert!(runtime_function_declaration("profile.default.late").is_none());
    }

    fn runtime_declaration(name: &str) -> AdminRuntimeFunctionDeclarationMetadata {
        AdminRuntimeFunctionDeclarationMetadata {
            module_name: "catalog-test".to_owned(),
            module_source: ModuleSource::Linked,
            name: name.to_owned(),
            version: 1,
            queue: "catalog-test".to_owned(),
            input_schema: None,
            retry_policy: None,
        }
    }
}
