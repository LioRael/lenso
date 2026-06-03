//! Schema-admin data API: generic endpoints that render any module's declared
//! admin entities. Depends on NO business domain — it works only through the
//! injected [`AdminDataSource`] registry and the manifest schema, mirroring
//! `platform-admin`'s seam-only discipline.

use platform_core::{AppError, ErrorCode, RequestContext};
use platform_http::{ApiErrorResponse, ApiOpenApiRouter, OpenApiRouter, routes};
use platform_module::{AdminDataSource, AdminSchema, ModuleLoadStatus, ModuleSource};
use std::sync::{Arc, OnceLock};

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
    /// Live read access to the module's records.
    pub data_source: Arc<dyn AdminDataSource>,
}

static ADMIN_REGISTRY: OnceLock<Vec<AdminModule>> = OnceLock::new();

/// Install the admin-capable module registry. Called once by the composition
/// root before the router serves traffic. Idempotent: later calls are ignored.
pub fn install_admin_modules(modules: Vec<AdminModule>) {
    let _ = ADMIN_REGISTRY.set(modules);
}

fn admin_modules() -> &'static [AdminModule] {
    ADMIN_REGISTRY.get().map(Vec::as_slice).unwrap_or_default()
}

fn find_module(
    module: &str,
    ctx: &RequestContext,
) -> Result<&'static AdminModule, ApiErrorResponse> {
    admin_modules()
        .iter()
        .find(|m| m.module_name == module)
        .ok_or_else(|| {
            ApiErrorResponse::with_context(
                AppError::new(ErrorCode::NotFound, format!("unknown module: {module}")),
                ctx,
            )
        })
}

/// The schema-admin router, mounted by the API app.
pub fn router() -> ApiOpenApiRouter {
    OpenApiRouter::new()
        .routes(routes!(list_schemas))
        .routes(routes!(list_records))
        .routes(routes!(get_record))
}
