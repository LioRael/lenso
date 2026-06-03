use platform_module::{AdminPage, ModuleManifest};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type RemoteManifestResponse = ModuleManifest;

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
