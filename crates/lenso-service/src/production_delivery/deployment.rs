use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::extraction_input_digest;

use super::{
    ConfigContractDefinition, ConfigRevision, DeliveryEffects, DeliveryIssue, DeliveryIssueCode,
    ReleaseWorkloadRole, SecretProvider, ServiceRelease, config_revision_matches_contract, issue,
    service_release_integrity_is_valid,
};

pub const DEPLOYMENT_PLAN_PROTOCOL: &str = "lenso.deployment-plan.v1";
pub const DEPLOYMENT_RECEIPT_PROTOCOL: &str = "lenso.deployment-receipt.v1";
pub const DEPLOYMENT_OBSERVATION_PROTOCOL: &str = "lenso.deployment-observation.v1";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentAdapterKind {
    Local,
    ExternallyManaged,
    Kubernetes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentWorkloadSettings {
    pub workload_id: String,
    pub replicas: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disruption_min_available: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentEnvironmentBinding {
    pub environment: String,
    pub expected_environment_revision: u64,
    pub config_revision_id: String,
    #[serde(default)]
    pub secret_reference_ids: Vec<String>,
    #[serde(default)]
    pub endpoints: BTreeMap<String, String>,
    #[serde(default)]
    pub placement: BTreeMap<String, String>,
    pub workloads: Vec<DeploymentWorkloadSettings>,
    #[serde(default)]
    pub adapter_inputs: BTreeMap<String, String>,
    pub gateway_plan_digest: String,
    #[serde(default)]
    pub policy_evidence_references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentWorkloadPlan {
    pub workload_id: String,
    pub role: ReleaseWorkloadRole,
    pub artifact_reference: String,
    pub artifact_digest: String,
    pub media_type: String,
    pub settings: DeploymentWorkloadSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentPlan {
    pub protocol: String,
    pub plan_id: String,
    pub plan_digest: String,
    pub adapter: DeploymentAdapterKind,
    pub environment: String,
    pub expected_environment_revision: u64,
    pub release_id: String,
    pub release_digest: String,
    pub service_id: String,
    pub config_revision_id: String,
    pub secret_reference_ids: Vec<String>,
    pub endpoints: BTreeMap<String, String>,
    pub placement: BTreeMap<String, String>,
    pub workloads: Vec<DeploymentWorkloadPlan>,
    pub adapter_inputs: BTreeMap<String, String>,
    pub gateway_plan_digest: String,
    pub policy_evidence_references: Vec<String>,
    pub rollback_capable: bool,
    pub next_actions: Vec<String>,
    pub effects: DeliveryEffects,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeploymentPlanDigestInput<'a> {
    protocol: &'a str,
    adapter: DeploymentAdapterKind,
    environment: &'a str,
    expected_environment_revision: u64,
    release_id: &'a str,
    release_digest: &'a str,
    service_id: &'a str,
    config_revision_id: &'a str,
    secret_reference_ids: &'a [String],
    endpoints: &'a BTreeMap<String, String>,
    placement: &'a BTreeMap<String, String>,
    workloads: &'a [DeploymentWorkloadPlan],
    adapter_inputs: &'a BTreeMap<String, String>,
    gateway_plan_digest: &'a str,
    policy_evidence_references: &'a [String],
    rollback_capable: bool,
    next_actions: &'a [String],
    effects: &'a DeliveryEffects,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentReceipt {
    pub protocol: String,
    pub receipt_id: String,
    pub plan_id: String,
    pub adapter: DeploymentAdapterKind,
    pub environment: String,
    pub environment_revision_before: u64,
    pub environment_revision_after: u64,
    pub release_id: String,
    pub release_digest: String,
    pub config_revision_id: String,
    pub workload_digests: BTreeMap<String, String>,
    pub gateway_plan_digest: String,
    pub effects: DeliveryEffects,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentObservation {
    pub protocol: String,
    pub observation_id: String,
    pub plan_id: String,
    pub receipt_id: String,
    pub source_observation_id: String,
    pub environment: String,
    pub desired_release_id: String,
    pub observed_release_id: String,
    pub observed_release_digest: String,
    pub desired_workload_digests: BTreeMap<String, String>,
    pub observed_workload_digests: BTreeMap<String, String>,
    pub config_revision_id: String,
    pub drifted: bool,
    pub fresh: bool,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentState {
    pub environment: String,
    pub environment_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_release_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_config_revision_id: Option<String>,
    #[serde(default)]
    pub history: Vec<DeploymentReceipt>,
}

impl DeploymentState {
    #[must_use]
    pub fn new(environment: impl Into<String>, environment_revision: u64) -> Self {
        Self {
            environment: environment.into(),
            environment_revision,
            active_release_id: None,
            active_config_revision_id: None,
            history: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentApplyRejection {
    pub issues: Vec<DeliveryIssue>,
    pub effects: DeliveryEffects,
}

pub fn plan_deployment(
    release: &ServiceRelease,
    config_contract: &ConfigContractDefinition,
    config: &ConfigRevision,
    secret_provider: &dyn SecretProvider,
    binding: &DeploymentEnvironmentBinding,
    adapter: DeploymentAdapterKind,
) -> Result<DeploymentPlan, Vec<DeliveryIssue>> {
    let mut issues = Vec::new();
    if !service_release_integrity_is_valid(release)
        || !config_revision_matches_contract(config, config_contract, secret_provider)
        || config.service_id != release.service_id
        || config_contract.reference != release.config_contract.reference
        || config_contract.digest != release.config_contract.digest
        || binding.config_revision_id != config.revision_id
        || binding.environment.trim().is_empty()
    {
        issues.push(issue(
            DeliveryIssueCode::DeploymentInputInvalid,
            "Deployment planning requires one integrity-valid Service Release, matching Config Revision, and environment binding.",
            "Bind the exact release and Config Revision without modifying either artifact.",
            "Correct the binding and plan the Deployment again.",
        ));
    }
    if !deployment_public_inputs_are_valid(
        adapter,
        &binding.endpoints,
        &binding.placement,
        &binding.adapter_inputs,
    ) {
        issues.push(issue(
            DeliveryIssueCode::PlaintextSecretDetected,
            "Deployment bindings contain an unclassified or credential-shaped public value.",
            "Use only public HTTP endpoints, placement labels, and the adapter's typed public identifiers; pass Secret References separately.",
            "Remove free-form values and plan the Deployment again.",
        ));
    }
    let configured_secret_ids = config
        .secret_references
        .iter()
        .map(|reference| reference.reference_id.as_str())
        .collect::<BTreeSet<_>>();
    let bound_secret_ids = binding
        .secret_reference_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if configured_secret_ids != bound_secret_ids {
        issues.push(issue(
            DeliveryIssueCode::SecretReferenceUnresolved,
            "Deployment Secret References do not match the validated Config Revision.",
            "Bind the exact opaque Secret Reference identifiers without reading their values.",
            "Correct the environment binding and plan again.",
        ));
    }
    let settings = binding
        .workloads
        .iter()
        .map(|workload| (workload.workload_id.as_str(), workload))
        .collect::<BTreeMap<_, _>>();
    let release_ids = release
        .workloads
        .iter()
        .map(|workload| workload.workload_id.as_str())
        .collect::<BTreeSet<_>>();
    if settings.keys().copied().collect::<BTreeSet<_>>() != release_ids {
        issues.push(issue(
            DeliveryIssueCode::DeploymentInputInvalid,
            "Deployment Workload settings must cover every and only release Workload.",
            "Declare role-specific settings for the exact multi-Workload Service Release.",
            "Correct the Workload settings and plan again.",
        ));
    }
    if !issues.is_empty() {
        return Err(issues);
    }
    let mut workloads = release
        .workloads
        .iter()
        .map(|workload| DeploymentWorkloadPlan {
            workload_id: workload.workload_id.clone(),
            role: workload.role,
            artifact_reference: workload.artifact_reference.clone(),
            artifact_digest: workload.artifact_digest.clone(),
            media_type: workload.media_type.clone(),
            settings: (*settings[workload.workload_id.as_str()]).clone(),
        })
        .collect::<Vec<_>>();
    workloads.sort_by(|left, right| left.workload_id.cmp(&right.workload_id));
    let mut secret_reference_ids = binding.secret_reference_ids.clone();
    secret_reference_ids.sort();
    let mut policy_evidence_references = binding.policy_evidence_references.clone();
    policy_evidence_references.sort();
    policy_evidence_references.dedup();
    let rollback_capable = release.rollback.automatic_allowed;
    let next_actions =
        vec!["Review adapter diff and apply against the expected environment revision.".to_owned()];
    let effects = DeliveryEffects::default();
    let plan_digest = digest_json(&DeploymentPlanDigestInput {
        protocol: DEPLOYMENT_PLAN_PROTOCOL,
        adapter,
        environment: &binding.environment,
        expected_environment_revision: binding.expected_environment_revision,
        release_id: &release.release_id,
        release_digest: &release.release_digest,
        service_id: &release.service_id,
        config_revision_id: &config.revision_id,
        secret_reference_ids: &secret_reference_ids,
        endpoints: &binding.endpoints,
        placement: &binding.placement,
        workloads: &workloads,
        adapter_inputs: &binding.adapter_inputs,
        gateway_plan_digest: &binding.gateway_plan_digest,
        policy_evidence_references: &policy_evidence_references,
        rollback_capable,
        next_actions: &next_actions,
        effects: &effects,
    });
    Ok(DeploymentPlan {
        protocol: DEPLOYMENT_PLAN_PROTOCOL.to_owned(),
        plan_id: format!("deployment-plan:{plan_digest}"),
        plan_digest,
        adapter,
        environment: binding.environment.clone(),
        expected_environment_revision: binding.expected_environment_revision,
        release_id: release.release_id.clone(),
        release_digest: release.release_digest.clone(),
        service_id: release.service_id.clone(),
        config_revision_id: config.revision_id.clone(),
        secret_reference_ids,
        endpoints: binding.endpoints.clone(),
        placement: binding.placement.clone(),
        workloads,
        adapter_inputs: binding.adapter_inputs.clone(),
        gateway_plan_digest: binding.gateway_plan_digest.clone(),
        policy_evidence_references,
        rollback_capable,
        next_actions,
        effects,
    })
}

pub fn apply_deployment(
    state: &mut DeploymentState,
    plan: &DeploymentPlan,
) -> Result<DeploymentReceipt, DeploymentApplyRejection> {
    if !deployment_plan_integrity_is_valid(plan) {
        return Err(DeploymentApplyRejection {
            issues: vec![issue(
                DeliveryIssueCode::StaleInput,
                "Deployment inputs changed after the plan was generated.",
                "Generate a new plan from current adapter and environment observations.",
                "Refresh environment state and plan the Deployment again.",
            )],
            effects: DeliveryEffects::default(),
        });
    }
    if let Some(existing) = state
        .history
        .iter()
        .find(|receipt| receipt.plan_id == plan.plan_id)
    {
        return deployment_receipt_integrity_is_valid(existing, plan)
            .then(|| existing.clone())
            .ok_or_else(|| DeploymentApplyRejection {
                issues: vec![issue(
                    DeliveryIssueCode::StaleInput,
                    "The completed Deployment receipt no longer matches the exact plan.",
                    "Preserve the immutable plan and append-only receipt together.",
                    "Restore the original receipt or create a new Deployment plan.",
                )],
                effects: DeliveryEffects::default(),
            });
    }
    if state.environment != plan.environment
        || state.environment_revision != plan.expected_environment_revision
    {
        return Err(DeploymentApplyRejection {
            issues: vec![issue(
                DeliveryIssueCode::StaleInput,
                "Deployment inputs changed after the plan was generated.",
                "Generate a new plan from current adapter and environment observations.",
                "Refresh environment state and plan the Deployment again.",
            )],
            effects: DeliveryEffects::default(),
        });
    }
    let revision_before = state.environment_revision;
    state.environment_revision += 1;
    state.active_release_id = Some(plan.release_id.clone());
    state.active_config_revision_id = Some(plan.config_revision_id.clone());
    let workload_digests = plan
        .workloads
        .iter()
        .map(|workload| {
            (
                workload.workload_id.clone(),
                workload.artifact_digest.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let effects = DeliveryEffects {
        mutates_environment: true,
        mutates_deployment: true,
        appends_ledger: true,
        ..DeliveryEffects::default()
    };
    let receipt_digest = digest_json(&(
        DEPLOYMENT_RECEIPT_PROTOCOL,
        plan.plan_id.as_str(),
        plan.adapter,
        plan.environment.as_str(),
        revision_before,
        state.environment_revision,
        plan.release_id.as_str(),
        plan.release_digest.as_str(),
        plan.config_revision_id.as_str(),
        &workload_digests,
        plan.gateway_plan_digest.as_str(),
        &effects,
    ));
    let receipt_id = format!("deployment-receipt:{receipt_digest}");
    let receipt = DeploymentReceipt {
        protocol: DEPLOYMENT_RECEIPT_PROTOCOL.to_owned(),
        receipt_id,
        plan_id: plan.plan_id.clone(),
        adapter: plan.adapter,
        environment: plan.environment.clone(),
        environment_revision_before: revision_before,
        environment_revision_after: state.environment_revision,
        release_id: plan.release_id.clone(),
        release_digest: plan.release_digest.clone(),
        config_revision_id: plan.config_revision_id.clone(),
        workload_digests,
        gateway_plan_digest: plan.gateway_plan_digest.clone(),
        effects,
    };
    state.history.push(receipt.clone());
    Ok(receipt)
}

#[must_use]
pub fn observe_deployment(
    plan: &DeploymentPlan,
    receipt: &DeploymentReceipt,
    fresh: bool,
) -> DeploymentObservation {
    observe_deployment_adapter(
        plan,
        &receipt.receipt_id,
        &receipt.receipt_id,
        &receipt.release_id,
        &receipt.release_digest,
        &receipt.workload_digests,
        &receipt.config_revision_id,
        fresh,
    )
}

#[must_use]
pub fn observe_deployment_adapter(
    plan: &DeploymentPlan,
    receipt_id: &str,
    source_observation_id: &str,
    observed_release_id: &str,
    observed_release_digest: &str,
    observed_workload_digests: &BTreeMap<String, String>,
    observed_config_revision_id: &str,
    fresh: bool,
) -> DeploymentObservation {
    let desired_workload_digests = plan
        .workloads
        .iter()
        .map(|workload| {
            (
                workload.workload_id.clone(),
                workload.artifact_digest.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let drifted = observed_release_id != plan.release_id
        || observed_release_digest != plan.release_digest
        || observed_config_revision_id != plan.config_revision_id
        || observed_workload_digests != &desired_workload_digests;
    let next_actions = if drifted || !fresh {
        vec![
            "Refresh adapter observations and reconcile the Deployment before Promotion."
                .to_owned(),
        ]
    } else {
        vec!["Use this fresh observation as Deployment evidence.".to_owned()]
    };
    let observation_id = format!(
        "deployment-observation:{}",
        digest_json(&(
            DEPLOYMENT_OBSERVATION_PROTOCOL,
            plan.plan_id.as_str(),
            receipt_id,
            source_observation_id,
            plan.environment.as_str(),
            plan.release_id.as_str(),
            observed_release_id,
            observed_release_digest,
            &desired_workload_digests,
            observed_workload_digests,
            observed_config_revision_id,
            drifted,
            fresh,
            &next_actions,
        ))
    );
    DeploymentObservation {
        protocol: DEPLOYMENT_OBSERVATION_PROTOCOL.to_owned(),
        observation_id,
        plan_id: plan.plan_id.clone(),
        receipt_id: receipt_id.to_owned(),
        source_observation_id: source_observation_id.to_owned(),
        environment: plan.environment.clone(),
        desired_release_id: plan.release_id.clone(),
        observed_release_id: observed_release_id.to_owned(),
        observed_release_digest: observed_release_digest.to_owned(),
        desired_workload_digests,
        observed_workload_digests: observed_workload_digests.clone(),
        config_revision_id: observed_config_revision_id.to_owned(),
        drifted,
        fresh,
        next_actions,
    }
}

#[must_use]
pub fn deployment_plan_integrity_is_valid(plan: &DeploymentPlan) -> bool {
    plan.protocol == DEPLOYMENT_PLAN_PROTOCOL
        && plan.plan_id == format!("deployment-plan:{}", plan.plan_digest)
        && deployment_public_inputs_are_valid(
            plan.adapter,
            &plan.endpoints,
            &plan.placement,
            &plan.adapter_inputs,
        )
        && digest_json(&DeploymentPlanDigestInput {
            protocol: &plan.protocol,
            adapter: plan.adapter,
            environment: &plan.environment,
            expected_environment_revision: plan.expected_environment_revision,
            release_id: &plan.release_id,
            release_digest: &plan.release_digest,
            service_id: &plan.service_id,
            config_revision_id: &plan.config_revision_id,
            secret_reference_ids: &plan.secret_reference_ids,
            endpoints: &plan.endpoints,
            placement: &plan.placement,
            workloads: &plan.workloads,
            adapter_inputs: &plan.adapter_inputs,
            gateway_plan_digest: &plan.gateway_plan_digest,
            policy_evidence_references: &plan.policy_evidence_references,
            rollback_capable: plan.rollback_capable,
            next_actions: &plan.next_actions,
            effects: &plan.effects,
        }) == plan.plan_digest
}

fn deployment_public_inputs_are_valid(
    adapter: DeploymentAdapterKind,
    endpoints: &BTreeMap<String, String>,
    placement: &BTreeMap<String, String>,
    adapter_inputs: &BTreeMap<String, String>,
) -> bool {
    endpoints.iter().all(|(key, value)| {
        public_field_name_is_safe(key)
            && value.len() <= 2_048
            && (value.starts_with("http://") || value.starts_with("https://"))
            && !value.contains(['@', '?', '#', '\\'])
            && !value.chars().any(char::is_whitespace)
    }) && placement.iter().all(|(key, value)| {
        public_field_name_is_safe(key)
            && !key.is_empty()
            && key.len() <= 253
            && !value.is_empty()
            && value.len() <= 63
            && key
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || ".-_/".contains(character))
            && value
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || ".-_".contains(character))
    }) && match adapter {
        DeploymentAdapterKind::Kubernetes => {
            adapter_inputs
                .iter()
                .all(|(key, value)| match key.as_str() {
                    "resourceName" => kubernetes_resource_name_is_safe(value),
                    "rollbackReleaseId" => immutable_release_id_is_safe(value),
                    _ => false,
                })
        }
        DeploymentAdapterKind::Local | DeploymentAdapterKind::ExternallyManaged => {
            adapter_inputs.is_empty()
        }
    }
}

fn public_field_name_is_safe(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase();
    ![
        "secret",
        "password",
        "credential",
        "privatekey",
        "signingkey",
        "accesstoken",
        "token",
    ]
    .iter()
    .any(|forbidden| normalized.contains(forbidden))
}

fn kubernetes_resource_name_is_safe(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && value.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '-'
                || character == '.'
        })
        && value
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        && value
            .chars()
            .last()
            .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
}

fn immutable_release_id_is_safe(value: &str) -> bool {
    value
        .strip_prefix("service-release:sha256:")
        .is_some_and(|digest| {
            digest.len() == 64
                && digest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
}

#[must_use]
pub fn deployment_receipt_integrity_is_valid(
    receipt: &DeploymentReceipt,
    plan: &DeploymentPlan,
) -> bool {
    let expected_workloads = plan
        .workloads
        .iter()
        .map(|workload| {
            (
                workload.workload_id.clone(),
                workload.artifact_digest.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let expected_id = format!(
        "deployment-receipt:{}",
        digest_json(&(
            receipt.protocol.as_str(),
            receipt.plan_id.as_str(),
            receipt.adapter,
            receipt.environment.as_str(),
            receipt.environment_revision_before,
            receipt.environment_revision_after,
            receipt.release_id.as_str(),
            receipt.release_digest.as_str(),
            receipt.config_revision_id.as_str(),
            &receipt.workload_digests,
            receipt.gateway_plan_digest.as_str(),
            &receipt.effects,
        ))
    );
    deployment_plan_integrity_is_valid(plan)
        && receipt.protocol == DEPLOYMENT_RECEIPT_PROTOCOL
        && receipt.receipt_id == expected_id
        && receipt.plan_id == plan.plan_id
        && receipt.adapter == plan.adapter
        && receipt.environment == plan.environment
        && receipt.environment_revision_before == plan.expected_environment_revision
        && receipt.environment_revision_after == receipt.environment_revision_before + 1
        && receipt.release_id == plan.release_id
        && receipt.release_digest == plan.release_digest
        && receipt.config_revision_id == plan.config_revision_id
        && receipt.workload_digests == expected_workloads
        && receipt.gateway_plan_digest == plan.gateway_plan_digest
        && receipt.effects
            == DeliveryEffects {
                mutates_environment: true,
                mutates_deployment: true,
                appends_ledger: true,
                ..DeliveryEffects::default()
            }
}

#[must_use]
pub fn deployment_observation_integrity_is_valid(
    observation: &DeploymentObservation,
    plan: &DeploymentPlan,
    receipt: &DeploymentReceipt,
) -> bool {
    let expected = observe_deployment_adapter(
        plan,
        &receipt.receipt_id,
        &observation.source_observation_id,
        &receipt.release_id,
        &receipt.release_digest,
        &receipt.workload_digests,
        &receipt.config_revision_id,
        observation.fresh,
    );
    deployment_receipt_integrity_is_valid(receipt, plan)
        && deployment_observation_content_integrity_is_valid(observation)
        && !observation.source_observation_id.trim().is_empty()
        && observation == &expected
}

#[must_use]
pub fn deployment_observation_content_integrity_is_valid(
    observation: &DeploymentObservation,
) -> bool {
    observation.protocol == DEPLOYMENT_OBSERVATION_PROTOCOL
        && observation.observation_id
            == format!(
                "deployment-observation:{}",
                digest_json(&(
                    observation.protocol.as_str(),
                    observation.plan_id.as_str(),
                    observation.receipt_id.as_str(),
                    observation.source_observation_id.as_str(),
                    observation.environment.as_str(),
                    observation.desired_release_id.as_str(),
                    observation.observed_release_id.as_str(),
                    observation.observed_release_digest.as_str(),
                    &observation.desired_workload_digests,
                    &observation.observed_workload_digests,
                    observation.config_revision_id.as_str(),
                    observation.drifted,
                    observation.fresh,
                    observation.next_actions.as_slice(),
                ))
            )
}

fn digest_json(value: &impl Serialize) -> String {
    extraction_input_digest(serde_json::to_vec(value).expect("Deployment values must serialize"))
}
