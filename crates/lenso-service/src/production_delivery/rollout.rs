use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::extraction_input_digest;

use super::{
    DeliveryDecision, DeliveryEffects, DeliveryIssue, DeliveryIssueCode, DeliveryPolicyInputs,
    DeploymentObservation, DeploymentPlan, DeploymentReceipt, EnvironmentVerification,
    GatewayConfigurationPlan, GatewayObservation, GatewayObservationProvider,
    OperatorObservationAuthorityProvider, PolicyEvidence, ReleaseRollbackConstraints,
    ReleaseTrustProvider, SecretProvider, ServiceRelease,
    deployment_observation_integrity_is_valid, deployment_plan_integrity_is_valid,
    deployment_receipt_integrity_is_valid, environment_verification_authority_is_valid,
    gateway_observation_integrity_is_valid, gateway_plan_authority_is_valid, issue,
    production_policy_evidence_is_valid, service_release_integrity_is_valid,
};

pub const RELIABILITY_CONTRACT_PROTOCOL: &str = "lenso.reliability-contract.v1";
pub const CANARY_PLAN_PROTOCOL: &str = "lenso.canary-plan.v1";
pub const CANARY_DECISION_PROTOCOL: &str = "lenso.canary-decision.v1";
pub const RELIABILITY_OBSERVATION_PROTOCOL: &str = "lenso.reliability-observation.v1";
pub const ROLLBACK_PLAN_PROTOCOL: &str = "lenso.rollback-plan.v1";
pub const ROLLBACK_SAFETY_PROTOCOL: &str = "lenso.rollback-safety-evidence.v1";
pub const ROLLBACK_CONVERGENCE_PROTOCOL: &str = "lenso.rollback-convergence.v1";
pub const ROLLBACK_RECEIPT_PROTOCOL: &str = "lenso.rollback-receipt.v1";

pub trait ReliabilityObservationProvider: std::fmt::Debug + Send + Sync {
    fn sign(&self, collector_id: &str, observation_id: &str) -> Option<String>;

    fn verify(&self, collector_id: &str, observation_id: &str, proof: &str) -> bool;
}

#[derive(Debug, Clone, Default)]
pub struct DeterministicReliabilityObservationProvider {
    collector_keys: BTreeMap<String, String>,
}

impl DeterministicReliabilityObservationProvider {
    #[must_use]
    pub fn new<I, K, V>(collector_keys: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        Self {
            collector_keys: collector_keys
                .into_iter()
                .map(|(collector, key)| (collector.into(), key.into()))
                .collect(),
        }
    }

    fn expected_proof(&self, collector_id: &str, observation_id: &str) -> Option<String> {
        let key = self.collector_keys.get(collector_id)?;
        Some(extraction_input_digest(
            format!("{key}\0{observation_id}").as_bytes(),
        ))
    }
}

impl ReliabilityObservationProvider for DeterministicReliabilityObservationProvider {
    fn sign(&self, collector_id: &str, observation_id: &str) -> Option<String> {
        self.expected_proof(collector_id, observation_id)
    }

    fn verify(&self, collector_id: &str, observation_id: &str, proof: &str) -> bool {
        self.expected_proof(collector_id, observation_id).as_deref() == Some(proof)
    }
}

pub trait RollbackSafetyProvider: std::fmt::Debug + Send + Sync {
    fn provider_id(&self) -> &str;

    fn sign(&self, evidence_id: &str) -> Option<String>;

    fn verify(&self, evidence_id: &str, proof: &str) -> bool;
}

#[derive(Debug, Clone)]
pub struct DeterministicRollbackSafetyProvider {
    provider_id: String,
    key: String,
}

impl DeterministicRollbackSafetyProvider {
    #[must_use]
    pub fn new(provider_id: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            key: key.into(),
        }
    }

    fn expected_proof(&self, evidence_id: &str) -> String {
        extraction_input_digest(format!("{}\0{evidence_id}", self.key).as_bytes())
    }
}

impl RollbackSafetyProvider for DeterministicRollbackSafetyProvider {
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    fn sign(&self, evidence_id: &str) -> Option<String> {
        Some(self.expected_proof(evidence_id))
    }

    fn verify(&self, evidence_id: &str, proof: &str) -> bool {
        self.expected_proof(evidence_id) == proof
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DependencyCriticality {
    Critical,
    Degradable,
    Optional,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DependencyReliability {
    pub dependency_id: String,
    pub criticality: DependencyCriticality,
    #[serde(default)]
    pub allowed_degraded_modes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryReliabilityContract {
    pub protocol: String,
    pub contract_id: String,
    pub minimum_observation_seconds: u64,
    pub minimum_sample_count: u64,
    pub minimum_availability_basis_points: u32,
    pub maximum_latency_p99_ms: u64,
    pub maximum_error_budget_used_basis_points: u32,
    pub maximum_queue_backlog: u64,
    pub maximum_workflow_backlog: u64,
    pub maximum_timer_lag_ms: u64,
    pub maximum_retry_exhaustion: u64,
    pub maximum_compensation_pressure: u64,
    pub minimum_healthy_failure_domains: u32,
    pub dependencies: Vec<DependencyReliability>,
}

/// Returns the canonical digest that a signed Service Release must bind for this
/// exact Reliability Contract.
#[must_use]
pub fn reliability_contract_digest(contract: &DeliveryReliabilityContract) -> String {
    extraction_input_digest(
        serde_json::to_vec(contract).expect("Reliability Contract must serialize"),
    )
}

#[derive(Debug, Clone)]
pub struct CanaryPlanInput {
    pub release: ServiceRelease,
    pub production_deployment: DeploymentPlan,
    pub production_deployment_receipt: DeploymentReceipt,
    pub production_deployment_observation: DeploymentObservation,
    pub reliability_contract: DeliveryReliabilityContract,
    pub policy: PolicyEvidence,
    pub policy_inputs: DeliveryPolicyInputs,
    pub environment_verification: EnvironmentVerification,
    pub previous_known_good_deployment: DeploymentPlan,
    pub previous_known_good_receipt: DeploymentReceipt,
    pub previous_known_good_observation: DeploymentObservation,
    pub previous_known_good_release: ServiceRelease,
    pub previous_known_good_policy: PolicyEvidence,
    pub previous_known_good_policy_inputs: DeliveryPolicyInputs,
    pub previous_known_good_gateway: GatewayConfigurationPlan,
    pub previous_known_good_gateway_observation: GatewayObservation,
    pub initial_percent: u8,
    pub maximum_percent: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CanaryPlan {
    pub protocol: String,
    pub plan_id: String,
    pub plan_digest: String,
    pub release_id: String,
    pub release_digest: String,
    pub production_deployment_plan_id: String,
    pub production_deployment_digest: String,
    pub production_deployment_receipt_id: String,
    pub production_deployment_observation_id: String,
    pub production_environment: String,
    pub production_expected_environment_revision: u64,
    pub reliability_contract: DeliveryReliabilityContract,
    pub release_rollback_constraints: ReleaseRollbackConstraints,
    pub policy_evidence_id: String,
    pub policy_evidence_digest: String,
    pub environment_verification_id: String,
    pub environment_verification_digest: String,
    pub previous_known_good_plan_id: String,
    pub previous_known_good_deployment_digest: String,
    pub previous_known_good_release_id: String,
    pub previous_known_good_release_digest: String,
    pub previous_known_good_receipt_id: String,
    pub previous_known_good_observation_id: String,
    pub previous_known_good_policy_evidence_id: String,
    pub previous_known_good_policy_evidence_digest: String,
    pub previous_known_good_gateway_plan_id: String,
    pub previous_known_good_gateway_plan_digest: String,
    pub previous_known_good_gateway_configuration_identity: String,
    pub previous_known_good_gateway_observation_id: String,
    pub previous_known_good_gateway_observation_revision: u64,
    pub previous_known_good_gateway_observed_after: String,
    pub initial_percent: u8,
    pub maximum_percent: u8,
    pub workload_ids: Vec<String>,
    pub effects: DeliveryEffects,
}

pub fn plan_canary(
    input: CanaryPlanInput,
    trust_provider: &dyn ReleaseTrustProvider,
    secret_provider: &dyn SecretProvider,
    operator_observation_provider: &dyn OperatorObservationAuthorityProvider,
    gateway_observation_provider: &dyn GatewayObservationProvider,
) -> Result<CanaryPlan, Vec<DeliveryIssue>> {
    let mut issues = Vec::new();
    if !service_release_integrity_is_valid(&input.release)
        || !deployment_plan_integrity_is_valid(&input.production_deployment)
        || !deployment_receipt_integrity_is_valid(
            &input.production_deployment_receipt,
            &input.production_deployment,
        )
        || !deployment_observation_integrity_is_valid(
            &input.production_deployment_observation,
            &input.production_deployment,
            &input.production_deployment_receipt,
        )
        || input.production_deployment_observation.drifted
        || !input.production_deployment_observation.fresh
        || input.production_deployment.release_id != input.release.release_id
        || input.production_deployment.release_digest != input.release.release_digest
    {
        issues.push(issue(
            DeliveryIssueCode::ReleaseTampered,
            "Canary planning requires the exact integrity-valid Service Release and production Deployment plan.",
            "Use the immutable promoted release and its digest-pinned production Deployment plan.",
            "Refresh the production plan before starting a canary.",
        ));
    }
    if input.policy_inputs.release != input.release
        || !production_policy_evidence_is_valid(
            &input.policy,
            &input.policy_inputs,
            trust_provider,
            secret_provider,
        )
        || !environment_verification_authority_is_valid(
            &input.environment_verification,
            operator_observation_provider,
            gateway_observation_provider,
        )
        || input.policy.decision != DeliveryDecision::Passed
        || input.policy.evaluated_subject != input.release.release_id
        || input.environment_verification.decision != DeliveryDecision::Passed
        || input.environment_verification.release_id != input.release.release_id
    {
        issues.push(issue(
            DeliveryIssueCode::PolicyEvidenceMissing,
            "Canary planning requires passing Policy Evidence and Environment Verification for the exact release.",
            "Refresh policy and environment evidence without rebuilding the release.",
            "Verify the release again before starting a production canary.",
        ));
    }
    if !deployment_plan_integrity_is_valid(&input.previous_known_good_deployment)
        || !service_release_integrity_is_valid(&input.previous_known_good_release)
        || !deployment_receipt_integrity_is_valid(
            &input.previous_known_good_receipt,
            &input.previous_known_good_deployment,
        )
        || !deployment_observation_integrity_is_valid(
            &input.previous_known_good_observation,
            &input.previous_known_good_deployment,
            &input.previous_known_good_receipt,
        )
        || input.previous_known_good_observation.drifted
        || !input.previous_known_good_observation.fresh
        || input.previous_known_good_deployment.environment
            != input.production_deployment.environment
        || input.previous_known_good_deployment.release_id
            != input.previous_known_good_release.release_id
        || input.previous_known_good_deployment.release_digest
            != input.previous_known_good_release.release_digest
        || input
            .previous_known_good_deployment
            .workloads
            .iter()
            .any(|workload| {
                !input
                    .previous_known_good_release
                    .workloads
                    .iter()
                    .any(|known_good| {
                        known_good.workload_id == workload.workload_id
                            && known_good.artifact_digest == workload.artifact_digest
                    })
            })
        || input.previous_known_good_release.release_id == input.release.release_id
        || input.previous_known_good_release.release_digest == input.release.release_digest
        || input
            .previous_known_good_deployment
            .workloads
            .iter()
            .all(|previous| {
                input
                    .production_deployment
                    .workloads
                    .iter()
                    .any(|candidate| {
                        candidate.workload_id == previous.workload_id
                            && candidate.artifact_digest == previous.artifact_digest
                    })
            })
        || input.previous_known_good_policy_inputs.release != input.previous_known_good_release
        || input.previous_known_good_policy_inputs.config.revision_id
            != input.previous_known_good_deployment.config_revision_id
        || !production_policy_evidence_is_valid(
            &input.previous_known_good_policy,
            &input.previous_known_good_policy_inputs,
            trust_provider,
            secret_provider,
        )
        || input.previous_known_good_policy.decision != DeliveryDecision::Passed
        || input.previous_known_good_policy.evaluated_subject
            != input.previous_known_good_release.release_id
        || !gateway_plan_authority_is_valid(&input.previous_known_good_gateway, trust_provider)
        || input.previous_known_good_gateway.edge_release_id
            != input.previous_known_good_release.release_id
        || input.previous_known_good_gateway.edge_release_digest
            != input.previous_known_good_release.release_digest
        || input.previous_known_good_deployment.gateway_plan_digest
            != input.previous_known_good_gateway.plan_digest
        || !gateway_observation_integrity_is_valid(
            &input.previous_known_good_gateway_observation,
            gateway_observation_provider,
        )
        || input
            .previous_known_good_gateway_observation
            .configuration_identity
            != input.previous_known_good_gateway.configuration_identity
        || input.previous_known_good_gateway_observation.revision
            != input.previous_known_good_gateway.expected_gateway_revision
        || input.previous_known_good_gateway_observation.observed_after
            != input.previous_known_good_observation.source_observation_id
        || !input.previous_known_good_gateway_observation.fresh
    {
        issues.push(issue(
            DeliveryIssueCode::RollbackUnsafe,
            "Canary planning requires an integrity-valid previous known-good Deployment in the same environment.",
            "Retain a verified rollback target and its complete adapter inputs.",
            "Provide a known-good Deployment before starting the canary.",
        ));
    }
    if input.initial_percent == 0
        || input.initial_percent > input.maximum_percent
        || input.maximum_percent > 100
        || input.reliability_contract.protocol != RELIABILITY_CONTRACT_PROTOCOL
        || input.release.reliability_contract.digest
            != reliability_contract_digest(&input.reliability_contract)
        || input.reliability_contract.minimum_observation_seconds == 0
        || input.reliability_contract.minimum_sample_count == 0
        || input.reliability_contract.minimum_availability_basis_points > 10_000
        || input
            .reliability_contract
            .maximum_error_budget_used_basis_points
            > 10_000
        || input.reliability_contract.minimum_healthy_failure_domains == 0
    {
        issues.push(issue(
            DeliveryIssueCode::DeploymentInputInvalid,
            "Canary bounds are invalid or the Reliability Contract differs from the exact contract bound by the signed Service Release.",
            "Declare a non-zero bounded canary and use the content-addressed Service-level objectives from the release.",
            "Correct the Reliability Contract and plan the canary again.",
        ));
    }
    let dependency_ids = input
        .reliability_contract
        .dependencies
        .iter()
        .map(|dependency| dependency.dependency_id.as_str())
        .collect::<BTreeSet<_>>();
    if dependency_ids.len() != input.reliability_contract.dependencies.len() {
        issues.push(issue(
            DeliveryIssueCode::DeploymentInputInvalid,
            "Reliability dependency declarations must have unique identifiers.",
            "Declare each critical, degradable, or optional dependency exactly once.",
            "Correct the Reliability Contract and plan the canary again.",
        ));
    }
    if !issues.is_empty() {
        return Err(issues);
    }
    let mut workload_ids = input
        .release
        .workloads
        .iter()
        .map(|workload| workload.workload_id.clone())
        .collect::<Vec<_>>();
    workload_ids.sort();
    let digest_input = CanaryPlanDigestInput {
        protocol: CANARY_PLAN_PROTOCOL,
        release_id: &input.release.release_id,
        release_digest: &input.release.release_digest,
        production_deployment_plan_id: &input.production_deployment.plan_id,
        production_deployment_digest: &input.production_deployment.plan_digest,
        production_deployment_receipt_id: &input.production_deployment_receipt.receipt_id,
        production_deployment_observation_id: &input
            .production_deployment_observation
            .observation_id,
        production_environment: &input.production_deployment.environment,
        production_expected_environment_revision: input
            .production_deployment
            .expected_environment_revision,
        reliability_contract: &input.reliability_contract,
        release_rollback_constraints: input.release.rollback,
        policy_evidence_id: &input.policy.evidence_id,
        policy_evidence_digest: &input.policy.evidence_digest,
        environment_verification_id: &input.environment_verification.verification_id,
        environment_verification_digest: &input.environment_verification.verification_digest,
        previous_known_good_plan_id: &input.previous_known_good_deployment.plan_id,
        previous_known_good_digest: &input.previous_known_good_deployment.plan_digest,
        previous_known_good_release_id: &input.previous_known_good_deployment.release_id,
        previous_known_good_release_digest: &input.previous_known_good_deployment.release_digest,
        previous_known_good_receipt_id: &input.previous_known_good_receipt.receipt_id,
        previous_known_good_observation_id: &input.previous_known_good_observation.observation_id,
        previous_known_good_policy_evidence_id: &input.previous_known_good_policy.evidence_id,
        previous_known_good_policy_evidence_digest: &input
            .previous_known_good_policy
            .evidence_digest,
        previous_known_good_gateway_plan_id: &input.previous_known_good_gateway.plan_id,
        previous_known_good_gateway_plan_digest: &input.previous_known_good_gateway.plan_digest,
        previous_known_good_gateway_configuration_identity: &input
            .previous_known_good_gateway
            .configuration_identity,
        previous_known_good_gateway_observation_id: &input
            .previous_known_good_gateway_observation
            .observation_id,
        previous_known_good_gateway_observation_revision: input
            .previous_known_good_gateway_observation
            .revision,
        previous_known_good_gateway_observed_after: &input
            .previous_known_good_gateway_observation
            .observed_after,
        initial_percent: input.initial_percent,
        maximum_percent: input.maximum_percent,
        workload_ids: &workload_ids,
    };
    let effects = DeliveryEffects::default();
    let plan_digest = digest_json(&(digest_input, &effects));
    Ok(CanaryPlan {
        protocol: CANARY_PLAN_PROTOCOL.to_owned(),
        plan_id: format!("canary-plan:{plan_digest}"),
        plan_digest,
        release_id: input.release.release_id,
        release_digest: input.release.release_digest,
        production_deployment_plan_id: input.production_deployment.plan_id,
        production_deployment_digest: input.production_deployment.plan_digest,
        production_deployment_receipt_id: input.production_deployment_receipt.receipt_id,
        production_deployment_observation_id: input
            .production_deployment_observation
            .observation_id,
        production_environment: input.production_deployment.environment,
        production_expected_environment_revision: input
            .production_deployment
            .expected_environment_revision,
        reliability_contract: input.reliability_contract,
        release_rollback_constraints: input.release.rollback,
        policy_evidence_id: input.policy.evidence_id,
        policy_evidence_digest: input.policy.evidence_digest,
        environment_verification_id: input.environment_verification.verification_id,
        environment_verification_digest: input.environment_verification.verification_digest,
        previous_known_good_plan_id: input.previous_known_good_deployment.plan_id,
        previous_known_good_deployment_digest: input.previous_known_good_deployment.plan_digest,
        previous_known_good_release_id: input.previous_known_good_deployment.release_id,
        previous_known_good_release_digest: input.previous_known_good_deployment.release_digest,
        previous_known_good_receipt_id: input.previous_known_good_receipt.receipt_id,
        previous_known_good_observation_id: input.previous_known_good_observation.observation_id,
        previous_known_good_policy_evidence_id: input.previous_known_good_policy.evidence_id,
        previous_known_good_policy_evidence_digest: input
            .previous_known_good_policy
            .evidence_digest,
        previous_known_good_gateway_plan_id: input.previous_known_good_gateway.plan_id,
        previous_known_good_gateway_plan_digest: input.previous_known_good_gateway.plan_digest,
        previous_known_good_gateway_configuration_identity: input
            .previous_known_good_gateway
            .configuration_identity,
        previous_known_good_gateway_observation_id: input
            .previous_known_good_gateway_observation
            .observation_id,
        previous_known_good_gateway_observation_revision: input
            .previous_known_good_gateway_observation
            .revision,
        previous_known_good_gateway_observed_after: input
            .previous_known_good_gateway_observation
            .observed_after,
        initial_percent: input.initial_percent,
        maximum_percent: input.maximum_percent,
        workload_ids,
        effects,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CanaryPlanDigestInput<'a> {
    protocol: &'a str,
    release_id: &'a str,
    release_digest: &'a str,
    production_deployment_plan_id: &'a str,
    production_deployment_digest: &'a str,
    production_deployment_receipt_id: &'a str,
    production_deployment_observation_id: &'a str,
    production_environment: &'a str,
    production_expected_environment_revision: u64,
    reliability_contract: &'a DeliveryReliabilityContract,
    release_rollback_constraints: ReleaseRollbackConstraints,
    policy_evidence_id: &'a str,
    policy_evidence_digest: &'a str,
    environment_verification_id: &'a str,
    environment_verification_digest: &'a str,
    previous_known_good_plan_id: &'a str,
    previous_known_good_digest: &'a str,
    previous_known_good_release_id: &'a str,
    previous_known_good_release_digest: &'a str,
    previous_known_good_receipt_id: &'a str,
    previous_known_good_observation_id: &'a str,
    previous_known_good_policy_evidence_id: &'a str,
    previous_known_good_policy_evidence_digest: &'a str,
    previous_known_good_gateway_plan_id: &'a str,
    previous_known_good_gateway_plan_digest: &'a str,
    previous_known_good_gateway_configuration_identity: &'a str,
    previous_known_good_gateway_observation_id: &'a str,
    previous_known_good_gateway_observation_revision: u64,
    previous_known_good_gateway_observed_after: &'a str,
    initial_percent: u8,
    maximum_percent: u8,
    workload_ids: &'a [String],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DependencyReliabilityObservation {
    pub dependency_id: String,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_degraded_mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReliabilityObservation {
    pub protocol: String,
    pub observation_id: String,
    pub canary_plan_id: String,
    pub canary_plan_digest: String,
    pub release_id: String,
    pub release_digest: String,
    pub environment: String,
    pub deployment_plan_id: String,
    pub deployment_plan_digest: String,
    pub deployment_observation_id: String,
    pub collector_id: String,
    pub collector_proof: String,
    pub observed_revision: u64,
    pub freshness_horizon_revision: u64,
    pub fresh: bool,
    pub observation_window_seconds: u64,
    pub sample_count: u64,
    pub generic_process_healthy: bool,
    pub workload_readiness: BTreeMap<String, bool>,
    pub workload_liveness: BTreeMap<String, bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub availability_basis_points: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_p99_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_budget_used_basis_points: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_backlog: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_backlog: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timer_lag_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_exhaustion: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compensation_pressure: Option<u64>,
    pub dependencies: Vec<DependencyReliabilityObservation>,
    pub failure_domains: BTreeMap<String, bool>,
    pub scaling_check_passed: Option<bool>,
    pub disruption_check_passed: Option<bool>,
    pub availability_check_passed: Option<bool>,
    #[serde(default)]
    pub evidence_references: Vec<String>,
}

#[must_use]
pub fn seal_reliability_observation(
    plan: &CanaryPlan,
    deployment_observation: &DeploymentObservation,
    provider: &dyn ReliabilityObservationProvider,
    mut observation: ReliabilityObservation,
) -> Result<ReliabilityObservation, DeliveryIssue> {
    observation.protocol = RELIABILITY_OBSERVATION_PROTOCOL.to_owned();
    observation.canary_plan_id = plan.plan_id.clone();
    observation.canary_plan_digest = plan.plan_digest.clone();
    observation.release_id = plan.release_id.clone();
    observation.release_digest = plan.release_digest.clone();
    observation.environment = plan.production_environment.clone();
    observation.deployment_plan_id = plan.production_deployment_plan_id.clone();
    observation.deployment_plan_digest = plan.production_deployment_digest.clone();
    observation.deployment_observation_id = deployment_observation.observation_id.clone();
    if !observation
        .evidence_references
        .contains(&deployment_observation.observation_id)
    {
        observation
            .evidence_references
            .push(deployment_observation.observation_id.clone());
    }
    observation.evidence_references.sort();
    observation.observation_id = format!(
        "reliability-observation:{}",
        reliability_observation_digest(&observation)
    );
    let Some(proof) = provider.sign(&observation.collector_id, &observation.observation_id) else {
        return Err(issue(
            DeliveryIssueCode::ReliabilityEvidenceMissing,
            "The Reliability collector is not authorized by the selected observation provider.",
            "Use a configured collector identity without exposing its signing material.",
            "Configure the Reliability observation provider and collect the window again.",
        ));
    };
    observation.collector_proof = proof;
    Ok(observation)
}

#[must_use]
pub fn reliability_observation_integrity_is_valid(
    observation: &ReliabilityObservation,
    provider: &dyn ReliabilityObservationProvider,
) -> bool {
    observation.protocol == RELIABILITY_OBSERVATION_PROTOCOL
        && observation.observation_id
            == format!(
                "reliability-observation:{}",
                reliability_observation_digest(observation)
            )
        && provider.verify(
            &observation.collector_id,
            &observation.observation_id,
            &observation.collector_proof,
        )
}

fn reliability_observation_digest(observation: &ReliabilityObservation) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct DigestInput<'a> {
        protocol: &'a str,
        canary_plan_id: &'a str,
        canary_plan_digest: &'a str,
        release_id: &'a str,
        release_digest: &'a str,
        environment: &'a str,
        deployment_plan_id: &'a str,
        deployment_plan_digest: &'a str,
        deployment_observation_id: &'a str,
        collector_id: &'a str,
        observed_revision: u64,
        freshness_horizon_revision: u64,
        fresh: bool,
        observation_window_seconds: u64,
        sample_count: u64,
        generic_process_healthy: bool,
        workload_readiness: &'a BTreeMap<String, bool>,
        workload_liveness: &'a BTreeMap<String, bool>,
        availability_basis_points: Option<u32>,
        latency_p99_ms: Option<u64>,
        error_budget_used_basis_points: Option<u32>,
        queue_backlog: Option<u64>,
        workflow_backlog: Option<u64>,
        timer_lag_ms: Option<u64>,
        retry_exhaustion: Option<u64>,
        compensation_pressure: Option<u64>,
        dependencies: &'a [DependencyReliabilityObservation],
        failure_domains: &'a BTreeMap<String, bool>,
        scaling_check_passed: Option<bool>,
        disruption_check_passed: Option<bool>,
        availability_check_passed: Option<bool>,
        evidence_references: &'a [String],
    }
    digest_json(&DigestInput {
        protocol: observation.protocol.as_str(),
        canary_plan_id: observation.canary_plan_id.as_str(),
        canary_plan_digest: observation.canary_plan_digest.as_str(),
        release_id: observation.release_id.as_str(),
        release_digest: observation.release_digest.as_str(),
        environment: observation.environment.as_str(),
        deployment_plan_id: observation.deployment_plan_id.as_str(),
        deployment_plan_digest: observation.deployment_plan_digest.as_str(),
        deployment_observation_id: observation.deployment_observation_id.as_str(),
        collector_id: observation.collector_id.as_str(),
        observed_revision: observation.observed_revision,
        freshness_horizon_revision: observation.freshness_horizon_revision,
        fresh: observation.fresh,
        observation_window_seconds: observation.observation_window_seconds,
        sample_count: observation.sample_count,
        generic_process_healthy: observation.generic_process_healthy,
        workload_readiness: &observation.workload_readiness,
        workload_liveness: &observation.workload_liveness,
        availability_basis_points: observation.availability_basis_points,
        latency_p99_ms: observation.latency_p99_ms,
        error_budget_used_basis_points: observation.error_budget_used_basis_points,
        queue_backlog: observation.queue_backlog,
        workflow_backlog: observation.workflow_backlog,
        timer_lag_ms: observation.timer_lag_ms,
        retry_exhaustion: observation.retry_exhaustion,
        compensation_pressure: observation.compensation_pressure,
        dependencies: observation.dependencies.as_slice(),
        failure_domains: &observation.failure_domains,
        scaling_check_passed: observation.scaling_check_passed,
        disruption_check_passed: observation.disruption_check_passed,
        availability_check_passed: observation.availability_check_passed,
        evidence_references: observation.evidence_references.as_slice(),
    })
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CanaryOutcome {
    Expand,
    HoldDegraded,
    Pause,
    Rollback,
    Converged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CanaryDecision {
    pub protocol: String,
    pub decision_id: String,
    pub plan_id: String,
    pub observation_id: String,
    pub decision: DeliveryDecision,
    pub outcome: CanaryOutcome,
    pub current_percent: u8,
    pub next_percent: u8,
    pub issues: Vec<DeliveryIssue>,
    pub active_degraded_modes: Vec<String>,
    pub evidence_references: Vec<String>,
    pub effects: DeliveryEffects,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CanaryState {
    pub plan_id: String,
    pub current_percent: u8,
    #[serde(default)]
    pub observations: Vec<ReliabilityObservation>,
    #[serde(default)]
    pub decisions: Vec<CanaryDecision>,
}

impl CanaryState {
    #[must_use]
    pub fn new(plan_id: impl Into<String>) -> Self {
        Self {
            plan_id: plan_id.into(),
            current_percent: 0,
            observations: Vec::new(),
            decisions: Vec::new(),
        }
    }
}

#[must_use]
pub fn evaluate_canary(
    state: &mut CanaryState,
    plan: &CanaryPlan,
    observation: ReliabilityObservation,
    provider: &dyn ReliabilityObservationProvider,
) -> CanaryDecision {
    evaluate_canary_internal(state, plan, observation, provider, true)
}

fn evaluate_canary_internal(
    state: &mut CanaryState,
    plan: &CanaryPlan,
    mut observation: ReliabilityObservation,
    provider: &dyn ReliabilityObservationProvider,
    verify_history: bool,
) -> CanaryDecision {
    if !canary_plan_integrity_is_valid(plan) {
        let issues = vec![issue(
            DeliveryIssueCode::StaleInput,
            "The Canary plan identity or protected evidence was modified after planning.",
            "Use the exact content-addressed Canary plan.",
            "Regenerate the Canary plan from current production evidence.",
        )];
        let mut blocked = CanaryDecision {
            protocol: CANARY_DECISION_PROTOCOL.to_owned(),
            decision_id: String::new(),
            plan_id: plan.plan_id.clone(),
            observation_id: observation.observation_id,
            decision: DeliveryDecision::Blocked,
            outcome: CanaryOutcome::Pause,
            current_percent: state.current_percent,
            next_percent: state.current_percent,
            issues,
            active_degraded_modes: Vec::new(),
            evidence_references: Vec::new(),
            effects: DeliveryEffects::default(),
        };
        blocked.decision_id = format!("canary-decision:{}", canary_decision_digest(&blocked));
        return blocked;
    }
    observation.evidence_references.sort();
    if verify_history && !canary_history_is_valid(state, plan, provider) {
        return blocked_canary_decision(
            plan,
            state.current_percent,
            observation.observation_id,
            "The Canary state is not the verified result of its append-only observation and decision history.",
            "Rebuild state from every signed observation and canonical decision before accepting another observation.",
            "Repair the evidence ledger before changing traffic exposure.",
        );
    }
    let replay_observation_index = state
        .observations
        .iter()
        .position(|stored| stored.observation_id == observation.observation_id);
    let replay_decision_index = state
        .decisions
        .iter()
        .position(|decision| decision.observation_id == observation.observation_id);
    if replay_observation_index.is_some() || replay_decision_index.is_some() {
        if replay_observation_index == replay_decision_index
            && let Some(index) = replay_observation_index
            && state.observations[index] == observation
        {
            return state.decisions[index].clone();
        }
        return blocked_canary_decision(
            plan,
            state.current_percent,
            observation.observation_id,
            "The Reliability observation replay does not match a verified Canary history.",
            "Preserve each signed observation and recomputed decision as one append-only pair.",
            "Repair the evidence ledger and collect a new observation for the current exposure.",
        );
    }
    if state
        .decisions
        .last()
        .is_some_and(|decision| decision.outcome == CanaryOutcome::Rollback)
    {
        return blocked_canary_decision(
            plan,
            state.current_percent,
            observation.observation_id,
            "The Canary rollout already reached its terminal rollback boundary.",
            "Create a new Service Release and Canary plan instead of reopening a rolled-back release.",
            "Keep exposure at zero and investigate the failed release.",
        );
    }
    if state.current_percent == 0 {
        state.current_percent = plan.initial_percent;
    }
    let current_percent = state.current_percent;
    let mut issues = Vec::new();
    let mut active_degraded_modes = Vec::new();
    let workload_ids = plan
        .workload_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let readiness_ids = observation
        .workload_readiness
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let liveness_ids = observation
        .workload_liveness
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let required_metrics_present = observation.availability_basis_points.is_some()
        && observation.latency_p99_ms.is_some()
        && observation.error_budget_used_basis_points.is_some()
        && observation.queue_backlog.is_some()
        && observation.workflow_backlog.is_some()
        && observation.timer_lag_ms.is_some()
        && observation.retry_exhaustion.is_some()
        && observation.compensation_pressure.is_some()
        && observation.scaling_check_passed.is_some()
        && observation.disruption_check_passed.is_some()
        && observation.availability_check_passed.is_some();
    let observed_dependencies = observation
        .dependencies
        .iter()
        .map(|dependency| dependency.dependency_id.as_str())
        .collect::<BTreeSet<_>>();
    let dependencies_are_unique = observed_dependencies.len() == observation.dependencies.len();
    let declared_dependencies = plan
        .reliability_contract
        .dependencies
        .iter()
        .map(|dependency| dependency.dependency_id.as_str())
        .collect::<BTreeSet<_>>();
    let reliability_evidence_invalid = state.plan_id != plan.plan_id
        || !reliability_observation_integrity_is_valid(&observation, provider)
        || observation.canary_plan_id != plan.plan_id
        || observation.canary_plan_digest != plan.plan_digest
        || observation.release_id != plan.release_id
        || observation.release_digest != plan.release_digest
        || observation.environment != plan.production_environment
        || observation.deployment_plan_id != plan.production_deployment_plan_id
        || observation.deployment_plan_digest != plan.production_deployment_digest
        || observation.deployment_observation_id != plan.production_deployment_observation_id
        || observation.collector_id.trim().is_empty()
        || observation.observed_revision <= plan.production_expected_environment_revision
        || observation.freshness_horizon_revision < observation.observed_revision
        || !observation.fresh
        || observation.observation_window_seconds
            < plan.reliability_contract.minimum_observation_seconds
        || observation.sample_count < plan.reliability_contract.minimum_sample_count
        || !required_metrics_present
        || readiness_ids != workload_ids
        || liveness_ids != workload_ids
        || observed_dependencies != declared_dependencies
        || !dependencies_are_unique;
    if reliability_evidence_invalid {
        issues.push(issue(
            DeliveryIssueCode::ReliabilityEvidenceMissing,
            "Canary evaluation lacks fresh, complete Service Reliability observations or the minimum observation window.",
            "Collect every declared Workload, objective, dependency, backlog, timer, retry, compensation, and failure-domain observation.",
            "Hold exposure and refresh the Service Reliability evidence.",
        ));
    }
    let mut objective_breach = observation
        .workload_readiness
        .values()
        .chain(observation.workload_liveness.values())
        .any(|healthy| !healthy);
    objective_breach |= observation
        .availability_basis_points
        .is_some_and(|value| value < plan.reliability_contract.minimum_availability_basis_points);
    objective_breach |= observation
        .latency_p99_ms
        .is_some_and(|value| value > plan.reliability_contract.maximum_latency_p99_ms);
    objective_breach |= observation
        .error_budget_used_basis_points
        .is_some_and(|value| {
            value
                > plan
                    .reliability_contract
                    .maximum_error_budget_used_basis_points
        });
    objective_breach |= observation
        .queue_backlog
        .is_some_and(|value| value > plan.reliability_contract.maximum_queue_backlog);
    objective_breach |= observation
        .workflow_backlog
        .is_some_and(|value| value > plan.reliability_contract.maximum_workflow_backlog);
    objective_breach |= observation
        .timer_lag_ms
        .is_some_and(|value| value > plan.reliability_contract.maximum_timer_lag_ms);
    objective_breach |= observation
        .retry_exhaustion
        .is_some_and(|value| value > plan.reliability_contract.maximum_retry_exhaustion);
    objective_breach |= observation
        .compensation_pressure
        .is_some_and(|value| value > plan.reliability_contract.maximum_compensation_pressure);
    objective_breach |= observation
        .failure_domains
        .values()
        .filter(|healthy| **healthy)
        .count()
        < plan.reliability_contract.minimum_healthy_failure_domains as usize;

    let observations_by_dependency = observation
        .dependencies
        .iter()
        .map(|item| (item.dependency_id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    for dependency in &plan.reliability_contract.dependencies {
        let Some(observed) = observations_by_dependency.get(dependency.dependency_id.as_str())
        else {
            continue;
        };
        if observed.available {
            continue;
        }
        match dependency.criticality {
            DependencyCriticality::Critical => objective_breach = true,
            DependencyCriticality::Degradable => {
                if let Some(mode) = observed.active_degraded_mode.as_ref()
                    && dependency.allowed_degraded_modes.contains(mode)
                {
                    active_degraded_modes.push(format!("{}:{mode}", dependency.dependency_id));
                } else {
                    objective_breach = true;
                }
            }
            DependencyCriticality::Optional => {
                active_degraded_modes.push(format!("{}:unavailable", dependency.dependency_id));
            }
        }
    }
    if reliability_evidence_invalid {
        objective_breach = false;
        active_degraded_modes.clear();
    }
    if objective_breach {
        issues.push(issue(
            DeliveryIssueCode::CanaryBreach,
            "The canary breached one or more declared Service Reliability objectives.",
            "Limit exposure and evaluate the verified rollback boundary.",
            "Start bounded rollback when safe, otherwise pause for intervention.",
        ));
    }
    let expansion_checks_passed = observation.scaling_check_passed == Some(true)
        && observation.disruption_check_passed == Some(true)
        && observation.availability_check_passed == Some(true);
    let (decision, outcome, next_percent) = if objective_breach {
        (DeliveryDecision::Blocked, CanaryOutcome::Rollback, 0)
    } else if !issues.is_empty() || !expansion_checks_passed {
        if !expansion_checks_passed && issues.is_empty() {
            issues.push(issue(
                DeliveryIssueCode::ReliabilityEvidenceMissing,
                "Scaling, disruption, or availability safety checks do not permit canary expansion.",
                "Satisfy all declared availability and disruption constraints.",
                "Hold exposure and refresh rollout safety evidence.",
            ));
        }
        (
            DeliveryDecision::Blocked,
            CanaryOutcome::Pause,
            current_percent,
        )
    } else if !active_degraded_modes.is_empty() {
        (
            DeliveryDecision::Advisory,
            CanaryOutcome::HoldDegraded,
            current_percent,
        )
    } else if current_percent >= plan.maximum_percent {
        (
            DeliveryDecision::Passed,
            CanaryOutcome::Converged,
            current_percent,
        )
    } else {
        (
            DeliveryDecision::Passed,
            CanaryOutcome::Expand,
            current_percent.saturating_mul(2).min(plan.maximum_percent),
        )
    };
    let mut result = CanaryDecision {
        protocol: CANARY_DECISION_PROTOCOL.to_owned(),
        decision_id: String::new(),
        plan_id: plan.plan_id.clone(),
        observation_id: observation.observation_id.clone(),
        decision,
        outcome,
        current_percent,
        next_percent,
        issues,
        active_degraded_modes,
        evidence_references: observation.evidence_references.clone(),
        effects: DeliveryEffects::default(),
    };
    result.decision_id = format!("canary-decision:{}", canary_decision_digest(&result));
    state.observations.push(observation);
    state.decisions.push(result.clone());
    if matches!(
        outcome,
        CanaryOutcome::Expand | CanaryOutcome::Converged | CanaryOutcome::Rollback
    ) {
        state.current_percent = next_percent;
    }
    result
}

fn canary_history_is_valid(
    state: &CanaryState,
    plan: &CanaryPlan,
    provider: &dyn ReliabilityObservationProvider,
) -> bool {
    let observation_ids = state
        .observations
        .iter()
        .map(|stored| stored.observation_id.as_str())
        .collect::<BTreeSet<_>>();
    let decision_observation_ids = state
        .decisions
        .iter()
        .map(|decision| decision.observation_id.as_str())
        .collect::<BTreeSet<_>>();
    if state.plan_id != plan.plan_id
        || state.current_percent > plan.maximum_percent
        || state.observations.len() != state.decisions.len()
        || observation_ids.len() != state.observations.len()
        || decision_observation_ids.len() != state.decisions.len()
        || state
            .observations
            .iter()
            .zip(&state.decisions)
            .any(|(stored, decision)| stored.observation_id != decision.observation_id)
    {
        return false;
    }
    let mut rebuilt = CanaryState::new(plan.plan_id.clone());
    for (stored, decision) in state.observations.iter().zip(&state.decisions) {
        let recomputed =
            evaluate_canary_internal(&mut rebuilt, plan, stored.clone(), provider, false);
        if recomputed != *decision || !canary_decision_integrity_is_valid(decision) {
            return false;
        }
    }
    rebuilt.current_percent == state.current_percent
}

fn blocked_canary_decision(
    plan: &CanaryPlan,
    current_percent: u8,
    observation_id: String,
    message: &str,
    remediation: &str,
    next_action: &str,
) -> CanaryDecision {
    let mut blocked = CanaryDecision {
        protocol: CANARY_DECISION_PROTOCOL.to_owned(),
        decision_id: String::new(),
        plan_id: plan.plan_id.clone(),
        observation_id,
        decision: DeliveryDecision::Blocked,
        outcome: CanaryOutcome::Pause,
        current_percent,
        next_percent: current_percent,
        issues: vec![issue(
            DeliveryIssueCode::StaleInput,
            message,
            remediation,
            next_action,
        )],
        active_degraded_modes: Vec::new(),
        evidence_references: Vec::new(),
        effects: DeliveryEffects::default(),
    };
    blocked.decision_id = format!("canary-decision:{}", canary_decision_digest(&blocked));
    blocked
}

#[must_use]
pub fn canary_plan_integrity_is_valid(plan: &CanaryPlan) -> bool {
    plan.protocol == CANARY_PLAN_PROTOCOL
        && plan.plan_id == format!("canary-plan:{}", plan.plan_digest)
        && digest_json(&(
            CanaryPlanDigestInput {
                protocol: plan.protocol.as_str(),
                release_id: plan.release_id.as_str(),
                release_digest: plan.release_digest.as_str(),
                production_deployment_plan_id: plan.production_deployment_plan_id.as_str(),
                production_deployment_digest: plan.production_deployment_digest.as_str(),
                production_deployment_receipt_id: plan.production_deployment_receipt_id.as_str(),
                production_deployment_observation_id: plan
                    .production_deployment_observation_id
                    .as_str(),
                production_environment: plan.production_environment.as_str(),
                production_expected_environment_revision: plan
                    .production_expected_environment_revision,
                reliability_contract: &plan.reliability_contract,
                release_rollback_constraints: plan.release_rollback_constraints,
                policy_evidence_id: plan.policy_evidence_id.as_str(),
                policy_evidence_digest: plan.policy_evidence_digest.as_str(),
                environment_verification_id: plan.environment_verification_id.as_str(),
                environment_verification_digest: plan.environment_verification_digest.as_str(),
                previous_known_good_plan_id: plan.previous_known_good_plan_id.as_str(),
                previous_known_good_digest: plan.previous_known_good_deployment_digest.as_str(),
                previous_known_good_release_id: plan.previous_known_good_release_id.as_str(),
                previous_known_good_release_digest: plan
                    .previous_known_good_release_digest
                    .as_str(),
                previous_known_good_receipt_id: plan.previous_known_good_receipt_id.as_str(),
                previous_known_good_observation_id: plan
                    .previous_known_good_observation_id
                    .as_str(),
                previous_known_good_policy_evidence_id: plan
                    .previous_known_good_policy_evidence_id
                    .as_str(),
                previous_known_good_policy_evidence_digest: plan
                    .previous_known_good_policy_evidence_digest
                    .as_str(),
                previous_known_good_gateway_plan_id: plan
                    .previous_known_good_gateway_plan_id
                    .as_str(),
                previous_known_good_gateway_plan_digest: plan
                    .previous_known_good_gateway_plan_digest
                    .as_str(),
                previous_known_good_gateway_configuration_identity: plan
                    .previous_known_good_gateway_configuration_identity
                    .as_str(),
                previous_known_good_gateway_observation_id: plan
                    .previous_known_good_gateway_observation_id
                    .as_str(),
                previous_known_good_gateway_observation_revision: plan
                    .previous_known_good_gateway_observation_revision,
                previous_known_good_gateway_observed_after: plan
                    .previous_known_good_gateway_observed_after
                    .as_str(),
                initial_percent: plan.initial_percent,
                maximum_percent: plan.maximum_percent,
                workload_ids: plan.workload_ids.as_slice(),
            },
            &plan.effects,
        )) == plan.plan_digest
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CanaryDecisionDigestInput<'a> {
    protocol: &'a str,
    plan_id: &'a str,
    observation_id: &'a str,
    decision: DeliveryDecision,
    outcome: CanaryOutcome,
    current_percent: u8,
    next_percent: u8,
    issues: &'a [DeliveryIssue],
    active_degraded_modes: &'a [String],
    evidence_references: &'a [String],
    effects: &'a DeliveryEffects,
}

fn canary_decision_digest(decision: &CanaryDecision) -> String {
    digest_json(&CanaryDecisionDigestInput {
        protocol: decision.protocol.as_str(),
        plan_id: decision.plan_id.as_str(),
        observation_id: decision.observation_id.as_str(),
        decision: decision.decision,
        outcome: decision.outcome,
        current_percent: decision.current_percent,
        next_percent: decision.next_percent,
        issues: decision.issues.as_slice(),
        active_degraded_modes: decision.active_degraded_modes.as_slice(),
        evidence_references: decision.evidence_references.as_slice(),
        effects: &decision.effects,
    })
}

#[must_use]
pub fn canary_decision_integrity_is_valid(decision: &CanaryDecision) -> bool {
    decision.protocol == CANARY_DECISION_PROTOCOL
        && decision.decision_id == format!("canary-decision:{}", canary_decision_digest(decision))
        && decision.effects == DeliveryEffects::default()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RollbackSafetyEvidence {
    pub protocol: String,
    pub evidence_id: String,
    pub canary_plan_id: String,
    pub failed_deployment_plan_id: String,
    pub previous_deployment_plan_id: String,
    pub expected_environment_revision: u64,
    pub provider_id: String,
    pub provider_proof: String,
    pub migrations_reversible: bool,
    pub destructive_changes_absent: bool,
    pub workflows_downgrade_safe: bool,
    pub config_revision_compatible: bool,
    pub secret_references_resolvable: bool,
    pub edge_configuration_compatible: bool,
    pub adapter_recovery_complete: bool,
    pub policy_approved: bool,
    #[serde(default)]
    pub evidence_references: Vec<String>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackSafetyInput {
    pub migrations_reversible: bool,
    pub destructive_changes_absent: bool,
    pub workflows_downgrade_safe: bool,
    pub config_revision_compatible: bool,
    pub secret_references_resolvable: bool,
    pub edge_configuration_compatible: bool,
    pub adapter_recovery_complete: bool,
    pub policy_approved: bool,
    pub evidence_references: Vec<String>,
}

pub fn seal_rollback_safety_evidence(
    canary: &CanaryPlan,
    failed: &DeploymentPlan,
    previous: &DeploymentPlan,
    expected_environment_revision: u64,
    provider: &dyn RollbackSafetyProvider,
    input: RollbackSafetyInput,
) -> Result<RollbackSafetyEvidence, DeliveryIssue> {
    let mut evidence = RollbackSafetyEvidence {
        protocol: ROLLBACK_SAFETY_PROTOCOL.to_owned(),
        evidence_id: String::new(),
        canary_plan_id: canary.plan_id.clone(),
        failed_deployment_plan_id: failed.plan_id.clone(),
        previous_deployment_plan_id: previous.plan_id.clone(),
        expected_environment_revision,
        provider_id: provider.provider_id().to_owned(),
        provider_proof: String::new(),
        migrations_reversible: input.migrations_reversible,
        destructive_changes_absent: input.destructive_changes_absent,
        workflows_downgrade_safe: input.workflows_downgrade_safe,
        config_revision_compatible: input.config_revision_compatible,
        secret_references_resolvable: input.secret_references_resolvable,
        edge_configuration_compatible: input.edge_configuration_compatible,
        adapter_recovery_complete: input.adapter_recovery_complete,
        policy_approved: input.policy_approved,
        evidence_references: input.evidence_references,
    };
    evidence.evidence_references.sort();
    evidence.evidence_references.dedup();
    evidence.evidence_id = format!(
        "rollback-safety:{}",
        rollback_safety_evidence_digest(&evidence)
    );
    evidence.provider_proof = provider.sign(&evidence.evidence_id).ok_or_else(|| {
        issue(
            DeliveryIssueCode::RollbackUnsafe,
            "The rollback safety provider refused to attest the exact recovery inputs.",
            "Collect complete migration, workflow, configuration, secret, edge, adapter, and policy evidence.",
            "Pause recovery and refresh rollback safety evidence.",
        )
    })?;
    Ok(evidence)
}

#[must_use]
pub fn rollback_safety_evidence_integrity_is_valid(
    evidence: &RollbackSafetyEvidence,
    provider: &dyn RollbackSafetyProvider,
) -> bool {
    evidence.protocol == ROLLBACK_SAFETY_PROTOCOL
        && evidence.provider_id == provider.provider_id()
        && evidence.evidence_id
            == format!(
                "rollback-safety:{}",
                rollback_safety_evidence_digest(evidence)
            )
        && provider.verify(&evidence.evidence_id, &evidence.provider_proof)
}

fn rollback_safety_evidence_digest(evidence: &RollbackSafetyEvidence) -> String {
    digest_json(&(
        evidence.protocol.as_str(),
        evidence.canary_plan_id.as_str(),
        evidence.failed_deployment_plan_id.as_str(),
        evidence.previous_deployment_plan_id.as_str(),
        evidence.expected_environment_revision,
        evidence.provider_id.as_str(),
        evidence.migrations_reversible,
        evidence.destructive_changes_absent,
        evidence.workflows_downgrade_safe,
        evidence.config_revision_compatible,
        evidence.secret_references_resolvable,
        evidence.edge_configuration_compatible,
        evidence.adapter_recovery_complete,
        evidence.policy_approved,
        evidence.evidence_references.as_slice(),
    ))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RollbackPlan {
    pub protocol: String,
    pub plan_id: String,
    pub plan_digest: String,
    pub canary_plan_id: String,
    pub canary_plan_digest: String,
    pub canary_decision_id: String,
    pub environment: String,
    pub expected_environment_revision: u64,
    pub failed_release_id: String,
    pub failed_release_digest: String,
    pub failed_deployment_plan_id: String,
    pub failed_deployment_plan_digest: String,
    pub failed_config_revision_id: String,
    pub failed_workload_digests: BTreeMap<String, String>,
    pub failed_gateway_plan_id: String,
    pub failed_gateway_plan_digest: String,
    pub previous_release_id: String,
    pub previous_release_digest: String,
    pub previous_deployment_plan_id: String,
    pub previous_deployment_plan_digest: String,
    pub previous_config_revision_id: String,
    pub previous_workload_digests: BTreeMap<String, String>,
    pub previous_gateway_plan_id: String,
    pub previous_gateway_plan_digest: String,
    pub previous_gateway_configuration_identity: String,
    pub previous_gateway_revision: u64,
    pub release_rollback_constraints: ReleaseRollbackConstraints,
    pub safety: RollbackSafetyEvidence,
    pub automatic_allowed: bool,
    pub issues: Vec<DeliveryIssue>,
    pub prohibited_actions: Vec<String>,
    pub effects: DeliveryEffects,
}

pub fn plan_rollback(
    canary: &CanaryPlan,
    breach: &CanaryDecision,
    breach_observation: &ReliabilityObservation,
    reliability_provider: &dyn ReliabilityObservationProvider,
    failed: &DeploymentPlan,
    failed_gateway: &GatewayConfigurationPlan,
    previous: &DeploymentPlan,
    previous_gateway: &GatewayConfigurationPlan,
    expected_environment_revision: u64,
    mut safety: RollbackSafetyEvidence,
    safety_provider: &dyn RollbackSafetyProvider,
    trust_provider: &dyn ReleaseTrustProvider,
) -> Result<RollbackPlan, Vec<DeliveryIssue>> {
    safety.evidence_references.sort();
    safety.evidence_references.dedup();
    let mut expected_canary_state = CanaryState::new(canary.plan_id.clone());
    expected_canary_state.current_percent = breach.current_percent;
    let expected_breach = evaluate_canary_internal(
        &mut expected_canary_state,
        canary,
        breach_observation.clone(),
        reliability_provider,
        false,
    );
    if !canary_plan_integrity_is_valid(canary)
        || !canary_decision_integrity_is_valid(breach)
        || !reliability_observation_integrity_is_valid(breach_observation, reliability_provider)
        || breach.observation_id != breach_observation.observation_id
        || breach != &expected_breach
        || breach.plan_id != canary.plan_id
        || breach.decision != DeliveryDecision::Blocked
        || breach.outcome != CanaryOutcome::Rollback
        || !deployment_plan_integrity_is_valid(failed)
        || !deployment_plan_integrity_is_valid(previous)
        || !gateway_plan_authority_is_valid(failed_gateway, trust_provider)
        || !gateway_plan_authority_is_valid(previous_gateway, trust_provider)
        || failed.plan_id != canary.production_deployment_plan_id
        || failed.plan_digest != canary.production_deployment_digest
        || previous.plan_id != canary.previous_known_good_plan_id
        || previous.plan_digest != canary.previous_known_good_deployment_digest
        || previous.release_id != canary.previous_known_good_release_id
        || previous.release_digest != canary.previous_known_good_release_digest
        || failed.plan_id == previous.plan_id
        || failed.release_id == previous.release_id
        || failed.gateway_plan_digest != failed_gateway.plan_digest
        || previous.gateway_plan_digest != previous_gateway.plan_digest
        || failed_gateway.edge_release_id != failed.release_id
        || failed_gateway.edge_release_digest != failed.release_digest
        || previous_gateway.edge_release_id != previous.release_id
        || previous_gateway.edge_release_digest != previous.release_digest
        || failed_gateway.configuration_identity == previous_gateway.configuration_identity
        || failed.environment != previous.environment
        || failed.environment != failed_gateway.environment
        || previous.environment != previous_gateway.environment
        || !rollback_safety_evidence_integrity_is_valid(&safety, safety_provider)
        || safety.canary_plan_id != canary.plan_id
        || safety.failed_deployment_plan_id != failed.plan_id
        || safety.previous_deployment_plan_id != previous.plan_id
        || safety.expected_environment_revision != expected_environment_revision
    {
        return Err(vec![issue(
            DeliveryIssueCode::RollbackUnsafe,
            "Rollback planning requires an objective canary breach and exact current and previous Deployment plans.",
            "Preserve the failed and previous known-good release evidence.",
            "Refresh canary and Deployment evidence before rollback.",
        )]);
    }
    let automatic_allowed = canary.release_rollback_constraints.automatic_allowed
        && safety.migrations_reversible
        && safety.destructive_changes_absent
        && safety.workflows_downgrade_safe
        && safety.config_revision_compatible
        && safety.secret_references_resolvable
        && safety.edge_configuration_compatible
        && safety.adapter_recovery_complete
        && safety.policy_approved;
    let issues = if automatic_allowed {
        Vec::new()
    } else {
        vec![issue(
            DeliveryIssueCode::RollbackIncomplete,
            "Automatic rollback is unsafe, incomplete, or lacks required recovery evidence.",
            "Limit exposure without deleting Service Data or reversing irreversible effects.",
            "Pause for an explicit intervention Approval Boundary.",
        )]
    };
    let prohibited_actions = vec![
        "delete_service_data".to_owned(),
        "reverse_irreversible_migration".to_owned(),
        "retire_contract_version".to_owned(),
        "change_trust_root".to_owned(),
        "invent_business_compensation".to_owned(),
    ];
    let failed_workload_digests = deployment_workload_digests(failed);
    let previous_workload_digests = deployment_workload_digests(previous);
    let mut plan = RollbackPlan {
        protocol: ROLLBACK_PLAN_PROTOCOL.to_owned(),
        plan_id: String::new(),
        plan_digest: String::new(),
        canary_plan_id: canary.plan_id.clone(),
        canary_plan_digest: canary.plan_digest.clone(),
        canary_decision_id: breach.decision_id.clone(),
        environment: failed.environment.clone(),
        expected_environment_revision,
        failed_release_id: failed.release_id.clone(),
        failed_release_digest: failed.release_digest.clone(),
        failed_deployment_plan_id: failed.plan_id.clone(),
        failed_deployment_plan_digest: failed.plan_digest.clone(),
        failed_config_revision_id: failed.config_revision_id.clone(),
        failed_workload_digests,
        failed_gateway_plan_id: failed_gateway.plan_id.clone(),
        failed_gateway_plan_digest: failed.gateway_plan_digest.clone(),
        previous_release_id: previous.release_id.clone(),
        previous_release_digest: previous.release_digest.clone(),
        previous_deployment_plan_id: previous.plan_id.clone(),
        previous_deployment_plan_digest: previous.plan_digest.clone(),
        previous_config_revision_id: previous.config_revision_id.clone(),
        previous_workload_digests,
        previous_gateway_plan_id: previous_gateway.plan_id.clone(),
        previous_gateway_plan_digest: previous.gateway_plan_digest.clone(),
        previous_gateway_configuration_identity: previous_gateway.configuration_identity.clone(),
        previous_gateway_revision: previous_gateway.expected_gateway_revision,
        release_rollback_constraints: canary.release_rollback_constraints,
        safety,
        automatic_allowed,
        issues,
        prohibited_actions,
        effects: DeliveryEffects::default(),
    };
    plan.plan_digest = rollback_plan_digest(&plan);
    plan.plan_id = format!("rollback-plan:{}", plan.plan_digest);
    Ok(plan)
}

fn deployment_workload_digests(plan: &DeploymentPlan) -> BTreeMap<String, String> {
    plan.workloads
        .iter()
        .map(|workload| {
            (
                workload.workload_id.clone(),
                workload.artifact_digest.clone(),
            )
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RollbackConvergenceEvidence {
    pub protocol: String,
    pub evidence_id: String,
    pub evidence_digest: String,
    pub rollback_plan_id: String,
    pub previous_deployment_plan_id: String,
    pub previous_deployment_plan_digest: String,
    pub previous_deployment_receipt_id: String,
    pub deployment_observation_id: String,
    pub previous_gateway_plan_id: String,
    pub previous_gateway_plan_digest: String,
    pub gateway_observation_id: String,
    pub provider_id: String,
    pub provider_proof: String,
    pub observed_release_id: String,
    pub observed_release_digest: String,
    pub observed_config_revision_id: String,
    pub observed_workload_digests: BTreeMap<String, String>,
    pub observed_gateway_configuration_identity: String,
    pub observed_gateway_revision: u64,
    pub fresh: bool,
    #[serde(default)]
    pub evidence_references: Vec<String>,
}

#[must_use]
pub fn observe_rollback_convergence(
    plan: &RollbackPlan,
    previous_deployment: &DeploymentPlan,
    previous_receipt: &DeploymentReceipt,
    deployment: &DeploymentObservation,
    previous_gateway: &GatewayConfigurationPlan,
    gateway: &GatewayObservation,
    trust_provider: &dyn ReleaseTrustProvider,
    gateway_observation_provider: &dyn GatewayObservationProvider,
    convergence_provider: &dyn RollbackSafetyProvider,
    mut evidence_references: Vec<String>,
) -> Result<RollbackConvergenceEvidence, DeliveryIssue> {
    if !rollback_plan_integrity_is_valid(plan)
        || !deployment_plan_integrity_is_valid(previous_deployment)
        || previous_deployment.plan_id != plan.previous_deployment_plan_id
        || previous_deployment.plan_digest != plan.previous_deployment_plan_digest
        || previous_deployment.release_id != plan.previous_release_id
        || previous_deployment.release_digest != plan.previous_release_digest
        || previous_deployment.config_revision_id != plan.previous_config_revision_id
        || deployment_workload_digests(previous_deployment) != plan.previous_workload_digests
        || !deployment_receipt_integrity_is_valid(previous_receipt, previous_deployment)
        || !deployment_observation_integrity_is_valid(
            deployment,
            previous_deployment,
            previous_receipt,
        )
        || !gateway_plan_authority_is_valid(previous_gateway, trust_provider)
        || previous_gateway.plan_id != plan.previous_gateway_plan_id
        || previous_gateway.plan_digest != plan.previous_gateway_plan_digest
        || previous_gateway.configuration_identity != plan.previous_gateway_configuration_identity
        || previous_gateway.expected_gateway_revision != plan.previous_gateway_revision
        || !gateway_observation_integrity_is_valid(gateway, gateway_observation_provider)
        || gateway.configuration_identity != previous_gateway.configuration_identity
        || gateway.revision != previous_gateway.expected_gateway_revision
        || gateway.observed_after != deployment.source_observation_id
        || !gateway.fresh
    {
        return Err(issue(
            DeliveryIssueCode::RollbackIncomplete,
            "Rollback convergence requires the exact previous Deployment plan and receipt plus trusted Deployment and Gateway observations.",
            "Preserve the adapter receipts and collect observations for the exact previous known-good plans.",
            "Re-observe both adapters through the configured convergence provider.",
        ));
    }
    evidence_references.extend([
        previous_deployment.plan_id.clone(),
        previous_deployment.plan_digest.clone(),
        previous_receipt.receipt_id.clone(),
        deployment.observation_id.clone(),
        previous_gateway.plan_id.clone(),
        previous_gateway.plan_digest.clone(),
        gateway.observation_id.clone(),
    ]);
    evidence_references.sort();
    evidence_references.dedup();
    let mut evidence = RollbackConvergenceEvidence {
        protocol: ROLLBACK_CONVERGENCE_PROTOCOL.to_owned(),
        evidence_id: String::new(),
        evidence_digest: String::new(),
        rollback_plan_id: plan.plan_id.clone(),
        previous_deployment_plan_id: previous_deployment.plan_id.clone(),
        previous_deployment_plan_digest: previous_deployment.plan_digest.clone(),
        previous_deployment_receipt_id: previous_receipt.receipt_id.clone(),
        deployment_observation_id: deployment.observation_id.clone(),
        previous_gateway_plan_id: previous_gateway.plan_id.clone(),
        previous_gateway_plan_digest: previous_gateway.plan_digest.clone(),
        gateway_observation_id: gateway.observation_id.clone(),
        provider_id: convergence_provider.provider_id().to_owned(),
        provider_proof: String::new(),
        observed_release_id: deployment.observed_release_id.clone(),
        observed_release_digest: deployment.observed_release_digest.clone(),
        observed_config_revision_id: deployment.config_revision_id.clone(),
        observed_workload_digests: deployment.observed_workload_digests.clone(),
        observed_gateway_configuration_identity: gateway.configuration_identity.clone(),
        observed_gateway_revision: gateway.revision,
        fresh: true,
        evidence_references,
    };
    evidence.evidence_digest = rollback_convergence_digest(&evidence);
    evidence.evidence_id = format!("rollback-convergence:{}", evidence.evidence_digest);
    evidence.provider_proof = convergence_provider
        .sign(&evidence.evidence_id)
        .ok_or_else(|| {
            issue(
                DeliveryIssueCode::RollbackIncomplete,
                "The convergence provider refused to attest the exact post-rollback observations.",
                "Use the configured provider for the exact previous Deployment and Gateway evidence.",
                "Pause completion and refresh convergence evidence.",
            )
        })?;
    Ok(evidence)
}

#[must_use]
pub fn rollback_convergence_integrity_is_valid(
    evidence: &RollbackConvergenceEvidence,
    plan: &RollbackPlan,
    provider: &dyn RollbackSafetyProvider,
) -> bool {
    evidence.protocol == ROLLBACK_CONVERGENCE_PROTOCOL
        && evidence.evidence_id == format!("rollback-convergence:{}", evidence.evidence_digest)
        && evidence.evidence_digest == rollback_convergence_digest(evidence)
        && evidence.rollback_plan_id == plan.plan_id
        && evidence.previous_deployment_plan_id == plan.previous_deployment_plan_id
        && evidence.previous_deployment_plan_digest == plan.previous_deployment_plan_digest
        && !evidence.previous_deployment_receipt_id.trim().is_empty()
        && evidence.previous_gateway_plan_id == plan.previous_gateway_plan_id
        && evidence.previous_gateway_plan_digest == plan.previous_gateway_plan_digest
        && evidence.provider_id == provider.provider_id()
        && provider.verify(&evidence.evidence_id, &evidence.provider_proof)
        && evidence.observed_release_id == plan.previous_release_id
        && evidence.observed_release_digest == plan.previous_release_digest
        && evidence.observed_config_revision_id == plan.previous_config_revision_id
        && evidence.observed_workload_digests == plan.previous_workload_digests
        && evidence.observed_gateway_configuration_identity
            == plan.previous_gateway_configuration_identity
        && evidence.observed_gateway_revision == plan.previous_gateway_revision
        && evidence.fresh
}

fn rollback_convergence_digest(evidence: &RollbackConvergenceEvidence) -> String {
    digest_json(&RollbackConvergenceDigestInput {
        protocol: &evidence.protocol,
        rollback_plan_id: &evidence.rollback_plan_id,
        previous_deployment_plan_id: &evidence.previous_deployment_plan_id,
        previous_deployment_plan_digest: &evidence.previous_deployment_plan_digest,
        previous_deployment_receipt_id: &evidence.previous_deployment_receipt_id,
        deployment_observation_id: &evidence.deployment_observation_id,
        previous_gateway_plan_id: &evidence.previous_gateway_plan_id,
        previous_gateway_plan_digest: &evidence.previous_gateway_plan_digest,
        gateway_observation_id: &evidence.gateway_observation_id,
        provider_id: &evidence.provider_id,
        observed_release_id: &evidence.observed_release_id,
        observed_release_digest: &evidence.observed_release_digest,
        observed_config_revision_id: &evidence.observed_config_revision_id,
        observed_workload_digests: &evidence.observed_workload_digests,
        observed_gateway_configuration_identity: &evidence.observed_gateway_configuration_identity,
        observed_gateway_revision: evidence.observed_gateway_revision,
        fresh: evidence.fresh,
        evidence_references: &evidence.evidence_references,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RollbackConvergenceDigestInput<'a> {
    protocol: &'a str,
    rollback_plan_id: &'a str,
    previous_deployment_plan_id: &'a str,
    previous_deployment_plan_digest: &'a str,
    previous_deployment_receipt_id: &'a str,
    deployment_observation_id: &'a str,
    previous_gateway_plan_id: &'a str,
    previous_gateway_plan_digest: &'a str,
    gateway_observation_id: &'a str,
    provider_id: &'a str,
    observed_release_id: &'a str,
    observed_release_digest: &'a str,
    observed_config_revision_id: &'a str,
    observed_workload_digests: &'a BTreeMap<String, String>,
    observed_gateway_configuration_identity: &'a str,
    observed_gateway_revision: u64,
    fresh: bool,
    evidence_references: &'a [String],
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RollbackOutcome {
    RolledBack,
    InterventionRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RollbackReceipt {
    pub protocol: String,
    pub receipt_id: String,
    pub plan_id: String,
    pub actor: String,
    pub outcome: RollbackOutcome,
    pub restored_release_id: String,
    pub restored_config_revision_id: String,
    pub environment_revision_before: u64,
    pub environment_revision_after: u64,
    pub exposure_percent: u8,
    pub remaining_risks: Vec<DeliveryIssue>,
    pub approval_boundary_required: bool,
    pub evidence_references: Vec<String>,
    pub effects: DeliveryEffects,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RollbackState {
    pub environment: String,
    pub active_release_id: String,
    pub active_config_revision_id: String,
    pub environment_revision: u64,
    pub exposure_percent: u8,
    #[serde(default)]
    pub history: Vec<RollbackReceipt>,
}

impl RollbackState {
    #[must_use]
    pub fn new(
        environment: impl Into<String>,
        active_release_id: impl Into<String>,
        active_config_revision_id: impl Into<String>,
        environment_revision: u64,
        exposure_percent: u8,
    ) -> Self {
        Self {
            environment: environment.into(),
            active_release_id: active_release_id.into(),
            active_config_revision_id: active_config_revision_id.into(),
            environment_revision,
            exposure_percent,
            history: Vec::new(),
        }
    }
}

pub fn apply_rollback(
    state: &mut RollbackState,
    plan: &RollbackPlan,
    convergence: Option<&RollbackConvergenceEvidence>,
    convergence_provider: &dyn RollbackSafetyProvider,
    actor: &str,
) -> Result<RollbackReceipt, Vec<DeliveryIssue>> {
    if !rollback_plan_integrity_is_valid(plan) || actor.trim().is_empty() {
        return Err(vec![issue(
            DeliveryIssueCode::StaleInput,
            "Rollback plan integrity or actor identity no longer matches the protected operation.",
            "Use the exact content-addressed plan and authenticated recovery actor.",
            "Refresh rollback evidence before retrying.",
        )]);
    }
    if plan.automatic_allowed
        && !convergence.is_some_and(|evidence| {
            rollback_convergence_integrity_is_valid(evidence, plan, convergence_provider)
        })
    {
        return Err(vec![issue(
            DeliveryIssueCode::RollbackIncomplete,
            "Rollback cannot be declared complete before the previous Deployment and Gateway converge.",
            "Collect fresh content-addressed post-rollback observations from both adapters.",
            "Re-observe the previous release and apply the rollback receipt again.",
        )]);
    }
    if let Some(receipt) = state
        .history
        .iter()
        .find(|receipt| receipt.plan_id == plan.plan_id)
    {
        return rollback_receipt_integrity_is_valid(
            receipt,
            plan,
            convergence,
            convergence_provider,
            actor,
        )
            .then(|| receipt.clone())
            .ok_or_else(|| {
                vec![issue(
                    DeliveryIssueCode::StaleInput,
                    "The completed rollback receipt no longer matches the exact plan, actor, and convergence evidence.",
                    "Preserve the protected rollback inputs and append-only receipt together.",
                    "Restore the original evidence or create a new rollback plan.",
                )]
            });
    }
    if state.environment != plan.environment
        || state.environment_revision != plan.expected_environment_revision
        || state.active_release_id != plan.failed_release_id
        || state.active_config_revision_id != plan.failed_config_revision_id
    {
        return Err(vec![issue(
            DeliveryIssueCode::StaleInput,
            "Rollback state, environment, failed release, or actor no longer matches the plan.",
            "Refresh the durable rollout state without overwriting concurrent recovery evidence.",
            "Re-plan rollback from the current environment revision.",
        )]);
    }
    let revision_before = state.environment_revision;
    let (outcome, restored_release_id, restored_config_revision_id, effects) =
        if plan.automatic_allowed {
            state.active_release_id = plan.previous_release_id.clone();
            state.active_config_revision_id = plan.previous_config_revision_id.clone();
            state.environment_revision += 1;
            state.exposure_percent = 0;
            (
                RollbackOutcome::RolledBack,
                plan.previous_release_id.clone(),
                plan.previous_config_revision_id.clone(),
                DeliveryEffects {
                    mutates_environment: true,
                    mutates_configuration: true,
                    mutates_gateway: true,
                    mutates_deployment: true,
                    appends_ledger: true,
                },
            )
        } else {
            (
                RollbackOutcome::InterventionRequired,
                state.active_release_id.clone(),
                state.active_config_revision_id.clone(),
                DeliveryEffects {
                    appends_ledger: true,
                    ..DeliveryEffects::default()
                },
            )
        };
    let mut evidence_references = plan.safety.evidence_references.clone();
    if let Some(convergence) = convergence {
        evidence_references.push(convergence.evidence_id.clone());
        evidence_references.extend(convergence.evidence_references.clone());
    }
    evidence_references.sort();
    evidence_references.dedup();
    let receipt_id = format!(
        "rollback-receipt:{}",
        digest_json(&(
            ROLLBACK_RECEIPT_PROTOCOL,
            plan.plan_id.as_str(),
            actor,
            outcome,
            restored_release_id.as_str(),
            restored_config_revision_id.as_str(),
            revision_before,
            state.environment_revision,
            state.exposure_percent,
            evidence_references.as_slice(),
            &effects,
        ))
    );
    let receipt = RollbackReceipt {
        protocol: ROLLBACK_RECEIPT_PROTOCOL.to_owned(),
        receipt_id,
        plan_id: plan.plan_id.clone(),
        actor: actor.to_owned(),
        outcome,
        restored_release_id,
        restored_config_revision_id,
        environment_revision_before: revision_before,
        environment_revision_after: state.environment_revision,
        exposure_percent: state.exposure_percent,
        remaining_risks: plan.issues.clone(),
        approval_boundary_required: !plan.automatic_allowed,
        evidence_references,
        effects,
    };
    state.history.push(receipt.clone());
    Ok(receipt)
}

#[must_use]
pub fn rollback_receipt_integrity_is_valid(
    receipt: &RollbackReceipt,
    plan: &RollbackPlan,
    convergence: Option<&RollbackConvergenceEvidence>,
    convergence_provider: &dyn RollbackSafetyProvider,
    actor: &str,
) -> bool {
    let mut evidence_references = plan.safety.evidence_references.clone();
    if let Some(convergence) = convergence {
        evidence_references.push(convergence.evidence_id.clone());
        evidence_references.extend(convergence.evidence_references.clone());
    }
    evidence_references.sort();
    evidence_references.dedup();
    let (outcome, restored_release_id, restored_config_revision_id, expected_after, effects) =
        if plan.automatic_allowed {
            (
                RollbackOutcome::RolledBack,
                plan.previous_release_id.as_str(),
                plan.previous_config_revision_id.as_str(),
                receipt.environment_revision_before + 1,
                DeliveryEffects {
                    mutates_environment: true,
                    mutates_configuration: true,
                    mutates_gateway: true,
                    mutates_deployment: true,
                    appends_ledger: true,
                },
            )
        } else {
            (
                RollbackOutcome::InterventionRequired,
                plan.failed_release_id.as_str(),
                plan.failed_config_revision_id.as_str(),
                receipt.environment_revision_before,
                DeliveryEffects {
                    appends_ledger: true,
                    ..DeliveryEffects::default()
                },
            )
        };
    receipt.protocol == ROLLBACK_RECEIPT_PROTOCOL
        && rollback_plan_integrity_is_valid(plan)
        && (!plan.automatic_allowed
            || convergence.is_some_and(|evidence| {
                rollback_convergence_integrity_is_valid(evidence, plan, convergence_provider)
            }))
        && receipt.plan_id == plan.plan_id
        && receipt.actor == actor
        && receipt.outcome == outcome
        && receipt.restored_release_id == restored_release_id
        && receipt.restored_config_revision_id == restored_config_revision_id
        && receipt.environment_revision_before == plan.expected_environment_revision
        && receipt.environment_revision_after == expected_after
        && (!plan.automatic_allowed || receipt.exposure_percent == 0)
        && receipt.remaining_risks == plan.issues
        && receipt.approval_boundary_required == !plan.automatic_allowed
        && receipt.evidence_references == evidence_references
        && receipt.effects == effects
        && receipt.receipt_id
            == format!(
                "rollback-receipt:{}",
                digest_json(&(
                    ROLLBACK_RECEIPT_PROTOCOL,
                    plan.plan_id.as_str(),
                    actor,
                    outcome,
                    restored_release_id,
                    restored_config_revision_id,
                    receipt.environment_revision_before,
                    receipt.environment_revision_after,
                    receipt.exposure_percent,
                    evidence_references.as_slice(),
                    &effects,
                ))
            )
}

#[must_use]
pub fn rollback_plan_integrity_is_valid(plan: &RollbackPlan) -> bool {
    plan.protocol == ROLLBACK_PLAN_PROTOCOL
        && plan.plan_id == format!("rollback-plan:{}", plan.plan_digest)
        && rollback_plan_digest(plan) == plan.plan_digest
        && plan.failed_deployment_plan_id != plan.previous_deployment_plan_id
        && plan.failed_release_id != plan.previous_release_id
        && (!plan.automatic_allowed || plan.release_rollback_constraints.automatic_allowed)
        && plan.effects == DeliveryEffects::default()
}

fn rollback_plan_digest(plan: &RollbackPlan) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct DigestInput<'a> {
        protocol: &'a str,
        canary_plan_id: &'a str,
        canary_plan_digest: &'a str,
        canary_decision_id: &'a str,
        environment: &'a str,
        expected_environment_revision: u64,
        failed_release_id: &'a str,
        failed_release_digest: &'a str,
        failed_deployment_plan_id: &'a str,
        failed_deployment_plan_digest: &'a str,
        failed_config_revision_id: &'a str,
        failed_workload_digests: &'a BTreeMap<String, String>,
        failed_gateway_plan_id: &'a str,
        failed_gateway_plan_digest: &'a str,
        previous_release_id: &'a str,
        previous_release_digest: &'a str,
        previous_deployment_plan_id: &'a str,
        previous_deployment_plan_digest: &'a str,
        previous_config_revision_id: &'a str,
        previous_workload_digests: &'a BTreeMap<String, String>,
        previous_gateway_plan_id: &'a str,
        previous_gateway_plan_digest: &'a str,
        previous_gateway_configuration_identity: &'a str,
        previous_gateway_revision: u64,
        release_rollback_constraints: ReleaseRollbackConstraints,
        safety: &'a RollbackSafetyEvidence,
        automatic_allowed: bool,
        issues: &'a [DeliveryIssue],
        prohibited_actions: &'a [String],
    }
    digest_json(&DigestInput {
        protocol: plan.protocol.as_str(),
        canary_plan_id: plan.canary_plan_id.as_str(),
        canary_plan_digest: plan.canary_plan_digest.as_str(),
        canary_decision_id: plan.canary_decision_id.as_str(),
        environment: plan.environment.as_str(),
        expected_environment_revision: plan.expected_environment_revision,
        failed_release_id: plan.failed_release_id.as_str(),
        failed_release_digest: plan.failed_release_digest.as_str(),
        failed_deployment_plan_id: plan.failed_deployment_plan_id.as_str(),
        failed_deployment_plan_digest: plan.failed_deployment_plan_digest.as_str(),
        failed_config_revision_id: plan.failed_config_revision_id.as_str(),
        failed_workload_digests: &plan.failed_workload_digests,
        failed_gateway_plan_id: plan.failed_gateway_plan_id.as_str(),
        failed_gateway_plan_digest: plan.failed_gateway_plan_digest.as_str(),
        previous_release_id: plan.previous_release_id.as_str(),
        previous_release_digest: plan.previous_release_digest.as_str(),
        previous_deployment_plan_id: plan.previous_deployment_plan_id.as_str(),
        previous_deployment_plan_digest: plan.previous_deployment_plan_digest.as_str(),
        previous_config_revision_id: plan.previous_config_revision_id.as_str(),
        previous_workload_digests: &plan.previous_workload_digests,
        previous_gateway_plan_id: plan.previous_gateway_plan_id.as_str(),
        previous_gateway_plan_digest: plan.previous_gateway_plan_digest.as_str(),
        previous_gateway_configuration_identity: plan
            .previous_gateway_configuration_identity
            .as_str(),
        previous_gateway_revision: plan.previous_gateway_revision,
        release_rollback_constraints: plan.release_rollback_constraints,
        safety: &plan.safety,
        automatic_allowed: plan.automatic_allowed,
        issues: plan.issues.as_slice(),
        prohibited_actions: plan.prohibited_actions.as_slice(),
    })
}

fn digest_json(value: &impl Serialize) -> String {
    extraction_input_digest(serde_json::to_vec(value).expect("rollout values must serialize"))
}
