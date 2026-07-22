use std::collections::{BTreeMap, BTreeSet};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signature, Signer as _, SigningKey, Verifier as _, VerifyingKey};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::extraction_input_digest;

use super::{
    ConfigRevision, DeliveryDecision, DeliveryEffects, DeliveryIssue, DeliveryIssueCode,
    DeliveryPolicyInputs, DeploymentApplyRejection, DeploymentObservation, DeploymentPlan,
    DeploymentReceipt, DeploymentState, GatewayConfigurationPlan, GatewayObservation,
    GatewayObservationProvider, PolicyEvidence, ReleaseTrustEvidence, ReleaseTrustProvider,
    SecretProvider, ServiceRelease, apply_deployment, config_revision_integrity_is_valid,
    deployment_observation_integrity_is_valid, deployment_plan_integrity_is_valid,
    deployment_receipt_integrity_is_valid, gateway_observation_integrity_is_valid,
    gateway_plan_authority_is_valid, gateway_plan_integrity_is_valid, issue,
    production_policy_evidence_is_valid, release_trust_evidence_integrity_is_valid,
    service_release_integrity_is_valid,
};

pub const ENVIRONMENT_VERIFICATION_PROTOCOL: &str = "lenso.environment-verification.v1";
pub const PROMOTION_PLAN_PROTOCOL: &str = "lenso.promotion-plan.v1";
pub const PROMOTION_APPROVAL_PROTOCOL: &str = "lenso.promotion-approval.v1";
pub const PROMOTION_RECEIPT_PROTOCOL: &str = "lenso.promotion-receipt.v1";
pub const OPERATOR_OBSERVATION_CLAIMS_PROTOCOL: &str = "lenso.operator-observation-claims.v1";

pub trait OperatorObservationAuthorityProvider: std::fmt::Debug + Send + Sync {
    fn sign(&self, authority_id: &str, observation_digest: &str) -> Option<String>;

    fn verify(&self, authority_id: &str, observation_digest: &str, proof: &str) -> bool;
}

#[derive(Debug, Clone, Default)]
pub struct DeterministicOperatorObservationAuthorityProvider {
    authority_keys: BTreeMap<String, String>,
}

impl DeterministicOperatorObservationAuthorityProvider {
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

    fn expected_proof(&self, authority_id: &str, observation_digest: &str) -> Option<String> {
        let key = self.authority_keys.get(authority_id)?;
        Some(digest_json(&(
            "lenso.operator-observation-authority-proof.v1",
            authority_id,
            observation_digest,
            key.as_str(),
        )))
    }
}

impl OperatorObservationAuthorityProvider for DeterministicOperatorObservationAuthorityProvider {
    fn sign(&self, authority_id: &str, observation_digest: &str) -> Option<String> {
        self.expected_proof(authority_id, observation_digest)
    }

    fn verify(&self, authority_id: &str, observation_digest: &str, proof: &str) -> bool {
        self.expected_proof(authority_id, observation_digest)
            .as_deref()
            == Some(proof)
    }
}

/// Verify-only Operator observation authority backed by Ed25519 public keys.
/// The protected-operation caller never receives signing material.
#[derive(Debug, Clone, Default)]
pub struct Ed25519OperatorObservationAuthorityProvider {
    authority_keys: BTreeMap<String, VerifyingKey>,
    signing_keys: BTreeMap<String, SigningKey>,
}

impl Ed25519OperatorObservationAuthorityProvider {
    pub fn from_base64_public_keys<I, K, V>(authority_keys: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: AsRef<str>,
    {
        let mut parsed = BTreeMap::new();
        for (authority_id, encoded) in authority_keys {
            let bytes = BASE64
                .decode(encoded.as_ref())
                .map_err(|error| format!("invalid Ed25519 public key encoding: {error}"))?;
            let bytes: [u8; 32] = bytes
                .try_into()
                .map_err(|_| "Ed25519 public keys must contain exactly 32 bytes".to_owned())?;
            let key = VerifyingKey::from_bytes(&bytes)
                .map_err(|error| format!("invalid Ed25519 public key: {error}"))?;
            parsed.insert(authority_id.into(), key);
        }
        Ok(Self {
            authority_keys: parsed,
            signing_keys: BTreeMap::new(),
        })
    }

    pub fn from_base64_private_keys<I, K, V>(authority_keys: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: AsRef<str>,
    {
        let mut signing_keys = BTreeMap::new();
        let mut verifying_keys = BTreeMap::new();
        for (authority_id, encoded) in authority_keys {
            let authority_id = authority_id.into();
            let bytes = BASE64
                .decode(encoded.as_ref())
                .map_err(|error| format!("invalid Ed25519 private key encoding: {error}"))?;
            let bytes: [u8; 32] = bytes
                .try_into()
                .map_err(|_| "Ed25519 private keys must contain exactly 32 bytes".to_owned())?;
            let signing_key = SigningKey::from_bytes(&bytes);
            verifying_keys.insert(authority_id.clone(), signing_key.verifying_key());
            signing_keys.insert(authority_id, signing_key);
        }
        Ok(Self {
            authority_keys: verifying_keys,
            signing_keys,
        })
    }
}

impl OperatorObservationAuthorityProvider for Ed25519OperatorObservationAuthorityProvider {
    fn sign(&self, authority_id: &str, observation_digest: &str) -> Option<String> {
        self.signing_keys
            .get(authority_id)
            .map(|key| BASE64.encode(key.sign(observation_digest.as_bytes()).to_bytes()))
    }

    fn verify(&self, authority_id: &str, observation_digest: &str, proof: &str) -> bool {
        let Some(key) = self.authority_keys.get(authority_id) else {
            return false;
        };
        let Ok(bytes) = BASE64.decode(proof) else {
            return false;
        };
        let Ok(signature) = Signature::from_slice(&bytes) else {
            return false;
        };
        key.verify(observation_digest.as_bytes(), &signature)
            .is_ok()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OperatorObservationClaims {
    pub protocol: String,
    pub service_id: String,
    pub environment: String,
    pub deployment_plan_id: String,
    pub deployment_plan_digest: String,
    pub expected_environment_revision: u64,
    pub environment_revision: u64,
    pub authority_context: String,
    pub resource_uid: String,
    pub resource_version: String,
    pub desired_release_id: String,
    pub desired_release_digest: String,
    pub observed_release_id: String,
    pub observed_release_digest: String,
    pub desired_workload_digests: BTreeMap<String, String>,
    pub observed_workload_digests: BTreeMap<String, String>,
    pub workload_health: BTreeMap<String, bool>,
    pub config_revision_id: String,
    pub state: String,
    pub rollout_phase: String,
    pub rollback_state: String,
    pub drifted: bool,
    pub fresh: bool,
    pub decision: DeliveryDecision,
}

#[must_use]
pub fn operator_observation_claims_digest(claims: &OperatorObservationClaims) -> String {
    digest_json(claims)
}

#[must_use]
pub fn operator_observation_claims_from_deployment(
    plan: &DeploymentPlan,
    observation: &DeploymentObservation,
    workload_health: BTreeMap<String, bool>,
) -> OperatorObservationClaims {
    let passed = observation.fresh
        && !observation.drifted
        && workload_health.values().all(|healthy| *healthy);
    OperatorObservationClaims {
        protocol: OPERATOR_OBSERVATION_CLAIMS_PROTOCOL.to_owned(),
        service_id: plan.service_id.clone(),
        environment: observation.environment.clone(),
        deployment_plan_id: plan.plan_id.clone(),
        deployment_plan_digest: plan.plan_digest.clone(),
        expected_environment_revision: plan.expected_environment_revision,
        environment_revision: plan.expected_environment_revision.saturating_add(1),
        authority_context: plan.plan_id.clone(),
        resource_uid: format!("synthetic:{}", plan.plan_id),
        resource_version: "1".to_owned(),
        desired_release_id: observation.desired_release_id.clone(),
        desired_release_digest: plan.release_digest.clone(),
        observed_release_id: observation.observed_release_id.clone(),
        observed_release_digest: observation.observed_release_digest.clone(),
        desired_workload_digests: observation.desired_workload_digests.clone(),
        observed_workload_digests: observation.observed_workload_digests.clone(),
        workload_health,
        config_revision_id: observation.config_revision_id.clone(),
        state: if passed { "ready" } else { "progressing" }.to_owned(),
        rollout_phase: if passed { "ready" } else { "observing" }.to_owned(),
        rollback_state: "available".to_owned(),
        drifted: observation.drifted,
        fresh: observation.fresh,
        decision: if passed {
            DeliveryDecision::Passed
        } else {
            DeliveryDecision::Blocked
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OperatorObservationAttestation {
    pub observation_id: String,
    pub observation_digest: String,
    pub authority_id: String,
    pub authority_proof: String,
    pub claims: OperatorObservationClaims,
}

pub fn attest_operator_observation(
    claims: OperatorObservationClaims,
    authority_id: impl Into<String>,
    provider: &dyn OperatorObservationAuthorityProvider,
) -> Result<OperatorObservationAttestation, DeliveryIssue> {
    let observation_digest = operator_observation_claims_digest(&claims);
    let authority_id = authority_id.into();
    let authority_proof = provider
        .sign(&authority_id, &observation_digest)
        .ok_or_else(|| {
            issue(
                DeliveryIssueCode::ObservationStale,
                "The Operator adapter authority refused to attest the observation.",
                "Use the configured Operator observation authority at the adapter read boundary.",
                "Collect a new Operator observation before verifying the environment.",
            )
        })?;
    Ok(OperatorObservationAttestation {
        observation_id: format!("operator-observation:{observation_digest}"),
        observation_digest,
        authority_id,
        authority_proof,
        claims,
    })
}

#[must_use]
pub fn operator_observation_attestation_is_valid(
    attestation: &OperatorObservationAttestation,
    provider: &dyn OperatorObservationAuthorityProvider,
) -> bool {
    attestation.claims.protocol == OPERATOR_OBSERVATION_CLAIMS_PROTOCOL
        && attestation.observation_digest == operator_observation_claims_digest(&attestation.claims)
        && attestation.observation_id
            == format!("operator-observation:{}", attestation.observation_digest)
        && provider.verify(
            &attestation.authority_id,
            &attestation.observation_digest,
            &attestation.authority_proof,
        )
}

#[must_use]
pub fn operator_observation_matches_deployment(
    attestation: &OperatorObservationAttestation,
    plan: &DeploymentPlan,
    receipt: &DeploymentReceipt,
    observation: &DeploymentObservation,
    workload_health: &BTreeMap<String, bool>,
) -> bool {
    let claims = &attestation.claims;
    claims.protocol == OPERATOR_OBSERVATION_CLAIMS_PROTOCOL
        && observation.source_observation_id == attestation.observation_id
        && claims.service_id == plan.service_id
        && claims.environment == plan.environment
        && claims.deployment_plan_id == plan.plan_id
        && claims.deployment_plan_digest == plan.plan_digest
        && claims.expected_environment_revision == plan.expected_environment_revision
        && claims.environment_revision == receipt.environment_revision_after
        && claims.authority_context == plan.plan_id
        && claims.desired_release_id == observation.desired_release_id
        && claims.desired_release_digest == plan.release_digest
        && claims.observed_release_id == observation.observed_release_id
        && claims.observed_release_digest == observation.observed_release_digest
        && claims.desired_workload_digests == observation.desired_workload_digests
        && claims.observed_workload_digests == observation.observed_workload_digests
        && claims.workload_health == *workload_health
        && claims.config_revision_id == observation.config_revision_id
        && claims.drifted == observation.drifted
        && claims.fresh == observation.fresh
        && claims.state == "ready"
        && claims.rollout_phase == "ready"
        && claims.decision == DeliveryDecision::Passed
        && claims.fresh
        && !claims.drifted
        && claims.workload_health.values().all(|healthy| *healthy)
}

#[derive(Debug, Clone)]
pub struct EnvironmentVerificationInput {
    pub release: ServiceRelease,
    pub trust: ReleaseTrustEvidence,
    pub policy: PolicyEvidence,
    pub policy_inputs: DeliveryPolicyInputs,
    pub config: ConfigRevision,
    pub deployment_plan: DeploymentPlan,
    pub deployment: DeploymentReceipt,
    pub deployment_observation: DeploymentObservation,
    pub operator_observation: OperatorObservationAttestation,
    pub gateway_plan: GatewayConfigurationPlan,
    pub gateway_observation: GatewayObservation,
    pub topology_digest: String,
    pub workload_health: BTreeMap<String, bool>,
    pub evidence_references: Vec<String>,
    pub freshness_horizon_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentVerification {
    pub protocol: String,
    pub verification_id: String,
    pub verification_digest: String,
    pub environment: String,
    pub environment_revision: u64,
    pub release_id: String,
    pub release_digest: String,
    pub workload_digests: BTreeMap<String, String>,
    pub workload_health: BTreeMap<String, bool>,
    pub config_revision_id: String,
    pub trust_evidence_digest: String,
    pub policy_evidence_id: String,
    pub policy_evidence_digest: String,
    pub deployment_plan_id: String,
    pub deployment_plan_digest: String,
    pub deployment_receipt_id: String,
    pub deployment_observation_id: String,
    pub operator_observation_id: String,
    pub operator_observation_digest: String,
    pub operator_observation_authority_id: String,
    pub operator_observation_authority_proof: String,
    pub operator_observation_claims: OperatorObservationClaims,
    pub gateway_plan_id: String,
    pub gateway_plan_digest: String,
    pub gateway_observation_id: String,
    pub gateway_resource_uid: String,
    pub gateway_resource_version: String,
    pub gateway_authority_context: String,
    pub gateway_configuration_identity: String,
    pub gateway_observation_revision: u64,
    pub gateway_observation_observed_after: String,
    pub gateway_observation_fresh: bool,
    pub gateway_observation_provider_id: String,
    pub gateway_observation_provider_proof: String,
    pub topology_digest: String,
    pub evidence_references: Vec<String>,
    pub freshness_horizon_revision: u64,
    pub decision: DeliveryDecision,
    pub issues: Vec<DeliveryIssue>,
    pub effects: DeliveryEffects,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvironmentVerificationDigestInput<'a> {
    protocol: &'a str,
    environment: &'a str,
    environment_revision: u64,
    release_id: &'a str,
    release_digest: &'a str,
    workload_digests: &'a BTreeMap<String, String>,
    workload_health: &'a BTreeMap<String, bool>,
    config_revision_id: &'a str,
    trust_evidence_digest: &'a str,
    policy_evidence_id: &'a str,
    policy_evidence_digest: &'a str,
    deployment_plan_id: &'a str,
    deployment_plan_digest: &'a str,
    deployment_receipt_id: &'a str,
    deployment_observation_id: &'a str,
    operator_observation_id: &'a str,
    operator_observation_digest: &'a str,
    operator_observation_authority_id: &'a str,
    operator_observation_authority_proof: &'a str,
    operator_observation_claims: &'a OperatorObservationClaims,
    gateway_plan_id: &'a str,
    gateway_plan_digest: &'a str,
    gateway_observation_id: &'a str,
    gateway_resource_uid: &'a str,
    gateway_resource_version: &'a str,
    gateway_authority_context: &'a str,
    gateway_configuration_identity: &'a str,
    gateway_observation_revision: u64,
    gateway_observation_observed_after: &'a str,
    gateway_observation_fresh: bool,
    gateway_observation_provider_id: &'a str,
    gateway_observation_provider_proof: &'a str,
    topology_digest: &'a str,
    evidence_references: &'a [String],
    freshness_horizon_revision: u64,
    decision: DeliveryDecision,
    issues: &'a [DeliveryIssue],
}

#[must_use]
pub fn verify_staging_environment(
    mut input: EnvironmentVerificationInput,
    trust_provider: &dyn ReleaseTrustProvider,
    secret_provider: &dyn SecretProvider,
    operator_observation_provider: &dyn OperatorObservationAuthorityProvider,
    gateway_observation_provider: &dyn GatewayObservationProvider,
) -> EnvironmentVerification {
    input.evidence_references.sort();
    let mut issues = Vec::new();
    if !service_release_integrity_is_valid(&input.release)
        || !release_trust_evidence_integrity_is_valid(&input.trust, &input.release, trust_provider)
        || input.trust.decision != DeliveryDecision::Passed
    {
        issues.push(issue(
            DeliveryIssueCode::PolicyEvidenceMissing,
            "Staging verification requires an integrity-valid and trusted Service Release.",
            "Verify the exact signed release before deployment.",
            "Refresh release trust evidence and verify staging again.",
        ));
    }
    if input.policy_inputs.release != input.release
        || input.policy_inputs.trust != input.trust
        || input.policy_inputs.config != input.config
        || !production_policy_evidence_is_valid(
            &input.policy,
            &input.policy_inputs,
            trust_provider,
            secret_provider,
        )
        || input.policy.decision != DeliveryDecision::Passed
        || input.policy.evaluated_subject != input.release.release_id
        || !deployment_binds_policy_evidence(&input.deployment_plan, &input.policy)
    {
        issues.push(issue(
            DeliveryIssueCode::PolicyRuleBlocked,
            "Staging verification requires passing Policy Evidence for the exact release.",
            "Evaluate the selected Policy Pack over current canonical inputs.",
            "Refresh Policy Evidence and verify staging again.",
        ));
    }
    if !config_revision_integrity_is_valid(&input.config)
        || input.config.contract_digest != input.release.config_contract.digest
        || input.config.service_id != input.release.service_id
        || input.deployment.config_revision_id != input.config.revision_id
    {
        issues.push(issue(
            DeliveryIssueCode::ConfigContractMismatch,
            "The observed Deployment does not use the validated Config Revision.",
            "Deploy the exact validated Config Revision and opaque Secret References.",
            "Correct configuration drift and verify staging again.",
        ));
    }
    if !deployment_plan_integrity_is_valid(&input.deployment_plan)
        || !deployment_receipt_integrity_is_valid(&input.deployment, &input.deployment_plan)
        || !deployment_observation_integrity_is_valid(
            &input.deployment_observation,
            &input.deployment_plan,
            &input.deployment,
        )
        || input.deployment_plan.release_id != input.release.release_id
        || input.deployment_plan.release_digest != input.release.release_digest
        || input.deployment_plan.config_revision_id != input.config.revision_id
        || input.deployment_plan.gateway_plan_digest != input.gateway_plan.plan_digest
        || input.deployment_observation.observed_release_digest != input.release.release_digest
        || input.deployment_observation.drifted
        || !input.deployment_observation.fresh
        || !operator_observation_attestation_is_valid(
            &input.operator_observation,
            operator_observation_provider,
        )
        || !operator_observation_matches_deployment(
            &input.operator_observation,
            &input.deployment_plan,
            &input.deployment,
            &input.deployment_observation,
            &input.workload_health,
        )
        || !operator_observation_evidence_is_bound(
            &input.operator_observation,
            &input.evidence_references,
        )
    {
        issues.push(issue(
            DeliveryIssueCode::ObservationStale,
            "Staging Deployment observations are stale, drifted, or identify another release.",
            "Refresh adapter observations after the exact release converges.",
            "Reconcile staging and collect fresh Deployment evidence.",
        ));
    }
    if !gateway_plan_authority_is_valid(&input.gateway_plan, trust_provider)
        || input.gateway_plan.edge_release_id != input.release.release_id
        || input.gateway_plan.edge_release_digest != input.release.release_digest
        || !gateway_observation_integrity_is_valid(
            &input.gateway_observation,
            gateway_observation_provider,
        )
        || input.gateway_observation.plan_id != input.gateway_plan.plan_id
        || input.gateway_observation.plan_digest != input.gateway_plan.plan_digest
        || input.gateway_observation.environment != input.deployment.environment
        || input.gateway_observation.release_id != input.release.release_id
        || input.gateway_observation.release_digest != input.release.release_digest
        || input.gateway_observation.resource_uid.trim().is_empty()
        || input.gateway_observation.resource_version.trim().is_empty()
        || input.gateway_observation.authority_context != input.gateway_plan.plan_id
        || input.gateway_observation.configuration_identity
            != input.gateway_plan.configuration_identity
        || input.gateway_observation.revision != input.gateway_plan.expected_gateway_revision
        || input.gateway_observation.observed_after
            != input.deployment_observation.source_observation_id
        || !input.gateway_observation.fresh
    {
        issues.push(issue(
            DeliveryIssueCode::EdgeExposureUnsafe,
            "Gateway observations do not match the planned Edge configuration.",
            "Reconcile the exact Edge Contract and collect a fresh gateway observation.",
            "Correct gateway drift and verify staging again.",
        ));
    }
    let release_workloads = input
        .release
        .workloads
        .iter()
        .map(|workload| workload.workload_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if release_workloads.iter().any(|workload_id| {
        !input
            .workload_health
            .get(*workload_id)
            .copied()
            .unwrap_or(false)
    }) {
        issues.push(issue(
            DeliveryIssueCode::ObservationStale,
            "One or more release Workloads lack fresh healthy staging observations.",
            "Wait for Migration, API, and Worker convergence and collect Service-level health evidence.",
            "Refresh Workload observations before Promotion.",
        ));
    }
    if input.freshness_horizon_revision <= input.deployment.environment_revision_after {
        issues.push(issue(
            DeliveryIssueCode::ObservationStale,
            "Environment Verification freshness horizon is already exhausted.",
            "Choose a horizon beyond the observed environment revision.",
            "Collect fresh staging evidence.",
        ));
    }
    let workload_digests = input.deployment.workload_digests.clone();
    let decision = if issues.is_empty() {
        DeliveryDecision::Passed
    } else {
        DeliveryDecision::Blocked
    };
    let verification_digest = digest_json(&EnvironmentVerificationDigestInput {
        protocol: ENVIRONMENT_VERIFICATION_PROTOCOL,
        environment: input.deployment.environment.as_str(),
        environment_revision: input.deployment.environment_revision_after,
        release_id: input.release.release_id.as_str(),
        release_digest: input.release.release_digest.as_str(),
        workload_digests: &workload_digests,
        workload_health: &input.workload_health,
        config_revision_id: input.config.revision_id.as_str(),
        trust_evidence_digest: input.trust.evidence_digest.as_str(),
        policy_evidence_id: input.policy.evidence_id.as_str(),
        policy_evidence_digest: input.policy.evidence_digest.as_str(),
        deployment_plan_id: input.deployment_plan.plan_id.as_str(),
        deployment_plan_digest: input.deployment_plan.plan_digest.as_str(),
        deployment_receipt_id: input.deployment.receipt_id.as_str(),
        deployment_observation_id: input.deployment_observation.observation_id.as_str(),
        operator_observation_id: input.operator_observation.observation_id.as_str(),
        operator_observation_digest: input.operator_observation.observation_digest.as_str(),
        operator_observation_authority_id: input.operator_observation.authority_id.as_str(),
        operator_observation_authority_proof: input.operator_observation.authority_proof.as_str(),
        operator_observation_claims: &input.operator_observation.claims,
        gateway_plan_id: input.gateway_plan.plan_id.as_str(),
        gateway_plan_digest: input.gateway_plan.plan_digest.as_str(),
        gateway_observation_id: input.gateway_observation.observation_id.as_str(),
        gateway_resource_uid: input.gateway_observation.resource_uid.as_str(),
        gateway_resource_version: input.gateway_observation.resource_version.as_str(),
        gateway_authority_context: input.gateway_observation.authority_context.as_str(),
        gateway_configuration_identity: input.gateway_observation.configuration_identity.as_str(),
        gateway_observation_revision: input.gateway_observation.revision,
        gateway_observation_observed_after: input.gateway_observation.observed_after.as_str(),
        gateway_observation_fresh: input.gateway_observation.fresh,
        gateway_observation_provider_id: input.gateway_observation.provider_id.as_str(),
        gateway_observation_provider_proof: input.gateway_observation.provider_proof.as_str(),
        topology_digest: input.topology_digest.as_str(),
        evidence_references: input.evidence_references.as_slice(),
        freshness_horizon_revision: input.freshness_horizon_revision,
        decision,
        issues: issues.as_slice(),
    });
    EnvironmentVerification {
        protocol: ENVIRONMENT_VERIFICATION_PROTOCOL.to_owned(),
        verification_id: format!("environment-verification:{verification_digest}"),
        verification_digest,
        environment: input.deployment.environment,
        environment_revision: input.deployment.environment_revision_after,
        release_id: input.release.release_id,
        release_digest: input.release.release_digest,
        workload_digests,
        workload_health: input.workload_health,
        config_revision_id: input.config.revision_id,
        trust_evidence_digest: input.trust.evidence_digest,
        policy_evidence_id: input.policy.evidence_id,
        policy_evidence_digest: input.policy.evidence_digest,
        deployment_plan_id: input.deployment_plan.plan_id,
        deployment_plan_digest: input.deployment_plan.plan_digest,
        deployment_receipt_id: input.deployment.receipt_id,
        deployment_observation_id: input.deployment_observation.observation_id,
        operator_observation_id: input.operator_observation.observation_id,
        operator_observation_digest: input.operator_observation.observation_digest,
        operator_observation_authority_id: input.operator_observation.authority_id,
        operator_observation_authority_proof: input.operator_observation.authority_proof,
        operator_observation_claims: input.operator_observation.claims,
        gateway_plan_id: input.gateway_plan.plan_id,
        gateway_plan_digest: input.gateway_plan.plan_digest,
        gateway_observation_id: input.gateway_observation.observation_id,
        gateway_resource_uid: input.gateway_observation.resource_uid,
        gateway_resource_version: input.gateway_observation.resource_version,
        gateway_authority_context: input.gateway_observation.authority_context,
        gateway_configuration_identity: input.gateway_observation.configuration_identity,
        gateway_observation_revision: input.gateway_observation.revision,
        gateway_observation_observed_after: input.gateway_observation.observed_after,
        gateway_observation_fresh: input.gateway_observation.fresh,
        gateway_observation_provider_id: input.gateway_observation.provider_id,
        gateway_observation_provider_proof: input.gateway_observation.provider_proof,
        topology_digest: input.topology_digest,
        evidence_references: input.evidence_references,
        freshness_horizon_revision: input.freshness_horizon_revision,
        decision,
        issues,
        effects: DeliveryEffects::default(),
    }
}

#[must_use]
pub fn environment_verification_digest(verification: &EnvironmentVerification) -> String {
    digest_json(&EnvironmentVerificationDigestInput {
        protocol: verification.protocol.as_str(),
        environment: verification.environment.as_str(),
        environment_revision: verification.environment_revision,
        release_id: verification.release_id.as_str(),
        release_digest: verification.release_digest.as_str(),
        workload_digests: &verification.workload_digests,
        workload_health: &verification.workload_health,
        config_revision_id: verification.config_revision_id.as_str(),
        trust_evidence_digest: verification.trust_evidence_digest.as_str(),
        policy_evidence_id: verification.policy_evidence_id.as_str(),
        policy_evidence_digest: verification.policy_evidence_digest.as_str(),
        deployment_plan_id: verification.deployment_plan_id.as_str(),
        deployment_plan_digest: verification.deployment_plan_digest.as_str(),
        deployment_receipt_id: verification.deployment_receipt_id.as_str(),
        deployment_observation_id: verification.deployment_observation_id.as_str(),
        operator_observation_id: verification.operator_observation_id.as_str(),
        operator_observation_digest: verification.operator_observation_digest.as_str(),
        operator_observation_authority_id: verification.operator_observation_authority_id.as_str(),
        operator_observation_authority_proof: verification
            .operator_observation_authority_proof
            .as_str(),
        operator_observation_claims: &verification.operator_observation_claims,
        gateway_plan_id: verification.gateway_plan_id.as_str(),
        gateway_plan_digest: verification.gateway_plan_digest.as_str(),
        gateway_observation_id: verification.gateway_observation_id.as_str(),
        gateway_resource_uid: verification.gateway_resource_uid.as_str(),
        gateway_resource_version: verification.gateway_resource_version.as_str(),
        gateway_authority_context: verification.gateway_authority_context.as_str(),
        gateway_configuration_identity: verification.gateway_configuration_identity.as_str(),
        gateway_observation_revision: verification.gateway_observation_revision,
        gateway_observation_observed_after: verification
            .gateway_observation_observed_after
            .as_str(),
        gateway_observation_fresh: verification.gateway_observation_fresh,
        gateway_observation_provider_id: verification.gateway_observation_provider_id.as_str(),
        gateway_observation_provider_proof: verification
            .gateway_observation_provider_proof
            .as_str(),
        topology_digest: verification.topology_digest.as_str(),
        evidence_references: verification.evidence_references.as_slice(),
        freshness_horizon_revision: verification.freshness_horizon_revision,
        decision: verification.decision,
        issues: verification.issues.as_slice(),
    })
}

#[must_use]
pub fn environment_verification_integrity_is_valid(verification: &EnvironmentVerification) -> bool {
    verification.protocol == ENVIRONMENT_VERIFICATION_PROTOCOL
        && verification.verification_id
            == format!(
                "environment-verification:{}",
                verification.verification_digest
            )
        && environment_verification_digest(verification) == verification.verification_digest
        && verification.decision
            == if verification.issues.is_empty() {
                DeliveryDecision::Passed
            } else {
                DeliveryDecision::Blocked
            }
        && verification.effects == DeliveryEffects::default()
}

fn operator_observation_evidence_is_bound(
    observation: &OperatorObservationAttestation,
    evidence_references: &[String],
) -> bool {
    let expected = [
        observation.observation_id.clone(),
        observation.observation_digest.clone(),
        format!(
            "operator-observation-authority:{}",
            observation.authority_id
        ),
        format!("operator-observation-proof:{}", observation.authority_proof),
    ];
    expected
        .iter()
        .all(|reference| evidence_references.contains(reference))
}

#[must_use]
pub fn environment_verification_authority_is_valid(
    verification: &EnvironmentVerification,
    operator_provider: &dyn OperatorObservationAuthorityProvider,
    gateway_provider: &dyn GatewayObservationProvider,
) -> bool {
    let operator_observation = OperatorObservationAttestation {
        observation_id: verification.operator_observation_id.clone(),
        observation_digest: verification.operator_observation_digest.clone(),
        authority_id: verification.operator_observation_authority_id.clone(),
        authority_proof: verification.operator_observation_authority_proof.clone(),
        claims: verification.operator_observation_claims.clone(),
    };
    let gateway_observation = GatewayObservation {
        protocol: super::GATEWAY_OBSERVATION_PROTOCOL.to_owned(),
        observation_id: verification.gateway_observation_id.clone(),
        plan_id: verification.gateway_plan_id.clone(),
        plan_digest: verification.gateway_plan_digest.clone(),
        environment: verification.environment.clone(),
        release_id: verification.release_id.clone(),
        release_digest: verification.release_digest.clone(),
        resource_uid: verification.gateway_resource_uid.clone(),
        resource_version: verification.gateway_resource_version.clone(),
        authority_context: verification.gateway_authority_context.clone(),
        configuration_identity: verification.gateway_configuration_identity.clone(),
        revision: verification.gateway_observation_revision,
        observed_after: verification.gateway_observation_observed_after.clone(),
        fresh: verification.gateway_observation_fresh,
        provider_id: verification.gateway_observation_provider_id.clone(),
        provider_proof: verification.gateway_observation_provider_proof.clone(),
    };
    environment_verification_integrity_is_valid(verification)
        && operator_observation_attestation_is_valid(&operator_observation, operator_provider)
        && verification.operator_observation_claims.environment == verification.environment
        && verification
            .operator_observation_claims
            .environment_revision
            == verification.environment_revision
        && verification.operator_observation_claims.authority_context
            == verification.deployment_plan_id
        && verification.operator_observation_claims.deployment_plan_id
            == verification.deployment_plan_id
        && verification
            .operator_observation_claims
            .deployment_plan_digest
            == verification.deployment_plan_digest
        && verification.operator_observation_claims.desired_release_id == verification.release_id
        && verification
            .operator_observation_claims
            .desired_release_digest
            == verification.release_digest
        && verification.operator_observation_claims.observed_release_id == verification.release_id
        && verification
            .operator_observation_claims
            .observed_release_digest
            == verification.release_digest
        && verification
            .operator_observation_claims
            .desired_workload_digests
            == verification.workload_digests
        && verification
            .operator_observation_claims
            .observed_workload_digests
            == verification.workload_digests
        && verification.operator_observation_claims.workload_health == verification.workload_health
        && verification.operator_observation_claims.config_revision_id
            == verification.config_revision_id
        && verification.operator_observation_claims.state == "ready"
        && verification.operator_observation_claims.rollout_phase == "ready"
        && verification.operator_observation_claims.decision == DeliveryDecision::Passed
        && verification.operator_observation_claims.fresh
        && !verification.operator_observation_claims.drifted
        && operator_observation_evidence_is_bound(
            &operator_observation,
            &verification.evidence_references,
        )
        && gateway_observation_integrity_is_valid(&gateway_observation, gateway_provider)
        && gateway_observation.plan_id == verification.gateway_plan_id
        && gateway_observation.plan_digest == verification.gateway_plan_digest
        && gateway_observation.environment == verification.environment
        && gateway_observation.release_id == verification.release_id
        && gateway_observation.release_digest == verification.release_digest
        && !gateway_observation.resource_uid.trim().is_empty()
        && !gateway_observation.resource_version.trim().is_empty()
        && gateway_observation.authority_context == verification.gateway_plan_id
        && gateway_observation.observed_after == verification.operator_observation_id
        && gateway_observation.revision == verification.gateway_observation_revision
        && gateway_observation.fresh
}

#[derive(Debug, Clone)]
pub struct PromotionPlanInput {
    pub source: EnvironmentVerification,
    pub target_deployment: DeploymentPlan,
    pub target_gateway: GatewayConfigurationPlan,
    pub policy: PolicyEvidence,
    pub policy_inputs: DeliveryPolicyInputs,
    pub source_environment_revision: u64,
    pub target_environment_revision: u64,
    pub target_topology_digest: String,
    pub secret_reference_ids: Vec<String>,
    pub evidence_references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromotionPlan {
    pub protocol: String,
    pub plan_id: String,
    pub plan_digest: String,
    pub release_id: String,
    pub release_digest: String,
    pub workload_digests: BTreeMap<String, String>,
    pub source_environment: String,
    pub target_environment: String,
    pub source_environment_revision: u64,
    pub target_environment_revision: u64,
    pub source_verification_id: String,
    pub source_verification_digest: String,
    pub policy_evidence_id: String,
    pub policy_evidence_digest: String,
    pub config_revision_id: String,
    pub secret_reference_ids: Vec<String>,
    pub target_deployment: DeploymentPlan,
    pub target_gateway: GatewayConfigurationPlan,
    pub source_topology_digest: String,
    pub target_topology_digest: String,
    pub evidence_references: Vec<String>,
    pub freshness_horizon_revision: u64,
    pub effects: DeliveryEffects,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PromotionPlanDigestInput<'a> {
    protocol: &'a str,
    release_id: &'a str,
    release_digest: &'a str,
    workload_digests: &'a BTreeMap<String, String>,
    source_environment: &'a str,
    target_environment: &'a str,
    source_environment_revision: u64,
    target_environment_revision: u64,
    source_verification_id: &'a str,
    source_verification_digest: &'a str,
    policy_evidence_id: &'a str,
    policy_evidence_digest: &'a str,
    config_revision_id: &'a str,
    secret_reference_ids: &'a [String],
    target_deployment_digest: &'a str,
    target_gateway_digest: &'a str,
    source_topology_digest: &'a str,
    target_topology_digest: &'a str,
    evidence_references: &'a [String],
    freshness_horizon_revision: u64,
    effects: &'a DeliveryEffects,
}

pub fn plan_promotion(
    mut input: PromotionPlanInput,
    trust_provider: &dyn ReleaseTrustProvider,
    secret_provider: &dyn SecretProvider,
    operator_observation_provider: &dyn OperatorObservationAuthorityProvider,
    gateway_observation_provider: &dyn GatewayObservationProvider,
) -> Result<PromotionPlan, Vec<DeliveryIssue>> {
    input.secret_reference_ids.sort();
    input.evidence_references.sort();
    let mut issues = Vec::new();
    if !environment_verification_authority_is_valid(
        &input.source,
        operator_observation_provider,
        gateway_observation_provider,
    ) || input.source.decision != DeliveryDecision::Passed
        || input.source_environment_revision != input.source.environment_revision
    {
        issues.push(issue(
            DeliveryIssueCode::ObservationStale,
            "Promotion requires passing source Environment Verification.",
            "Deploy and verify the exact release in the source environment.",
            "Refresh source verification and plan Promotion again.",
        ));
    }
    if !deployment_plan_integrity_is_valid(&input.target_deployment)
        || !gateway_plan_authority_is_valid(&input.target_gateway, trust_provider)
        || input.target_gateway.edge_release_id != input.source.release_id
        || input.target_gateway.edge_release_digest != input.source.release_digest
        || input.target_deployment.gateway_plan_digest != input.target_gateway.plan_digest
        || input.target_deployment.release_id != input.source.release_id
        || input.target_deployment.release_digest != input.source.release_digest
        || input.target_deployment.workloads.iter().any(|workload| {
            input.source.workload_digests.get(&workload.workload_id)
                != Some(&workload.artifact_digest)
        })
    {
        issues.push(issue(
            DeliveryIssueCode::ReleaseTampered,
            "Promotion target attempts to rebuild, substitute, or change release Workload digests.",
            "Use the exact source-verified Service Release and immutable Workload digests.",
            "Regenerate the target Deployment plan without rebuilding.",
        ));
    }
    if input.policy_inputs.release.release_id != input.source.release_id
        || input.policy_inputs.release.release_digest != input.source.release_digest
        || !production_policy_evidence_is_valid(
            &input.policy,
            &input.policy_inputs,
            trust_provider,
            secret_provider,
        )
        || input.policy.decision != DeliveryDecision::Passed
        || input.policy.evaluated_subject != input.source.release_id
        || !deployment_binds_policy_evidence(&input.target_deployment, &input.policy)
    {
        issues.push(issue(
            DeliveryIssueCode::PolicyRuleBlocked,
            "Promotion Policy Evidence is blocked, stale, or evaluates another release.",
            "Evaluate the exact release and target evidence through the production Policy Pack.",
            "Refresh Policy Evidence and plan Promotion again.",
        ));
    }
    let mut target_secret_references = input.target_deployment.secret_reference_ids.clone();
    target_secret_references.sort();
    if input.target_deployment.config_revision_id != input.source.config_revision_id
        || target_secret_references != input.secret_reference_ids
    {
        issues.push(issue(
            DeliveryIssueCode::ConfigContractMismatch,
            "Promotion configuration or Secret References differ from the reviewed source binding.",
            "Bind the exact verified Config Revision and opaque Secret Reference identifiers.",
            "Regenerate the target Deployment and Promotion plan.",
        ));
    }
    if input.source.environment == input.target_deployment.environment
        || input.target_deployment.environment != input.target_gateway.environment
        || input.target_deployment.expected_environment_revision
            != input.target_environment_revision
        || input.source.freshness_horizon_revision < input.source_environment_revision
    {
        issues.push(issue(
            DeliveryIssueCode::StaleInput,
            "Promotion environment, topology, or freshness inputs are inconsistent.",
            "Refresh source and target revisions and bind both adapters to the intended environment.",
            "Correct stale inputs and plan Promotion again.",
        ));
    }
    if !issues.is_empty() {
        return Err(issues);
    }
    let effects = DeliveryEffects::default();
    let plan_digest = digest_json(&PromotionPlanDigestInput {
        protocol: PROMOTION_PLAN_PROTOCOL,
        release_id: input.source.release_id.as_str(),
        release_digest: input.source.release_digest.as_str(),
        workload_digests: &input.source.workload_digests,
        source_environment: input.source.environment.as_str(),
        target_environment: input.target_deployment.environment.as_str(),
        source_environment_revision: input.source_environment_revision,
        target_environment_revision: input.target_environment_revision,
        source_verification_id: input.source.verification_id.as_str(),
        source_verification_digest: input.source.verification_digest.as_str(),
        policy_evidence_id: input.policy.evidence_id.as_str(),
        policy_evidence_digest: input.policy.evidence_digest.as_str(),
        config_revision_id: input.source.config_revision_id.as_str(),
        secret_reference_ids: input.secret_reference_ids.as_slice(),
        target_deployment_digest: input.target_deployment.plan_digest.as_str(),
        target_gateway_digest: input.target_gateway.plan_digest.as_str(),
        source_topology_digest: input.source.topology_digest.as_str(),
        target_topology_digest: input.target_topology_digest.as_str(),
        evidence_references: input.evidence_references.as_slice(),
        freshness_horizon_revision: input.source.freshness_horizon_revision,
        effects: &effects,
    });
    Ok(PromotionPlan {
        protocol: PROMOTION_PLAN_PROTOCOL.to_owned(),
        plan_id: format!("promotion-plan:{plan_digest}"),
        plan_digest,
        release_id: input.source.release_id,
        release_digest: input.source.release_digest,
        workload_digests: input.source.workload_digests,
        source_environment: input.source.environment,
        target_environment: input.target_deployment.environment.clone(),
        source_environment_revision: input.source_environment_revision,
        target_environment_revision: input.target_environment_revision,
        source_verification_id: input.source.verification_id,
        source_verification_digest: input.source.verification_digest,
        policy_evidence_id: input.policy.evidence_id,
        policy_evidence_digest: input.policy.evidence_digest,
        config_revision_id: input.source.config_revision_id,
        secret_reference_ids: input.secret_reference_ids,
        target_deployment: input.target_deployment,
        target_gateway: input.target_gateway,
        source_topology_digest: input.source.topology_digest,
        target_topology_digest: input.target_topology_digest,
        evidence_references: input.evidence_references,
        freshness_horizon_revision: input.source.freshness_horizon_revision,
        effects,
    })
}

fn deployment_binds_policy_evidence(plan: &DeploymentPlan, policy: &PolicyEvidence) -> bool {
    let expected = [policy.evidence_id.as_str(), policy.evidence_digest.as_str()]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    let actual = plan
        .policy_evidence_references
        .iter()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    actual == expected && plan.policy_evidence_references.len() == expected.len()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromotionApproval {
    pub protocol: String,
    pub approval_id: String,
    pub plan_digest: String,
    pub actor: String,
    pub authority: String,
    pub authority_proof: String,
    pub issued_for_target_revision: u64,
    pub approved: bool,
}

pub trait PromotionApprovalAuthority {
    fn authority(&self) -> &str;
    fn issue_proof(&self, actor: &str, plan_digest: &str, target_revision: u64) -> Option<String>;
    fn verify_proof(
        &self,
        actor: &str,
        plan_digest: &str,
        target_revision: u64,
        proof: &str,
    ) -> bool;
}

#[derive(Debug, Clone)]
pub struct DeterministicPromotionApprovalAuthority {
    authority: String,
    authorized_actors: BTreeSet<String>,
    signing_key: String,
}

impl DeterministicPromotionApprovalAuthority {
    pub fn new(
        authority: impl Into<String>,
        authorized_actors: impl IntoIterator<Item = impl Into<String>>,
        signing_key: impl Into<String>,
    ) -> Self {
        Self {
            authority: authority.into(),
            authorized_actors: authorized_actors.into_iter().map(Into::into).collect(),
            signing_key: signing_key.into(),
        }
    }

    fn proof(&self, actor: &str, plan_digest: &str, target_revision: u64) -> String {
        digest_json(&(
            "lenso.promotion-authority-proof.v1",
            self.authority.as_str(),
            actor,
            plan_digest,
            target_revision,
            self.signing_key.as_str(),
        ))
    }
}

impl PromotionApprovalAuthority for DeterministicPromotionApprovalAuthority {
    fn authority(&self) -> &str {
        &self.authority
    }

    fn issue_proof(&self, actor: &str, plan_digest: &str, target_revision: u64) -> Option<String> {
        self.authorized_actors
            .contains(actor)
            .then(|| self.proof(actor, plan_digest, target_revision))
    }

    fn verify_proof(
        &self,
        actor: &str,
        plan_digest: &str,
        target_revision: u64,
        proof: &str,
    ) -> bool {
        self.authorized_actors.contains(actor)
            && proof == self.proof(actor, plan_digest, target_revision)
    }
}

pub fn approve_promotion(
    plan: &PromotionPlan,
    actor: impl Into<String>,
    authority: &impl PromotionApprovalAuthority,
) -> Result<PromotionApproval, DeliveryIssue> {
    let actor = actor.into();
    let authority_name = authority.authority().to_owned();
    let authority_proof = authority.issue_proof(
        actor.as_str(),
        plan.plan_digest.as_str(),
        plan.target_environment_revision,
    );
    if !promotion_plan_integrity_is_valid(plan)
        || !actor.starts_with("user:")
        || authority_name.trim().is_empty()
        || authority_proof.is_none()
    {
        return Err(issue(
            DeliveryIssueCode::ApprovalInvalid,
            "Production Promotion approval requires an integrity-valid plan and explicit human actor authority.",
            "Bind a reviewed plan digest to an authorized human operator.",
            "Correct the Approval Boundary before apply.",
        ));
    }
    let approval_id = format!(
        "promotion-approval:{}",
        digest_json(&(
            PROMOTION_APPROVAL_PROTOCOL,
            plan.plan_digest.as_str(),
            actor.as_str(),
            authority_name.as_str(),
            authority_proof.as_deref(),
            plan.target_environment_revision,
            true,
        ))
    );
    Ok(PromotionApproval {
        protocol: PROMOTION_APPROVAL_PROTOCOL.to_owned(),
        approval_id,
        plan_digest: plan.plan_digest.clone(),
        actor,
        authority: authority_name,
        authority_proof: authority_proof.expect("authority proof was checked"),
        issued_for_target_revision: plan.target_environment_revision,
        approved: true,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromotionProtectedEvidence {
    pub source_verification_id: String,
    pub source_verification_digest: String,
    pub policy_evidence_id: String,
    pub policy_evidence_digest: String,
    pub source_environment_revision: u64,
    pub source_topology_digest: String,
    pub target_topology_digest: String,
    pub config_revision_id: String,
    pub secret_reference_ids: Vec<String>,
    pub evidence_references: Vec<String>,
}

impl PromotionProtectedEvidence {
    #[must_use]
    pub fn from_plan(plan: &PromotionPlan) -> Self {
        Self {
            source_verification_id: plan.source_verification_id.clone(),
            source_verification_digest: plan.source_verification_digest.clone(),
            policy_evidence_id: plan.policy_evidence_id.clone(),
            policy_evidence_digest: plan.policy_evidence_digest.clone(),
            source_environment_revision: plan.source_environment_revision,
            source_topology_digest: plan.source_topology_digest.clone(),
            target_topology_digest: plan.target_topology_digest.clone(),
            config_revision_id: plan.config_revision_id.clone(),
            secret_reference_ids: plan.secret_reference_ids.clone(),
            evidence_references: plan.evidence_references.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromotionReceipt {
    pub protocol: String,
    pub receipt_id: String,
    pub plan_id: String,
    pub approval_id: String,
    pub actor: String,
    pub source_environment: String,
    pub target_environment: String,
    pub release_id: String,
    pub release_digest: String,
    pub workload_digests: BTreeMap<String, String>,
    pub deployment_receipt: DeploymentReceipt,
    pub environment_revision_before: u64,
    pub environment_revision_after: u64,
    pub effects: DeliveryEffects,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromotionState {
    pub environment: String,
    pub environment_revision: u64,
    pub coordination_available: bool,
    #[serde(default)]
    pub history: Vec<PromotionReceipt>,
}

impl PromotionState {
    #[must_use]
    pub fn new(environment: impl Into<String>, environment_revision: u64) -> Self {
        Self {
            environment: environment.into(),
            environment_revision,
            coordination_available: true,
            history: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromotionApplyRejection {
    pub issues: Vec<DeliveryIssue>,
    pub effects: DeliveryEffects,
}

pub fn apply_promotion(
    state: &mut PromotionState,
    deployment_state: &mut DeploymentState,
    plan: &PromotionPlan,
    approval: &PromotionApproval,
    protected_evidence: &PromotionProtectedEvidence,
    approval_authority: &impl PromotionApprovalAuthority,
) -> Result<PromotionReceipt, PromotionApplyRejection> {
    let valid_approval = promotion_approval_integrity_is_valid(approval, approval_authority)
        && approval.approved
        && approval.plan_digest == plan.plan_digest
        && approval.issued_for_target_revision == plan.target_environment_revision;
    if !promotion_plan_integrity_is_valid(plan) || !valid_approval {
        return Err(rejection(
            DeliveryIssueCode::ApprovalInvalid,
            "Promotion plan integrity or its exact human Approval Boundary is invalid.",
            "Review and approve the current plan digest with explicit production authority.",
            "Regenerate or reapprove the Promotion plan.",
        ));
    }
    let mut expected_evidence = PromotionProtectedEvidence::from_plan(plan);
    expected_evidence.secret_reference_ids.sort();
    expected_evidence.evidence_references.sort();
    let mut current_evidence = protected_evidence.clone();
    current_evidence.secret_reference_ids.sort();
    current_evidence.evidence_references.sort();
    if current_evidence != expected_evidence
        || current_evidence.source_environment_revision > plan.freshness_horizon_revision
    {
        return Err(rejection(
            DeliveryIssueCode::StaleInput,
            "Source verification, policy, configuration, topology, reliability, edge, or evidence changed after Promotion planning.",
            "Collect current protected evidence and bind a new content-addressed Promotion plan.",
            "Refresh staging and replan Promotion before mutation.",
        ));
    }
    if let Some(existing) = state
        .history
        .iter()
        .find(|item| item.plan_id == plan.plan_id)
    {
        return promotion_receipt_integrity_is_valid(existing, plan, approval)
            .then(|| existing.clone())
            .ok_or_else(|| {
                rejection(
                    DeliveryIssueCode::StaleInput,
                    "The completed Promotion receipt no longer matches the exact plan and Approval Boundary.",
                    "Preserve the immutable plan, approval, protected evidence, and receipt together.",
                    "Restore the original receipt or create and approve a new Promotion plan.",
                )
            });
    }
    if !state.coordination_available {
        return Err(rejection(
            DeliveryIssueCode::CoordinationUnavailable,
            "New Promotion is paused while coordination evidence is unavailable.",
            "Restore the System Plane and refresh protected-action evidence.",
            "Retry Promotion after coordination recovers.",
        ));
    }
    if state.environment != plan.target_environment
        || state.environment_revision != plan.target_environment_revision
        || deployment_state.environment != plan.target_environment
        || deployment_state.environment_revision
            != plan.target_deployment.expected_environment_revision
    {
        return Err(rejection(
            DeliveryIssueCode::ConcurrentMutation,
            "Target environment changed after Promotion planning.",
            "Use compare-and-set revisions and replan from current target observations.",
            "Refresh target state and plan Promotion again.",
        ));
    }
    let mut next_deployment_state = deployment_state.clone();
    let deployment_receipt = apply_deployment(&mut next_deployment_state, &plan.target_deployment)
        .map_err(deployment_rejection)?;
    let revision_before = state.environment_revision;
    state.environment_revision += 1;
    let effects = DeliveryEffects {
        mutates_environment: true,
        mutates_deployment: true,
        appends_ledger: true,
        ..DeliveryEffects::default()
    };
    let receipt_id = format!(
        "promotion-receipt:{}",
        digest_json(&(
            plan.plan_id.as_str(),
            approval.approval_id.as_str(),
            deployment_receipt.receipt_id.as_str(),
            revision_before,
            state.environment_revision,
        ))
    );
    let receipt = PromotionReceipt {
        protocol: PROMOTION_RECEIPT_PROTOCOL.to_owned(),
        receipt_id,
        plan_id: plan.plan_id.clone(),
        approval_id: approval.approval_id.clone(),
        actor: approval.actor.clone(),
        source_environment: plan.source_environment.clone(),
        target_environment: plan.target_environment.clone(),
        release_id: plan.release_id.clone(),
        release_digest: plan.release_digest.clone(),
        workload_digests: plan.workload_digests.clone(),
        deployment_receipt,
        environment_revision_before: revision_before,
        environment_revision_after: state.environment_revision,
        effects,
    };
    *deployment_state = next_deployment_state;
    state.history.push(receipt.clone());
    Ok(receipt)
}

#[must_use]
pub fn promotion_receipt_integrity_is_valid(
    receipt: &PromotionReceipt,
    plan: &PromotionPlan,
    approval: &PromotionApproval,
) -> bool {
    let effects = DeliveryEffects {
        mutates_environment: true,
        mutates_deployment: true,
        appends_ledger: true,
        ..DeliveryEffects::default()
    };
    receipt.protocol == PROMOTION_RECEIPT_PROTOCOL
        && promotion_plan_integrity_is_valid(plan)
        && receipt.plan_id == plan.plan_id
        && receipt.approval_id == approval.approval_id
        && receipt.actor == approval.actor
        && receipt.source_environment == plan.source_environment
        && receipt.target_environment == plan.target_environment
        && receipt.release_id == plan.release_id
        && receipt.release_digest == plan.release_digest
        && receipt.workload_digests == plan.workload_digests
        && deployment_receipt_integrity_is_valid(
            &receipt.deployment_receipt,
            &plan.target_deployment,
        )
        && receipt.environment_revision_before == plan.target_environment_revision
        && receipt.environment_revision_after == receipt.environment_revision_before + 1
        && receipt.effects == effects
        && receipt.receipt_id
            == format!(
                "promotion-receipt:{}",
                digest_json(&(
                    plan.plan_id.as_str(),
                    approval.approval_id.as_str(),
                    receipt.deployment_receipt.receipt_id.as_str(),
                    receipt.environment_revision_before,
                    receipt.environment_revision_after,
                ))
            )
}

#[must_use]
pub fn promotion_approval_integrity_is_valid(
    approval: &PromotionApproval,
    authority: &impl PromotionApprovalAuthority,
) -> bool {
    let expected_id = format!(
        "promotion-approval:{}",
        digest_json(&(
            PROMOTION_APPROVAL_PROTOCOL,
            approval.plan_digest.as_str(),
            approval.actor.as_str(),
            approval.authority.as_str(),
            Some(approval.authority_proof.as_str()),
            approval.issued_for_target_revision,
            approval.approved,
        ))
    );
    approval.protocol == PROMOTION_APPROVAL_PROTOCOL
        && approval.approval_id == expected_id
        && approval.authority == authority.authority()
        && approval.actor.starts_with("user:")
        && authority.verify_proof(
            approval.actor.as_str(),
            approval.plan_digest.as_str(),
            approval.issued_for_target_revision,
            approval.authority_proof.as_str(),
        )
}

#[must_use]
pub fn promotion_plan_integrity_is_valid(plan: &PromotionPlan) -> bool {
    let target_workload_digests = plan
        .target_deployment
        .workloads
        .iter()
        .map(|workload| {
            (
                workload.workload_id.clone(),
                workload.artifact_digest.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    plan.protocol == PROMOTION_PLAN_PROTOCOL
        && deployment_plan_integrity_is_valid(&plan.target_deployment)
        && gateway_plan_integrity_is_valid(&plan.target_gateway)
        && plan.release_id == plan.target_deployment.release_id
        && plan.release_digest == plan.target_deployment.release_digest
        && plan.workload_digests == target_workload_digests
        && plan.target_environment == plan.target_deployment.environment
        && plan.target_environment_revision == plan.target_deployment.expected_environment_revision
        && plan.config_revision_id == plan.target_deployment.config_revision_id
        && plan.target_deployment.gateway_plan_digest == plan.target_gateway.plan_digest
        && plan.release_id == plan.target_gateway.edge_release_id
        && plan.release_digest == plan.target_gateway.edge_release_digest
        && plan.target_environment == plan.target_gateway.environment
        && plan.plan_id == format!("promotion-plan:{}", plan.plan_digest)
        && digest_json(&PromotionPlanDigestInput {
            protocol: plan.protocol.as_str(),
            release_id: plan.release_id.as_str(),
            release_digest: plan.release_digest.as_str(),
            workload_digests: &plan.workload_digests,
            source_environment: plan.source_environment.as_str(),
            target_environment: plan.target_environment.as_str(),
            source_environment_revision: plan.source_environment_revision,
            target_environment_revision: plan.target_environment_revision,
            source_verification_id: plan.source_verification_id.as_str(),
            source_verification_digest: plan.source_verification_digest.as_str(),
            policy_evidence_id: plan.policy_evidence_id.as_str(),
            policy_evidence_digest: plan.policy_evidence_digest.as_str(),
            config_revision_id: plan.config_revision_id.as_str(),
            secret_reference_ids: plan.secret_reference_ids.as_slice(),
            target_deployment_digest: plan.target_deployment.plan_digest.as_str(),
            target_gateway_digest: plan.target_gateway.plan_digest.as_str(),
            source_topology_digest: plan.source_topology_digest.as_str(),
            target_topology_digest: plan.target_topology_digest.as_str(),
            evidence_references: plan.evidence_references.as_slice(),
            freshness_horizon_revision: plan.freshness_horizon_revision,
            effects: &plan.effects,
        }) == plan.plan_digest
}

fn deployment_rejection(error: DeploymentApplyRejection) -> PromotionApplyRejection {
    PromotionApplyRejection {
        issues: error.issues,
        effects: DeliveryEffects::default(),
    }
}

fn rejection(
    code: DeliveryIssueCode,
    message: &str,
    remediation: &str,
    next_action: &str,
) -> PromotionApplyRejection {
    PromotionApplyRejection {
        issues: vec![issue(code, message, remediation, next_action)],
        effects: DeliveryEffects::default(),
    }
}

fn digest_json(value: &impl Serialize) -> String {
    extraction_input_digest(serde_json::to_vec(value).expect("Promotion values must serialize"))
}
