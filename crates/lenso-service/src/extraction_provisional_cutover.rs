use crate::{
    ExtractionQuiescenceRun, ExtractionQuiescenceStatus, ExtractionVerificationResult,
    ExtractionVerificationStatus, extraction_input_digest,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

pub const EXTRACTION_PROVISIONAL_CUTOVER_PROTOCOL: &str = "lenso.extraction-provisional-cutover.v1";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionProvisionalCutoverStatus {
    Provisional,
    Verified,
    RolledBack,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionTrafficRoute {
    Linked,
    CandidateVerificationOnly,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionProvisionalCutoverIssueCode {
    PlanStale,
    AuthorityChanged,
    SourceNotQuiesced,
    FinalReconciliationMissing,
    CandidateUnhealthy,
    CompatibilityVerificationFailed,
    PolicyEvidenceFailed,
    StoryComparisonFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionProvisionalCutoverInputs {
    pub plan_id: String,
    pub plan_digest: String,
    pub authority_revision: String,
    pub routing_revision: String,
    pub candidate_service_id: String,
    pub candidate_healthy: bool,
    pub verification: ExtractionVerificationResult,
    pub quiescence: ExtractionQuiescenceRun,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionCutoverReceipt {
    pub step_id: String,
    pub step_digest: String,
    pub from_revision: String,
    pub to_revision: String,
    pub outcome: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionCutoverEvidence {
    pub kind: String,
    pub subject: String,
    pub digest: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionProvisionalCutoverRun {
    pub protocol: String,
    pub cutover_id: String,
    pub cutover_digest: String,
    pub revision: u64,
    pub status: ExtractionProvisionalCutoverStatus,
    pub plan_id: String,
    pub plan_digest: String,
    pub authority_revision: String,
    pub routing_revision_before: String,
    pub routing_revision_current: String,
    pub candidate_service_id: String,
    pub verification_digest: String,
    pub quiescence_digest: String,
    pub source_high_water_mark: String,
    pub destination_checkpoint: String,
    pub route: ExtractionTrafficRoute,
    pub external_mutations_paused: bool,
    pub linked_mutations_open: bool,
    pub linked_authoritative: bool,
    pub candidate_authoritative: bool,
    pub candidate_healthy: bool,
    pub declared_verification_traffic_only: bool,
    pub verification_effects_isolated: bool,
    pub linked_business_probe_passed: bool,
    #[serde(default)]
    pub apply_receipts: Vec<ExtractionCutoverReceipt>,
    #[serde(default)]
    pub rollback_receipts: Vec<ExtractionCutoverReceipt>,
    #[serde(default)]
    pub evidence: Vec<ExtractionCutoverEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractionProvisionalCutoverError {
    pub code: ExtractionProvisionalCutoverIssueCode,
    pub message: String,
    pub next_actions: Vec<String>,
}

impl fmt::Display for ExtractionProvisionalCutoverError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ExtractionProvisionalCutoverError {}

pub fn start_provisional_cutover(
    inputs: ExtractionProvisionalCutoverInputs,
) -> Result<ExtractionProvisionalCutoverRun, ExtractionProvisionalCutoverError> {
    if inputs.plan_id != inputs.quiescence.plan_id
        || inputs.plan_digest != inputs.quiescence.plan_digest
        || inputs.plan_id != inputs.verification.plan_id
    {
        return Err(error(
            ExtractionProvisionalCutoverIssueCode::PlanStale,
            "Provisional Cutover evidence does not belong to one exact plan.",
            "Regenerate verification and quiescence evidence for the current plan.",
        ));
    }
    if inputs.authority_revision != inputs.quiescence.expected_authority_revision {
        return Err(error(
            ExtractionProvisionalCutoverIssueCode::AuthorityChanged,
            "Authority changed after quiescence began.",
            "Restore or regenerate evidence for the current linked authority.",
        ));
    }
    if inputs.quiescence.status != ExtractionQuiescenceStatus::Quiesced
        || !inputs.quiescence.linked_mutations_paused
        || !inputs.quiescence.linked_authority_remains_authoritative
    {
        return Err(error(
            ExtractionProvisionalCutoverIssueCode::SourceNotQuiesced,
            "Linked source is not stably quiesced.",
            "Complete drain, final delta, and reconciliation under the write pause.",
        ));
    }
    if inputs.quiescence.stable_source_high_water_mark.is_none()
        || inputs.quiescence.destination_checkpoint.is_none()
    {
        return Err(error(
            ExtractionProvisionalCutoverIssueCode::FinalReconciliationMissing,
            "Stable final reconciliation pins are missing.",
            "Record the final high-water mark and destination checkpoint.",
        ));
    }
    if !inputs.candidate_healthy {
        return Err(error(
            ExtractionProvisionalCutoverIssueCode::CandidateUnhealthy,
            "Candidate health verification failed.",
            "Restore candidate health before provisional routing.",
        ));
    }
    if inputs.verification.status != ExtractionVerificationStatus::Verified
        || !inputs.verification.provisional_cutover_eligible
    {
        return Err(error(
            ExtractionProvisionalCutoverIssueCode::CompatibilityVerificationFailed,
            "Compatibility and behavior verification did not pass.",
            "Remediate all verification blockers before provisional routing.",
        ));
    }
    let identity = digest(&(
        inputs.plan_id.as_str(),
        inputs.plan_digest.as_str(),
        inputs.authority_revision.as_str(),
        inputs.routing_revision.as_str(),
        inputs.verification.verification_digest.as_str(),
        inputs.quiescence.quiescence_digest.as_str(),
    ));
    let provisional_routing_revision = format!("provisional:{identity}");
    let apply_receipt = receipt(
        "route-verification-traffic",
        &inputs.routing_revision,
        &provisional_routing_revision,
        "applied",
    );
    let mut run = ExtractionProvisionalCutoverRun {
        protocol: EXTRACTION_PROVISIONAL_CUTOVER_PROTOCOL.to_owned(),
        cutover_id: format!("extraction-cutover:{identity}"),
        cutover_digest: String::new(),
        revision: 1,
        status: ExtractionProvisionalCutoverStatus::Provisional,
        plan_id: inputs.plan_id,
        plan_digest: inputs.plan_digest,
        authority_revision: inputs.authority_revision,
        routing_revision_before: inputs.routing_revision,
        routing_revision_current: provisional_routing_revision,
        candidate_service_id: inputs.candidate_service_id,
        verification_digest: inputs.verification.verification_digest,
        quiescence_digest: inputs.quiescence.quiescence_digest,
        source_high_water_mark: inputs
            .quiescence
            .stable_source_high_water_mark
            .expect("validated"),
        destination_checkpoint: inputs.quiescence.destination_checkpoint.expect("validated"),
        route: ExtractionTrafficRoute::CandidateVerificationOnly,
        external_mutations_paused: true,
        linked_mutations_open: false,
        linked_authoritative: true,
        candidate_authoritative: false,
        candidate_healthy: true,
        declared_verification_traffic_only: true,
        verification_effects_isolated: true,
        linked_business_probe_passed: false,
        apply_receipts: vec![apply_receipt],
        rollback_receipts: Vec::new(),
        evidence: vec![evidence(
            "provisional_routing",
            "candidate-verification-only",
            &identity,
            "Only declared read-only, recorded, or isolated synthetic verification traffic is routed to the candidate.",
        )],
    };
    refresh(&mut run);
    Ok(run)
}

#[must_use]
pub fn verify_provisional_cutover(
    mut run: ExtractionProvisionalCutoverRun,
    audit_identity: &str,
) -> ExtractionProvisionalCutoverRun {
    if run.status == ExtractionProvisionalCutoverStatus::Provisional {
        run.status = ExtractionProvisionalCutoverStatus::Verified;
        run.evidence.push(evidence(
            "provisional_verification",
            audit_identity,
            &run.verification_digest,
            "Candidate provisional verification passed while external mutations remained paused.",
        ));
        run.revision += 1;
        refresh(&mut run);
    }
    run
}

#[must_use]
pub fn fail_provisional_cutover(
    mut run: ExtractionProvisionalCutoverRun,
    failure: &str,
    audit_identity: &str,
    linked_business_probe_passed: bool,
) -> ExtractionProvisionalCutoverRun {
    if run.status == ExtractionProvisionalCutoverStatus::RolledBack {
        return run;
    }
    let restore_routing = receipt(
        "restore-linked-routing",
        &run.routing_revision_current,
        &run.routing_revision_before,
        "restored",
    );
    let reopen_mutations = receipt(
        "reopen-linked-mutations",
        "paused",
        "open",
        if linked_business_probe_passed {
            "validated"
        } else {
            "probe_failed"
        },
    );
    run.rollback_receipts = vec![restore_routing, reopen_mutations];
    run.status = ExtractionProvisionalCutoverStatus::RolledBack;
    run.routing_revision_current = run.routing_revision_before.clone();
    run.route = ExtractionTrafficRoute::Linked;
    run.external_mutations_paused = false;
    run.linked_mutations_open = true;
    run.linked_authoritative = true;
    run.candidate_authoritative = false;
    run.linked_business_probe_passed = linked_business_probe_passed;
    run.evidence.push(evidence(
        "rollback",
        audit_identity,
        failure,
        &format!(
            "Candidate verification failed: {failure}. Linked routing and authority were restored without reverse data movement."
        ),
    ));
    run.revision += 1;
    refresh(&mut run);
    run
}

fn error(
    code: ExtractionProvisionalCutoverIssueCode,
    message: &str,
    next_action: &str,
) -> ExtractionProvisionalCutoverError {
    ExtractionProvisionalCutoverError {
        code,
        message: message.to_owned(),
        next_actions: vec![next_action.to_owned()],
    }
}

fn receipt(
    step_id: &str,
    from_revision: &str,
    to_revision: &str,
    outcome: &str,
) -> ExtractionCutoverReceipt {
    let step_digest = digest(&(step_id, from_revision, to_revision, outcome));
    ExtractionCutoverReceipt {
        step_id: step_id.to_owned(),
        step_digest,
        from_revision: from_revision.to_owned(),
        to_revision: to_revision.to_owned(),
        outcome: outcome.to_owned(),
    }
}

fn evidence(kind: &str, subject: &str, value: &str, detail: &str) -> ExtractionCutoverEvidence {
    ExtractionCutoverEvidence {
        kind: kind.to_owned(),
        subject: subject.to_owned(),
        digest: extraction_input_digest(value.as_bytes()),
        detail: detail.to_owned(),
    }
}

fn refresh(run: &mut ExtractionProvisionalCutoverRun) {
    run.cutover_digest.clear();
    run.cutover_digest = digest(run);
}

fn digest(value: &impl Serialize) -> String {
    extraction_input_digest(&serde_json::to_vec(value).expect("Cutover values serialize"))
}
