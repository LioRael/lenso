use chrono::{DateTime, Utc};
use platform_module::{ModuleSource, RuntimeRetryPolicyDeclaration};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Deserialize, IntoParams)]
pub struct OutboxQuery {
    pub status: Option<String>,
    pub event_name: Option<String>,
    pub limit: Option<i64>,
    pub created_before: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct FunctionRunQuery {
    pub status: Option<String>,
    pub function_name: Option<String>,
    pub limit: Option<i64>,
    pub created_before: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct RemoteProxyCallQuery {
    pub module_name: Option<String>,
    pub correlation_id: Option<String>,
    pub success: Option<bool>,
    pub limit: Option<i64>,
    pub created_before: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct AdminActionInvocationQuery {
    pub module_name: Option<String>,
    pub action_name: Option<String>,
    pub capability: Option<String>,
    pub correlation_id: Option<String>,
    pub success: Option<bool>,
    pub limit: Option<i64>,
    pub created_before: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct StoryQuery {
    pub limit: Option<i64>,
    pub created_before: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct HeatmapQuery {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub bucket_seconds: Option<i64>,
    pub status: Option<String>,
    pub event_name: Option<String>,
    pub function_name: Option<String>,
    pub limit: Option<i64>,
    pub created_before: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ExecutionLogQuery {
    pub limit: Option<i64>,
    pub created_before: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminServiceRestartResponse {
    pub status: String,
    pub service: String,
    pub requires_supervisor: bool,
}

#[derive(Debug, Serialize, ToSchema, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AdminContextActor {
    Service { service_id: String },
    User { user_id: String },
    System,
}

#[derive(Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct AdminContextResponse {
    pub actor: AdminContextActor,
    pub scopes: Vec<String>,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PageInfo {
    pub limit: i64,
    pub next_created_before: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminOutboxListResponse)]
pub struct AdminOutboxListResponse {
    pub data: Vec<AdminOutboxEvent>,
    pub page: PageInfo,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminFunctionRunListResponse)]
pub struct AdminFunctionRunListResponse {
    pub data: Vec<AdminFunctionRun>,
    pub page: PageInfo,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminRemoteProxyCallListResponse)]
pub struct AdminRemoteProxyCallListResponse {
    pub data: Vec<AdminRemoteProxyCall>,
    pub page: PageInfo,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminActionInvocationListResponse)]
pub struct AdminActionInvocationListResponse {
    pub data: Vec<AdminActionInvocation>,
    pub page: PageInfo,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminRuntimeHeatmapResponse)]
pub struct AdminRuntimeHeatmapResponse {
    pub data: Vec<AdminRuntimeHeatmapCell>,
    pub bucket_seconds: i64,
    pub page: PageInfo,
    pub order: &'static str,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminRuntimeTechnicalOperationListResponse)]
pub struct AdminRuntimeTechnicalOperationListResponse {
    pub data: Vec<AdminRuntimeTechnicalOperation>,
    pub order: &'static str,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminRuntimeExecutionPayloadResponse)]
pub struct AdminRuntimeExecutionPayloadResponse {
    pub data: AdminRuntimeExecutionPayload,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminRuntimeExecutionLogListResponse)]
pub struct AdminRuntimeExecutionLogListResponse {
    pub data: Vec<AdminRuntimeExecutionLog>,
    pub page: PageInfo,
    pub order: &'static str,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminRuntimeExecutionPayload {
    pub node_id: String,
    pub node_type: String,
    pub input: Value,
    pub output: Option<Value>,
    pub metadata: Value,
    pub redacted_fields: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminRuntimeExecutionLog {
    pub id: String,
    pub node_id: String,
    pub node_type: String,
    pub correlation_id: String,
    pub story_id: String,
    pub execution_name: String,
    pub occurred_at: DateTime<Utc>,
    pub severity: String,
    pub body: String,
    pub attributes: Value,
    pub service_name: String,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub redacted_fields: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminRuntimeTechnicalOperation {
    pub id: String,
    pub story_id: String,
    pub correlation_id: String,
    pub related_node_id: Option<String>,
    pub category: String,
    pub name: String,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub duration_ms: i64,
    pub attributes: Value,
    pub source: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminActionInvocationItem)]
pub struct AdminActionInvocation {
    pub id: String,
    pub module_name: String,
    pub action_name: String,
    pub label: String,
    pub capability: Option<String>,
    pub duration_ms: i64,
    pub success: bool,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub request_id: Option<String>,
    pub correlation_id: String,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub input_summary: Option<String>,
    pub result_summary: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminRuntimeSummaryResponse)]
pub struct AdminRuntimeSummaryResponse {
    pub status: String,
    pub outbox: AdminRuntimeOutboxSummary,
    pub functions: AdminRuntimeFunctionSummary,
    pub recent_activity: Vec<AdminRuntimeSummaryItem>,
    pub recent_failures: Vec<AdminRuntimeSummaryItem>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminRuntimeOutboxSummary {
    pub pending: i64,
    pub processing: i64,
    pub published: i64,
    pub failed: i64,
    pub dead: i64,
    pub oldest_pending_age_seconds: Option<i64>,
    pub oldest_failed_age_seconds: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminRuntimeFunctionSummary {
    pub pending: i64,
    pub running: i64,
    pub completed: i64,
    pub failed: i64,
    pub dead: i64,
    pub oldest_pending_age_seconds: Option<i64>,
    pub oldest_failed_age_seconds: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminRuntimeSummaryItem {
    #[serde(rename = "type")]
    pub item_type: String,
    pub id: String,
    pub name: String,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub correlation_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminRuntimeOutboxItem)]
pub struct AdminOutboxEvent {
    pub id: String,
    pub event_name: String,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub available_at: DateTime<Utc>,
    pub locked_by: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub correlation_id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminOutboxEventDetail {
    pub id: String,
    pub event_name: String,
    pub event_version: i32,
    pub source_module: String,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub available_at: DateTime<Utc>,
    pub locked_by: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub correlation_id: String,
    pub causation_id: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub payload: Value,
    pub actor: Value,
    pub trace: Value,
    pub headers: Value,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminRuntimeFunctionRunItem)]
pub struct AdminFunctionRun {
    pub id: String,
    pub function_name: String,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub available_at: DateTime<Utc>,
    pub locked_by: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub correlation_id: String,
    pub created_at: DateTime<Utc>,
    pub runtime_declaration: Option<AdminRuntimeFunctionDeclarationMetadata>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminFunctionRunDetail {
    pub id: String,
    pub function_name: String,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub available_at: DateTime<Utc>,
    pub locked_by: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub correlation_id: String,
    pub created_at: DateTime<Utc>,
    pub input_json: Value,
    pub actor: Value,
    pub runtime_declaration: Option<AdminRuntimeFunctionDeclarationMetadata>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AdminRuntimeFunctionDeclarationMetadata {
    pub module_name: String,
    pub module_source: ModuleSource,
    pub name: String,
    pub version: u16,
    pub queue: String,
    pub input_schema: Option<String>,
    pub retry_policy: Option<RuntimeRetryPolicyDeclaration>,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminRemoteProxyCallItem)]
pub struct AdminRemoteProxyCall {
    pub id: String,
    pub module_name: String,
    pub method: String,
    pub declared_path: String,
    pub remote_path: String,
    pub capability: Option<String>,
    pub remote_status: Option<i32>,
    pub duration_ms: i64,
    pub success: bool,
    pub error_code: Option<String>,
    pub retryable: bool,
    pub request_id: String,
    pub correlation_id: String,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub path_params: Value,
    pub error_details: Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub(crate) struct StoryEventDetail {
    pub(crate) id: String,
    pub(crate) node_type: String,
    pub(crate) name: String,
    pub(crate) status: String,
    pub(crate) service: String,
    pub(crate) correlation_id: String,
    pub(crate) causation_id: Option<String>,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) completed_at: Option<DateTime<Utc>>,
    pub(crate) duration_ms: i64,
    pub(crate) error: Option<String>,
    pub(crate) metadata: Value,
    pub(crate) trace_id: Option<String>,
    pub(crate) span_id: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminRuntimeHeatmapCell {
    pub bucket_start: DateTime<Utc>,
    pub bucket_end: DateTime<Utc>,
    pub service: String,
    pub node_type: String,
    pub total_count: i64,
    pub error_count: i64,
    pub retry_count: i64,
    pub dead_count: i64,
    pub avg_duration_ms: Option<i64>,
    pub max_duration_ms: Option<i64>,
}
