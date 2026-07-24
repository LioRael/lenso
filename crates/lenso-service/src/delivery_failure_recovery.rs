use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::{DeliveryEffects, extraction_input_digest};

pub const DELIVERY_FAILURE_RECOVERY_PROTOCOL: &str = "lenso.delivery-failure-recovery-evidence.v1";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryFailureCondition {
    DeploymentAdapterRejected,
    OperatorReconciliationFailed,
    GatewayDrift,
    InvalidConfigRevision,
    SecretReferenceUnavailable,
    MigrationFailed,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryFailureStage {
    BeforeApply,
    Reconciling,
    PartiallyApplied,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryRecoveryScope {
    Deterministic,
    EnvironmentVerification,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryRecoveryDecision {
    Passed,
    Blocked,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryRecoveryIssueCode {
    InputInvalid,
    PreApplyMutation,
    DesiredObservedStateConflated,
    LastValidConfigurationLost,
    MigrationEvidenceIncomplete,
    MigrationEffectWouldRepeat,
    EnvironmentEvidenceInvalid,
    CleanupIncomplete,
}

impl DeliveryRecoveryIssueCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InputInvalid => "delivery_recovery_input_invalid",
            Self::PreApplyMutation => "delivery_recovery_pre_apply_mutation",
            Self::DesiredObservedStateConflated => {
                "delivery_recovery_desired_observed_state_conflated"
            }
            Self::LastValidConfigurationLost => "delivery_recovery_last_valid_configuration_lost",
            Self::MigrationEvidenceIncomplete => "delivery_recovery_migration_evidence_incomplete",
            Self::MigrationEffectWouldRepeat => "delivery_recovery_migration_effect_would_repeat",
            Self::EnvironmentEvidenceInvalid => "delivery_recovery_environment_evidence_invalid",
            Self::CleanupIncomplete => "delivery_recovery_cleanup_incomplete",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryRecoveryIssue {
    pub code: DeliveryRecoveryIssueCode,
    pub message: String,
    pub remediation: String,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryStateObservation {
    pub observation_id: String,
    pub source: String,
    pub revision: u64,
    pub desired_digest: String,
    pub observed_digest: String,
    pub fresh: bool,
    pub drifted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MigrationRecoveryEvidence {
    pub migration_id: String,
    pub completed_effects: Vec<String>,
    pub remaining_steps: Vec<String>,
    pub retry_steps: Vec<String>,
    pub state_compatible: bool,
    pub rollback_allowed: bool,
    pub intervention_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct KubernetesRecoveryObservation {
    pub cluster_identity: String,
    pub api_server_version: String,
    pub operator_version: String,
    pub gateway_adapter_version: String,
    pub used_real_api: bool,
    pub observed_resource_version: String,
    pub evidence_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryFailureRecoveryInput {
    pub scenario_id: String,
    pub condition: DeliveryFailureCondition,
    pub stage: DeliveryFailureStage,
    pub scope: DeliveryRecoveryScope,
    pub desired_state: DeliveryStateObservation,
    pub observed_state: DeliveryStateObservation,
    pub previous_valid_config_revision_id: String,
    pub attempted_config_revision_id: String,
    pub active_config_revision_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration: Option<MigrationRecoveryEvidence>,
    #[serde(default)]
    pub infrastructure_mutations: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kubernetes: Option<KubernetesRecoveryObservation>,
    pub cleanup_complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryFailureRecoveryEvidence {
    pub protocol: String,
    pub evidence_id: String,
    pub evidence_digest: String,
    pub scenario_id: String,
    pub condition: DeliveryFailureCondition,
    pub stage: DeliveryFailureStage,
    pub scope: DeliveryRecoveryScope,
    pub desired_state: DeliveryStateObservation,
    pub observed_state: DeliveryStateObservation,
    pub retained_config_revision_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration: Option<MigrationRecoveryEvidence>,
    pub decision: DeliveryRecoveryDecision,
    pub issues: Vec<DeliveryRecoveryIssue>,
    pub next_actions: Vec<String>,
    pub effects: DeliveryEffects,
}

#[must_use]
pub fn evaluate_delivery_failure_recovery(
    mut input: DeliveryFailureRecoveryInput,
) -> DeliveryFailureRecoveryEvidence {
    input.infrastructure_mutations.sort();
    if let Some(migration) = &mut input.migration {
        migration.completed_effects.sort();
        migration.completed_effects.dedup();
        migration.remaining_steps.sort();
        migration.remaining_steps.dedup();
        migration.retry_steps.sort();
        migration.retry_steps.dedup();
    }

    let mut issues = Vec::new();
    if input.scenario_id.trim().is_empty()
        || input.desired_state.observation_id.trim().is_empty()
        || input.observed_state.observation_id.trim().is_empty()
        || input.observed_state.source.trim().is_empty()
        || input.observed_state.revision < input.desired_state.revision
        || !valid_digest(&input.desired_state.desired_digest)
        || !valid_digest(&input.observed_state.observed_digest)
    {
        issues.push(issue(
            DeliveryRecoveryIssueCode::InputInvalid,
            "Delivery recovery evidence is missing a stable identity, digest, or monotonic observation.",
            "Collect exact desired and observed state from their authoritative boundaries.",
            "Refresh the failure observation before choosing a recovery action.",
        ));
    }
    if input.stage == DeliveryFailureStage::BeforeApply
        && !input.infrastructure_mutations.is_empty()
    {
        issues.push(issue(
            DeliveryRecoveryIssueCode::PreApplyMutation,
            "A validation, trust, policy, compatibility, configuration, freshness, or approval failure mutated infrastructure before apply.",
            "Keep every pre-apply failure zero-effect.",
            "Restore the prior state and repeat validation before apply.",
        ));
    }
    if input.condition == DeliveryFailureCondition::GatewayDrift
        && (!input.observed_state.drifted
            || input.desired_state.desired_digest == input.observed_state.observed_digest)
    {
        issues.push(issue(
            DeliveryRecoveryIssueCode::DesiredObservedStateConflated,
            "Gateway drift evidence does not preserve the difference between desired and observed state.",
            "Record both states and their independent source revisions.",
            "Refresh the gateway observation without overwriting newer observed state.",
        ));
    }
    if input.active_config_revision_id != input.previous_valid_config_revision_id {
        issues.push(issue(
            DeliveryRecoveryIssueCode::LastValidConfigurationLost,
            "The Service did not retain its last valid Config Revision after activation failed.",
            "Leave the rejected revision inactive and keep the previous revision authoritative.",
            "Restore the previous valid Config Revision before retrying activation.",
        ));
    }
    if input.condition == DeliveryFailureCondition::MigrationFailed {
        match &input.migration {
            Some(migration)
                if !migration.migration_id.trim().is_empty()
                    && !migration.completed_effects.is_empty()
                    && !migration.remaining_steps.is_empty()
                    && (migration.state_compatible || migration.intervention_required) =>
            {
                let completed = migration
                    .completed_effects
                    .iter()
                    .map(String::as_str)
                    .collect::<BTreeSet<_>>();
                if migration
                    .retry_steps
                    .iter()
                    .any(|step| completed.contains(step.as_str()))
                {
                    issues.push(issue(
                        DeliveryRecoveryIssueCode::MigrationEffectWouldRepeat,
                        "The recovery plan would repeat a completed Migration effect.",
                        "Resume only the remaining steps identified by durable receipts.",
                        "Remove completed effects from the retry set and rebuild the plan.",
                    ));
                }
            }
            _ => issues.push(issue(
                DeliveryRecoveryIssueCode::MigrationEvidenceIncomplete,
                "Partial Migration evidence does not identify completed effects, remaining steps, compatibility, and intervention constraints.",
                "Bind recovery to durable per-effect receipts and current schema compatibility.",
                "Collect complete Migration evidence before resuming or rolling back.",
            )),
        }
    }
    if input.scope == DeliveryRecoveryScope::EnvironmentVerification {
        let valid = input.kubernetes.as_ref().is_some_and(|observation| {
            observation.used_real_api
                && !observation.cluster_identity.trim().is_empty()
                && !observation.api_server_version.trim().is_empty()
                && !observation.operator_version.trim().is_empty()
                && !observation.gateway_adapter_version.trim().is_empty()
                && !observation.observed_resource_version.trim().is_empty()
                && valid_digest(&observation.evidence_digest)
        });
        if !valid {
            issues.push(issue(
                DeliveryRecoveryIssueCode::EnvironmentEvidenceInvalid,
                "Environment Verification lacks a real Kubernetes API, Operator, and gateway observation.",
                "Use the pinned real-cluster verification lane rather than mock-only desired resources.",
                "Repeat the scenario against the supported environment adapter set.",
            ));
        }
    }
    if !input.cleanup_complete {
        issues.push(issue(
            DeliveryRecoveryIssueCode::CleanupIncomplete,
            "Delivery failure cleanup is incomplete.",
            "Remove or isolate every disposable resource while preserving durable evidence.",
            "Finish cleanup before accepting the recovery result.",
        ));
    }

    let decision = if issues.is_empty() {
        DeliveryRecoveryDecision::Passed
    } else {
        DeliveryRecoveryDecision::Blocked
    };
    let next_actions = if issues.is_empty() {
        match input.condition {
            DeliveryFailureCondition::GatewayDrift => {
                vec!["Reconcile desired state against the newer observed revision.".to_owned()]
            }
            DeliveryFailureCondition::MigrationFailed => {
                vec!["Resume only the receipt-backed remaining Migration steps.".to_owned()]
            }
            _ => vec!["Retry the idempotent operation from the preserved state.".to_owned()],
        }
    } else {
        issues
            .iter()
            .flat_map(|issue| issue.next_actions.iter().cloned())
            .collect()
    };
    let mut evidence = DeliveryFailureRecoveryEvidence {
        protocol: DELIVERY_FAILURE_RECOVERY_PROTOCOL.to_owned(),
        evidence_id: String::new(),
        evidence_digest: String::new(),
        scenario_id: input.scenario_id,
        condition: input.condition,
        stage: input.stage,
        scope: input.scope,
        desired_state: input.desired_state,
        observed_state: input.observed_state,
        retained_config_revision_id: input.active_config_revision_id,
        migration: input.migration,
        decision,
        issues,
        next_actions,
        effects: DeliveryEffects::default(),
    };
    evidence.evidence_digest = digest_without_identity(&evidence);
    evidence.evidence_id = format!(
        "delivery-failure-recovery:{}",
        &evidence.evidence_digest[7..23]
    );
    evidence
}

#[must_use]
pub fn delivery_failure_recovery_schema() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(DeliveryFailureRecoveryEvidence))
        .expect("delivery failure recovery schema serializes");
    schema["$id"] = Value::String(
        "https://contracts.lenso.local/ga/lenso.delivery-failure-recovery-evidence.v1.schema.json"
            .to_owned(),
    );
    schema
}

fn issue(
    code: DeliveryRecoveryIssueCode,
    message: impl Into<String>,
    remediation: impl Into<String>,
    next_action: impl Into<String>,
) -> DeliveryRecoveryIssue {
    DeliveryRecoveryIssue {
        code,
        message: message.into(),
        remediation: remediation.into(),
        next_actions: vec![next_action.into()],
    }
}

fn valid_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn digest_without_identity(evidence: &DeliveryFailureRecoveryEvidence) -> String {
    let mut canonical = evidence.clone();
    canonical.evidence_id.clear();
    canonical.evidence_digest.clear();
    extraction_input_digest(
        &serde_json::to_vec(&canonical).expect("delivery recovery evidence serializes"),
    )
}
