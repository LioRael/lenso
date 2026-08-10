use lenso_contracts::digest_json;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::num::NonZeroU32;
use utoipa::ToSchema;

const WORKLOAD_CONTROL_SCALAR_MAX_LENGTH: usize = 255;
const WORKLOAD_CONTROL_SAFE_MESSAGE_MAX_LENGTH: usize = 1_024;

pub const WORKLOAD_CONTROL_PROTOCOL: &str = "lenso.workload-control.v1";
pub const WORKLOAD_CONTROL_OBSERVE_PATH: &str = "/workload-control/v1/observe";
pub const WORKLOAD_CONTROL_OPERATIONS_PATH: &str = "/workload-control/v1/operations";
pub const WORKLOAD_CONTROL_OPERATION_PATH: &str = "/workload-control/v1/operations/{operationId}";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkloadReference {
    pub system_id: String,
    pub service_id: String,
    pub workload_id: String,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadControlCapability {
    Suspend,
    Resume,
    Restart,
    Scale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadOperationalState {
    Running,
    Suspended,
    Transitioning,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadProtection {
    Controllable,
    ControlPlane,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkloadControlAction {
    Suspend,
    Resume,
    Restart,
    Scale {
        #[serde(rename = "targetCapacity")]
        #[schema(value_type = u32, minimum = 1)]
        target_capacity: NonZeroU32,
    },
}

impl<'de> Deserialize<'de> for WorkloadControlAction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        parse_workload_control_action(&value).map_err(de::Error::custom)
    }
}

fn parse_workload_control_action(value: &Value) -> Result<WorkloadControlAction, &'static str> {
    let object = value
        .as_object()
        .ok_or("Workload Control action must be an object")?;
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .ok_or("Workload Control action requires kind")?;
    match kind {
        "suspend" if object.len() == 1 => Ok(WorkloadControlAction::Suspend),
        "resume" if object.len() == 1 => Ok(WorkloadControlAction::Resume),
        "restart" if object.len() == 1 => Ok(WorkloadControlAction::Restart),
        "scale" if object.len() == 2 => {
            let capacity = object
                .get("targetCapacity")
                .and_then(Value::as_u64)
                .and_then(|capacity| u32::try_from(capacity).ok())
                .and_then(NonZeroU32::new)
                .ok_or("Scale requires a positive targetCapacity")?;
            Ok(WorkloadControlAction::Scale {
                target_capacity: capacity,
            })
        }
        "suspend" | "resume" | "restart" | "scale" => {
            Err("Workload Control action contains unknown fields")
        }
        _ => Err("Workload Control action kind is unsupported"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadControlActorKind {
    Operator,
    Automation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkloadControlActor {
    pub kind: WorkloadControlActorKind,
    pub subject: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkloadMutationRequest {
    pub protocol: String,
    pub workload: WorkloadReference,
    pub action: WorkloadControlAction,
    pub observed_revision: String,
    pub idempotency_key: String,
    pub actor: WorkloadControlActor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadControlAuthorityDecision {
    Accepted,
    Denied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkloadControlAuthority {
    pub adapter_id: String,
    pub decision: WorkloadControlAuthorityDecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadOperationPhase {
    Accepted,
    Executing,
    Verifying,
    Succeeded,
    Failed,
    Denied,
}

impl WorkloadOperationPhase {
    #[must_use]
    pub const fn can_advance_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Accepted, Self::Accepted)
                | (
                    Self::Accepted,
                    Self::Executing | Self::Verifying | Self::Succeeded | Self::Failed
                )
                | (Self::Executing, Self::Executing)
                | (
                    Self::Executing,
                    Self::Verifying | Self::Succeeded | Self::Failed
                )
                | (
                    Self::Verifying,
                    Self::Verifying | Self::Succeeded | Self::Failed
                )
                | (Self::Succeeded, Self::Succeeded)
                | (Self::Failed, Self::Failed)
                | (Self::Denied, Self::Denied)
        )
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Denied)
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadControlErrorCode {
    Unauthenticated,
    Unauthorized,
    UnsupportedAction,
    ProtectedWorkload,
    StaleRevision,
    ActiveMutation,
    IdempotencyConflict,
    AuthorityUnavailable,
    IncompatibleProtocol,
    WorkloadNotFound,
    OperationNotFound,
    InvalidCapacity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkloadControlFailure {
    pub code: WorkloadControlErrorCode,
    /// Sanitized, provider-neutral text limited to 1,024 Unicode characters.
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkloadControlError {
    pub protocol: String,
    pub code: WorkloadControlErrorCode,
    /// Sanitized, provider-neutral text limited to 1,024 Unicode characters.
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_operation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkloadOperationResult {
    pub state: WorkloadOperationalState,
    pub observed_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationRecord {
    pub protocol: String,
    pub operation_id: String,
    pub request: WorkloadMutationRequest,
    pub authority: WorkloadControlAuthority,
    pub phase: WorkloadOperationPhase,
    pub requested_at_unix_ms: u64,
    pub decided_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at_unix_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<WorkloadOperationResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<WorkloadControlFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkloadObservation {
    pub protocol: String,
    pub workload: WorkloadReference,
    pub state: WorkloadOperationalState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_revision: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub capabilities: BTreeSet<WorkloadControlCapability>,
    pub protection: WorkloadProtection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_operation: Option<String>,
    pub observed_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkloadObservationRequest {
    pub protocol: String,
    pub workload: WorkloadReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(
    tag = "kind",
    content = "document",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum WorkloadControlMessage {
    ObservationRequest(WorkloadObservationRequest),
    Observation(WorkloadObservation),
    MutationRequest(WorkloadMutationRequest),
    OperationRecord(OperationRecord),
    Error(WorkloadControlError),
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadControlValidationIssueCode {
    InvalidProtocol,
    InvalidWorkloadReference,
    InvalidMutationRequest,
    InvalidOperationRecord,
    InvalidOperationResult,
    InvalidOperationFailure,
    InvalidErrorDocument,
    KnownStateMissingRevision,
    UnknownStateHasRevision,
    AuthorityDecisionMismatch,
    NonMonotonicTimestamps,
    TerminalOutcomeMismatch,
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkloadControlValidationIssue {
    pub code: WorkloadControlValidationIssueCode,
    pub path: String,
    pub message: String,
    pub next_action: String,
}

#[must_use]
pub fn validate_workload_control_message(
    message: &WorkloadControlMessage,
) -> Vec<WorkloadControlValidationIssue> {
    let mut issues = Vec::new();
    match message {
        WorkloadControlMessage::ObservationRequest(request) => {
            validate_protocol(&request.protocol, "$.document.protocol", &mut issues);
            validate_workload_reference(&request.workload, "$.document.workload", &mut issues);
        }
        WorkloadControlMessage::Observation(observation) => {
            validate_protocol(&observation.protocol, "$.document.protocol", &mut issues);
            validate_workload_reference(&observation.workload, "$.document.workload", &mut issues);
            validate_observation_revision(observation, &mut issues);
        }
        WorkloadControlMessage::MutationRequest(request) => {
            validate_mutation_request(request, "$.document", &mut issues);
        }
        WorkloadControlMessage::OperationRecord(record) => {
            validate_operation_record(record, &mut issues);
        }
        WorkloadControlMessage::Error(error) => {
            validate_error_document(error, &mut issues);
        }
    }
    issues
}

fn validate_protocol(protocol: &str, path: &str, issues: &mut Vec<WorkloadControlValidationIssue>) {
    if protocol != WORKLOAD_CONTROL_PROTOCOL {
        push_validation_issue(
            issues,
            WorkloadControlValidationIssueCode::InvalidProtocol,
            path,
            "protocol does not identify Workload Control v1",
            "Use the exact negotiated Workload Control protocol.",
        );
    }
}

fn validate_workload_reference(
    workload: &WorkloadReference,
    path: &str,
    issues: &mut Vec<WorkloadControlValidationIssue>,
) {
    for (field, value) in [
        ("systemId", workload.system_id.as_str()),
        ("serviceId", workload.service_id.as_str()),
        ("workloadId", workload.workload_id.as_str()),
    ] {
        if !valid_control_scalar(value) {
            push_validation_issue(
                issues,
                WorkloadControlValidationIssueCode::InvalidWorkloadReference,
                &format!("{path}.{field}"),
                "Workload Reference fields must be non-empty stable identities",
                "Use System, Service, and Workload identities declared by the Lenso topology.",
            );
        }
    }
}

fn validate_mutation_request(
    request: &WorkloadMutationRequest,
    path: &str,
    issues: &mut Vec<WorkloadControlValidationIssue>,
) {
    validate_protocol(&request.protocol, &format!("{path}.protocol"), issues);
    validate_workload_reference(&request.workload, &format!("{path}.workload"), issues);
    for (field, value, message) in [
        (
            "observedRevision",
            request.observed_revision.as_str(),
            "mutation requires the authority revision that was observed",
        ),
        (
            "idempotencyKey",
            request.idempotency_key.as_str(),
            "mutation requires a stable idempotency key",
        ),
        (
            "actor.subject",
            request.actor.subject.as_str(),
            "mutation actor requires a stable subject",
        ),
    ] {
        if !valid_control_scalar(value) {
            push_validation_issue(
                issues,
                WorkloadControlValidationIssueCode::InvalidMutationRequest,
                &format!("{path}.{field}"),
                message,
                "Supply the missing authority input before submitting the mutation.",
            );
        }
    }
}

fn validate_observation_revision(
    observation: &WorkloadObservation,
    issues: &mut Vec<WorkloadControlValidationIssue>,
) {
    if observation
        .active_operation
        .as_deref()
        .is_some_and(|operation_id| !valid_control_scalar(operation_id))
    {
        push_validation_issue(
            issues,
            WorkloadControlValidationIssueCode::InvalidOperationRecord,
            "$.document.activeOperation",
            "active operation identity must be non-empty and at most 255 characters when present",
            "Omit an unavailable handle or use the bounded identity assigned by the accepting Adapter.",
        );
    }

    match observation.state {
        WorkloadOperationalState::Unknown if observation.observed_revision.is_some() => {
            push_validation_issue(
                issues,
                WorkloadControlValidationIssueCode::UnknownStateHasRevision,
                "$.document.observedRevision",
                "unknown state cannot carry an observed revision",
                "Remove the revision until the authority supplies current operational state.",
            );
        }
        WorkloadOperationalState::Running
        | WorkloadOperationalState::Suspended
        | WorkloadOperationalState::Transitioning
        | WorkloadOperationalState::Failed
            if observation
                .observed_revision
                .as_deref()
                .is_none_or(|revision| !valid_control_scalar(revision)) =>
        {
            push_validation_issue(
                issues,
                WorkloadControlValidationIssueCode::KnownStateMissingRevision,
                "$.document.observedRevision",
                "known operational state requires the authority's observed revision",
                "Observe the Workload through its active Workload Control Adapter.",
            );
        }
        _ => {}
    }
}

fn validate_operation_record(
    record: &OperationRecord,
    issues: &mut Vec<WorkloadControlValidationIssue>,
) {
    validate_protocol(&record.protocol, "$.document.protocol", issues);
    validate_mutation_request(&record.request, "$.document.request", issues);
    for (path, value) in [
        ("$.document.operationId", record.operation_id.as_str()),
        (
            "$.document.authority.adapterId",
            record.authority.adapter_id.as_str(),
        ),
    ] {
        if !valid_control_scalar(value) {
            push_validation_issue(
                issues,
                WorkloadControlValidationIssueCode::InvalidOperationRecord,
                path,
                "operation identity and Adapter identity must be non-empty and at most 255 characters",
                "Use stable identities assigned by the accepting Adapter.",
            );
        }
    }
    let authority_matches = matches!(
        (record.authority.decision, record.phase),
        (
            WorkloadControlAuthorityDecision::Accepted,
            WorkloadOperationPhase::Accepted
                | WorkloadOperationPhase::Executing
                | WorkloadOperationPhase::Verifying
                | WorkloadOperationPhase::Succeeded
                | WorkloadOperationPhase::Failed
        ) | (
            WorkloadControlAuthorityDecision::Denied,
            WorkloadOperationPhase::Denied
        )
    );
    if !authority_matches {
        push_validation_issue(
            issues,
            WorkloadControlValidationIssueCode::AuthorityDecisionMismatch,
            "$.document.authority.decision",
            "authority decision must agree with the operation phase",
            "Use denied only for an authority denial and accepted for executable operations.",
        );
    }

    let timestamps_are_monotonic = record.requested_at_unix_ms <= record.decided_at_unix_ms
        && record.decided_at_unix_ms <= record.updated_at_unix_ms
        && record
            .finished_at_unix_ms
            .is_none_or(|finished| record.updated_at_unix_ms <= finished);
    if !timestamps_are_monotonic {
        push_validation_issue(
            issues,
            WorkloadControlValidationIssueCode::NonMonotonicTimestamps,
            "$.document",
            "operation timestamps must be monotonic",
            "Preserve request, decision, update, and finish ordering from the Adapter.",
        );
    }

    let outcome_matches = match record.phase {
        WorkloadOperationPhase::Accepted
        | WorkloadOperationPhase::Executing
        | WorkloadOperationPhase::Verifying => {
            record.finished_at_unix_ms.is_none()
                && record.result.is_none()
                && record.failure.is_none()
        }
        WorkloadOperationPhase::Succeeded => {
            record.finished_at_unix_ms.is_some()
                && record.result.is_some()
                && record.failure.is_none()
        }
        WorkloadOperationPhase::Failed | WorkloadOperationPhase::Denied => {
            record.finished_at_unix_ms.is_some()
                && record.result.is_none()
                && record.failure.is_some()
        }
    };
    if !outcome_matches {
        push_validation_issue(
            issues,
            WorkloadControlValidationIssueCode::TerminalOutcomeMismatch,
            "$.document",
            "operation phase, finish timestamp, result, and failure are inconsistent",
            "Use a result only for succeeded and a typed failure only for failed or denied.",
        );
    }

    if record.phase == WorkloadOperationPhase::Succeeded
        && let Some(result) = &record.result
    {
        let is_final_known_state = matches!(
            result.state,
            WorkloadOperationalState::Running | WorkloadOperationalState::Suspended
        );
        if !is_final_known_state {
            push_validation_issue(
                issues,
                WorkloadControlValidationIssueCode::InvalidOperationResult,
                "$.document.result.state",
                "a succeeded operation requires a final known operational state",
                "Use running or suspended only after the Adapter verifies the final state.",
            );
        } else {
            let expected_state = match record.request.action {
                WorkloadControlAction::Suspend => WorkloadOperationalState::Suspended,
                WorkloadControlAction::Resume
                | WorkloadControlAction::Restart
                | WorkloadControlAction::Scale { .. } => WorkloadOperationalState::Running,
            };
            if result.state != expected_state {
                push_validation_issue(
                    issues,
                    WorkloadControlValidationIssueCode::InvalidOperationResult,
                    "$.document.result.state",
                    "the succeeded result state does not match the requested action",
                    "Return suspended for Suspend and running for Resume, Restart, or Scale.",
                );
            }
        }
        if !valid_control_scalar(&result.observed_revision) {
            push_validation_issue(
                issues,
                WorkloadControlValidationIssueCode::InvalidOperationResult,
                "$.document.result.observedRevision",
                "a succeeded operation requires a valid authority revision",
                "Return the non-empty bounded revision observed after verification.",
            );
        }
    }

    if let Some(failure) = &record.failure
        && !valid_safe_message(&failure.message)
    {
        push_validation_issue(
            issues,
            WorkloadControlValidationIssueCode::InvalidOperationFailure,
            "$.document.failure.message",
            "operation failure message must be sanitized, non-empty, and at most 1,024 characters",
            "Return a bounded provider-neutral explanation without secrets or infrastructure identifiers.",
        );
    }
}

fn validate_error_document(
    error: &WorkloadControlError,
    issues: &mut Vec<WorkloadControlValidationIssue>,
) {
    validate_protocol(&error.protocol, "$.document.protocol", issues);
    if !valid_safe_message(&error.message) {
        push_validation_issue(
            issues,
            WorkloadControlValidationIssueCode::InvalidErrorDocument,
            "$.document.message",
            "error message must be sanitized, non-empty, and at most 1,024 characters",
            "Return a bounded provider-neutral explanation without secrets or infrastructure identifiers.",
        );
    }
    for (field, value) in [
        ("operationId", error.operation_id.as_deref()),
        ("currentRevision", error.current_revision.as_deref()),
        ("activeOperation", error.active_operation.as_deref()),
    ] {
        if value.is_some_and(|value| !valid_control_scalar(value)) {
            push_validation_issue(
                issues,
                WorkloadControlValidationIssueCode::InvalidErrorDocument,
                &format!("$.document.{field}"),
                "error references must be non-empty and at most 255 characters when present",
                "Omit unavailable references and use bounded authority identities when present.",
            );
        }
    }
}

fn push_validation_issue(
    issues: &mut Vec<WorkloadControlValidationIssue>,
    code: WorkloadControlValidationIssueCode,
    path: &str,
    message: &str,
    next_action: &str,
) {
    issues.push(WorkloadControlValidationIssue {
        code,
        path: path.to_owned(),
        message: message.to_owned(),
        next_action: next_action.to_owned(),
    });
}

fn valid_control_scalar(value: &str) -> bool {
    !value.trim().is_empty() && value.chars().count() <= WORKLOAD_CONTROL_SCALAR_MAX_LENGTH
}

fn valid_safe_message(value: &str) -> bool {
    !value.trim().is_empty() && value.chars().count() <= WORKLOAD_CONTROL_SAFE_MESSAGE_MAX_LENGTH
}

#[must_use]
pub fn workload_control_schema() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(WorkloadControlMessage))
        .expect("Workload Control schema must serialize");
    schema["$id"] = Value::String(
        "https://contracts.lenso.local/workload-control/lenso.workload-control.v1.schema.json"
            .to_owned(),
    );
    schema["title"] = Value::String("Lenso Workload Control Messages".to_owned());
    for definition in [
        "WorkloadObservationRequest",
        "WorkloadObservation",
        "WorkloadMutationRequest",
        "OperationRecord",
        "WorkloadControlError",
    ] {
        schema["$defs"][definition]["properties"]["protocol"] =
            json!({ "type": "string", "const": WORKLOAD_CONTROL_PROTOCOL });
    }
    for field in ["systemId", "serviceId", "workloadId"] {
        patch_control_scalar(&mut schema, "WorkloadReference", field);
    }
    patch_control_scalar(&mut schema, "WorkloadMutationRequest", "observedRevision");
    patch_control_scalar(&mut schema, "WorkloadMutationRequest", "idempotencyKey");
    patch_control_scalar(&mut schema, "WorkloadControlActor", "subject");
    patch_control_scalar(&mut schema, "WorkloadObservation", "observedRevision");
    patch_control_scalar(&mut schema, "WorkloadObservation", "activeOperation");
    patch_control_scalar(&mut schema, "WorkloadOperationResult", "observedRevision");
    patch_control_scalar(&mut schema, "OperationRecord", "operationId");
    patch_control_scalar(&mut schema, "WorkloadControlAuthority", "adapterId");
    patch_safe_message(&mut schema, "WorkloadControlFailure", "message");
    patch_safe_message(&mut schema, "WorkloadControlError", "message");
    for field in ["operationId", "currentRevision", "activeOperation"] {
        patch_control_scalar(&mut schema, "WorkloadControlError", field);
    }
    schema
}

fn patch_control_scalar(schema: &mut Value, definition: &str, field: &str) {
    schema["$defs"][definition]["properties"][field]["minLength"] = json!(1);
    schema["$defs"][definition]["properties"][field]["maxLength"] =
        json!(WORKLOAD_CONTROL_SCALAR_MAX_LENGTH);
    schema["$defs"][definition]["properties"][field]["pattern"] = json!(r".*\S.*");
}

fn patch_safe_message(schema: &mut Value, definition: &str, field: &str) {
    schema["$defs"][definition]["properties"][field]["minLength"] = json!(1);
    schema["$defs"][definition]["properties"][field]["maxLength"] =
        json!(WORKLOAD_CONTROL_SAFE_MESSAGE_MAX_LENGTH);
    schema["$defs"][definition]["properties"][field]["pattern"] = json!(r".*\S.*");
}

#[must_use]
pub fn workload_control_schema_digest() -> String {
    digest_json(&workload_control_schema()).expect("Workload Control schema must be digestible")
}
