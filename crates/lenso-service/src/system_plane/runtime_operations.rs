use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use utoipa::ToSchema;

pub const RUNTIME_OPERATIONS_PROTOCOL: &str = "lenso.system-plane.runtime-operations.v1";
pub const RUNTIME_OPERATIONS_PATH: &str = "/system-plane/v1/runtime-operations";
pub const RUNTIME_OPERATIONS_FEATURE_FUNCTION_RETRY: &str = "function-run-retry";
pub const RUNTIME_OPERATIONS_FEATURE_OUTBOX_RETRY: &str = "outbox-event-retry";
pub const RUNTIME_OPERATIONS_FEATURE_EVIDENCE: &str = "operation-evidence";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeOperationTargetKind {
    FunctionRun,
    OutboxEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeOperationTargetStatus {
    Pending,
    Processing,
    Running,
    Completed,
    Published,
    Failed,
    Dead,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeOperationDesiredOutcome {
    Retry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ManagementActorKind {
    Operator,
    Automation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeOperationTarget {
    pub kind: RuntimeOperationTargetKind,
    pub target_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeOperationTargetSnapshot {
    pub protocol: String,
    pub service_id: String,
    pub service_revision: String,
    pub target: RuntimeOperationTarget,
    pub target_revision: String,
    pub observed_at_unix_ms: u64,
    pub target_name: String,
    pub status: RuntimeOperationTargetStatus,
    pub attempts: u32,
    pub max_attempts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagementActor {
    pub kind: ManagementActorKind,
    pub subject: String,
    pub delegated_authority_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagementApproval {
    pub approval_id: String,
    pub approval_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagementIntent {
    pub protocol: String,
    pub intent_id: String,
    pub service_id: String,
    pub service_revision: String,
    pub target: RuntimeOperationTarget,
    pub desired_outcome: RuntimeOperationDesiredOutcome,
    pub expected_target_revision: String,
    pub actor: ManagementActor,
    pub approvals: Vec<ManagementApproval>,
    pub deadline_unix_ms: u64,
    pub idempotency_key: String,
    pub capability_contract_id: String,
    pub capability_schema_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeOperationRisk {
    DuplicateExternalEffect,
    RepeatedBusinessNotification,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeOperationAvailabilityImpact {
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeOperationCompensationSupport {
    NotAvailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeOperationPlanReceipt {
    pub protocol: String,
    pub intent_digest: String,
    pub plan_digest: String,
    pub service_id: String,
    pub service_revision: String,
    pub target: RuntimeOperationTarget,
    pub expected_target_revision: String,
    pub expected_effects: Vec<String>,
    pub risks: Vec<RuntimeOperationRisk>,
    pub availability_impact: RuntimeOperationAvailabilityImpact,
    pub compensation_support: RuntimeOperationCompensationSupport,
    pub approval_required: bool,
    pub expires_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeOperationSubmission {
    pub intent: ManagementIntent,
    pub plan: RuntimeOperationPlanReceipt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeOperationState {
    Accepted,
    Succeeded,
    Rejected,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeOperationAcknowledgement {
    pub protocol: String,
    pub operation_id: String,
    pub idempotency_key: String,
    pub intent_digest: String,
    pub plan_digest: String,
    pub state: RuntimeOperationState,
    pub accepted_at_unix_ms: u64,
    pub authorization_epoch: u64,
    pub enrollment_receipt_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeOperationEvidence {
    pub protocol: String,
    pub operation_id: String,
    pub sequence: u64,
    pub state: RuntimeOperationState,
    pub recorded_at_unix_ms: u64,
    pub service_id: String,
    pub service_revision: String,
    pub target: RuntimeOperationTarget,
    pub target_revision_before: String,
    pub target_revision_after: Option<String>,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "document", rename_all = "snake_case")]
pub enum RuntimeOperationsMessage {
    TargetSnapshot(RuntimeOperationTargetSnapshot),
    Intent(ManagementIntent),
    PlanReceipt(RuntimeOperationPlanReceipt),
    Submission(RuntimeOperationSubmission),
    Acknowledgement(RuntimeOperationAcknowledgement),
    Evidence(RuntimeOperationEvidence),
}

#[must_use]
pub fn management_intent_digest(intent: &ManagementIntent) -> String {
    digest_json(intent)
}

#[must_use]
pub fn runtime_operation_plan_digest(plan: &RuntimeOperationPlanReceipt) -> String {
    let mut unsigned = plan.clone();
    unsigned.plan_digest.clear();
    digest_json(&unsigned)
}

#[must_use]
pub fn runtime_operations_schema() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(RuntimeOperationsMessage))
        .expect("Runtime Operations schema serializes");
    schema["$id"] = Value::String(
        "https://contracts.lenso.local/system-plane/lenso.system-plane.runtime-operations.v1.schema.json"
            .to_owned(),
    );
    schema["title"] = Value::String("Lenso Runtime Operations Messages".to_owned());
    for definition in [
        "RuntimeOperationTargetSnapshot",
        "ManagementIntent",
        "RuntimeOperationPlanReceipt",
        "RuntimeOperationAcknowledgement",
        "RuntimeOperationEvidence",
    ] {
        schema["$defs"][definition]["properties"]["protocol"] =
            json!({ "const": RUNTIME_OPERATIONS_PROTOCOL });
    }
    patch_digest(&mut schema, "ManagementActor", "delegatedAuthorityDigest");
    patch_digest(&mut schema, "ManagementApproval", "approvalDigest");
    patch_digest(&mut schema, "ManagementIntent", "expectedTargetRevision");
    patch_digest(&mut schema, "ManagementIntent", "capabilitySchemaDigest");
    patch_digest(&mut schema, "RuntimeOperationPlanReceipt", "intentDigest");
    patch_digest(&mut schema, "RuntimeOperationPlanReceipt", "planDigest");
    patch_digest(
        &mut schema,
        "RuntimeOperationPlanReceipt",
        "expectedTargetRevision",
    );
    patch_digest(
        &mut schema,
        "RuntimeOperationAcknowledgement",
        "intentDigest",
    );
    patch_digest(&mut schema, "RuntimeOperationAcknowledgement", "planDigest");
    patch_digest(
        &mut schema,
        "RuntimeOperationAcknowledgement",
        "enrollmentReceiptDigest",
    );
    patch_digest(
        &mut schema,
        "RuntimeOperationEvidence",
        "targetRevisionBefore",
    );
    schema
}

#[must_use]
pub fn runtime_operations_schema_digest() -> String {
    digest_bytes(
        &serde_json::to_vec(&runtime_operations_schema())
            .expect("Runtime Operations schema serializes to bytes"),
    )
}

fn patch_digest(schema: &mut Value, definition: &str, field: &str) {
    schema["$defs"][definition]["properties"][field]["pattern"] = json!("^sha256:[0-9a-f]{64}$");
}

fn digest_json<T: Serialize>(value: &T) -> String {
    digest_bytes(&serde_json::to_vec(value).expect("Runtime Operations document serializes"))
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("sha256:{}", hex(&Sha256::digest(bytes)))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
