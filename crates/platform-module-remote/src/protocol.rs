use platform_core::{ActorContext, TraceContext};
use platform_module::{AdminPage, ModuleManifest};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
