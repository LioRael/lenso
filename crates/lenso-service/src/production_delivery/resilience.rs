use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::extraction_input_digest;

use super::{
    ConfigRevision, ContractRetirementEvidence, DeliveryDecision, DeliveryEffects, DeliveryIssue,
    DeliveryIssueCode, DeploymentObservation, DeploymentPlan, DeploymentReceipt,
    OperatorObservationAttestation, OperatorObservationAuthorityProvider, PromotionPlan,
    deployment_observation_integrity_is_valid, deployment_plan_integrity_is_valid, issue,
    operator_observation_attestation_is_valid, operator_observation_matches_deployment,
};

pub const COORDINATION_OUTAGE_OBSERVATION_PROTOCOL: &str =
    "lenso.coordination-outage-observation.v1";
pub const COORDINATION_OUTAGE_PROTOCOL: &str = "lenso.coordination-outage-proof.v1";
pub const COORDINATION_RESUME_APPROVAL_PROTOCOL: &str = "lenso.coordination-resume-approval.v1";
pub const COORDINATION_RESUME_PROTOCOL: &str = "lenso.coordination-resume-receipt.v1";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DataPlaneOperation {
    DirectRequest,
    Event,
    DurableWorkflow,
    Inbox,
    Outbox,
    Timer,
    Retry,
    Compensation,
    RuntimeStory,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ProtectedCoordinationOperation {
    Promotion,
    ConfigurationActivation,
    ContractRetirement,
    DeploymentMutation,
}

/// The exact typed mutation payload protected by a coordination resume approval.
///
/// Callers cannot supply a detached digest: both approval and apply derive the
/// canonical subject digest from this value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(tag = "operation", content = "subject", rename_all = "snake_case")]
pub enum CoordinationOperationSubject {
    Promotion(PromotionPlan),
    ConfigurationActivation(ConfigRevision),
    ContractRetirement(ContractRetirementEvidence),
    DeploymentMutation(DeploymentPlan),
}

impl CoordinationOperationSubject {
    #[must_use]
    pub const fn operation(&self) -> ProtectedCoordinationOperation {
        match self {
            Self::Promotion(_) => ProtectedCoordinationOperation::Promotion,
            Self::ConfigurationActivation(_) => {
                ProtectedCoordinationOperation::ConfigurationActivation
            }
            Self::ContractRetirement(_) => ProtectedCoordinationOperation::ContractRetirement,
            Self::DeploymentMutation(_) => ProtectedCoordinationOperation::DeploymentMutation,
        }
    }
}

#[must_use]
pub fn coordination_operation_subject_digest(subject: &CoordinationOperationSubject) -> String {
    digest_json(&("lenso.coordination-operation-subject.v1", subject))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SecurityContinuity {
    pub workload_identity_enforced: bool,
    pub tenant_context_enforced: bool,
    pub call_policy_enforced: bool,
    pub service_authorization_enforced: bool,
}

pub trait CoordinationAuthorityProvider: std::fmt::Debug + Send + Sync {
    fn sign(&self, authority_id: &str, subject_digest: &str) -> Option<String>;

    fn verify(&self, authority_id: &str, subject_digest: &str, authority_proof: &str) -> bool;
}

#[derive(Debug, Clone, Default)]
pub struct DeterministicCoordinationAuthorityProvider {
    authority_keys: BTreeMap<String, String>,
}

impl DeterministicCoordinationAuthorityProvider {
    #[must_use]
    pub fn new<I, K, V>(authority_keys: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        Self {
            authority_keys: authority_keys
                .into_iter()
                .map(|(authority, key)| (authority.into(), key.into()))
                .collect(),
        }
    }

    fn expected_proof(&self, authority_id: &str, subject_digest: &str) -> Option<String> {
        let key = self.authority_keys.get(authority_id)?;
        Some(digest_json(&(
            "lenso.coordination-authority-proof.v1",
            authority_id,
            subject_digest,
            key.as_str(),
        )))
    }
}

impl CoordinationAuthorityProvider for DeterministicCoordinationAuthorityProvider {
    fn sign(&self, authority_id: &str, subject_digest: &str) -> Option<String> {
        self.expected_proof(authority_id, subject_digest)
    }

    fn verify(&self, authority_id: &str, subject_digest: &str, authority_proof: &str) -> bool {
        self.expected_proof(authority_id, subject_digest).as_deref() == Some(authority_proof)
    }
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CoordinationOutageClaims {
    pub protocol: String,
    pub deployment_plan_id: String,
    pub deployment_plan_digest: String,
    pub deployment_receipt_id: String,
    pub deployment_observation_id: String,
    pub operator_observation_id: String,
    pub operator_observation_digest: String,
    pub environment_revision_after: u64,
    pub release_id: String,
    pub release_digest: String,
    pub config_revision_id: String,
    pub system_plane_available: bool,
    pub runtime_console_available: bool,
    pub autonomous_service_running: bool,
    pub selected_gateway_running: bool,
    pub selected_transport_running: bool,
    pub gateway_is_data_plane: bool,
    pub gateway_requires_live_policy: bool,
    pub gateway_requires_live_release_metadata: bool,
    pub last_valid_config_revision_available: bool,
    pub secret_provider_lease_valid: bool,
    pub secret_rotation_policy_preserved: bool,
    pub operation_results: BTreeMap<DataPlaneOperation, bool>,
    pub security: SecurityContinuity,
    pub durable_checkpoint_id: String,
    pub evidence_references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CoordinationOutageObservation {
    pub protocol: String,
    pub observation_id: String,
    pub observation_digest: String,
    pub authority_id: String,
    pub authority_proof: String,
    pub claims: CoordinationOutageClaims,
}

pub fn attest_coordination_outage(
    mut claims: CoordinationOutageClaims,
    authority_id: impl Into<String>,
    provider: &dyn CoordinationAuthorityProvider,
) -> Result<CoordinationOutageObservation, DeliveryIssue> {
    claims.evidence_references.sort();
    let observation_digest = digest_json(&claims);
    let authority_id = authority_id.into();
    let authority_proof = provider
        .sign(&authority_id, &observation_digest)
        .ok_or_else(|| {
            issue(
                DeliveryIssueCode::CoordinationUnavailable,
                "The Data Plane observation authority refused to attest the outage window.",
                "Use the configured outage observation authority at the probe boundary.",
                "Collect new signed continuity evidence before claiming resilience.",
            )
        })?;
    Ok(CoordinationOutageObservation {
        protocol: COORDINATION_OUTAGE_OBSERVATION_PROTOCOL.to_owned(),
        observation_id: format!("coordination-outage-observation:{observation_digest}"),
        observation_digest,
        authority_id,
        authority_proof,
        claims,
    })
}

#[must_use]
pub fn coordination_outage_observation_integrity_is_valid(
    observation: &CoordinationOutageObservation,
    provider: &dyn CoordinationAuthorityProvider,
) -> bool {
    observation.protocol == COORDINATION_OUTAGE_OBSERVATION_PROTOCOL
        && observation.claims.protocol == COORDINATION_OUTAGE_OBSERVATION_PROTOCOL
        && observation.observation_digest == digest_json(&observation.claims)
        && observation.observation_id
            == format!(
                "coordination-outage-observation:{}",
                observation.observation_digest
            )
        && provider.verify(
            &observation.authority_id,
            &observation.observation_digest,
            &observation.authority_proof,
        )
}

#[derive(Debug, Clone)]
pub struct CoordinationOutageInput {
    pub deployment_plan: DeploymentPlan,
    pub deployment: DeploymentReceipt,
    pub deployment_observation: DeploymentObservation,
    pub operator_observation: OperatorObservationAttestation,
    pub outage_observation: CoordinationOutageObservation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BlockedCoordinationOperation {
    pub operation: ProtectedCoordinationOperation,
    pub issue_code: DeliveryIssueCode,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CoordinationOutageEvidence {
    pub protocol: String,
    pub proof_id: String,
    pub proof_digest: String,
    pub decision: DeliveryDecision,
    pub deployment_plan_id: String,
    pub deployment_plan_digest: String,
    pub deployment_receipt_id: String,
    pub deployment_observation_id: String,
    pub environment_revision_after: u64,
    pub release_id: String,
    pub release_digest: String,
    pub config_revision_id: String,
    pub durable_checkpoint_id: String,
    pub deployment_plan: DeploymentPlan,
    pub deployment: DeploymentReceipt,
    pub deployment_observation: DeploymentObservation,
    pub outage_observation: CoordinationOutageObservation,
    pub operator_observation: OperatorObservationAttestation,
    pub continued_operations: Vec<DataPlaneOperation>,
    pub blocked_operations: Vec<BlockedCoordinationOperation>,
    pub security: SecurityContinuity,
    pub issues: Vec<DeliveryIssue>,
    pub evidence_references: Vec<String>,
    pub effects: DeliveryEffects,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CoordinationOutageProofDigestInput<'a> {
    protocol: &'a str,
    deployment_plan_id: &'a str,
    deployment_plan_digest: &'a str,
    deployment_receipt_id: &'a str,
    deployment_observation_id: &'a str,
    environment_revision_after: u64,
    release_id: &'a str,
    release_digest: &'a str,
    config_revision_id: &'a str,
    durable_checkpoint_id: &'a str,
    deployment_plan: &'a DeploymentPlan,
    deployment: &'a DeploymentReceipt,
    deployment_observation: &'a DeploymentObservation,
    outage_observation: &'a CoordinationOutageObservation,
    operator_observation: &'a OperatorObservationAttestation,
    continued_operations: &'a [DataPlaneOperation],
    blocked_operations: &'a [BlockedCoordinationOperation],
    security: &'a SecurityContinuity,
    issues: &'a [DeliveryIssue],
    evidence_references: &'a [String],
    effects: &'a DeliveryEffects,
}

pub fn prove_system_plane_outage(
    input: CoordinationOutageInput,
    outage_provider: &dyn CoordinationAuthorityProvider,
    operator_provider: &dyn OperatorObservationAuthorityProvider,
) -> CoordinationOutageEvidence {
    let claims = &input.outage_observation.claims;
    let mut issues = Vec::new();
    if !deployment_plan_integrity_is_valid(&input.deployment_plan)
        || !deployment_observation_integrity_is_valid(
            &input.deployment_observation,
            &input.deployment_plan,
            &input.deployment,
        )
        || !operator_observation_attestation_is_valid(
            &input.operator_observation,
            operator_provider,
        )
        || !operator_observation_matches_deployment(
            &input.operator_observation,
            &input.deployment_plan,
            &input.deployment,
            &input.deployment_observation,
            &input.operator_observation.claims.workload_health,
        )
        || !coordination_outage_observation_integrity_is_valid(
            &input.outage_observation,
            outage_provider,
        )
        || claims.deployment_plan_id != input.deployment_plan.plan_id
        || claims.deployment_plan_digest != input.deployment_plan.plan_digest
        || claims.deployment_receipt_id != input.deployment.receipt_id
        || claims.deployment_observation_id != input.deployment_observation.observation_id
        || claims.operator_observation_id != input.operator_observation.observation_id
        || claims.operator_observation_digest != input.operator_observation.observation_digest
        || claims.environment_revision_after != input.deployment.environment_revision_after
        || claims.release_id != input.deployment.release_id
        || claims.release_digest != input.deployment.release_digest
        || claims.config_revision_id != input.deployment.config_revision_id
    {
        issues.push(issue(
            DeliveryIssueCode::ObservationStale,
            "Outage evidence is not signed for the exact Deployment plan, receipt, checkpoint, and Operator observation.",
            "Collect canonical evidence from the trusted Operator and Data Plane probe boundaries.",
            "Refresh the exact signed observations before proving an outage.",
        ));
    }
    if claims.system_plane_available || claims.runtime_console_available {
        issues.push(issue(
            DeliveryIssueCode::CoordinationUnavailable,
            "The outage proof must actually withhold the System Plane and Runtime Console.",
            "Run the proof with both coordination surfaces unavailable.",
            "Repeat the outage window without stopping the Autonomous Service.",
        ));
    }
    if !claims.autonomous_service_running
        || !claims.selected_gateway_running
        || !claims.selected_transport_running
        || !claims.gateway_is_data_plane
        || claims.gateway_requires_live_policy
        || claims.gateway_requires_live_release_metadata
    {
        issues.push(issue(
            DeliveryIssueCode::CoordinationUnavailable,
            "Autonomous Service, gateway, or transport continuity still depends on a live coordination surface.",
            "Keep established traffic and transports entirely inside the Data Plane.",
            "Correct the topology and repeat the outage proof.",
        ));
    }
    if input.deployment_observation.desired_release_id != input.deployment.release_id
        || input.deployment_observation.observed_release_id != input.deployment.release_id
        || input.deployment_observation.drifted
        || !input.deployment_observation.fresh
        || input.deployment_observation.config_revision_id != input.deployment.config_revision_id
    {
        issues.push(issue(
            DeliveryIssueCode::ObservationStale,
            "Outage proof requires a freshly converged production Deployment.",
            "Converge the exact release and Config Revision before withholding coordination.",
            "Refresh the Deployment observation and repeat the proof.",
        ));
    }
    if !claims.last_valid_config_revision_available
        || !claims.secret_provider_lease_valid
        || !claims.secret_rotation_policy_preserved
    {
        issues.push(issue(
            DeliveryIssueCode::SecretReferenceUnresolved,
            "Last-valid configuration or declared Secret Provider lease and rotation behavior did not survive the outage.",
            "Retain only opaque references and honor provider lease and rotation boundaries locally.",
            "Restore safe secret continuity before claiming Data Plane resilience.",
        ));
    }
    let security_preserved = claims.security.workload_identity_enforced
        && claims.security.tenant_context_enforced
        && claims.security.call_policy_enforced
        && claims.security.service_authorization_enforced;
    if !security_preserved {
        issues.push(issue(
            DeliveryIssueCode::PolicyRuleBlocked,
            "Coordination loss weakened Workload Identity, Tenant Context, Call Policy, or Service authorization.",
            "Fail closed rather than bypassing Service-owned security controls.",
            "Restore the security guarantee and repeat the outage proof.",
        ));
    }
    let expected_operations = expected_data_plane_operations();
    let observed_operations = claims
        .operation_results
        .keys()
        .copied()
        .collect::<BTreeSet<_>>();
    let continued_operations = claims
        .operation_results
        .iter()
        .filter_map(|(operation, continued)| continued.then_some(*operation))
        .collect::<Vec<_>>();
    if observed_operations != expected_operations
        || continued_operations.len() != expected_operations.len()
    {
        issues.push(issue(
            DeliveryIssueCode::ReliabilityEvidenceMissing,
            "Established requests, events, workflows, inbox, outbox, timers, retries, compensation, or Runtime Story did not all continue.",
            "Capture every declared Data Plane execution path from the last durable state.",
            "Repair continuity and repeat the outage proof.",
        ));
    }
    if claims.durable_checkpoint_id.trim().is_empty() {
        issues.push(issue(
            DeliveryIssueCode::ReliabilityEvidenceMissing,
            "Outage evidence lacks a durable resume checkpoint.",
            "Persist the last valid coordination and Data Plane checkpoint locally.",
            "Capture durable evidence before restoring coordination.",
        ));
    }
    let blocked_operations = protected_operations();
    let decision = if issues.is_empty() {
        DeliveryDecision::Passed
    } else {
        DeliveryDecision::Blocked
    };
    let effects = DeliveryEffects::default();
    let proof_digest = digest_json(&CoordinationOutageProofDigestInput {
        protocol: COORDINATION_OUTAGE_PROTOCOL,
        deployment_plan_id: input.deployment_plan.plan_id.as_str(),
        deployment_plan_digest: input.deployment_plan.plan_digest.as_str(),
        deployment_receipt_id: input.deployment.receipt_id.as_str(),
        deployment_observation_id: input.deployment_observation.observation_id.as_str(),
        environment_revision_after: input.deployment.environment_revision_after,
        release_id: input.deployment.release_id.as_str(),
        release_digest: input.deployment.release_digest.as_str(),
        config_revision_id: input.deployment.config_revision_id.as_str(),
        durable_checkpoint_id: claims.durable_checkpoint_id.as_str(),
        deployment_plan: &input.deployment_plan,
        deployment: &input.deployment,
        deployment_observation: &input.deployment_observation,
        outage_observation: &input.outage_observation,
        operator_observation: &input.operator_observation,
        continued_operations: continued_operations.as_slice(),
        blocked_operations: blocked_operations.as_slice(),
        security: &claims.security,
        issues: issues.as_slice(),
        evidence_references: claims.evidence_references.as_slice(),
        effects: &effects,
    });
    CoordinationOutageEvidence {
        protocol: COORDINATION_OUTAGE_PROTOCOL.to_owned(),
        proof_id: format!("coordination-outage-proof:{proof_digest}"),
        proof_digest,
        decision,
        deployment_plan_id: input.deployment_plan.plan_id.clone(),
        deployment_plan_digest: input.deployment_plan.plan_digest.clone(),
        deployment_receipt_id: input.deployment.receipt_id.clone(),
        deployment_observation_id: input.deployment_observation.observation_id.clone(),
        environment_revision_after: input.deployment.environment_revision_after,
        release_id: input.deployment.release_id.clone(),
        release_digest: input.deployment.release_digest.clone(),
        config_revision_id: input.deployment.config_revision_id.clone(),
        durable_checkpoint_id: claims.durable_checkpoint_id.clone(),
        deployment_plan: input.deployment_plan.clone(),
        deployment: input.deployment.clone(),
        deployment_observation: input.deployment_observation.clone(),
        outage_observation: input.outage_observation.clone(),
        operator_observation: input.operator_observation.clone(),
        continued_operations,
        blocked_operations,
        security: claims.security.clone(),
        issues,
        evidence_references: claims.evidence_references.clone(),
        effects,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CoordinationResumeApproval {
    pub protocol: String,
    pub approval_id: String,
    pub approval_digest: String,
    pub operation_id: String,
    pub operation: ProtectedCoordinationOperation,
    pub operation_subject_digest: String,
    pub outage_proof_id: String,
    pub outage_observation_id: String,
    pub deployment_receipt_id: String,
    pub durable_checkpoint_id: String,
    pub environment_revision_after: u64,
    pub coordination_revision: u64,
    pub authority_id: String,
    pub authority_proof: String,
}

#[allow(clippy::too_many_arguments)]
pub fn approve_coordination_resume(
    proof: &CoordinationOutageEvidence,
    operation_id: impl Into<String>,
    subject: &CoordinationOperationSubject,
    coordination_revision: u64,
    authority_id: impl Into<String>,
    outage_provider: &dyn CoordinationAuthorityProvider,
    operator_provider: &dyn OperatorObservationAuthorityProvider,
    approval_provider: &dyn CoordinationAuthorityProvider,
) -> Result<CoordinationResumeApproval, Vec<DeliveryIssue>> {
    let operation_id = operation_id.into();
    let operation = subject.operation();
    let operation_subject_digest = coordination_operation_subject_digest(subject);
    let authority_id = authority_id.into();
    if !coordination_outage_evidence_integrity_is_valid(proof, outage_provider, operator_provider)
        || proof.decision != DeliveryDecision::Passed
        || operation_id.trim().is_empty()
        || coordination_revision <= proof.environment_revision_after
        || !proof
            .blocked_operations
            .iter()
            .any(|blocked| blocked.operation == operation)
    {
        return Err(vec![coordination_resume_issue()]);
    }
    let approval_digest = coordination_resume_approval_digest(
        &operation_id,
        operation,
        &operation_subject_digest,
        proof,
        coordination_revision,
        &authority_id,
    );
    let Some(authority_proof) = approval_provider.sign(&authority_id, &approval_digest) else {
        return Err(vec![coordination_resume_issue()]);
    };
    Ok(CoordinationResumeApproval {
        protocol: COORDINATION_RESUME_APPROVAL_PROTOCOL.to_owned(),
        approval_id: format!("coordination-resume-approval:{approval_digest}"),
        approval_digest,
        operation_id,
        operation,
        operation_subject_digest,
        outage_proof_id: proof.proof_id.clone(),
        outage_observation_id: proof.outage_observation.observation_id.clone(),
        deployment_receipt_id: proof.deployment_receipt_id.clone(),
        durable_checkpoint_id: proof.durable_checkpoint_id.clone(),
        environment_revision_after: proof.environment_revision_after,
        coordination_revision,
        authority_id,
        authority_proof,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
/// A deterministic authorization receipt for retrying the protected operation.
///
/// This receipt deliberately records no environment, deployment, configuration,
/// or ledger effect. The operation named by `operation_id` must execute through
/// its own durable idempotency boundary after this authorization is obtained.
pub struct CoordinationResumeReceipt {
    pub protocol: String,
    pub receipt_id: String,
    pub operation_id: String,
    pub operation: ProtectedCoordinationOperation,
    pub operation_subject_digest: String,
    pub outage_proof_id: String,
    pub durable_checkpoint_id: String,
    pub approval_id: String,
    pub approval_digest: String,
    pub coordination_revision: u64,
    pub effects: DeliveryEffects,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CoordinationResumeState {
    #[serde(default)]
    pub receipts: Vec<CoordinationResumeReceipt>,
}

#[allow(clippy::too_many_arguments)]
pub fn resume_protected_operation(
    state: &mut CoordinationResumeState,
    proof: &CoordinationOutageEvidence,
    approval: &CoordinationResumeApproval,
    subject: &CoordinationOperationSubject,
    current_coordination_revision: u64,
    outage_provider: &dyn CoordinationAuthorityProvider,
    operator_provider: &dyn OperatorObservationAuthorityProvider,
    approval_provider: &dyn CoordinationAuthorityProvider,
) -> Result<CoordinationResumeReceipt, Vec<DeliveryIssue>> {
    if !coordination_outage_evidence_integrity_is_valid(proof, outage_provider, operator_provider)
        || proof.decision != DeliveryDecision::Passed
        || !coordination_resume_approval_integrity_is_valid(approval, proof, approval_provider)
        || subject.operation() != approval.operation
        || coordination_operation_subject_digest(subject) != approval.operation_subject_digest
        || current_coordination_revision != approval.coordination_revision
    {
        return Err(vec![coordination_resume_issue()]);
    }
    if let Some(receipt) = state
        .receipts
        .iter()
        .find(|receipt| receipt.operation_id == approval.operation_id)
    {
        return coordination_resume_receipt_integrity_is_valid(receipt, proof, approval)
            .then(|| receipt.clone())
            .ok_or_else(|| {
                vec![issue(
                    DeliveryIssueCode::StaleInput,
                    "The resume operation identity was reused with different protected evidence.",
                    "Preserve the exact operation, outage proof, durable checkpoint, and approval for idempotent replay.",
                    "Use the original request or allocate a new stable operation identifier.",
                )]
            });
    }
    let effects = coordination_resume_authorization_effects();
    let receipt_id = format!(
        "coordination-resume-receipt:{}",
        digest_json(&(
            COORDINATION_RESUME_PROTOCOL,
            approval.operation_id.as_str(),
            approval.operation,
            approval.operation_subject_digest.as_str(),
            proof.proof_id.as_str(),
            proof.durable_checkpoint_id.as_str(),
            approval.approval_id.as_str(),
            approval.approval_digest.as_str(),
            approval.coordination_revision,
            &effects,
        ))
    );
    let receipt = CoordinationResumeReceipt {
        protocol: COORDINATION_RESUME_PROTOCOL.to_owned(),
        receipt_id,
        operation_id: approval.operation_id.clone(),
        operation: approval.operation,
        operation_subject_digest: approval.operation_subject_digest.clone(),
        outage_proof_id: proof.proof_id.clone(),
        durable_checkpoint_id: proof.durable_checkpoint_id.clone(),
        approval_id: approval.approval_id.clone(),
        approval_digest: approval.approval_digest.clone(),
        coordination_revision: approval.coordination_revision,
        effects,
    };
    state.receipts.push(receipt.clone());
    Ok(receipt)
}

#[must_use]
pub fn coordination_outage_evidence_integrity_is_valid(
    proof: &CoordinationOutageEvidence,
    outage_provider: &dyn CoordinationAuthorityProvider,
    operator_provider: &dyn OperatorObservationAuthorityProvider,
) -> bool {
    let expected = prove_system_plane_outage(
        CoordinationOutageInput {
            deployment_plan: proof.deployment_plan.clone(),
            deployment: proof.deployment.clone(),
            deployment_observation: proof.deployment_observation.clone(),
            operator_observation: proof.operator_observation.clone(),
            outage_observation: proof.outage_observation.clone(),
        },
        outage_provider,
        operator_provider,
    );
    *proof == expected
}

/// Recomputes the content digest of an outage proof without conferring authority.
/// Consumers must still call [`coordination_outage_evidence_integrity_is_valid`],
/// which deterministically replays the signed evidence semantics.
#[must_use]
pub fn coordination_outage_evidence_digest(proof: &CoordinationOutageEvidence) -> String {
    digest_json(&CoordinationOutageProofDigestInput {
        protocol: proof.protocol.as_str(),
        deployment_plan_id: proof.deployment_plan_id.as_str(),
        deployment_plan_digest: proof.deployment_plan_digest.as_str(),
        deployment_receipt_id: proof.deployment_receipt_id.as_str(),
        deployment_observation_id: proof.deployment_observation_id.as_str(),
        environment_revision_after: proof.environment_revision_after,
        release_id: proof.release_id.as_str(),
        release_digest: proof.release_digest.as_str(),
        config_revision_id: proof.config_revision_id.as_str(),
        durable_checkpoint_id: proof.durable_checkpoint_id.as_str(),
        deployment_plan: &proof.deployment_plan,
        deployment: &proof.deployment,
        deployment_observation: &proof.deployment_observation,
        outage_observation: &proof.outage_observation,
        operator_observation: &proof.operator_observation,
        continued_operations: proof.continued_operations.as_slice(),
        blocked_operations: proof.blocked_operations.as_slice(),
        security: &proof.security,
        issues: proof.issues.as_slice(),
        evidence_references: proof.evidence_references.as_slice(),
        effects: &proof.effects,
    })
}

#[must_use]
pub fn coordination_resume_approval_integrity_is_valid(
    approval: &CoordinationResumeApproval,
    proof: &CoordinationOutageEvidence,
    provider: &dyn CoordinationAuthorityProvider,
) -> bool {
    approval.protocol == COORDINATION_RESUME_APPROVAL_PROTOCOL
        && approval.approval_digest
            == coordination_resume_approval_digest(
                &approval.operation_id,
                approval.operation,
                &approval.operation_subject_digest,
                proof,
                approval.coordination_revision,
                &approval.authority_id,
            )
        && approval.approval_id
            == format!("coordination-resume-approval:{}", approval.approval_digest)
        && approval.outage_proof_id == proof.proof_id
        && approval.outage_observation_id == proof.outage_observation.observation_id
        && approval.deployment_receipt_id == proof.deployment_receipt_id
        && approval.durable_checkpoint_id == proof.durable_checkpoint_id
        && approval.environment_revision_after == proof.environment_revision_after
        && approval.coordination_revision > proof.environment_revision_after
        && !approval.operation_id.trim().is_empty()
        && valid_operation_subject_digest(&approval.operation_subject_digest)
        && proof
            .blocked_operations
            .iter()
            .any(|blocked| blocked.operation == approval.operation)
        && provider.verify(
            &approval.authority_id,
            &approval.approval_digest,
            &approval.authority_proof,
        )
}

#[must_use]
pub fn coordination_resume_receipt_integrity_is_valid(
    receipt: &CoordinationResumeReceipt,
    proof: &CoordinationOutageEvidence,
    approval: &CoordinationResumeApproval,
) -> bool {
    let effects = coordination_resume_authorization_effects();
    receipt.protocol == COORDINATION_RESUME_PROTOCOL
        && receipt.operation_id == approval.operation_id
        && receipt.operation == approval.operation
        && receipt.operation_subject_digest == approval.operation_subject_digest
        && receipt.outage_proof_id == proof.proof_id
        && receipt.durable_checkpoint_id == proof.durable_checkpoint_id
        && receipt.approval_id == approval.approval_id
        && receipt.approval_digest == approval.approval_digest
        && receipt.coordination_revision == approval.coordination_revision
        && receipt.effects == effects
        && receipt.receipt_id
            == format!(
                "coordination-resume-receipt:{}",
                digest_json(&(
                    receipt.protocol.as_str(),
                    receipt.operation_id.as_str(),
                    receipt.operation,
                    receipt.operation_subject_digest.as_str(),
                    receipt.outage_proof_id.as_str(),
                    receipt.durable_checkpoint_id.as_str(),
                    receipt.approval_id.as_str(),
                    receipt.approval_digest.as_str(),
                    receipt.coordination_revision,
                    &receipt.effects,
                ))
            )
}

fn coordination_resume_approval_digest(
    operation_id: &str,
    operation: ProtectedCoordinationOperation,
    operation_subject_digest: &str,
    proof: &CoordinationOutageEvidence,
    coordination_revision: u64,
    authority_id: &str,
) -> String {
    digest_json(&(
        COORDINATION_RESUME_APPROVAL_PROTOCOL,
        operation_id,
        operation,
        operation_subject_digest,
        proof.proof_id.as_str(),
        proof.outage_observation.observation_id.as_str(),
        proof.deployment_receipt_id.as_str(),
        proof.durable_checkpoint_id.as_str(),
        proof.environment_revision_after,
        coordination_revision,
        authority_id,
    ))
}

fn valid_operation_subject_digest(digest: &str) -> bool {
    digest.strip_prefix("sha256:").is_some_and(|value| {
        value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

fn expected_data_plane_operations() -> BTreeSet<DataPlaneOperation> {
    [
        DataPlaneOperation::DirectRequest,
        DataPlaneOperation::Event,
        DataPlaneOperation::DurableWorkflow,
        DataPlaneOperation::Inbox,
        DataPlaneOperation::Outbox,
        DataPlaneOperation::Timer,
        DataPlaneOperation::Retry,
        DataPlaneOperation::Compensation,
        DataPlaneOperation::RuntimeStory,
    ]
    .into_iter()
    .collect()
}

fn protected_operations() -> Vec<BlockedCoordinationOperation> {
    [
        ProtectedCoordinationOperation::Promotion,
        ProtectedCoordinationOperation::ConfigurationActivation,
        ProtectedCoordinationOperation::ContractRetirement,
        ProtectedCoordinationOperation::DeploymentMutation,
    ]
    .into_iter()
    .map(|operation| BlockedCoordinationOperation {
        operation,
        issue_code: DeliveryIssueCode::CoordinationUnavailable,
        next_actions: vec![
            "Restore coordination and refresh policy, approval, and environment evidence."
                .to_owned(),
            "Resume from the durable checkpoint without replaying completed effects.".to_owned(),
        ],
    })
    .collect()
}

fn coordination_resume_issue() -> DeliveryIssue {
    issue(
        DeliveryIssueCode::CoordinationUnavailable,
        "Protected coordination cannot resume without signed outage evidence, a newer coordination revision, and typed approval for the exact operation.",
        "Restore coordination and refresh the exact signed operation approval boundary.",
        "Retry the same stable operation identifier with current authority evidence.",
    )
}

fn coordination_resume_authorization_effects() -> DeliveryEffects {
    DeliveryEffects::default()
}

fn digest_json(value: &impl Serialize) -> String {
    extraction_input_digest(serde_json::to_vec(value).expect("resilience values must serialize"))
}
