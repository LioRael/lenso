use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(CustomResource, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[kube(
    group = "lenso.dev",
    version = "v1alpha1",
    kind = "LensoServiceProvider",
    plural = "lensoserviceproviders",
    namespaced,
    status = "LensoServiceProviderStatus",
    printcolumn = r#"{"name":"State","type":"string","jsonPath":".status.state"}"#,
    printcolumn = r#"{"name":"Release","type":"string","jsonPath":".status.observedReleaseId"}"#,
    printcolumn = r#"{"name":"Image","type":"string","jsonPath":".status.observedImage"}"#,
    printcolumn = r#"{"name":"Ready","type":"integer","jsonPath":".status.readyReplicas"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct LensoServiceProviderSpec {
    pub service_name: String,
    pub environment: String,
    pub image: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_reference: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<String>,
    #[serde(default = "default_replicas")]
    pub replicas: i32,
    pub port: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_from: Option<LensoServiceProviderEnvFrom>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingress: Option<LensoServiceProviderIngress>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autoscaling: Option<LensoServiceProviderAutoscaling>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disruption_budget: Option<LensoServiceProviderDisruptionBudget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_policy: Option<LensoServiceProviderNetworkPolicy>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LensoServiceProviderEnvFrom {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_map: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LensoServiceProviderIngress {
    pub host: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LensoServiceProviderAutoscaling {
    pub enabled: bool,
    #[serde(default = "default_replicas")]
    pub min_replicas: i32,
    #[serde(default = "default_max_replicas")]
    pub max_replicas: i32,
    #[serde(default = "default_target_cpu_utilization")]
    pub target_cpu_utilization: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LensoServiceProviderDisruptionBudget {
    pub enabled: bool,
    #[serde(default = "default_replicas")]
    pub min_available: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LensoServiceProviderNetworkPolicy {
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LensoServiceProviderState {
    Ready,
    Progressing,
    Failed,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LensoServiceProviderStatus {
    pub state: LensoServiceProviderState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_generation: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_release_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready_replicas: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desired_replicas: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_replicas: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_reference: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<LensoServiceProviderCondition>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LensoServiceProviderCondition {
    #[serde(rename = "type")]
    pub type_: String,
    pub status: String,
    pub reason: String,
    pub message: String,
    pub last_transition_time: Time,
}

const fn default_replicas() -> i32 {
    1
}

const fn default_max_replicas() -> i32 {
    3
}

const fn default_target_cpu_utilization() -> i32 {
    70
}
