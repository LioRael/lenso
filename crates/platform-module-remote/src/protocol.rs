use platform_core::{ActorContext, TraceContext};
use platform_module::{AdminPage, ModuleManifest};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub type RemoteManifestResponse = ModuleManifest;

/// Standard error response shape for the remote module protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteErrorEnvelope {
    pub error: RemoteErrorBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteErrorBody {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub retryable: bool,
    #[serde(default)]
    pub details: Vec<RemoteErrorDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteErrorDetail {
    pub field: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteListResponse {
    pub records: Vec<Value>,
    pub next_cursor: Option<String>,
}

impl From<RemoteListResponse> for AdminPage {
    fn from(value: RemoteListResponse) -> Self {
        Self {
            records: value.records,
            next_cursor: value.next_cursor,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteGetResponse {
    pub record: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteActionInvokeResponse {
    pub result: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteQueryResponse {
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteAdminListRequest {
    pub entity: String,
    pub limit: i64,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteAdminGetRequest {
    pub entity: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteAdminActionInvokeRequest {
    pub action: String,
    pub input: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteAdminQueryRequest {
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteHttpProxyInvokeRequest {
    pub request_id: String,
    pub correlation_id: String,
    pub module_name: String,
    pub method: String,
    pub declared_path: String,
    pub remote_path: String,
    pub path_params: BTreeMap<String, String>,
    pub headers: BTreeMap<String, String>,
    pub body: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteHttpProxyInvokeResponse {
    pub status_code: u16,
    pub body: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteFunctionInvokeRequest {
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
pub struct RemoteFunctionInvokeResponse {
    pub output: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteEventHandleRequest {
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
pub struct RemoteEventHandleResponse {
    #[serde(default)]
    pub actions: Vec<RemoteEventResultAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RemoteEventResultAction {
    EnqueueFunction { function_name: String, input: Value },
}
