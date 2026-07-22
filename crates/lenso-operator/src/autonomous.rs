use std::collections::BTreeMap;

use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(CustomResource, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[kube(
    group = "lenso.dev",
    version = "v1alpha1",
    kind = "LensoAutonomousService",
    plural = "lensoautonomousservices",
    namespaced,
    status = "LensoAutonomousServiceStatus",
    printcolumn = r#"{"name":"State","type":"string","jsonPath":".status.state"}"#,
    printcolumn = r#"{"name":"Release","type":"string","jsonPath":".status.observedReleaseId"}"#,
    printcolumn = r#"{"name":"Config","type":"string","jsonPath":".status.configRevisionId"}"#,
    printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.rolloutPhase"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct LensoAutonomousServiceSpec {
    pub service_id: String,
    pub environment: String,
    pub release_id: String,
    pub release_digest: String,
    pub config_revision_id: String,
    pub expected_environment_revision: u64,
    #[serde(default)]
    pub secret_references: Vec<OperatorSecretReference>,
    #[serde(default)]
    pub policy_evidence_references: Vec<String>,
    #[serde(default)]
    pub evidence_references: Vec<String>,
    pub workloads: Vec<LensoAutonomousWorkload>,
    pub rollout_strategy: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback_release_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OperatorSecretReference {
    pub reference_id: String,
    pub provider: String,
    pub target_name: String,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OperatorWorkloadRole {
    Api,
    Worker,
    Migration,
    Extension,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OperatorPlacement {
    #[serde(default)]
    pub node_selector: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OperatorScaling {
    pub min_replicas: i32,
    pub max_replicas: i32,
    pub target_cpu_utilization: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LensoAutonomousWorkload {
    pub workload_id: String,
    pub role: OperatorWorkloadRole,
    pub image: String,
    pub replicas: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<i32>,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_map_name: Option<String>,
    #[serde(default)]
    pub secret_reference_ids: Vec<String>,
    #[serde(default)]
    pub placement: OperatorPlacement,
    pub scaling: OperatorScaling,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disruption_min_available: Option<i32>,
    pub network_policy_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readiness_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liveness_path: Option<String>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum LensoAutonomousServiceState {
    Pending,
    Migrating,
    Progressing,
    Ready,
    Failed,
    Paused,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OperatorWorkloadStatus {
    pub workload_id: String,
    pub role: OperatorWorkloadRole,
    pub desired_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_digest: Option<String>,
    pub ready: bool,
    pub failed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LensoAutonomousServiceStatus {
    pub state: LensoAutonomousServiceState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_generation: Option<i64>,
    pub observed_release_id: String,
    pub observed_release_digest: String,
    pub config_revision_id: String,
    pub rollout_phase: String,
    #[serde(default)]
    pub policy_evidence_references: Vec<String>,
    #[serde(default)]
    pub evidence_references: Vec<String>,
    #[serde(default)]
    pub workloads: Vec<OperatorWorkloadStatus>,
    pub drifted: bool,
    pub rollback_state: String,
    #[serde(default)]
    pub issues: Vec<OperatorDeliveryIssue>,
    #[serde(default)]
    pub next_actions: Vec<String>,
    #[serde(default)]
    pub conditions: Vec<LensoAutonomousServiceCondition>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OperatorDeliveryIssue {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub evidence_references: Vec<String>,
    pub remediation: String,
    #[serde(default)]
    pub next_actions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LensoAutonomousServiceCondition {
    #[serde(rename = "type")]
    pub type_: String,
    pub status: String,
    pub reason: String,
    pub message: String,
    pub last_transition_time: Time,
}
