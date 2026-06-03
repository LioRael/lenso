//! Generic container DTOs for schema-admin endpoints. The record shape is
//! `serde_json::Value` because the renderer is generic across arbitrary modules.

use platform_module::AdminSchema;
use serde::Serialize;
use utoipa::ToSchema;

/// Response for `GET /admin/data/schema`: every admin-capable module's schema.
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminSchemaListResponse {
    pub modules: Vec<AdminModuleSchema>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminModuleSchema {
    pub module_name: String,
    pub schema: AdminSchema,
}

/// Response for `GET /admin/data/{module}/{entity}`: a page of records.
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminDataListResponse {
    /// Each record is an arbitrary JSON object whose keys match the entity schema.
    pub data: Vec<serde_json::Value>,
    pub page: AdminDataPageInfo,
}

/// Pagination info. `next_cursor` is an opaque token (not a timestamp like
/// platform-admin's PageInfo) so it makes no assumption about entity shape.
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminDataPageInfo {
    pub limit: i64,
    pub next_cursor: Option<String>,
}

/// Response for `GET /admin/data/{module}/{entity}/{id}`.
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminDataDetailResponse {
    pub data: serde_json::Value,
}
