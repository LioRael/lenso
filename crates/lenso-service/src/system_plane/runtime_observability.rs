use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use utoipa::ToSchema;

pub const RUNTIME_OBSERVABILITY_PROTOCOL: &str = "lenso.system-plane.runtime-observability.v1";
pub const RUNTIME_OBSERVABILITY_PATH: &str = "/system-plane/v1/runtime-observability";
pub const RUNTIME_OBSERVABILITY_FEATURE_QUEUE_SUMMARY: &str = "queue-summary";
pub const RUNTIME_OBSERVABILITY_FEATURE_RECOVERY_FEED: &str = "recovery-feed";

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
    #[schema(min_length = 1)]
    pub schema_digest: String,
    #[schema(min_length = 1)]
    pub next_cursor: String,
    #[schemars(with = "String")]
    pub observed_at: DateTime<Utc>,
    pub status: RuntimeObservabilityStatus,
    pub queues: Vec<RuntimeQueueSummary>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeObservationChangeKind {
    Upserted,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeObservationChange {
    pub sequence: u64,
    pub queue: RuntimeQueueKind,
    pub resource_id: String,
    pub change_kind: RuntimeObservationChangeKind,
    #[schemars(with = "String")]
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeObservationContinuity {
    Continuous,
    ResetRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeObservationGapReason {
    InvalidCursor,
    ServiceRevisionChanged,
    SchemaChanged,
    RetentionLost,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeObservationEvidenceGap {
    pub reason: RuntimeObservationGapReason,
    pub message: String,
    pub required_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeObservationFeed {
    pub protocol: String,
    pub service_id: String,
    pub service_revision: String,
    pub schema_digest: String,
    #[schemars(with = "String")]
    pub collected_at: DateTime<Utc>,
    pub continuity: RuntimeObservationContinuity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_gap: Option<RuntimeObservationEvidenceGap>,
    pub changes: Vec<RuntimeObservationChange>,
    pub next_cursor: String,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "document", rename_all = "snake_case")]
pub enum RuntimeObservabilityMessage {
    Snapshot(RuntimeObservabilitySnapshot),
    Feed(RuntimeObservationFeed),
}

#[must_use]
pub fn runtime_observability_schema() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(RuntimeObservabilityMessage))
        .expect("Runtime Observability schema serializes");
    schema["$id"] = Value::String(
        "https://contracts.lenso.local/system-plane/lenso.system-plane.runtime-observability.v1.schema.json"
            .to_owned(),
    );
    schema["title"] = Value::String("Lenso Runtime Observability Messages".to_owned());
    for definition in ["RuntimeObservabilitySnapshot", "RuntimeObservationFeed"] {
        schema["$defs"][definition]["properties"]["protocol"] =
            json!({ "const": RUNTIME_OBSERVABILITY_PROTOCOL });
    }
    for field in [
        "serviceId",
        "serviceRevision",
        "snapshotRevision",
        "schemaDigest",
        "nextCursor",
    ] {
        schema["$defs"]["RuntimeObservabilitySnapshot"]["properties"][field]["minLength"] =
            json!(1);
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
