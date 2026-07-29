use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use utoipa::ToSchema;

pub const RUNTIME_OBSERVABILITY_PROTOCOL: &str = "lenso.system-plane.runtime-observability.v1";
pub const RUNTIME_OBSERVABILITY_PATH: &str = "/system-plane/v1/runtime-observability";
pub const RUNTIME_OBSERVABILITY_FEATURE_QUEUE_SUMMARY: &str = "queue-summary";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeObservabilityStatus {
    Healthy,
    Degraded,
    Failing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeQueueKind {
    Outbox,
    Functions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeQueueSummary {
    pub queue: RuntimeQueueKind,
    pub pending: u64,
    pub active: u64,
    pub completed: u64,
    pub failed: u64,
    pub dead: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oldest_pending_age_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oldest_failed_age_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeObservabilitySnapshot {
    pub protocol: String,
    #[schema(min_length = 1)]
    pub service_id: String,
    #[schema(min_length = 1)]
    pub service_revision: String,
    #[schema(min_length = 1)]
    pub snapshot_revision: String,
    #[schemars(with = "String")]
    pub observed_at: DateTime<Utc>,
    pub status: RuntimeObservabilityStatus,
    pub queues: Vec<RuntimeQueueSummary>,
}

#[must_use]
pub fn runtime_observability_schema() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(RuntimeObservabilitySnapshot))
        .expect("Runtime Observability schema serializes");
    schema["$id"] = Value::String(
        "https://contracts.lenso.local/system-plane/lenso.system-plane.runtime-observability.v1.schema.json"
            .to_owned(),
    );
    schema["title"] = Value::String("Lenso Runtime Observability Snapshot".to_owned());
    schema["properties"]["protocol"] = json!({ "const": RUNTIME_OBSERVABILITY_PROTOCOL });
    for field in ["serviceId", "serviceRevision", "snapshotRevision"] {
        schema["properties"][field]["minLength"] = json!(1);
    }
    schema
}

#[must_use]
pub fn runtime_observability_schema_digest() -> String {
    let bytes = serde_json::to_vec(&runtime_observability_schema())
        .expect("Runtime Observability schema serializes to bytes");
    format!("sha256:{}", hex(&Sha256::digest(bytes)))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
