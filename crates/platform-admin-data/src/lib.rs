//! Schema-admin data API: generic endpoints that render any module's declared
//! admin entities. Depends on NO business domain — it works only through the
//! injected [`AdminDataSource`] registry and the manifest schema, mirroring
//! `platform-admin`'s seam-only discipline.

use platform_core::{AppError, ErrorCode, RequestContext};
use platform_http::{ApiErrorResponse, ApiOpenApiRouter, OpenApiRouter, routes};
use platform_module::{AdminDataSource, AdminSchema, ModuleLoadStatus, ModuleSource};
use std::sync::{Arc, OnceLock, RwLock};

mod dto;
mod handlers;

pub use dto::*;
#[allow(clippy::wildcard_imports)]
use handlers::*;

/// One module's admin capability: its declared schema + its live data source.
#[derive(Clone, Debug)]
pub struct AdminModule {
    /// The owning module's stable name, e.g. "identity".
    pub module_name: String,
    /// The loading source that produced this module.
    pub source: ModuleSource,
    /// Current load state. The first remote slice only installs loaded modules;
    /// error entries are reserved for degraded loading in a later slice.
    pub load_status: ModuleLoadStatus,
    /// The module's declared admin surface (entities + fields).
    pub schema: AdminSchema,
    /// Live read access to the module's records. Missing for degraded modules
    /// whose manifest/data source failed to load.
    pub data_source: Option<Arc<dyn AdminDataSource>>,
}

static ADMIN_REGISTRY: OnceLock<RwLock<Vec<AdminModule>>> = OnceLock::new();

/// Install the admin-capable module registry. Called once by the composition
/// root before the router serves traffic. Later calls replace the registry,
/// which keeps tests isolated and leaves room for explicit refresh later.
pub fn install_admin_modules(modules: Vec<AdminModule>) {
    let registry = ADMIN_REGISTRY.get_or_init(|| RwLock::new(Vec::new()));
    *registry.write().expect("admin registry lock poisoned") = modules;
}

fn admin_modules() -> Vec<AdminModule> {
    ADMIN_REGISTRY
        .get()
        .map(|registry| {
            registry
                .read()
                .expect("admin registry lock poisoned")
                .clone()
        })
        .unwrap_or_default()
}

fn find_module(
    module: &str,
    ctx: &RequestContext,
) -> Result<AdminModule, ApiErrorResponse> {
    admin_modules()
        .into_iter()
        .find(|m| m.module_name == module)
        .ok_or_else(|| {
            ApiErrorResponse::with_context(
                AppError::new(ErrorCode::NotFound, format!("unknown module: {module}")),
                ctx,
            )
        })
}

fn find_loaded_module(
    module: &str,
    ctx: &RequestContext,
) -> Result<AdminModule, ApiErrorResponse> {
    let admin_module = find_module(module, ctx)?;
    if admin_module.data_source.is_some() {
        Ok(admin_module)
    } else {
        Err(ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::ExternalDependency,
                format!("module {module} is not loaded"),
            )
            .retryable(),
            ctx,
        ))
    }
}

/// The schema-admin router, mounted by the API app.
pub fn router() -> ApiOpenApiRouter {
    OpenApiRouter::new()
        .routes(routes!(list_schemas))
        .routes(routes!(list_records))
        .routes(routes!(get_record))
}
