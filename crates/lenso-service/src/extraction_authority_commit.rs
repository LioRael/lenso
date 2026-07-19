use crate::{
    ExtractionProvisionalCutoverRun, ExtractionProvisionalCutoverStatus, extraction_input_digest,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

pub const EXTRACTION_AUTHORITY_COMMIT_PROTOCOL: &str = "lenso.extraction-authority-commit.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionApproval {
    pub approval_id: String,
    pub approval_digest: String,
    pub approver: String,
    pub authorized: bool,
    pub cutover_id: String,
    pub cutover_digest: String,
    pub plan_digest: String,
    pub authority_revision: String,
    pub destination_checkpoint: String,
    pub verification_digest: String,
    pub quiescence_digest: String,
    pub candidate_service_id: String,
}

impl ExtractionApproval {
    #[must_use]
    pub fn bind(
        cutover: &ExtractionProvisionalCutoverRun,
        approval_id: impl Into<String>,
        approver: impl Into<String>,
        authorized: bool,
    ) -> Self {
        let mut approval = Self {
            approval_id: approval_id.into(),
            approval_digest: String::new(),
            approver: approver.into(),
            authorized,
            cutover_id: cutover.cutover_id.clone(),
            cutover_digest: cutover.cutover_digest.clone(),
            plan_digest: cutover.plan_digest.clone(),
            authority_revision: cutover.authority_revision.clone(),
            destination_checkpoint: cutover.destination_checkpoint.clone(),
            verification_digest: cutover.verification_digest.clone(),
            quiescence_digest: cutover.quiescence_digest.clone(),
            candidate_service_id: cutover.candidate_service_id.clone(),
        };
        approval.approval_digest = approval_digest(&approval);
        approval
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionAuthorityCommitInputs {
    pub cutover: ExtractionProvisionalCutoverRun,
    pub approval: ExtractionApproval,
    pub current_authority_revision: String,
    pub current_routing_revision: String,
    pub current_system_graph_revision: String,
    pub candidate_healthy: bool,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionAuthorityCommitStatus {
    Committed,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionAuthorityCommitErrorCode {
    ProvisionalVerificationIncomplete,
    ApprovalUnauthorized,
    ApprovalInvalid,
    ApprovalStale,
    AuthorityChanged,
    RoutingChanged,
    CandidateUnhealthy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionAuthorityCommitError {
    pub code: ExtractionAuthorityCommitErrorCode,
    pub message: String,
    pub next_actions: Vec<String>,
    pub mutation_started: bool,
}

impl fmt::Display for ExtractionAuthorityCommitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ExtractionAuthorityCommitError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionAuthorityCommitReceipt {
    pub receipt_id: String,
    pub receipt_digest: String,
    pub expected_authority_revision: String,
    pub expected_routing_revision: String,
    pub expected_system_graph_revision: String,
    pub committed_authority_revision: String,
    pub committed_routing_revision: String,
    pub committed_system_graph_revision: String,
    pub candidate_service_id: String,
    pub outcome: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionAuthorityCommitResult {
    pub protocol: String,
    pub commit_id: String,
    pub commit_digest: String,
    pub status: ExtractionAuthorityCommitStatus,
    pub plan_digest: String,
    pub approval: ExtractionApproval,
    pub authority_revision: String,
    pub routing_revision: String,
    pub system_graph_revision: String,
    pub candidate_service_id: String,
    pub candidate_authoritative: bool,
    pub linked_authoritative: bool,
    pub candidate_mutations_open: bool,
    pub linked_recovery_read_only: bool,
    pub source_cleanup_performed: bool,
    #[serde(default)]
    pub autonomous_mutation_ids: Vec<String>,
    pub fast_rollback_blocked: bool,
    pub business_execution_requires_runtime_console: bool,
    pub business_execution_requires_system_plane: bool,
    pub commit_receipts: Vec<ExtractionAuthorityCommitReceipt>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionFastRollbackIssueCode {
    ReverseMigrationEvidenceRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionFastRollbackError {
    pub code: ExtractionFastRollbackIssueCode,
    pub message: String,
    pub next_actions: Vec<String>,
}

impl fmt::Display for ExtractionFastRollbackError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ExtractionFastRollbackError {}

pub fn commit_extraction_authority(
    inputs: ExtractionAuthorityCommitInputs,
) -> Result<ExtractionAuthorityCommitResult, ExtractionAuthorityCommitError> {
    let cutover = &inputs.cutover;
    if cutover.status != ExtractionProvisionalCutoverStatus::Verified
        || !cutover.external_mutations_paused
        || !cutover.linked_authoritative
        || cutover.candidate_authoritative
    {
        return Err(error(
            ExtractionAuthorityCommitErrorCode::ProvisionalVerificationIncomplete,
            "Provisional Cutover is not verified in a single-authority paused state.",
            "Repeat provisional verification before requesting approval.",
        ));
    }
    if !inputs.approval.authorized || inputs.approval.approver.trim().is_empty() {
        return Err(error(
            ExtractionAuthorityCommitErrorCode::ApprovalUnauthorized,
            "The Approval Boundary was not crossed by an authorized identity.",
            "Request approval from an authorized operator.",
        ));
    }
    if inputs.approval.approval_digest != approval_digest(&inputs.approval) {
        return Err(error(
            ExtractionAuthorityCommitErrorCode::ApprovalInvalid,
            "Approval integrity validation failed.",
            "Discard the changed approval and bind a new one.",
        ));
    }
    if inputs.approval.cutover_id != cutover.cutover_id
        || inputs.approval.cutover_digest != cutover.cutover_digest
        || inputs.approval.plan_digest != cutover.plan_digest
        || inputs.approval.authority_revision != cutover.authority_revision
        || inputs.approval.destination_checkpoint != cutover.destination_checkpoint
        || inputs.approval.verification_digest != cutover.verification_digest
        || inputs.approval.quiescence_digest != cutover.quiescence_digest
        || inputs.approval.candidate_service_id != cutover.candidate_service_id
    {
        return Err(error(
            ExtractionAuthorityCommitErrorCode::ApprovalStale,
            "Approval does not match the exact verified Cutover state.",
            "Bind a fresh approval to the current Cutover evidence.",
        ));
    }
    if inputs.current_authority_revision != cutover.authority_revision {
        return Err(error(
            ExtractionAuthorityCommitErrorCode::AuthorityChanged,
            "Authority revision changed before commit.",
            "Regenerate Cutover evidence from the current authority revision.",
        ));
    }
    if inputs.current_routing_revision != cutover.routing_revision_current {
        return Err(error(
            ExtractionAuthorityCommitErrorCode::RoutingChanged,
            "Routing revision changed before commit.",
            "Restore the verified provisional route or restart Cutover.",
        ));
    }
    if !inputs.candidate_healthy || !cutover.candidate_healthy {
        return Err(error(
            ExtractionAuthorityCommitErrorCode::CandidateUnhealthy,
            "Candidate health changed before commit.",
            "Restore candidate health and repeat verification.",
        ));
    }
    let commit_identity = digest(&(
        inputs.approval.approval_digest.as_str(),
        inputs.current_authority_revision.as_str(),
        inputs.current_routing_revision.as_str(),
        inputs.current_system_graph_revision.as_str(),
    ));
    let authority_revision = format!("autonomous-authority:{commit_identity}");
    let routing_revision = format!("autonomous-routing:{commit_identity}");
    let system_graph_revision = format!("autonomous-system-graph:{commit_identity}");
    let mut receipt = ExtractionAuthorityCommitReceipt {
        receipt_id: format!("extraction-authority-commit-receipt:{commit_identity}"),
        receipt_digest: String::new(),
        expected_authority_revision: inputs.current_authority_revision,
        expected_routing_revision: inputs.current_routing_revision,
        expected_system_graph_revision: inputs.current_system_graph_revision,
        committed_authority_revision: authority_revision.clone(),
        committed_routing_revision: routing_revision.clone(),
        committed_system_graph_revision: system_graph_revision.clone(),
        candidate_service_id: cutover.candidate_service_id.clone(),
        outcome: "committed_single_compare_and_set".to_owned(),
    };
    receipt.receipt_digest = digest(&receipt_without_digest(&receipt));
    let mut result = ExtractionAuthorityCommitResult {
        protocol: EXTRACTION_AUTHORITY_COMMIT_PROTOCOL.to_owned(),
        commit_id: format!("extraction-authority-commit:{commit_identity}"),
        commit_digest: String::new(),
        status: ExtractionAuthorityCommitStatus::Committed,
        plan_digest: cutover.plan_digest.clone(),
        approval: inputs.approval,
        authority_revision,
        routing_revision,
        system_graph_revision,
        candidate_service_id: cutover.candidate_service_id.clone(),
        candidate_authoritative: true,
        linked_authoritative: false,
        candidate_mutations_open: true,
        linked_recovery_read_only: true,
        source_cleanup_performed: false,
        autonomous_mutation_ids: Vec::new(),
        fast_rollback_blocked: false,
        business_execution_requires_runtime_console: false,
        business_execution_requires_system_plane: false,
        commit_receipts: vec![receipt],
    };
    refresh(&mut result);
    Ok(result)
}

#[must_use]
pub fn record_autonomous_mutation(
    mut result: ExtractionAuthorityCommitResult,
    mutation_id: impl Into<String>,
) -> ExtractionAuthorityCommitResult {
    let mutation_id = mutation_id.into();
    if !result.autonomous_mutation_ids.contains(&mutation_id) {
        result.autonomous_mutation_ids.push(mutation_id);
        result.autonomous_mutation_ids.sort();
    }
    result.fast_rollback_blocked = !result.autonomous_mutation_ids.is_empty();
    refresh(&mut result);
    result
}

pub fn request_fast_extraction_rollback(
    result: &ExtractionAuthorityCommitResult,
    reviewed_reverse_migration_and_reconciliation: bool,
) -> Result<(), ExtractionFastRollbackError> {
    if result.fast_rollback_blocked && !reviewed_reverse_migration_and_reconciliation {
        return Err(ExtractionFastRollbackError {
            code: ExtractionFastRollbackIssueCode::ReverseMigrationEvidenceRequired,
            message: "Fast rollback is blocked after Autonomous writes began.".to_owned(),
            next_actions: vec!["Review a reverse-migration and reconciliation plan before changing authority again.".to_owned()],
        });
    }
    Ok(())
}

fn error(
    code: ExtractionAuthorityCommitErrorCode,
    message: &str,
    next_action: &str,
) -> ExtractionAuthorityCommitError {
    ExtractionAuthorityCommitError {
        code,
        message: message.to_owned(),
        next_actions: vec![next_action.to_owned()],
        mutation_started: false,
    }
}

fn approval_digest(approval: &ExtractionApproval) -> String {
    let mut value = approval.clone();
    value.approval_digest.clear();
    digest(&value)
}

fn receipt_without_digest(
    receipt: &ExtractionAuthorityCommitReceipt,
) -> ExtractionAuthorityCommitReceipt {
    let mut value = receipt.clone();
    value.receipt_digest.clear();
    value
}

fn refresh(result: &mut ExtractionAuthorityCommitResult) {
    result.commit_digest.clear();
    result.commit_digest = digest(result);
}

fn digest(value: &impl Serialize) -> String {
    extraction_input_digest(&serde_json::to_vec(value).expect("Authority commit values serialize"))
}
