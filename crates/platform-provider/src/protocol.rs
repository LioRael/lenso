use platform_core::{ActorContext, TraceContext};
use platform_module::{AdminPage, ModuleManifest};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const PROVIDER_PROTOCOL: &str = "lenso.provider.v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderDescriptor {
    pub protocol: String,
    pub protocol_contract_digest: String,
    pub service_id: String,
    pub service_release_version: String,
    pub service_release_digest: String,
    pub runtime_instance_id: String,
    #[serde(default)]
    pub features: Vec<String>,
    pub transports: Vec<ProviderTransportBinding>,
    pub exports: Vec<ProviderExport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderTransportBinding {
    HttpJson,
    Grpc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderExport {
    pub export_key: String,
    pub module_id: String,
    pub module_version: String,
    pub module_release_digest: String,
    pub manifest_digest: String,
    pub manifest: ModuleManifest,
    #[serde(default)]
    pub contract_digests: BTreeMap<String, String>,
    pub ready: bool,
    #[serde(default)]
    pub readiness_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderOperationKind {
    HttpRoute,
    AdminList,
    AdminGet,
    AdminQuery,
    AdminAction,
    RuntimeFunction,
    EventHandler,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderInvocationMode {
    ReadOnly,
    Durable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderInvocation {
    pub protocol: String,
    pub invocation_id: String,
    pub request_id: String,
    pub attempt: u32,
    pub deadline: String,
    pub service_release_digest: String,
    pub export_key: String,
    pub module_release_digest: String,
    pub manifest_digest: String,
    pub operation_kind: ProviderOperationKind,
    pub operation_name: String,
    pub operation_version: String,
    pub mode: ProviderInvocationMode,
    pub input_contract_digest: String,
    pub output_contract_digest: String,
    #[serde(default)]
    pub tenant_id: Option<String>,
    pub actor: ActorContext,
    #[serde(default)]
    pub delegation: Option<Value>,
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default)]
    pub context: BTreeMap<String, Value>,
    pub correlation_id: String,
    #[serde(default)]
    pub causation_id: Option<String>,
    pub trace: TraceContext,
    pub content_type: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderOutcomeStatus {
    Pending,
    Succeeded,
    Rejected,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderOutcome {
    pub protocol: String,
    pub invocation_id: String,
    pub status: ProviderOutcomeStatus,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<ProviderErrorBody>,
    #[serde(default)]
    pub effect_evidence: Vec<Value>,
    #[serde(default)]
    pub host_effects: ProviderHostEffectBatch,
    pub outcome_digest: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderHostEffectBatch {
    #[serde(default)]
    pub events: Vec<Value>,
    #[serde(default)]
    pub runtime_function_requests: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderHealth {
    pub protocol: String,
    pub service_id: String,
    pub service_release_digest: String,
    pub live: bool,
    pub ready: bool,
    pub observed_at: String,
    #[serde(default)]
    pub exports: BTreeMap<String, ProviderExportHealth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderExportHealth {
    pub ready: bool,
    #[serde(default)]
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderInvocationReference {
    pub invocation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderInvocationAcknowledgement {
    pub invocation_id: String,
    pub outcome_digest: String,
}

pub type ProviderManifestResponse = ModuleManifest;

/// Standard error response shape for the Provider Service protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderErrorEnvelope {
    pub error: ProviderErrorBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderErrorBody {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub retryable: bool,
    #[serde(default)]
    pub retry_after_ms: Option<u64>,
    #[serde(default)]
    pub provider_trace_reference: Option<String>,
    #[serde(default)]
    pub details: Vec<ProviderErrorDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderErrorDetail {
    pub field: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderListResponse {
    pub records: Vec<Value>,
    pub next_cursor: Option<String>,
}

impl From<ProviderListResponse> for AdminPage {
    fn from(value: ProviderListResponse) -> Self {
        Self {
            records: value.records,
            next_cursor: value.next_cursor,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderGetResponse {
    pub record: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderActionInvokeResponse {
    pub result: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderQueryResponse {
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderAdminListRequest {
    pub entity: String,
    pub limit: i64,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderAdminGetRequest {
    pub entity: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderAdminActionInvokeRequest {
    pub action: String,
    pub input: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderAdminQueryRequest {
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderHttpProxyInvokeRequest {
    pub request_id: String,
    pub correlation_id: String,
    pub module_name: String,
    pub method: String,
    pub declared_path: String,
    pub provider_path: String,
    pub path_params: BTreeMap<String, String>,
    pub headers: BTreeMap<String, String>,
    pub body: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderHttpProxyInvokeResponse {
    pub status_code: u16,
    pub body: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderFunctionInvokeRequest {
    pub request_id: String,
    pub function_run_id: String,
    pub function_name: String,
    pub attempt: u32,
    pub correlation_id: String,
    pub causation_id: Option<String>,
    pub actor: ActorContext,
    pub trace: TraceContext,
    pub input: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderFunctionInvokeResponse {
    pub output: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderEventHandleRequest {
    pub request_id: String,
    pub outbox_event_id: String,
    pub handler_name: String,
    pub event_name: String,
    pub event_version: u16,
    pub source_module: String,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub correlation_id: String,
    pub causation_id: Option<String>,
    pub occurred_at: String,
    pub actor: ActorContext,
    pub trace: TraceContext,
    pub payload: Value,
    pub headers: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderEventHandleResponse {
    #[serde(default)]
    pub actions: Vec<ProviderEventResultAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProviderEventResultAction {
    EnqueueFunction { function_name: String, input: Value },
}
