//! Generic container DTOs for schema-admin endpoints. The record shape is
//! `serde_json::Value` because the renderer is generic across arbitrary modules.

use platform_core::{StoryDisplayDescriptor, StoryDisplaySource};
use platform_module::{
    AdminSchema, AdminSurface, LifecycleSurface, ModuleHttpRoute, ModuleManifestLint, ModuleSource,
    RuntimeSurface,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Response for `GET /admin/data/schema`: every admin-capable module's schema.
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminSchemaListResponse {
    pub modules: Vec<AdminModuleSchema>,
}

/// Response for `POST /admin/data/schema/refresh`.
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminSchemaRefreshResponse {
    pub modules: Vec<AdminModuleSchema>,
}

/// Response for `GET /admin/data/modules`: every registered module's metadata,
/// including modules without admin surfaces.
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminModuleMetadataListResponse {
    pub modules: Vec<AdminModuleMetadataDto>,
    pub refreshed_at: Option<String>,
    pub refresh_error: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminModuleMetadataDto {
    pub module_name: String,
    pub source: ModuleSource,
    pub status: AdminModuleStatus,
    pub error: Option<String>,
    pub source_diagnostics: Option<AdminModuleSourceDiagnosticsDto>,
    pub http_routes: Vec<ModuleHttpRoute>,
    pub runtime: Option<RuntimeSurface>,
    pub lifecycle: Option<LifecycleSurface>,
    pub governance: AdminModuleGovernanceDto,
    pub manifest_lints: Vec<ModuleManifestLint>,
    pub story_display: Vec<StoryDisplayDescriptorDto>,
    pub capabilities: Vec<String>,
    pub admin: Option<AdminSurface>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AdminModuleSourceDiagnosticsDto {
    Remote(AdminRemoteModuleDiagnosticsDto),
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminRemoteModuleDiagnosticsDto {
    pub base_url: String,
    pub manifest_url: String,
    pub timeout_ms: u64,
    pub auth_configured: bool,
    pub last_checked_at: Option<String>,
    pub last_load_error: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminModuleGovernanceDto {
    pub activation_state: AdminModuleActivationState,
    pub activation_reasons: Vec<String>,
    pub capability_summary: AdminCapabilitySummaryDto,
    pub capability_issues: Vec<AdminCapabilityIssueDto>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdminModuleActivationState {
    Active,
    NeedsAttention,
    Blocked,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminCapabilitySummaryDto {
    pub declared_count: usize,
    pub referenced_count: usize,
    pub missing_count: usize,
    pub unused_count: usize,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminCapabilityIssueDto {
    pub capability: String,
    pub subject: String,
    pub message: String,
    pub suggestion: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StoryDisplayDescriptorDto {
    pub source: StoryDisplaySourceDto,
    pub display_name: String,
    pub story_title: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StoryDisplaySourceDto {
    ExecutionName { name: String },
    HttpRequest { method: String, path: String },
}

impl From<StoryDisplayDescriptor> for StoryDisplayDescriptorDto {
    fn from(descriptor: StoryDisplayDescriptor) -> Self {
        Self {
            source: descriptor.source.into(),
            display_name: descriptor.display_name,
            story_title: descriptor.story_title,
        }
    }
}

impl From<StoryDisplaySource> for StoryDisplaySourceDto {
    fn from(source: StoryDisplaySource) -> Self {
        match source {
            StoryDisplaySource::ExecutionName { name } => Self::ExecutionName { name },
            StoryDisplaySource::HttpRequest { method, path } => Self::HttpRequest { method, path },
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminModuleSchema {
    pub module_name: String,
    pub source: ModuleSource,
    pub status: AdminModuleStatus,
    pub error: Option<String>,
    pub schema: AdminSchema,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdminModuleStatus {
    Loaded,
    Error,
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

/// Request body for `POST /admin/data/{module}/actions/{action}`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct AdminActionInvokeRequest {
    #[serde(default)]
    pub input: serde_json::Value,
}

/// Response for `POST /admin/data/{module}/actions/{action}`.
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminActionInvokeResponse {
    pub data: serde_json::Value,
}
