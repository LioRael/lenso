use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use utoipa::ToSchema;

use crate::extraction_input_digest;
use crate::{
    CANARY_DECISION_PROTOCOL, CANARY_PLAN_PROTOCOL, CONFIG_ACTIVATION_RECEIPT_PROTOCOL,
    CONFIG_REVISION_PROTOCOL, CanaryDecision, CanaryPlan, ConfigActivationReceipt, ConfigRevision,
    DEPLOYMENT_OBSERVATION_PROTOCOL, DEPLOYMENT_PLAN_PROTOCOL, DEPLOYMENT_RECEIPT_PROTOCOL,
    DeploymentObservation, DeploymentPlan, DeploymentReceipt, EDGE_CONTRACT_PROTOCOL,
    ENVIRONMENT_VERIFICATION_PROTOCOL, EdgeContract, EnvironmentVerification,
    GATEWAY_OBSERVATION_PROTOCOL, GATEWAY_PLAN_PROTOCOL, GatewayConfigurationPlan,
    GatewayObservation, POLICY_EVIDENCE_PROTOCOL, PROMOTION_APPROVAL_PROTOCOL,
    PROMOTION_PLAN_PROTOCOL, PROMOTION_RECEIPT_PROTOCOL, PolicyEvidence, PromotionApproval,
    PromotionPlan, PromotionReceipt, RELEASE_TRUST_EVIDENCE_PROTOCOL,
    RELIABILITY_OBSERVATION_PROTOCOL, ROLLBACK_PLAN_PROTOCOL, ROLLBACK_RECEIPT_PROTOCOL,
    ReleaseTrustEvidence, ReliabilityObservation, RollbackPlan, RollbackReceipt,
    SERVICE_RELEASE_PROTOCOL, ServiceRelease, canary_decision_integrity_is_valid,
    canary_plan_integrity_is_valid, config_revision_integrity_is_valid,
    deployment_observation_content_integrity_is_valid, deployment_plan_integrity_is_valid,
    edge_contract_integrity_is_valid, environment_verification_integrity_is_valid,
    gateway_observation_content_integrity_is_valid, gateway_plan_integrity_is_valid,
    policy_evidence_integrity_is_valid, promotion_plan_integrity_is_valid,
    rollback_plan_integrity_is_valid, secret_reference_metadata_is_safe,
    service_release_integrity_is_valid,
};
use crate::{
    CONTRACT_RETIREMENT_PLAN_PROTOCOL, CONTRACT_RETIREMENT_RECEIPT_PROTOCOL,
    ContractRetirementPlan, ContractRetirementReceipt, DELIVERY_FAILURE_RECOVERY_PROTOCOL,
    DISASTER_RECOVERY_EVIDENCE_PROTOCOL, DeliveryFailureRecoveryEvidence, DisasterRecoveryEvidence,
    GA_SUPPORT_MANIFEST_PROTOCOL, GaSupportManifest, PERFORMANCE_PROFILE_PROTOCOL,
    PerformanceProfile, SECURITY_REVIEW_PROTOCOL, SERVICE_RESTORE_EVIDENCE_PROTOCOL,
    SUPPORT_ENVELOPE_PROTOCOL, SecurityReviewEvidence, ServiceRestoreEvidence, SupportEnvelope,
    contract_retirement_plan_integrity_is_valid, contract_retirement_receipt_integrity_is_valid,
    delivery_failure_recovery_integrity_is_valid, disaster_recovery_evidence_integrity_is_valid,
    ga_support_manifest_integrity_valid, performance_profile_integrity_is_valid,
    security_review_integrity_is_valid, service_restore_integrity_is_valid,
    support_envelope_integrity_is_valid,
};

pub const DELIVERY_CONSOLE_PROJECTION_PROTOCOL: &str = "lenso.delivery-console.v1";
pub const DELIVERY_ARTIFACT_BATCH_PROTOCOL: &str = "lenso.delivery-artifact-batch.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryConsoleArtifacts {
    #[serde(default)]
    pub artifacts: Vec<Value>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryConsoleState {
    Planned,
    Blocked,
    Staged,
    Approved,
    Canary,
    Converging,
    Ready,
    RollingBack,
    RolledBack,
    Paused,
    InterventionRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryConsoleRelease {
    pub service_id: String,
    pub release_id: String,
    pub release_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryConsoleSupplyChainWorkload {
    pub workload_id: String,
    pub artifact_digest: String,
    pub signature_status: String,
    pub sbom_reference: String,
    pub provenance_reference: String,
    pub provenance_subject_matches: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryConsolePolicy {
    pub evidence_id: String,
    pub pack_id: String,
    pub decision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryConsoleSecretReference {
    pub reference_id: String,
    pub provider: String,
    pub purpose: String,
    pub scope: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_revision: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryConsoleConfiguration {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desired_revision_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_revision_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_revision_id: Option<String>,
    pub drifted: bool,
    #[serde(default)]
    pub secret_references: Vec<DeliveryConsoleSecretReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryConsoleDeployment {
    pub environment: String,
    pub desired_release_id: String,
    pub observed_release_id: String,
    pub config_revision_id: String,
    pub drifted: bool,
    pub fresh: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryConsoleAdapterDrift {
    pub environment: String,
    pub drifted: bool,
    pub fresh: bool,
    #[serde(default)]
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryConsoleEdge {
    pub contract_id: String,
    #[serde(default)]
    pub public_routes: Vec<String>,
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryConsoleIssue {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub evidence_references: Vec<String>,
    pub remediation: String,
    #[serde(default)]
    pub next_actions: Vec<String>,
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryConsoleTimelineEntry {
    pub protocol: String,
    pub artifact_id: String,
    pub state: String,
    #[serde(default)]
    pub evidence_references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryConsoleDependencyObservation {
    pub dependency_id: String,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_degraded_mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryConsoleCanaryObservation {
    pub observation_id: String,
    pub observed_revision: u64,
    pub fresh: bool,
    pub observation_window_seconds: u64,
    pub sample_count: u64,
    pub generic_process_healthy: bool,
    #[serde(default)]
    pub workload_readiness: BTreeMap<String, bool>,
    #[serde(default)]
    pub workload_liveness: BTreeMap<String, bool>,
    pub availability_basis_points: Option<u32>,
    pub latency_p99_ms: Option<u64>,
    pub error_budget_used_basis_points: Option<u32>,
    pub queue_backlog: Option<u64>,
    pub workflow_backlog: Option<u64>,
    pub timer_lag_ms: Option<u64>,
    pub retry_exhaustion: Option<u64>,
    pub compensation_pressure: Option<u64>,
    #[serde(default)]
    pub dependencies: Vec<DeliveryConsoleDependencyObservation>,
    #[serde(default)]
    pub failure_domains: BTreeMap<String, bool>,
    pub scaling_check_passed: Option<bool>,
    pub disruption_check_passed: Option<bool>,
    pub availability_check_passed: Option<bool>,
    #[serde(default)]
    pub evidence_references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryConsoleGaEvidence {
    pub protocol: String,
    pub evidence_id: String,
    pub status: String,
    pub stale: bool,
    #[serde(default)]
    pub subjects: BTreeMap<String, String>,
    #[serde(default)]
    pub details: BTreeMap<String, Value>,
    #[serde(default)]
    pub issue_codes: Vec<String>,
    #[serde(default)]
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryConsoleGaOperations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_manifest: Option<DeliveryConsoleGaEvidence>,
    #[serde(default)]
    pub delivery_recovery: Vec<DeliveryConsoleGaEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore: Option<DeliveryConsoleGaEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disaster_recovery: Option<DeliveryConsoleGaEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub performance: Option<DeliveryConsoleGaEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_envelope: Option<DeliveryConsoleGaEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security_review: Option<DeliveryConsoleGaEvidence>,
    #[serde(default)]
    pub contract_lifecycle: Vec<DeliveryConsoleGaEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryConsoleProjection {
    pub protocol: String,
    pub projection_digest: String,
    pub state: DeliveryConsoleState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<DeliveryConsoleRelease>,
    #[serde(default)]
    pub supply_chain: Vec<DeliveryConsoleSupplyChainWorkload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<DeliveryConsolePolicy>,
    pub configuration: DeliveryConsoleConfiguration,
    #[serde(default)]
    pub deployments: Vec<DeliveryConsoleDeployment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge: Option<DeliveryConsoleEdge>,
    #[serde(default)]
    pub adapter_drift: Vec<DeliveryConsoleAdapterDrift>,
    #[serde(default)]
    pub promotion_history: Vec<DeliveryConsoleTimelineEntry>,
    #[serde(default)]
    pub canary_timeline: Vec<DeliveryConsoleTimelineEntry>,
    #[serde(default)]
    pub canary_observations: Vec<DeliveryConsoleCanaryObservation>,
    #[serde(default)]
    pub rollback_timeline: Vec<DeliveryConsoleTimelineEntry>,
    #[serde(default)]
    pub issues: Vec<DeliveryConsoleIssue>,
    #[serde(default)]
    pub next_actions: Vec<String>,
    #[serde(default)]
    pub runtime_story_references: Vec<String>,
    #[serde(default)]
    pub ga_operations: DeliveryConsoleGaOperations,
    pub read_only: bool,
    #[serde(default)]
    pub apply_actions: Vec<String>,
}

#[must_use]
pub fn project_delivery_console(input: DeliveryConsoleArtifacts) -> DeliveryConsoleProjection {
    // Artifact order is the append-only ledger order. Keep it intact so the
    // newest state is selected by recording sequence rather than hash-like IDs.
    let release_artifact = latest(&input.artifacts, "lenso.service-release.v1");
    let trust_artifact = latest(&input.artifacts, "lenso.release-trust-evidence.v1");
    let policy_artifact = input
        .artifacts
        .iter()
        .rev()
        .find(|artifact| protocol(artifact).contains("policy-evidence"));
    let config_artifact = latest(&input.artifacts, "lenso.config-revision.v1");
    let config_receipt = latest(&input.artifacts, "lenso.config-activation-receipt.v1");
    let release = release_artifact.map(|artifact| DeliveryConsoleRelease {
        service_id: text(artifact, "serviceId").unwrap_or_else(|| "unknown".to_owned()),
        release_id: text(artifact, "releaseId").unwrap_or_else(|| "unknown".to_owned()),
        release_digest: text(artifact, "releaseDigest").unwrap_or_else(|| "unknown".to_owned()),
    });
    let signature_status = trust_artifact
        .and_then(|artifact| array(artifact, "signatures").into_iter().next())
        .and_then(|signature| text(signature, "status"))
        .unwrap_or_else(|| "unknown".to_owned());
    let trust_workloads = trust_artifact
        .map(|artifact| array(artifact, "workloads"))
        .unwrap_or_default();
    let mut supply_chain = release_artifact
        .map(|artifact| array(artifact, "workloads"))
        .unwrap_or_default()
        .into_iter()
        .map(|workload| {
            let workload_id = text(workload, "workloadId").unwrap_or_else(|| "unknown".to_owned());
            let trust = trust_workloads
                .iter()
                .find(|item| text(item, "workloadId").as_deref() == Some(workload_id.as_str()));
            DeliveryConsoleSupplyChainWorkload {
                workload_id,
                artifact_digest: text(workload, "artifactDigest")
                    .unwrap_or_else(|| "unknown".to_owned()),
                signature_status: signature_status.clone(),
                sbom_reference: nested_text(workload, "sbom", "reference")
                    .unwrap_or_else(|| "missing".to_owned()),
                provenance_reference: nested_text(workload, "provenance", "reference")
                    .unwrap_or_else(|| "missing".to_owned()),
                provenance_subject_matches: trust
                    .and_then(|item| item.get("provenanceSubjectMatches"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            }
        })
        .collect::<Vec<_>>();
    supply_chain.sort_by(|left, right| left.workload_id.cmp(&right.workload_id));
    let policy = policy_artifact.map(|artifact| DeliveryConsolePolicy {
        evidence_id: text(artifact, "evidenceId").unwrap_or_else(|| "unknown".to_owned()),
        pack_id: text(artifact, "packId").unwrap_or_else(|| "unknown".to_owned()),
        decision: text(artifact, "decision").unwrap_or_else(|| "unknown".to_owned()),
    });
    let desired_revision_id = config_artifact.and_then(|artifact| text(artifact, "revisionId"));
    let active_revision_id =
        config_receipt.and_then(|artifact| match text(artifact, "activation").as_deref() {
            Some("active" | "rolled_back") => text(artifact, "targetRevisionId"),
            Some("staged") => text(artifact, "previousRevisionId"),
            _ => None,
        });
    let previous_revision_id =
        config_receipt.and_then(|artifact| text(artifact, "previousRevisionId"));
    let mut secret_references = config_artifact
        .map(|artifact| array(artifact, "secretReferences"))
        .unwrap_or_default()
        .into_iter()
        .map(|reference| DeliveryConsoleSecretReference {
            reference_id: text(reference, "referenceId").unwrap_or_else(|| "unknown".to_owned()),
            provider: text(reference, "provider").unwrap_or_else(|| "unknown".to_owned()),
            purpose: text(reference, "purpose").unwrap_or_else(|| "unknown".to_owned()),
            scope: text(reference, "scope").unwrap_or_else(|| "unknown".to_owned()),
            status: text(reference, "status").unwrap_or_else(|| "unknown".to_owned()),
            rotation_revision: reference
                .get("metadata")
                .and_then(|metadata| text(metadata, "rotationRevision")),
        })
        .collect::<Vec<_>>();
    secret_references.sort_by(|left, right| left.reference_id.cmp(&right.reference_id));
    let mut latest_deployments = std::collections::BTreeMap::new();
    for artifact in input
        .artifacts
        .iter()
        .filter(|artifact| protocol(artifact) == "lenso.deployment-observation.v1")
    {
        let deployment = DeliveryConsoleDeployment {
            environment: text(artifact, "environment").unwrap_or_else(|| "unknown".to_owned()),
            desired_release_id: text(artifact, "desiredReleaseId")
                .unwrap_or_else(|| "unknown".to_owned()),
            observed_release_id: text(artifact, "observedReleaseId")
                .unwrap_or_else(|| "unknown".to_owned()),
            config_revision_id: text(artifact, "configRevisionId")
                .unwrap_or_else(|| "unknown".to_owned()),
            drifted: boolean(artifact, "drifted"),
            fresh: boolean(artifact, "fresh"),
        };
        latest_deployments.insert(deployment.environment.clone(), deployment);
    }
    let mut deployments = latest_deployments.into_values().collect::<Vec<_>>();
    deployments.sort_by(|left, right| left.environment.cmp(&right.environment));
    let adapter_drift = deployments
        .iter()
        .map(|deployment| DeliveryConsoleAdapterDrift {
            environment: deployment.environment.clone(),
            drifted: deployment.drifted,
            fresh: deployment.fresh,
            next_actions: if deployment.drifted || !deployment.fresh {
                vec![
                    "Refresh adapter observations and reconcile the exact Deployment plan."
                        .to_owned(),
                ]
            } else {
                Vec::new()
            },
        })
        .collect::<Vec<_>>();
    let edge = latest(&input.artifacts, "lenso.edge-contract.v1").map(|artifact| {
        let mut public_routes = array(artifact, "routes")
            .into_iter()
            .filter_map(|route| text(route, "publicPath"))
            .collect::<Vec<_>>();
        public_routes.sort();
        DeliveryConsoleEdge {
            contract_id: text(artifact, "edgeContractId").unwrap_or_else(|| "unknown".to_owned()),
            public_routes,
        }
    });
    let mut timeline = input
        .artifacts
        .iter()
        .filter(|artifact| is_timeline_protocol(protocol(artifact)))
        .map(timeline_entry)
        .collect::<Vec<_>>();
    let mut seen_timeline = BTreeSet::new();
    timeline.retain(|entry| seen_timeline.insert(entry.clone()));
    let promotion_history = filter_timeline(&timeline, "promotion");
    let canary_timeline = filter_timeline(&timeline, "canary");
    let canary_observations = input
        .artifacts
        .iter()
        .filter(|artifact| protocol(artifact) == "lenso.reliability-observation.v1")
        .filter_map(canary_observation)
        .collect::<Vec<_>>();
    let rollback_timeline = filter_timeline(&timeline, "rollback");
    let mut issues = input
        .artifacts
        .iter()
        .flat_map(issues_from)
        .collect::<Vec<_>>();
    issues.sort();
    issues.dedup();
    let mut next_actions = issues
        .iter()
        .flat_map(|issue| issue.next_actions.iter().cloned())
        .collect::<Vec<_>>();
    next_actions.sort();
    next_actions.dedup();
    let mut runtime_story_references = BTreeSet::new();
    for artifact in &input.artifacts {
        collect_runtime_story_references(artifact, &mut runtime_story_references);
    }
    let configuration = DeliveryConsoleConfiguration {
        drifted: desired_revision_id != active_revision_id
            || deployments.iter().any(|deployment| {
                desired_revision_id
                    .as_deref()
                    .is_some_and(|desired| deployment.config_revision_id != desired)
            }),
        desired_revision_id,
        active_revision_id,
        previous_revision_id,
        secret_references,
    };
    let state = derive_state(&input.artifacts, !issues.is_empty(), &deployments);
    let ga_operations = ga_operations(&input.artifacts);
    let mut projection = DeliveryConsoleProjection {
        protocol: DELIVERY_CONSOLE_PROJECTION_PROTOCOL.to_owned(),
        projection_digest: String::new(),
        state,
        release,
        supply_chain,
        policy,
        configuration,
        deployments,
        edge,
        adapter_drift,
        promotion_history,
        canary_timeline,
        canary_observations,
        rollback_timeline,
        issues,
        next_actions,
        runtime_story_references: runtime_story_references.into_iter().collect(),
        ga_operations,
        read_only: true,
        apply_actions: Vec::new(),
    };
    projection.projection_digest = digest(&projection);
    projection
}

fn ga_operations(artifacts: &[Value]) -> DeliveryConsoleGaOperations {
    let summaries = |protocol_name: &str| {
        artifacts
            .iter()
            .filter(|artifact| protocol(artifact) == protocol_name)
            .filter_map(ga_evidence)
            .collect::<Vec<_>>()
    };
    let newest = |protocol_name: &str| summaries(protocol_name).pop();
    let mut contract_lifecycle = summaries("lenso.contract-retirement-plan.v1");
    contract_lifecycle.extend(summaries("lenso.contract-retirement-receipt.v2"));
    DeliveryConsoleGaOperations {
        support_manifest: newest("lenso.ga-support-manifest.v1"),
        delivery_recovery: summaries("lenso.delivery-failure-recovery-evidence.v1"),
        restore: newest("lenso.service-restore-evidence.v1"),
        disaster_recovery: newest("lenso.disaster-recovery-evidence.v1"),
        performance: newest("lenso.performance-profile.v1"),
        support_envelope: newest("lenso.support-envelope.v1"),
        security_review: newest("lenso.security-review-evidence.v1"),
        contract_lifecycle,
    }
}

fn ga_evidence(artifact: &Value) -> Option<DeliveryConsoleGaEvidence> {
    let protocol = text(artifact, "protocol")?;
    let evidence_id = [
        "evidenceId",
        "profileId",
        "envelopeId",
        "reviewId",
        "manifestId",
        "planId",
        "receiptId",
    ]
    .into_iter()
    .find_map(|key| text(artifact, key))
    .unwrap_or_else(|| "unknown".to_owned());
    let status = ["decision", "status", "outcome"]
        .into_iter()
        .find_map(|key| text(artifact, key))
        .unwrap_or_else(|| "unknown".to_owned());
    let issue_codes = array(artifact, "issues")
        .into_iter()
        .filter_map(|issue| text(issue, "code"))
        .collect::<Vec<_>>();
    let stale = issue_codes
        .iter()
        .any(|code| code.contains("stale") || code.contains("freshness"));
    let subjects = [
        "serviceId",
        "workloadId",
        "releaseId",
        "releaseDigest",
        "configRevisionId",
        "configRevisionDigest",
        "contractId",
        "contractSetDigest",
        "deploymentId",
        "storyId",
        "backupId",
        "supportManifestDigest",
        "primaryRegion",
        "passiveRegion",
        "phase",
        "observedRpoMs",
        "observedRtoMs",
        "recoveryTimeMs",
        "intentionalLossBoundMs",
        "replayBoundCount",
        "freshnessHorizonUnixMs",
        "upgradeStatus",
        "rollbackStatus",
    ]
    .into_iter()
    .filter_map(|key| scalar_text(artifact, key).map(|value| (key.to_owned(), value)))
    .chain(
        [
            ("activeConsumerCount", "activeConsumers"),
            ("performanceBudgetCount", "budgets"),
            ("findingCount", "findings"),
            ("contractVersionCount", "contractVersionDigests"),
            ("remainingStoryGapCount", "remainingStoryGaps"),
        ]
        .into_iter()
        .filter_map(|(label, key)| {
            artifact
                .get(key)
                .and_then(|value| match value {
                    Value::Array(values) => Some(values.len()),
                    Value::Object(values) => Some(values.len()),
                    _ => None,
                })
                .map(|count| (label.to_owned(), count.to_string()))
        }),
    )
    .collect();
    let details = [
        "components",
        "manifestFormats",
        "stateVersions",
        "adapterVersions",
        "combinations",
        "upgradeEdges",
        "activeConsumers",
        "budgets",
        "measurements",
        "findings",
        "contractVersionDigests",
        "remainingStoryGaps",
        "reconciliation",
        "environmentObservation",
    ]
    .into_iter()
    .filter_map(|key| {
        artifact
            .get(key)
            .filter(|value| !value.is_null())
            .cloned()
            .map(|value| (key.to_owned(), value))
    })
    .collect();
    Some(DeliveryConsoleGaEvidence {
        protocol,
        evidence_id,
        status,
        stale,
        subjects,
        details,
        issue_codes,
        next_actions: strings(artifact, "nextActions"),
    })
}

fn scalar_text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(|value| match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    })
}

fn canary_observation(artifact: &Value) -> Option<DeliveryConsoleCanaryObservation> {
    Some(DeliveryConsoleCanaryObservation {
        observation_id: text(artifact, "observationId")?,
        observed_revision: artifact.get("observedRevision")?.as_u64()?,
        fresh: boolean(artifact, "fresh"),
        observation_window_seconds: artifact.get("observationWindowSeconds")?.as_u64()?,
        sample_count: artifact.get("sampleCount")?.as_u64()?,
        generic_process_healthy: boolean(artifact, "genericProcessHealthy"),
        workload_readiness: bool_map(artifact, "workloadReadiness"),
        workload_liveness: bool_map(artifact, "workloadLiveness"),
        availability_basis_points: artifact
            .get("availabilityBasisPoints")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok()),
        latency_p99_ms: artifact.get("latencyP99Ms").and_then(Value::as_u64),
        error_budget_used_basis_points: artifact
            .get("errorBudgetUsedBasisPoints")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok()),
        queue_backlog: artifact.get("queueBacklog").and_then(Value::as_u64),
        workflow_backlog: artifact.get("workflowBacklog").and_then(Value::as_u64),
        timer_lag_ms: artifact.get("timerLagMs").and_then(Value::as_u64),
        retry_exhaustion: artifact.get("retryExhaustion").and_then(Value::as_u64),
        compensation_pressure: artifact.get("compensationPressure").and_then(Value::as_u64),
        dependencies: array(artifact, "dependencies")
            .into_iter()
            .filter_map(|dependency| {
                Some(DeliveryConsoleDependencyObservation {
                    dependency_id: text(dependency, "dependencyId")?,
                    available: boolean(dependency, "available"),
                    active_degraded_mode: text(dependency, "activeDegradedMode"),
                })
            })
            .collect(),
        failure_domains: bool_map(artifact, "failureDomains"),
        scaling_check_passed: artifact.get("scalingCheckPassed").and_then(Value::as_bool),
        disruption_check_passed: artifact
            .get("disruptionCheckPassed")
            .and_then(Value::as_bool),
        availability_check_passed: artifact
            .get("availabilityCheckPassed")
            .and_then(Value::as_bool),
        evidence_references: array(artifact, "evidenceReferences")
            .into_iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
    })
}

fn bool_map(artifact: &Value, field: &str) -> BTreeMap<String, bool> {
    artifact
        .get(field)
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(key, value)| value.as_bool().map(|value| (key.clone(), value)))
        .collect()
}

/// Persist one immutable production-delivery artifact for the read-only projection.
pub async fn record_delivery_artifact(
    pool: &sqlx::PgPool,
    delivery_id: &str,
    artifact: &Value,
) -> Result<(), sqlx::Error> {
    let persisted = persisted_delivery_artifact(artifact)?;
    sqlx::query(
        r#"
        insert into platform.delivery_artifacts
            (delivery_id, artifact_id, protocol, artifact_digest, artifact_json)
        values ($1, $2, $3, $4, $5)
        on conflict (delivery_id, artifact_id, artifact_digest) do nothing
        "#,
    )
    .bind(delivery_id)
    .bind(artifact_id(&persisted))
    .bind(protocol(&persisted))
    .bind(digest(artifact))
    .bind(&persisted)
    .execute(pool)
    .await?;
    Ok(())
}

/// Compute the canonical subject signed by the provider that records a delivery batch.
#[must_use]
pub fn delivery_artifact_batch_subject(delivery_id: &str, artifacts: &[Value]) -> String {
    extraction_input_digest(
        serde_json::to_vec(&(DELIVERY_ARTIFACT_BATCH_PROTOCOL, delivery_id, artifacts))
            .expect("delivery artifact batches must serialize"),
    )
}

/// Validate and atomically persist a signed production-delivery artifact batch.
pub async fn record_delivery_artifacts(
    pool: &sqlx::PgPool,
    delivery_id: &str,
    artifacts: &[Value],
) -> Result<(), sqlx::Error> {
    let persisted = artifacts
        .iter()
        .map(persisted_delivery_artifact)
        .collect::<Result<Vec<_>, _>>()?;
    let mut transaction = pool.begin().await?;
    for (artifact, persisted) in artifacts.iter().zip(&persisted) {
        sqlx::query(
            r#"
            insert into platform.delivery_artifacts
                (delivery_id, artifact_id, protocol, artifact_digest, artifact_json)
            values ($1, $2, $3, $4, $5)
            on conflict (delivery_id, artifact_id, artifact_digest) do nothing
            "#,
        )
        .bind(delivery_id)
        .bind(artifact_id(persisted))
        .bind(protocol(persisted))
        .bind(digest(artifact))
        .bind(persisted)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(())
}

fn persisted_delivery_artifact(artifact: &Value) -> Result<Value, sqlx::Error> {
    let protocol = protocol(artifact);
    if !artifact.is_object() {
        return Err(sqlx::Error::Protocol(
            "delivery artifact must be an identified Lenso protocol object".to_owned(),
        ));
    }
    let persisted = match protocol {
        SERVICE_RELEASE_PROTOCOL => validated_canonical_artifact::<ServiceRelease>(
            artifact,
            "Service Release",
            service_release_integrity_is_valid,
        )?,
        RELEASE_TRUST_EVIDENCE_PROTOCOL => {
            canonical_artifact::<ReleaseTrustEvidence>(artifact, "Release Trust Evidence")?
        }
        POLICY_EVIDENCE_PROTOCOL => validated_canonical_artifact::<PolicyEvidence>(
            artifact,
            "Policy Evidence",
            policy_evidence_integrity_is_valid,
        )?,
        CONFIG_REVISION_PROTOCOL => {
            let revision = canonical_typed_artifact::<ConfigRevision>(artifact, "Config Revision")?;
            if !config_revision_integrity_is_valid(&revision)
                || !revision
                    .secret_references
                    .iter()
                    .all(secret_reference_metadata_is_safe)
            {
                return Err(sqlx::Error::Protocol(
                    "Config Revision identity, digest, or Secret Reference metadata is invalid"
                        .to_owned(),
                ));
            }
            let mut value = serde_json::to_value(revision).expect("Config Revision serializes");
            let object = value.as_object_mut().expect("Config Revision is an object");
            object.insert("values".to_owned(), Value::Object(serde_json::Map::new()));
            object.insert("valuesRedacted".to_owned(), Value::Bool(true));
            value
        }
        CONFIG_ACTIVATION_RECEIPT_PROTOCOL => {
            canonical_artifact::<ConfigActivationReceipt>(artifact, "Config Activation Receipt")?
        }
        EDGE_CONTRACT_PROTOCOL => validated_canonical_artifact::<EdgeContract>(
            artifact,
            "Edge Contract",
            edge_contract_integrity_is_valid,
        )?,
        GATEWAY_PLAN_PROTOCOL => validated_canonical_artifact::<GatewayConfigurationPlan>(
            artifact,
            "Gateway Configuration Plan",
            gateway_plan_integrity_is_valid,
        )?,
        GATEWAY_OBSERVATION_PROTOCOL => validated_canonical_artifact::<GatewayObservation>(
            artifact,
            "Gateway Observation",
            gateway_observation_content_integrity_is_valid,
        )?,
        DEPLOYMENT_PLAN_PROTOCOL => validated_canonical_artifact::<DeploymentPlan>(
            artifact,
            "Deployment Plan",
            deployment_plan_integrity_is_valid,
        )?,
        DEPLOYMENT_RECEIPT_PROTOCOL => {
            canonical_artifact::<DeploymentReceipt>(artifact, "Deployment Receipt")?
        }
        DEPLOYMENT_OBSERVATION_PROTOCOL => validated_canonical_artifact::<DeploymentObservation>(
            artifact,
            "Deployment Observation",
            deployment_observation_content_integrity_is_valid,
        )?,
        ENVIRONMENT_VERIFICATION_PROTOCOL => {
            validated_canonical_artifact::<EnvironmentVerification>(
                artifact,
                "Environment Verification",
                environment_verification_integrity_is_valid,
            )?
        }
        PROMOTION_PLAN_PROTOCOL => validated_canonical_artifact::<PromotionPlan>(
            artifact,
            "Promotion Plan",
            promotion_plan_integrity_is_valid,
        )?,
        PROMOTION_APPROVAL_PROTOCOL => {
            canonical_artifact::<PromotionApproval>(artifact, "Promotion Approval")?
        }
        PROMOTION_RECEIPT_PROTOCOL => {
            canonical_artifact::<PromotionReceipt>(artifact, "Promotion Receipt")?
        }
        CANARY_PLAN_PROTOCOL => validated_canonical_artifact::<CanaryPlan>(
            artifact,
            "Canary Plan",
            canary_plan_integrity_is_valid,
        )?,
        RELIABILITY_OBSERVATION_PROTOCOL => {
            canonical_artifact::<ReliabilityObservation>(artifact, "Reliability Observation")?
        }
        CANARY_DECISION_PROTOCOL => validated_canonical_artifact::<CanaryDecision>(
            artifact,
            "Canary Decision",
            canary_decision_integrity_is_valid,
        )?,
        ROLLBACK_PLAN_PROTOCOL => validated_canonical_artifact::<RollbackPlan>(
            artifact,
            "Rollback Plan",
            rollback_plan_integrity_is_valid,
        )?,
        ROLLBACK_RECEIPT_PROTOCOL => {
            canonical_artifact::<RollbackReceipt>(artifact, "Rollback Receipt")?
        }
        DELIVERY_FAILURE_RECOVERY_PROTOCOL => {
            validated_canonical_artifact::<DeliveryFailureRecoveryEvidence>(
                artifact,
                "Delivery Failure Recovery Evidence",
                delivery_failure_recovery_integrity_is_valid,
            )?
        }
        SERVICE_RESTORE_EVIDENCE_PROTOCOL => {
            validated_canonical_artifact::<ServiceRestoreEvidence>(
                artifact,
                "Service Restore Evidence",
                service_restore_integrity_is_valid,
            )?
        }
        DISASTER_RECOVERY_EVIDENCE_PROTOCOL => {
            validated_canonical_artifact::<DisasterRecoveryEvidence>(
                artifact,
                "Disaster Recovery Evidence",
                disaster_recovery_evidence_integrity_is_valid,
            )?
        }
        PERFORMANCE_PROFILE_PROTOCOL => validated_canonical_artifact::<PerformanceProfile>(
            artifact,
            "Performance Profile",
            performance_profile_integrity_is_valid,
        )?,
        SUPPORT_ENVELOPE_PROTOCOL => validated_canonical_artifact::<SupportEnvelope>(
            artifact,
            "Support Envelope",
            support_envelope_integrity_is_valid,
        )?,
        SECURITY_REVIEW_PROTOCOL => validated_canonical_artifact::<SecurityReviewEvidence>(
            artifact,
            "Security Review Evidence",
            security_review_integrity_is_valid,
        )?,
        GA_SUPPORT_MANIFEST_PROTOCOL => validated_canonical_artifact::<GaSupportManifest>(
            artifact,
            "GA Support Manifest",
            ga_support_manifest_integrity_valid,
        )?,
        CONTRACT_RETIREMENT_PLAN_PROTOCOL => {
            validated_canonical_artifact::<ContractRetirementPlan>(
                artifact,
                "Contract Retirement Plan",
                contract_retirement_plan_integrity_is_valid,
            )?
        }
        CONTRACT_RETIREMENT_RECEIPT_PROTOCOL => {
            validated_canonical_artifact::<ContractRetirementReceipt>(
                artifact,
                "Contract Retirement Receipt",
                contract_retirement_receipt_integrity_is_valid,
            )?
        }
        _ => {
            return Err(sqlx::Error::Protocol(format!(
                "unsupported production delivery artifact protocol `{protocol}`"
            )));
        }
    };
    if delivery_artifact_contains_secret_shaped_field(&persisted) {
        return Err(sqlx::Error::Protocol(
            "delivery artifact contains a forbidden secret-shaped field".to_owned(),
        ));
    }
    Ok(persisted)
}

fn canonical_typed_artifact<T: DeserializeOwned + Serialize>(
    artifact: &Value,
    label: &str,
) -> Result<T, sqlx::Error> {
    let typed = serde_json::from_value::<T>(artifact.clone())
        .map_err(|error| sqlx::Error::Protocol(format!("invalid {label} artifact: {error}")))?;
    let canonical = serde_json::to_value(&typed).expect("typed delivery artifact serializes");
    if canonical != *artifact {
        return Err(sqlx::Error::Protocol(format!(
            "{label} artifact is non-canonical or contains unknown fields"
        )));
    }
    Ok(typed)
}

fn canonical_artifact<T: DeserializeOwned + Serialize>(
    artifact: &Value,
    label: &str,
) -> Result<Value, sqlx::Error> {
    canonical_typed_artifact::<T>(artifact, label)?;
    Ok(artifact.clone())
}

fn validated_canonical_artifact<T: DeserializeOwned + Serialize>(
    artifact: &Value,
    label: &str,
    is_valid: impl FnOnce(&T) -> bool,
) -> Result<Value, sqlx::Error> {
    let typed = canonical_typed_artifact::<T>(artifact, label)?;
    if !is_valid(&typed) {
        return Err(sqlx::Error::Protocol(format!(
            "{label} identity or content digest is invalid"
        )));
    }
    Ok(artifact.clone())
}

fn delivery_artifact_contains_secret_shaped_field(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, value)| {
            let key = key.to_ascii_lowercase();
            [
                "secretvalue",
                "passwordvalue",
                "credential",
                "privatekey",
                "signingkey",
                "accesstoken",
                "token",
            ]
            .iter()
            .any(|forbidden| key.contains(forbidden))
                || delivery_artifact_contains_secret_shaped_field(value)
        }),
        Value::Array(values) => values
            .iter()
            .any(delivery_artifact_contains_secret_shaped_field),
        _ => false,
    }
}

/// Load the newest persisted delivery evidence and evaluate its read-only projection.
pub async fn load_delivery_console_projection(
    pool: &sqlx::PgPool,
    requested_delivery_id: Option<&str>,
) -> Result<DeliveryConsoleProjection, sqlx::Error> {
    let exists = sqlx::query_scalar::<_, Option<String>>(
        "select to_regclass('platform.delivery_artifacts')::text",
    )
    .fetch_one(pool)
    .await?
    .is_some();
    if !exists {
        return Ok(project_delivery_console(DeliveryConsoleArtifacts {
            artifacts: Vec::new(),
        }));
    }
    let delivery_id = match requested_delivery_id {
        Some(delivery_id) => Some(delivery_id.to_owned()),
        None => sqlx::query_scalar::<_, String>(
            "select delivery_id from platform.delivery_artifacts order by recorded_at desc, record_index desc, delivery_id desc limit 1",
        )
        .fetch_optional(pool)
        .await?,
    };
    let Some(delivery_id) = delivery_id else {
        return Ok(project_delivery_console(DeliveryConsoleArtifacts {
            artifacts: Vec::new(),
        }));
    };
    let artifacts = sqlx::query_scalar::<_, Value>(
        "select artifact_json from platform.delivery_artifacts where delivery_id = $1 order by record_index",
    )
    .bind(delivery_id)
    .fetch_all(pool)
    .await?;
    Ok(project_delivery_console(DeliveryConsoleArtifacts {
        artifacts,
    }))
}

fn derive_state(
    artifacts: &[Value],
    blocked: bool,
    deployments: &[DeliveryConsoleDeployment],
) -> DeliveryConsoleState {
    if let Some(explicit) = latest(artifacts, "lenso.delivery-state.v1")
        .and_then(|artifact| text(artifact, "state"))
        .and_then(|state| serde_json::from_value(Value::String(state)).ok())
    {
        return explicit;
    }
    let converged = !deployments.is_empty()
        && deployments
            .iter()
            .all(|deployment| deployment.fresh && !deployment.drifted);
    let latest_lifecycle = artifacts.iter().rev().find(|artifact| {
        matches!(
            protocol(artifact),
            "lenso.rollback-receipt.v1"
                | "lenso.rollback-plan.v1"
                | "lenso.canary-decision.v1"
                | "lenso.promotion-receipt.v1"
                | "lenso.promotion-approval.v1"
                | "lenso.environment-verification.v1"
        )
    });
    if let Some(artifact) = latest_lifecycle {
        match protocol(artifact) {
            "lenso.rollback-receipt.v1" => {
                if text(artifact, "outcome").as_deref() == Some("intervention_required") {
                    DeliveryConsoleState::InterventionRequired
                } else {
                    DeliveryConsoleState::RolledBack
                }
            }
            "lenso.rollback-plan.v1" if !boolean(artifact, "automaticAllowed") => {
                DeliveryConsoleState::Paused
            }
            "lenso.rollback-plan.v1" => DeliveryConsoleState::RollingBack,
            "lenso.canary-decision.v1" => match text(artifact, "outcome").as_deref() {
                Some("rollback") => DeliveryConsoleState::RollingBack,
                Some("pause") => DeliveryConsoleState::Paused,
                Some("expand" | "hold_degraded") => DeliveryConsoleState::Canary,
                Some("converged") => DeliveryConsoleState::Ready,
                _ => DeliveryConsoleState::Blocked,
            },
            "lenso.promotion-receipt.v1" => {
                if converged {
                    DeliveryConsoleState::Ready
                } else {
                    DeliveryConsoleState::Converging
                }
            }
            "lenso.promotion-approval.v1" => DeliveryConsoleState::Approved,
            "lenso.environment-verification.v1"
                if text(artifact, "decision").as_deref() == Some("passed") =>
            {
                DeliveryConsoleState::Staged
            }
            _ => DeliveryConsoleState::Blocked,
        }
    } else if converged {
        DeliveryConsoleState::Ready
    } else if blocked {
        DeliveryConsoleState::Blocked
    } else {
        DeliveryConsoleState::Planned
    }
}

fn issues_from(artifact: &Value) -> Vec<DeliveryConsoleIssue> {
    ["issues", "remainingRisks"]
        .into_iter()
        .flat_map(|field| array(artifact, field))
        .map(|value| DeliveryConsoleIssue {
            code: text(value, "code").unwrap_or_else(|| "unknown".to_owned()),
            message: text(value, "message").unwrap_or_else(|| "Unknown delivery issue.".to_owned()),
            evidence_references: strings(value, "evidenceReferences"),
            remediation: text(value, "remediation")
                .unwrap_or_else(|| "Inspect the linked delivery evidence.".to_owned()),
            next_actions: strings(value, "nextActions"),
        })
        .collect()
}

fn timeline_entry(artifact: &Value) -> DeliveryConsoleTimelineEntry {
    let protocol = protocol(artifact).to_owned();
    let state = text(artifact, "outcome")
        .or_else(|| text(artifact, "decision"))
        .or_else(|| text(artifact, "activation"))
        .unwrap_or_else(|| "recorded".to_owned());
    let mut evidence_references = strings(artifact, "evidenceReferences");
    evidence_references.extend(
        issues_from(artifact)
            .into_iter()
            .flat_map(|issue| issue.evidence_references),
    );
    evidence_references.sort();
    evidence_references.dedup();
    DeliveryConsoleTimelineEntry {
        protocol,
        artifact_id: artifact_id(artifact),
        state,
        evidence_references,
    }
}

fn filter_timeline(
    timeline: &[DeliveryConsoleTimelineEntry],
    needle: &str,
) -> Vec<DeliveryConsoleTimelineEntry> {
    timeline
        .iter()
        .filter(|entry| entry.protocol.contains(needle))
        .cloned()
        .collect()
}

fn is_timeline_protocol(protocol: &str) -> bool {
    ["promotion", "canary", "rollback", "config-activation"]
        .iter()
        .any(|needle| protocol.contains(needle))
}

fn collect_runtime_story_references(value: &Value, found: &mut BTreeSet<String>) {
    match value {
        Value::String(value) if value.starts_with("runtime-story:") => {
            found.insert(value.clone());
        }
        Value::Array(values) => {
            for value in values {
                collect_runtime_story_references(value, found);
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                collect_runtime_story_references(value, found);
            }
        }
        _ => {}
    }
}

fn latest<'a>(artifacts: &'a [Value], expected_protocol: &str) -> Option<&'a Value> {
    artifacts
        .iter()
        .rev()
        .find(|artifact| protocol(artifact) == expected_protocol)
}

fn protocol(value: &Value) -> &str {
    value
        .get("protocol")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
}

fn artifact_id(value: &Value) -> String {
    [
        "releaseId",
        "receiptId",
        "decisionId",
        "evidenceId",
        "verificationId",
        "planId",
        "revisionId",
        "contractId",
        "observationId",
        "proofId",
    ]
    .into_iter()
    .find_map(|field| text(value, field))
    .unwrap_or_else(|| format!("artifact:{}", digest(value)))
}

fn text(value: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(Value::as_str).map(str::to_owned)
}

fn nested_text(value: &Value, parent: &str, field: &str) -> Option<String> {
    value.get(parent).and_then(|value| text(value, field))
}

fn boolean(value: &Value, field: &str) -> bool {
    value.get(field).and_then(Value::as_bool).unwrap_or(false)
}

fn array<'a>(value: &'a Value, field: &str) -> Vec<&'a Value> {
    value
        .get(field)
        .and_then(Value::as_array)
        .map(|values| values.iter().collect())
        .unwrap_or_default()
}

fn strings(value: &Value, field: &str) -> Vec<String> {
    array(value, field)
        .into_iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn digest(value: &impl Serialize) -> String {
    extraction_input_digest(
        serde_json::to_vec(value).expect("delivery projection values serialize"),
    )
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn persistence_rejects_unknown_delivery_protocols() {
        let error = persisted_delivery_artifact(&serde_json::json!({
            "protocol": "lenso.forged-delivery-object.v1",
            "receiptId": "forged:1"
        }))
        .expect_err("unknown delivery objects must fail closed");

        assert!(error.to_string().contains("unsupported"));
    }

    #[test]
    fn persistence_rejects_extra_token_fields_on_typed_receipts() {
        let receipt = crate::RollbackReceipt {
            protocol: crate::ROLLBACK_RECEIPT_PROTOCOL.to_owned(),
            receipt_id: "rollback-receipt:test".to_owned(),
            plan_id: "rollback-plan:test".to_owned(),
            actor: "automation:test".to_owned(),
            outcome: crate::RollbackOutcome::RolledBack,
            restored_release_id: "release:previous".to_owned(),
            restored_config_revision_id: "config:previous".to_owned(),
            environment_revision_before: 7,
            environment_revision_after: 8,
            exposure_percent: 0,
            remaining_risks: Vec::new(),
            approval_boundary_required: false,
            evidence_references: Vec::new(),
            effects: crate::DeliveryEffects {
                mutates_environment: true,
                mutates_configuration: true,
                mutates_gateway: true,
                mutates_deployment: true,
                appends_ledger: true,
            },
        };
        let mut artifact = serde_json::to_value(receipt).expect("receipt serializes");
        artifact
            .as_object_mut()
            .expect("receipt is an object")
            .insert(
                "metadata".to_owned(),
                serde_json::json!({"token": "forged"}),
            );

        let error = persisted_delivery_artifact(&artifact)
            .expect_err("unknown token-bearing receipt fields must fail closed");
        assert!(error.to_string().contains("non-canonical"));
    }

    #[test]
    fn persistence_accepts_integrity_valid_ga_support_and_rejects_tampering() {
        let manifest = crate::assemble_ga_support_manifest_with_trust(
            crate::GaSupportManifestInput {
                status: crate::SupportStatus::Candidate,
                components: vec![crate::GaComponent {
                    kind: crate::ComponentKind::Runtime,
                    component_id: "lenso-service".to_owned(),
                    version: "0.1.14".to_owned(),
                    digest: crate::extraction_input_digest(b"runtime"),
                }],
                manifest_formats: vec![crate::ManifestFormat {
                    kind: crate::ManifestKind::Service,
                    version: "lenso.service.v2".to_owned(),
                }],
                state_versions: vec!["service-store.v1".to_owned()],
                adapter_versions: BTreeMap::from([("postgresql".to_owned(), "18".to_owned())]),
                documentation: crate::DocumentationIdentity {
                    version: "m6-ga".to_owned(),
                    digest: crate::extraction_input_digest(b"docs"),
                },
                combinations: vec![crate::SupportCombinationInput {
                    combination_id: "candidate".to_owned(),
                    component_references: vec!["runtime:lenso-service@0.1.14".to_owned()],
                    state_version: "service-store.v1".to_owned(),
                    status: crate::SupportStatus::Candidate,
                }],
                upgrade_edges: Vec::new(),
            },
            crate::EvidenceReceiptTrust {
                authorities: BTreeMap::from([(
                    crate::PERFORMANCE_PROFILE_PROTOCOL.to_owned(),
                    "test-authority".to_owned(),
                )]),
                public_keys: BTreeMap::from([(
                    "test-authority".to_owned(),
                    "-----BEGIN PUBLIC KEY-----\ntest\n-----END PUBLIC KEY-----".to_owned(),
                )]),
            },
        )
        .expect("manifest is valid");
        let artifact = serde_json::to_value(&manifest).expect("manifest serializes");
        assert_eq!(
            persisted_delivery_artifact(&artifact).expect("manifest persists"),
            artifact
        );

        let mut tampered = artifact;
        tampered["status"] = serde_json::json!("general_availability");
        persisted_delivery_artifact(&tampered).expect_err("tampered manifest fails");
    }
}
