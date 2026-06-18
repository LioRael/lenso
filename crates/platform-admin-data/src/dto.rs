//! Generic container DTOs for schema-admin endpoints. The record shape is
//! `serde_json::Value` because the renderer is generic across arbitrary modules.

use platform_core::{StoryDisplayDescriptor, StoryDisplaySource};
use platform_module::{
    AdminSchema, AdminSurface, ConsoleSurface, EventSurface, LifecycleSurface, ModuleHttpRoute,
    ModuleManifestLint, ModuleSource, RuntimeSurface,
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
    pub refresh_history: Vec<AdminModuleRefreshRecordDto>,
}

/// Response for `GET /admin/data/available-modules`: a read-only list of
/// remote modules that can be installed from their manifest URL.
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminModuleRegistrySnapshotResponse {
    pub version: u8,
    pub status: AdminModuleRegistrySnapshotStatus,
    pub catalog: AdminModuleRegistrySnapshotCatalogDto,
    pub issues: Vec<AdminModuleRegistrySnapshotIssueDto>,
    pub modules: Vec<AdminModuleRegistrySnapshotModuleDto>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdminModuleRegistrySnapshotStatus {
    Passed,
    Failed,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminModuleRegistrySnapshotCatalogDto {
    pub modules: usize,
    pub registry_file: String,
    pub version: u8,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminModuleRegistrySnapshotIssueDto {
    pub group: String,
    pub message: String,
    pub fix: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminModuleRegistrySnapshotModuleDto {
    pub name: String,
    pub source: ModuleSource,
    pub catalog_version: String,
    pub manifest_reference: String,
    pub summary: Option<String>,
    pub base_url: Option<String>,
    pub capabilities: Vec<String>,
    pub console_package_hints: usize,
    pub compatibility: Option<AdminModuleCompatibilityDto>,
    pub host_compatibility: AdminModuleHostCompatibilityDto,
    pub archived_at: Option<String>,
    pub archive_reason: Option<String>,
    pub manifest_name: Option<String>,
    pub manifest_status: AdminModuleRegistrySnapshotManifestStatus,
    pub manifest_version: Option<String>,
    pub install_state: AdminModuleInstallStateDto,
    pub status: AdminModuleRegistrySnapshotModuleStatus,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminModuleInstallStateDto {
    pub module_registered: bool,
    pub linked_source: Option<AdminModuleLinkedSourceInstallStateDto>,
    pub remote_source: Option<AdminModuleRemoteSourceInstallStateDto>,
    pub console_plan: AdminModuleConsolePackagePlanStateDto,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminModuleLinkedSourceInstallStateDto {
    pub env_file: String,
    pub configured: bool,
    pub desired_enabled: Option<bool>,
    pub running_enabled: bool,
    pub restart_pending: bool,
    pub restart_reason: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminModuleRemoteSourceInstallStateDto {
    pub env_file: String,
    pub configured: bool,
    pub desired_base_url: Option<String>,
    pub running_base_url: Option<String>,
    pub restart_pending: bool,
    pub restart_reason: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminModuleConsolePackagePlanStateDto {
    pub plan_file: String,
    pub exists: bool,
    pub readable: bool,
    pub error: Option<String>,
    pub module_entry_present: bool,
    pub package_count: usize,
    pub restart_required: Option<bool>,
    pub packages: Vec<AdminModuleConsolePackagePlanPackageDto>,
}

/// Response for visually installing an available remote module from the
/// marketplace catalog.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminModuleInstallResponse {
    pub module_name: String,
    pub manifest_reference: String,
    pub linked_source: Option<AdminModuleLinkedSourceInstallStateDto>,
    pub remote_source: Option<AdminModuleRemoteSourceInstallStateDto>,
    pub console_plan: AdminModuleConsolePackagePlanStateDto,
    pub restart_required: bool,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminModuleConsolePackagePlanPackageDto {
    pub key: Option<String>,
    pub package_name: String,
    pub export_name: String,
    pub command: Option<String>,
    pub route: Option<String>,
    pub status: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminModuleCompatibilityDto {
    pub console_package_api: Option<String>,
    pub lenso: Option<AdminModuleLensoCompatibilityDto>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminModuleLensoCompatibilityDto {
    pub min_version: Option<String>,
    pub max_version: Option<String>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminModuleHostCompatibilityDto {
    pub console_package_api: String,
    pub lenso_version: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdminModuleRegistrySnapshotManifestStatus {
    Ok,
    Invalid,
    Unreadable,
    Archived,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdminModuleRegistrySnapshotModuleStatus {
    Ready,
    NeedsAttention,
    Archived,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminModuleRefreshRecordDto {
    pub id: String,
    pub status: AdminModuleRefreshStatusDto,
    pub started_at: String,
    pub completed_at: String,
    pub duration_ms: u64,
    pub module_count: usize,
    pub error: Option<String>,
    pub module_results: Vec<AdminModuleRefreshModuleResultDto>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdminModuleRefreshStatusDto {
    Success,
    Error,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminModuleRefreshModuleResultDto {
    pub module_name: String,
    pub source: ModuleSource,
    pub status: AdminModuleRefreshModuleStatusDto,
    pub duration_ms: Option<u64>,
    pub endpoint: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdminModuleRefreshModuleStatusDto {
    Loaded,
    Error,
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
    pub events: Option<EventSurface>,
    pub lifecycle: Option<LifecycleSurface>,
    pub console: Vec<ConsoleSurface>,
    pub governance: AdminModuleGovernanceDto,
    pub manifest_lints: Vec<ModuleManifestLint>,
    pub story_display: Vec<StoryDisplayDescriptorDto>,
    pub capabilities: Vec<String>,
    pub dependencies: Vec<String>,
    pub admin: Option<AdminSurface>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AdminModuleSourceDiagnosticsDto {
    Remote(AdminRemoteModuleDiagnosticsDto),
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminRemoteModuleDiagnosticsDto {
    pub transport: String,
    pub base_url: String,
    pub manifest_url: String,
    pub timeout_ms: u64,
    pub auth_configured: bool,
    pub load_duration_ms: Option<u64>,
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
    #[serde(default)]
    pub confirmation_phrase: Option<String>,
}

/// Response for `POST /admin/data/{module}/actions/{action}`.
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminActionInvokeResponse {
    pub data: serde_json::Value,
    pub invocation: AdminActionInvocationDto,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminActionInvocationDto {
    pub request_id: String,
    pub correlation_id: String,
    pub story_node_id: String,
}
