use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::{RestoreDecision, ServiceRestoreEvidence, extraction_input_digest};

pub const DISASTER_RECOVERY_PLAN_PROTOCOL: &str = "lenso.disaster-recovery-plan.v1";
pub const DISASTER_RECOVERY_EVIDENCE_PROTOCOL: &str = "lenso.disaster-recovery-evidence.v1";
pub const DISASTER_RECOVERY_APPROVAL_BOUNDARY: &str = "single_region_disaster_cutover";
pub const DISASTER_FAILBACK_APPROVAL_BOUNDARY: &str = "single_region_disaster_failback";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DisasterRecoveryPhase {
    Cutover,
    Failback,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DisasterRecoveryDecision {
    Ready,
    Passed,
    Blocked,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DisasterRecoveryIssueCode {
    RestoreEvidenceInvalid,
    RegionTopologyInvalid,
    PrimaryNotFenced,
    PassiveNotReady,
    ApprovalInvalid,
    RecoveryBudgetExceeded,
    IdentityOrContractMismatch,
    FailbackPlanMissing,
    PlanStale,
    ActiveStateChanged,
    ReconciliationIncomplete,
    CleanupIncomplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DisasterRecoveryIssue {
    pub code: DisasterRecoveryIssueCode,
    pub message: String,
    pub remediation: String,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DisasterRecoveryPlanInput {
    pub phase: DisasterRecoveryPhase,
    pub service_id: String,
    pub primary_region: String,
    pub passive_region: String,
    pub restore_evidence: ServiceRestoreEvidence,
    pub expected_release_digest: String,
    pub expected_config_revision_digest: String,
    pub expected_contract_set_digest: String,
    pub expected_active_state_digest: String,
    pub authoritative_environment_count_before: u32,
    pub planned_at_unix_ms: u64,
    pub freshness_horizon_unix_ms: u64,
    pub rpo_budget_ms: u64,
    pub rto_budget_ms: u64,
    pub primary_fenced: bool,
    pub passive_fenced: bool,
    pub passive_health_verified: bool,
    pub passive_identity_verified: bool,
    pub passive_contracts_verified: bool,
    pub failback_steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DisasterRecoveryPlan {
    pub protocol: String,
    pub plan_id: String,
    pub plan_digest: String,
    #[serde(flatten)]
    pub input: DisasterRecoveryPlanInput,
    pub decision: DisasterRecoveryDecision,
    pub issues: Vec<DisasterRecoveryIssue>,
    pub approval_boundary: String,
    pub effects: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DisasterRecoveryApproval {
    pub plan_digest: String,
    pub phase: DisasterRecoveryPhase,
    pub approver: String,
    pub reason: String,
    pub approved_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DisasterRecoveryObservation {
    pub plan_digest: String,
    pub phase: DisasterRecoveryPhase,
    pub observed_at_unix_ms: u64,
    pub active_state_digest: String,
    pub authoritative_environment_count: u32,
    pub primary_fenced: bool,
    pub passive_fenced: bool,
    pub passive_became_authoritative: bool,
    pub primary_became_authoritative: bool,
    pub traffic_switched: bool,
    pub observed_rpo_ms: u64,
    pub observed_rto_ms: u64,
    pub release_digest: String,
    pub config_revision_digest: String,
    pub contract_set_digest: String,
    pub workload_identity_preserved: bool,
    pub duplicate_business_effects: u64,
    pub lost_committed_effects: u64,
    pub requests_events_workflows_stories_verified: bool,
    pub cleanup_complete: bool,
    pub evidence_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DisasterRecoveryEvidence {
    pub protocol: String,
    pub evidence_id: String,
    pub evidence_digest: String,
    pub plan_id: String,
    pub plan_digest: String,
    pub phase: DisasterRecoveryPhase,
    pub service_id: String,
    pub primary_region: String,
    pub passive_region: String,
    pub observed_rpo_ms: u64,
    pub observed_rto_ms: u64,
    pub data_loss_bound_ms: u64,
    pub replay_bound_count: u64,
    pub cleanup_complete: bool,
    pub authoritative_environment_count: u32,
    pub decision: DisasterRecoveryDecision,
    pub issues: Vec<DisasterRecoveryIssue>,
    pub approval_boundary: String,
    pub failback_steps: Vec<String>,
}

#[must_use]
pub fn plan_disaster_recovery(input: DisasterRecoveryPlanInput) -> DisasterRecoveryPlan {
    let mut issues = Vec::new();
    if input.restore_evidence.decision != RestoreDecision::Passed
        || input.restore_evidence.production_mutated
        || input.restore_evidence.service_id != input.service_id
        || !valid_digest(&input.restore_evidence.evidence_digest)
    {
        issues.push(issue(
            DisasterRecoveryIssueCode::RestoreEvidenceInvalid,
            "Disaster recovery lacks a verified passive restore for the exact Service.",
            "Use immutable restore evidence from an isolated target Store.",
            "Repeat backup and restore before planning cutover.",
        ));
    }
    if !valid_digest(&input.expected_active_state_digest)
        || input.authoritative_environment_count_before != 1
        || input.planned_at_unix_ms == 0
        || input.freshness_horizon_unix_ms < input.planned_at_unix_ms
    {
        issues.push(issue(
            DisasterRecoveryIssueCode::PlanStale,
            "The plan lacks a fresh exact active-state revision.",
            "Bind the plan to the current authoritative state and freshness horizon.",
            "Refresh active-state evidence before requesting approval.",
        ));
    }
    if input.primary_region.trim().is_empty()
        || input.passive_region.trim().is_empty()
        || input.primary_region == input.passive_region
    {
        issues.push(issue(
            DisasterRecoveryIssueCode::RegionTopologyInvalid,
            "Active and passive regions are not distinct.",
            "Declare one authoritative primary and one isolated passive region.",
            "Correct the regional topology.",
        ));
    }
    let source_fenced = match input.phase {
        DisasterRecoveryPhase::Cutover => input.primary_fenced,
        DisasterRecoveryPhase::Failback => input.passive_fenced,
    };
    if !source_fenced {
        issues.push(issue(
            DisasterRecoveryIssueCode::PrimaryNotFenced,
            "The primary region can still accept authoritative writes.",
            "Fence the primary before granting authority to the passive region.",
            "Stop before cutover and verify the fencing observation.",
        ));
    }
    if !input.passive_health_verified {
        issues.push(issue(
            DisasterRecoveryIssueCode::PassiveNotReady,
            "The passive Workloads and restored Store are not ready.",
            "Verify health while the passive remains non-authoritative.",
            "Repair the passive region before requesting approval.",
        ));
    }
    if !input.passive_identity_verified || !input.passive_contracts_verified {
        issues.push(issue(
            DisasterRecoveryIssueCode::IdentityOrContractMismatch,
            "The passive region does not preserve Workload Identity or Contract identity.",
            "Bind the passive region to the exact supported release and identities.",
            "Correct the passive deployment before cutover.",
        ));
    }
    if input.failback_steps.is_empty() {
        issues.push(issue(
            DisasterRecoveryIssueCode::FailbackPlanMissing,
            "No stale-safe failback procedure is recorded.",
            "Plan re-seeding, verification, fencing, approval, and traffic reversal.",
            "Add the explicit failback steps before cutover.",
        ));
    }
    let decision = if issues.is_empty() {
        DisasterRecoveryDecision::Ready
    } else {
        DisasterRecoveryDecision::Blocked
    };
    let phase = input.phase;
    let mut plan = DisasterRecoveryPlan {
        protocol: DISASTER_RECOVERY_PLAN_PROTOCOL.to_owned(),
        plan_id: String::new(),
        plan_digest: String::new(),
        input,
        decision,
        issues,
        approval_boundary: approval_boundary(phase).to_owned(),
        effects: match phase {
            DisasterRecoveryPhase::Cutover => vec![
                "fence primary".to_owned(),
                "grant passive authority".to_owned(),
                "switch regional traffic".to_owned(),
            ],
            DisasterRecoveryPhase::Failback => vec![
                "fence passive".to_owned(),
                "grant primary authority".to_owned(),
                "switch regional traffic".to_owned(),
            ],
        },
    };
    plan.plan_digest = digest_without_plan_identity(&plan);
    plan.plan_id = format!("disaster-recovery-plan:{}", &plan.plan_digest[7..23]);
    plan
}

#[must_use]
pub fn evaluate_disaster_recovery(
    plan: &DisasterRecoveryPlan,
    approval: &DisasterRecoveryApproval,
    observation: DisasterRecoveryObservation,
) -> DisasterRecoveryEvidence {
    let mut issues = plan.issues.clone();
    if plan.protocol != DISASTER_RECOVERY_PLAN_PROTOCOL
        || !valid_digest(&plan.plan_digest)
        || plan.plan_digest != digest_without_plan_identity(plan)
        || plan.plan_id != format!("disaster-recovery-plan:{}", &plan.plan_digest[7..23])
    {
        issues.push(issue(
            DisasterRecoveryIssueCode::RestoreEvidenceInvalid,
            "Disaster recovery plan integrity is invalid.",
            "Reject modified plans after review.",
            "Regenerate the plan from current evidence.",
        ));
    }
    if plan.decision != DisasterRecoveryDecision::Ready
        || approval.plan_digest != plan.plan_digest
        || approval.phase != plan.input.phase
        || approval.approver.trim().is_empty()
        || approval.reason.trim().is_empty()
        || approval.approved_at_unix_ms == 0
    {
        issues.push(issue(
            DisasterRecoveryIssueCode::ApprovalInvalid,
            "Disaster cutover lacks explicit approval for the exact plan digest.",
            "Obtain named human approval at the disaster-cutover boundary.",
            "Stop without changing regional authority.",
        ));
    }
    if observation.observed_at_unix_ms == 0
        || observation.observed_at_unix_ms > plan.input.freshness_horizon_unix_ms
    {
        issues.push(issue(
            DisasterRecoveryIssueCode::PlanStale,
            "Cutover or failback observation is outside the plan freshness horizon.",
            "Revalidate the exact plan immediately before authority mutation.",
            "Stop and generate a new plan and approval.",
        ));
    }
    if observation.active_state_digest != plan.input.expected_active_state_digest {
        issues.push(issue(
            DisasterRecoveryIssueCode::ActiveStateChanged,
            "Authoritative state changed after the plan was reviewed.",
            "Never apply a stale authority-transfer plan.",
            "Refresh state, regenerate the plan, and obtain new approval.",
        ));
    }
    let authority_transition_valid = match plan.input.phase {
        DisasterRecoveryPhase::Cutover => {
            observation.primary_fenced && observation.passive_became_authoritative
        }
        DisasterRecoveryPhase::Failback => {
            observation.passive_fenced && observation.primary_became_authoritative
        }
    };
    if observation.plan_digest != plan.plan_digest
        || observation.phase != plan.input.phase
        || !valid_digest(&observation.evidence_digest)
        || !authority_transition_valid
        || observation.authoritative_environment_count != 1
        || !observation.traffic_switched
    {
        issues.push(issue(
            DisasterRecoveryIssueCode::PrimaryNotFenced,
            "Observed cutover does not prove fencing, passive authority, and traffic switch.",
            "Collect one authoritative regional observation.",
            "Repair or roll back the cutover before serving traffic.",
        ));
    }
    if !observation.requests_events_workflows_stories_verified {
        issues.push(issue(
            DisasterRecoveryIssueCode::ReconciliationIncomplete,
            "Requests, Events, Workflows, Inbox/Outbox, or Stories are not reconciled.",
            "Verify every declared recovery outcome before completing the drill.",
            "Keep the target isolated and finish reconciliation.",
        ));
    }
    if !observation.cleanup_complete {
        issues.push(issue(
            DisasterRecoveryIssueCode::CleanupIncomplete,
            "Disposable disaster-recovery resources remain active.",
            "Clean both regional fixtures without changing production authority.",
            "Complete deterministic cleanup.",
        ));
    }
    if observation.observed_rpo_ms > plan.input.rpo_budget_ms
        || observation.observed_rto_ms > plan.input.rto_budget_ms
    {
        issues.push(issue(
            DisasterRecoveryIssueCode::RecoveryBudgetExceeded,
            "Observed RPO or RTO exceeds the pinned environment budget.",
            "Report the observation without converting it into a universal guarantee.",
            "Improve the recovery path or revise the reviewed support envelope.",
        ));
    }
    if observation.release_digest != plan.input.expected_release_digest
        || observation.config_revision_digest != plan.input.expected_config_revision_digest
        || observation.contract_set_digest != plan.input.expected_contract_set_digest
        || !observation.workload_identity_preserved
        || observation.duplicate_business_effects > 0
        || observation.lost_committed_effects > 0
    {
        issues.push(issue(
            DisasterRecoveryIssueCode::IdentityOrContractMismatch,
            "Recovered authority changed identity or lost or duplicated committed work.",
            "Preserve release, configuration, Contract, Workload Identity, Inbox, and Outbox boundaries.",
            "Fail closed and restore the last verified authority state.",
        ));
    }
    let decision = if issues.is_empty() {
        DisasterRecoveryDecision::Passed
    } else {
        DisasterRecoveryDecision::Blocked
    };
    let mut evidence = DisasterRecoveryEvidence {
        protocol: DISASTER_RECOVERY_EVIDENCE_PROTOCOL.to_owned(),
        evidence_id: String::new(),
        evidence_digest: String::new(),
        plan_id: plan.plan_id.clone(),
        plan_digest: plan.plan_digest.clone(),
        phase: plan.input.phase,
        service_id: plan.input.service_id.clone(),
        primary_region: plan.input.primary_region.clone(),
        passive_region: plan.input.passive_region.clone(),
        observed_rpo_ms: observation.observed_rpo_ms,
        observed_rto_ms: observation.observed_rto_ms,
        data_loss_bound_ms: observation.observed_rpo_ms,
        replay_bound_count: observation.duplicate_business_effects,
        cleanup_complete: observation.cleanup_complete,
        authoritative_environment_count: observation.authoritative_environment_count,
        decision,
        issues,
        approval_boundary: approval_boundary(plan.input.phase).to_owned(),
        failback_steps: plan.input.failback_steps.clone(),
    };
    evidence.evidence_digest = digest_without_evidence_identity(&evidence);
    evidence.evidence_id = format!("disaster-recovery:{}", &evidence.evidence_digest[7..23]);
    evidence
}

const fn approval_boundary(phase: DisasterRecoveryPhase) -> &'static str {
    match phase {
        DisasterRecoveryPhase::Cutover => DISASTER_RECOVERY_APPROVAL_BOUNDARY,
        DisasterRecoveryPhase::Failback => DISASTER_FAILBACK_APPROVAL_BOUNDARY,
    }
}

#[must_use]
pub fn disaster_recovery_evidence_schema() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(DisasterRecoveryEvidence))
        .expect("disaster recovery schema serializes");
    schema["$id"] = Value::String(
        "https://contracts.lenso.local/ga/lenso.disaster-recovery-evidence.v1.schema.json"
            .to_owned(),
    );
    schema
}

fn issue(
    code: DisasterRecoveryIssueCode,
    message: impl Into<String>,
    remediation: impl Into<String>,
    next_action: impl Into<String>,
) -> DisasterRecoveryIssue {
    DisasterRecoveryIssue {
        code,
        message: message.into(),
        remediation: remediation.into(),
        next_actions: vec![next_action.into()],
    }
}

fn valid_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

fn digest_json(value: &impl Serialize) -> String {
    extraction_input_digest(&serde_json::to_vec(value).expect("DR evidence serializes"))
}

fn digest_without_plan_identity(plan: &DisasterRecoveryPlan) -> String {
    let mut canonical = plan.clone();
    canonical.plan_id.clear();
    canonical.plan_digest.clear();
    digest_json(&canonical)
}

fn digest_without_evidence_identity(evidence: &DisasterRecoveryEvidence) -> String {
    let mut canonical = evidence.clone();
    canonical.evidence_id.clear();
    canonical.evidence_digest.clear();
    digest_json(&canonical)
}

#[must_use]
pub fn disaster_recovery_evidence_integrity_is_valid(evidence: &DisasterRecoveryEvidence) -> bool {
    valid_digest(&evidence.evidence_digest)
        && evidence.evidence_digest == digest_without_evidence_identity(evidence)
        && evidence.evidence_id == format!("disaster-recovery:{}", &evidence.evidence_digest[7..23])
}
