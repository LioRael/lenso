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
/// catalog entries that can be installed from their manifest URL.
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
    #[serde(default, rename = "providedBy")]
    pub provided_by: Option<String>,
    #[serde(default, rename = "serviceManifest")]
    pub service_manifest: Option<String>,
    #[serde(default, rename = "moduleRelease")]
    pub module_release: Option<AdminModuleReleaseDto>,
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

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminModuleReleaseDto {
    pub manifest_reference: String,
    pub name: Option<String>,
    pub version: Option<String>,
    pub source: Option<String>,
    pub provider_name: Option<String>,
    pub service_package: Option<String>,
    pub service_manifest: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceModuleLifecycleResponse {
    pub version: u8,
    pub status: AdminServiceModuleLifecycleStatus,
    pub modules: Vec<AdminServiceModuleLifecycleModuleDto>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdminServiceModuleLifecycleStatus {
    Ready,
    NeedsAttention,
    Empty,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceModuleLifecycleModuleDto {
    pub module_name: String,
    pub provider_name: Option<String>,
    pub status: AdminServiceModuleLifecycleModuleStatus,
    pub installed: bool,
    pub configured: bool,
    pub loaded: bool,
    pub restart_pending: bool,
    pub base_url: Option<String>,
    pub manifest_url: Option<String>,
    pub manifest_status: AdminServiceModuleManifestStatus,
    pub status_url: Option<String>,
    pub service_status: AdminServiceModuleServiceStatusDto,
    pub health_history: Vec<AdminServiceModuleHealthCheckDto>,
    pub compatibility: AdminServiceModuleCompatibilityDto,
    pub config: AdminServiceModuleConfigDto,
    pub deployment: Option<AdminServiceModuleDeploymentDto>,
    pub environments: Vec<AdminServiceEnvironmentDto>,
    pub deployments: Vec<AdminServiceDeploymentObservationDto>,
    pub deployment_drift: Option<String>,
    pub deployment_next_action: Option<String>,
    pub services: Vec<AdminServiceModuleLifecycleServiceDto>,
    pub operations: Vec<AdminServiceOperationDto>,
    pub module_release: Option<AdminModuleReleaseDto>,
    pub latest_release: Option<AdminServiceReleaseRecordDto>,
    pub release_history: Vec<AdminServiceReleaseRecordDto>,
    pub fixes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceReleaseRecordDto {
    pub id: Option<String>,
    pub service_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub applied_at_unix_ms: Option<u64>,
    pub risk: String,
    pub current_version: Option<String>,
    pub candidate_version: Option<String>,
    pub current_manifest_reference: Option<String>,
    pub candidate_manifest_reference: Option<String>,
    pub candidate_package_reference: Option<String>,
    pub rollback_target: Option<String>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceEnvironmentDto {
    pub name: String,
    pub service_name: String,
    pub target: String,
    pub namespace: Option<String>,
    pub kube_context: Option<String>,
    pub image: Option<String>,
    pub public_base_url: Option<String>,
    pub manifest_reference: Option<String>,
    pub release_track: Option<String>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceDeploymentObservationDto {
    pub service_name: String,
    pub environment: String,
    pub target: String,
    pub observed_at_unix_ms: Option<u64>,
    pub state: String,
    pub drift: String,
    pub operator: Option<AdminServiceDeploymentOperatorObservationDto>,
    pub cluster: Option<AdminKubernetesDeploymentObservationDto>,
    pub host: Option<AdminServiceDeploymentHostObservationDto>,
    pub checks: Vec<AdminServiceDeploymentCheckDto>,
    pub next_action: Option<String>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceDeploymentOperatorObservationDto {
    pub resource: Option<String>,
    pub namespace: Option<String>,
    pub observed_generation: Option<u64>,
    pub conditions: Vec<AdminServiceDeploymentOperatorConditionDto>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceDeploymentOperatorConditionDto {
    #[serde(rename = "type")]
    pub type_: Option<String>,
    pub status: Option<String>,
    pub reason: Option<String>,
    pub message: Option<String>,
    pub last_transition_time: Option<String>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminKubernetesDeploymentObservationDto {
    pub namespace: Option<String>,
    pub deployment: Option<String>,
    pub ready_replicas: Option<u32>,
    pub desired_replicas: Option<u32>,
    pub available_replicas: Option<u32>,
    pub image: Option<String>,
    pub release_id: Option<String>,
    pub manifest_reference: Option<String>,
    pub service_endpoint: Option<String>,
    pub ingress_host: Option<String>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceDeploymentHostObservationDto {
    pub release_id: Option<String>,
    pub candidate_version: Option<String>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceDeploymentCheckDto {
    pub name: String,
    pub status: String,
    pub detail: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdminServiceOperationKindDto {
    HttpRoute,
    RuntimeFunction,
    EventHandler,
    AdminAction,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceOperationLinksDto {
    pub remote_calls: Option<String>,
    pub runtime: Option<String>,
    pub story: String,
    pub technical_operations: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceOperationDto {
    pub operation_id: String,
    pub provider_name: Option<String>,
    pub module_name: String,
    pub kind: AdminServiceOperationKindDto,
    pub name: String,
    pub method: Option<String>,
    pub path: Option<String>,
    pub capability: Option<String>,
    pub summary: Option<String>,
    pub safe_probe: bool,
    pub links: AdminServiceOperationLinksDto,
    pub next_action: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceModuleHealthCheckDto {
    pub module_name: String,
    pub checked_at_unix_ms: u64,
    pub status_url: String,
    pub state: String,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdminServiceModuleLifecycleModuleStatus {
    Ready,
    MissingConfig,
    RestartPending,
    ConfiguredNotLoaded,
    ManifestUnreachable,
    ServiceNotReady,
    StaleState,
    NotConfigured,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceModuleConfigDto {
    pub env_file: String,
    pub required_env: Vec<String>,
    pub configured_env: Vec<String>,
    pub missing_env: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdminServiceModuleManifestStatus {
    Reachable,
    Unreachable,
    Skipped,
    NotConfigured,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceModuleLifecycleServiceDto {
    pub name: String,
    pub ready_url: String,
    pub ready: bool,
    pub auto_start: bool,
    pub lock_file: Option<String>,
    pub pid_file: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceModuleServiceStatusDto {
    pub checked: bool,
    pub state: AdminServiceModuleServiceStatusState,
    pub error: Option<String>,
    pub checks: Vec<AdminServiceModuleServiceStatusCheckDto>,
}

#[derive(Debug, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdminServiceModuleServiceStatusState {
    Ready,
    Degraded,
    Starting,
    Unreachable,
    Unknown,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceModuleServiceStatusCheckDto {
    pub name: String,
    pub status: String,
    pub detail: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceModuleCompatibilityDto {
    pub state: AdminServiceModuleCompatibilityState,
    pub declared: Option<AdminModuleCompatibilityDto>,
    pub host: AdminModuleHostCompatibilityDto,
    pub issue: Option<String>,
    pub fix: Option<String>,
    pub override_allowed: bool,
}

#[derive(Debug, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdminServiceModuleCompatibilityState {
    Compatible,
    Blocked,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceModuleDeploymentDto {
    pub target: Option<String>,
    #[serde(default)]
    pub commands: Vec<String>,
    pub compose_service: Option<String>,
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

/// Response for visually installing an available service or linked module from
/// the curated catalog.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminModuleInstallResponse {
    pub module_name: String,
    pub manifest_reference: String,
    pub module_release: Option<AdminModuleReleaseDto>,
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
    #[serde(alias = "console_package_api")]
    pub console_package_api: Option<String>,
    pub lenso: Option<AdminModuleLensoCompatibilityDto>,
    #[serde(alias = "remote_protocol_version")]
    pub remote_protocol_version: Option<String>,
    #[serde(default, alias = "required_host_features")]
    pub required_host_features: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminModuleLensoCompatibilityDto {
    #[serde(alias = "min_version")]
    pub min_version: Option<String>,
    #[serde(alias = "max_version")]
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

/// Response for `GET /admin/data/{module}/queries/{query}`.
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminQueryResponse {
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
