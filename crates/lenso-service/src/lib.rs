use axum::{Json, Router, routing::get};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub use lenso_contracts::ModuleManifest;

pub const SERVICE_CONTRACT_PROTOCOL: &str = "lenso.service.v1";
pub const SERVICE_PACKAGE_PROTOCOL: &str = "lenso.service-package.v1";
pub const SERVICE_WORKSPACE_PROTOCOL: &str = "lenso.service-workspace.v1";
pub const SERVICE_RELEASE_PLAN_PROTOCOL: &str = "lenso.service-release-plan.v1";
pub const MODULE_CONTRACT_PROTOCOL: &str = "lenso.module.v1";
pub const MODULE_RELEASE_PROTOCOL: &str = "lenso.module-release.v1";
pub const SERVICE_CONTRACT_SCHEMA_JSON: &str =
    include_str!("../schemas/lenso-service.v1.schema.json");
pub const SERVICE_PACKAGE_SCHEMA_JSON: &str =
    include_str!("../schemas/lenso-service-package.v1.schema.json");
pub const SERVICE_WORKSPACE_SCHEMA_JSON: &str =
    include_str!("../schemas/lenso-service-workspace.v1.schema.json");
pub const MODULE_CONTRACT_SCHEMA_JSON: &str =
    include_str!("../schemas/lenso-module.v1.schema.json");
pub const MODULE_RELEASE_SCHEMA_JSON: &str =
    include_str!("../schemas/lenso-module-release.v1.schema.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceDeploymentTarget {
    Kubernetes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceEnvironmentsFile {
    pub version: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environments: Vec<ServiceEnvironment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceEnvironment {
    pub name: String,
    pub service_name: String,
    pub target: ServiceDeploymentTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kube_context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_track: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<KubernetesDeploymentConfig>,
}

impl ServiceEnvironment {
    #[must_use]
    pub fn kubernetes(name: impl Into<String>, service_name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            service_name: service_name.into(),
            target: ServiceDeploymentTarget::Kubernetes,
            namespace: None,
            kube_context: None,
            image: None,
            public_base_url: None,
            manifest_reference: None,
            release_track: None,
            config: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KubernetesDeploymentConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicas: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingress_host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_request: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_request: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_limit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_limit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autoscaling: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disruption_budget: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_policy: Option<bool>,
}

impl KubernetesDeploymentConfig {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    #[must_use]
    pub fn replicas(mut self, replicas: u32) -> Self {
        self.replicas = Some(replicas);
        self
    }

    #[must_use]
    pub fn ingress_host(mut self, ingress_host: impl Into<String>) -> Self {
        self.ingress_host = Some(ingress_host.into());
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceDeploymentState {
    Ready,
    Progressing,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceDeploymentDrift {
    InSync,
    HostAhead,
    ClusterAhead,
    ImageDrift,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDeploymentsFile {
    pub version: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observations: Vec<ServiceDeploymentObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDeploymentObservation {
    pub service_name: String,
    pub environment: String,
    pub target: ServiceDeploymentTarget,
    pub observed_at_unix_ms: u64,
    pub state: ServiceDeploymentState,
    pub drift: ServiceDeploymentDrift,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster: Option<KubernetesDeploymentObservation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<ServiceDeploymentHostObservation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<ServiceDeploymentCheck>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_action: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KubernetesDeploymentObservation {
    pub namespace: String,
    pub deployment: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready_replicas: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desired_replicas: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_replicas: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingress_host: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDeploymentHostObservation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDeploymentCheck {
    pub name: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceHealth {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liveness_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceProvider {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceCompatibility {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_protocol_version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_host_features: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sdk_language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sdk_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceConfigField {
    pub key: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<Value>,
    #[serde(default)]
    pub secret: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceEnvField {
    pub name: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub example: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceLocalProcess {
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default = "default_service_auto_start")]
    pub auto_start: bool,
    #[serde(default = "default_service_ready_timeout_ms")]
    pub ready_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceWorkspace {
    pub protocol: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<ServiceWorkspaceService>,
}

impl ServiceWorkspace {
    #[must_use]
    pub fn new(services: Vec<ServiceWorkspaceService>) -> Self {
        Self {
            protocol: SERVICE_WORKSPACE_PROTOCOL.to_owned(),
            services,
        }
    }
}

#[must_use]
pub fn service_workspace_to_module_services(
    workspace: &ServiceWorkspace,
) -> ServiceWorkspaceModuleServicesFile {
    ServiceWorkspaceModuleServicesFile {
        version: 1,
        modules: workspace
            .services
            .iter()
            .map(|service| ServiceWorkspaceModuleServices {
                module_name: service.name.clone(),
                services: vec![ServiceWorkspaceProcess {
                    name: service.name.clone(),
                    command: service.command.clone(),
                    cwd: service.cwd.clone(),
                    ready_url: service.ready_url.clone(),
                    auto_start: service.auto_start,
                    ready_timeout_ms: service.ready_timeout_ms,
                }],
            })
            .collect(),
    }
}

#[must_use]
pub fn service_workspace_base_url(service: &ServiceWorkspaceService) -> Option<String> {
    service_base_url_from_ready_url(&service.ready_url)
        .or_else(|| service_base_url_from_manifest_url(&service.manifest))
}

#[must_use]
pub fn service_base_url_from_ready_url(ready_url: &str) -> Option<String> {
    service_base_url_from_url_suffix(ready_url, &["/status", "/ready", "/health", "/healthz"])
}

#[must_use]
pub fn service_base_url_from_manifest_url(manifest_url: &str) -> Option<String> {
    service_base_url_from_url_suffix(manifest_url, &["/manifest"])
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceWorkspaceService {
    pub name: String,
    pub lang: String,
    pub cwd: String,
    #[serde(default = "default_service_manifest")]
    pub manifest: String,
    pub command: String,
    pub ready_url: String,
    #[serde(default = "default_service_auto_start")]
    pub auto_start: bool,
    #[serde(default = "default_workspace_service_ready_timeout_ms")]
    pub ready_timeout_ms: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceWorkspaceProcess {
    pub name: String,
    pub command: String,
    pub cwd: String,
    pub ready_url: String,
    pub auto_start: bool,
    pub ready_timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceWorkspaceModuleServices {
    pub module_name: String,
    pub services: Vec<ServiceWorkspaceProcess>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceWorkspaceModuleServicesFile {
    pub version: u64,
    pub modules: Vec<ServiceWorkspaceModuleServices>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceReleaseRisk {
    Safe,
    NeedsAttention,
    Breaking,
    Blocked,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReleaseChangeSet {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReleaseModuleChangeSet {
    pub module: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReleaseDiff {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<ServiceReleaseModuleChangeSet>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub compatibility_changed: bool,
    #[serde(default)]
    pub config: ServiceReleaseChangeSet,
    #[serde(default)]
    pub env: ServiceReleaseChangeSet,
    #[serde(default)]
    pub modules: ServiceReleaseChangeSet,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<ServiceReleaseModuleChangeSet>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReleaseManifestSummary {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub manifest_reference: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_reference: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility_issue: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReleasePolicyIssue {
    pub code: String,
    pub level: ServiceReleaseRisk,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReleasePolicy {
    pub risk: ServiceReleaseRisk,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<ServiceReleasePolicyIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReleasePlan {
    pub protocol: String,
    pub service: BTreeMap<String, String>,
    pub current: ServiceReleaseManifestSummary,
    pub candidate: ServiceReleaseManifestSummary,
    pub diff: ServiceReleaseDiff,
    pub policy: ServiceReleasePolicy,
    pub restart_required: bool,
    pub next_action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at_unix_ms: Option<u64>,
}

impl ServiceReleasePlan {
    #[must_use]
    pub fn new(
        service_name: impl Into<String>,
        current: ServiceReleaseManifestSummary,
        candidate: ServiceReleaseManifestSummary,
        diff: ServiceReleaseDiff,
    ) -> Self {
        let policy =
            evaluate_service_release_policy(&diff, candidate.compatibility_issue.as_deref());
        let mut service = BTreeMap::new();
        service.insert("name".to_owned(), service_name.into());
        Self {
            protocol: SERVICE_RELEASE_PLAN_PROTOCOL.to_owned(),
            service,
            current,
            candidate,
            restart_required: service_release_restart_required(&diff),
            next_action: service_release_next_action(policy.risk).to_owned(),
            diff,
            policy,
            created_at_unix_ms: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceContract {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<ServiceProvider>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<ServiceCompatibility>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config: Vec<ServiceConfigField>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<ServiceEnvField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<ServiceHealth>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_process: Option<ServiceLocalProcess>,
    pub modules: Vec<ModuleManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServicePackage {
    pub protocol: String,
    pub name: String,
    pub version: String,
    pub service_manifest: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<String>,
}

impl ServicePackage {
    #[must_use]
    pub fn new(name: impl Into<String>, version: impl Into<String>, modules: Vec<String>) -> Self {
        Self {
            protocol: SERVICE_PACKAGE_PROTOCOL.to_owned(),
            name: name.into(),
            version: version.into(),
            service_manifest: "lenso.service.json".to_owned(),
            modules,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleContract {
    pub protocol: String,
    pub name: String,
    pub version: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<ModuleManifest>,
}

impl ModuleContract {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            protocol: MODULE_CONTRACT_PROTOCOL.to_owned(),
            name: name.into(),
            version: version.into(),
            source: source.into(),
            summary: None,
            capabilities: Vec::new(),
            dependencies: Vec::new(),
            manifest: None,
        }
    }

    #[must_use]
    pub fn manifest(mut self, manifest: ModuleManifest) -> Self {
        self.manifest = Some(manifest);
        self
    }

    #[must_use]
    pub fn capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.capabilities = capabilities;
        self
    }

    #[must_use]
    pub fn dependencies(mut self, dependencies: Vec<String>) -> Self {
        self.dependencies = dependencies;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleReleaseProvider {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_package: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_manifest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleRelease {
    pub protocol: String,
    pub name: String,
    pub version: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<ModuleReleaseProvider>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
}

impl ModuleRelease {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        provider_name: impl Into<String>,
    ) -> Self {
        Self {
            protocol: MODULE_RELEASE_PROTOCOL.to_owned(),
            name: name.into(),
            version: version.into(),
            source: "service".to_owned(),
            provider: Some(ModuleReleaseProvider {
                name: provider_name.into(),
                service_package: Some("lenso.service-package.json".to_owned()),
                service_manifest: None,
            }),
            summary: None,
            capabilities: Vec::new(),
            dependencies: Vec::new(),
        }
    }

    #[must_use]
    pub fn capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.capabilities = capabilities;
        self
    }

    #[must_use]
    pub fn dependencies(mut self, dependencies: Vec<String>) -> Self {
        self.dependencies = dependencies;
        self
    }

    #[must_use]
    pub fn service_manifest(mut self, service_manifest: impl Into<String>) -> Self {
        if let Some(provider) = &mut self.provider {
            provider.service_package = None;
            provider.service_manifest = Some(service_manifest.into());
        }
        self
    }
}

impl ServiceContract {
    #[must_use]
    pub fn new(name: impl Into<String>, modules: Vec<ModuleManifest>) -> Self {
        Self {
            name: name.into(),
            version: None,
            provider: None,
            compatibility: None,
            config: Vec::new(),
            env: Vec::new(),
            health: None,
            local_process: None,
            modules,
        }
    }

    #[must_use]
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    #[must_use]
    pub fn provider(mut self, provider: ServiceProvider) -> Self {
        self.provider = Some(provider);
        self
    }

    #[must_use]
    pub fn compatibility(mut self, compatibility: ServiceCompatibility) -> Self {
        self.compatibility = Some(compatibility);
        self
    }

    #[must_use]
    pub fn config(mut self, config: Vec<ServiceConfigField>) -> Self {
        self.config = config;
        self
    }

    #[must_use]
    pub fn env(mut self, env: Vec<ServiceEnvField>) -> Self {
        self.env = env;
        self
    }

    #[must_use]
    pub fn health(mut self, health: ServiceHealth) -> Self {
        self.health = Some(health);
        self
    }

    #[must_use]
    pub fn local_process(mut self, local_process: ServiceLocalProcess) -> Self {
        self.local_process = Some(local_process);
        self
    }
}

#[must_use]
pub fn evaluate_service_release_policy(
    diff: &ServiceReleaseDiff,
    compatibility_issue: Option<&str>,
) -> ServiceReleasePolicy {
    let mut issues = Vec::new();
    if let Some(issue) = compatibility_issue {
        issues.push(ServiceReleasePolicyIssue {
            code: "host_incompatible".to_owned(),
            level: ServiceReleaseRisk::Blocked,
            message: issue.to_owned(),
        });
    } else if diff.compatibility_changed {
        issues.push(ServiceReleasePolicyIssue {
            code: "compatibility_changed".to_owned(),
            level: ServiceReleaseRisk::NeedsAttention,
            message: "Service compatibility metadata changed; review host support before applying."
                .to_owned(),
        });
    }
    for module in &diff.modules.removed {
        issues.push(ServiceReleasePolicyIssue {
            code: "module_removed".to_owned(),
            level: ServiceReleaseRisk::Breaking,
            message: format!("Module `{module}` is removed by this release."),
        });
    }
    for env in &diff.env.added {
        issues.push(ServiceReleasePolicyIssue {
            code: "env_added".to_owned(),
            level: ServiceReleaseRisk::NeedsAttention,
            message: format!("Environment value `{env}` is newly required by this release."),
        });
    }
    for config in &diff.config.added {
        issues.push(ServiceReleasePolicyIssue {
            code: "config_added".to_owned(),
            level: ServiceReleaseRisk::NeedsAttention,
            message: format!("Runtime config `{config}` is newly declared by this release."),
        });
    }
    for change in &diff.capabilities {
        for capability in &change.removed {
            issues.push(ServiceReleasePolicyIssue {
                code: "capability_removed".to_owned(),
                level: ServiceReleaseRisk::Breaking,
                message: format!(
                    "Capability `{capability}` is removed from module `{}`.",
                    change.module
                ),
            });
        }
    }
    for change in &diff.operations {
        for operation in &change.removed {
            issues.push(ServiceReleasePolicyIssue {
                code: "operation_removed".to_owned(),
                level: ServiceReleaseRisk::Breaking,
                message: format!(
                    "Operation `{operation}` is removed from module `{}`.",
                    change.module
                ),
            });
        }
    }
    let risk = issues
        .iter()
        .map(|issue| issue.level)
        .max_by_key(|risk| service_release_risk_rank(*risk))
        .unwrap_or(ServiceReleaseRisk::Safe);
    ServiceReleasePolicy { risk, issues }
}

#[must_use]
pub fn service_release_restart_required(diff: &ServiceReleaseDiff) -> bool {
    diff.compatibility_changed
        || !diff.modules.added.is_empty()
        || !diff.modules.removed.is_empty()
        || !diff.env.added.is_empty()
        || !diff.env.removed.is_empty()
        || !diff.config.added.is_empty()
        || !diff.config.removed.is_empty()
        || diff
            .capabilities
            .iter()
            .any(|change| !change.added.is_empty() || !change.removed.is_empty())
        || diff
            .operations
            .iter()
            .any(|change| !change.added.is_empty() || !change.removed.is_empty())
}

#[must_use]
pub fn service_release_next_action(risk: ServiceReleaseRisk) -> &'static str {
    match risk {
        ServiceReleaseRisk::Safe => "Run `lenso service release apply <plan.json>` when ready.",
        ServiceReleaseRisk::NeedsAttention => {
            "Review required env/config, then run `lenso service release apply <plan.json>`."
        }
        ServiceReleaseRisk::Breaking => {
            "Review removed modules, capabilities, or operations before applying."
        }
        ServiceReleaseRisk::Blocked => "Fix blocked policy issues before applying this release.",
    }
}

#[must_use]
pub fn health_router() -> Router {
    Router::new()
        .route(
            "/lenso/service/v1/ready",
            get(|| async { Json(serde_json::json!({"ready": true})) }),
        )
        .route(
            "/lenso/service/v1/status",
            get(|| async { Json(serde_json::json!({"state": "ready"})) }),
        )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceContractIssue {
    pub path: String,
    pub message: String,
}

impl ServiceContractIssue {
    fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

#[must_use]
pub fn validate_service_contract_value(value: &Value) -> Vec<ServiceContractIssue> {
    let Some(object) = value.as_object() else {
        return vec![ServiceContractIssue::new(
            "$",
            "service contract must be an object",
        )];
    };

    let mut issues = Vec::new();
    require_non_empty_string(object.get("name"), "$.name", &mut issues);
    if let Some(version) = object.get("version") {
        require_non_empty_string(Some(version), "$.version", &mut issues);
    }
    validate_provider(object.get("provider"), &mut issues);
    validate_named_fields_array(object.get("config"), "$.config", "key", &mut issues);
    validate_named_fields_array(object.get("env"), "$.env", "name", &mut issues);
    validate_string_array(
        object
            .get("requiredEnv")
            .or_else(|| object.get("required_env")),
        "$.requiredEnv",
        &mut issues,
    );
    validate_compatibility(object.get("compatibility"), &mut issues);
    validate_local_process(
        object
            .get("localProcess")
            .or_else(|| object.get("local_process")),
        "$.localProcess",
        &mut issues,
    );
    validate_install(object.get("install"), &mut issues);
    validate_modules(object.get("modules"), &mut issues);
    issues
}

#[must_use]
pub fn validate_service_package_value(value: &Value) -> Vec<ServiceContractIssue> {
    let Some(object) = value.as_object() else {
        return vec![ServiceContractIssue::new(
            "$",
            "service package must be an object",
        )];
    };

    let mut issues = Vec::new();
    match object.get("protocol").and_then(Value::as_str) {
        Some(SERVICE_PACKAGE_PROTOCOL) => {}
        Some(_) => issues.push(ServiceContractIssue::new(
            "$.protocol",
            format!("protocol must be `{SERVICE_PACKAGE_PROTOCOL}`"),
        )),
        None => issues.push(ServiceContractIssue::new(
            "$.protocol",
            "field must be a non-empty string",
        )),
    }
    require_non_empty_string(object.get("name"), "$.name", &mut issues);
    require_non_empty_string(object.get("version"), "$.version", &mut issues);
    require_non_empty_string(
        object
            .get("serviceManifest")
            .or_else(|| object.get("service_manifest")),
        "$.serviceManifest",
        &mut issues,
    );
    validate_service_package_modules(object.get("modules"), &mut issues);
    issues
}

#[must_use]
pub fn validate_service_workspace_value(value: &Value) -> Vec<ServiceContractIssue> {
    let Some(object) = value.as_object() else {
        return vec![ServiceContractIssue::new(
            "$",
            "service workspace must be an object",
        )];
    };

    let mut issues = Vec::new();
    match object.get("protocol").and_then(Value::as_str) {
        Some(SERVICE_WORKSPACE_PROTOCOL) => {}
        Some(_) => issues.push(ServiceContractIssue::new(
            "$.protocol",
            format!("protocol must be `{SERVICE_WORKSPACE_PROTOCOL}`"),
        )),
        None => issues.push(ServiceContractIssue::new(
            "$.protocol",
            "field must be a non-empty string",
        )),
    }
    validate_workspace_services(object.get("services"), &mut issues);
    issues
}

#[must_use]
pub fn validate_module_contract_value(value: &Value) -> Vec<ServiceContractIssue> {
    let Some(object) = value.as_object() else {
        return vec![ServiceContractIssue::new(
            "$",
            "module contract must be an object",
        )];
    };

    let mut issues = Vec::new();
    match object.get("protocol").and_then(Value::as_str) {
        Some(MODULE_CONTRACT_PROTOCOL) => {}
        Some(_) => issues.push(ServiceContractIssue::new(
            "$.protocol",
            format!("protocol must be `{MODULE_CONTRACT_PROTOCOL}`"),
        )),
        None => issues.push(ServiceContractIssue::new(
            "$.protocol",
            "field must be a non-empty string",
        )),
    }
    require_non_empty_string(object.get("name"), "$.name", &mut issues);
    require_non_empty_string(object.get("version"), "$.version", &mut issues);
    validate_module_artifact_source(object.get("source"), "$.source", &mut issues);
    validate_string_array(object.get("capabilities"), "$.capabilities", &mut issues);
    validate_string_array(object.get("dependencies"), "$.dependencies", &mut issues);
    if let Some(manifest) = object.get("manifest")
        && !manifest.is_object()
    {
        issues.push(ServiceContractIssue::new(
            "$.manifest",
            "manifest must be an object",
        ));
    }
    issues
}

#[must_use]
pub fn validate_module_release_value(value: &Value) -> Vec<ServiceContractIssue> {
    let Some(object) = value.as_object() else {
        return vec![ServiceContractIssue::new(
            "$",
            "module release must be an object",
        )];
    };

    let mut issues = Vec::new();
    match object.get("protocol").and_then(Value::as_str) {
        Some(MODULE_RELEASE_PROTOCOL) => {}
        Some(_) => issues.push(ServiceContractIssue::new(
            "$.protocol",
            format!("protocol must be `{MODULE_RELEASE_PROTOCOL}`"),
        )),
        None => issues.push(ServiceContractIssue::new(
            "$.protocol",
            "field must be a non-empty string",
        )),
    }
    require_non_empty_string(object.get("name"), "$.name", &mut issues);
    require_non_empty_string(object.get("version"), "$.version", &mut issues);
    let source = object.get("source").and_then(Value::as_str);
    validate_module_artifact_source(object.get("source"), "$.source", &mut issues);
    match source {
        Some("service") => validate_module_release_provider(object.get("provider"), &mut issues),
        Some("linked" | "bundled") if object.get("provider").is_some() => {
            validate_module_release_provider(object.get("provider"), &mut issues);
        }
        _ => {}
    }
    validate_string_array(object.get("capabilities"), "$.capabilities", &mut issues);
    validate_string_array(object.get("dependencies"), "$.dependencies", &mut issues);
    issues
}

fn validate_module_artifact_source(
    value: Option<&Value>,
    path: &str,
    issues: &mut Vec<ServiceContractIssue>,
) {
    match value.and_then(Value::as_str) {
        Some("service" | "linked" | "bundled") => {}
        Some(_) => issues.push(ServiceContractIssue::new(
            path,
            "source must be `service`, `linked`, or `bundled`",
        )),
        None => issues.push(ServiceContractIssue::new(
            path,
            "field must be a non-empty string",
        )),
    }
}

fn validate_module_release_provider(value: Option<&Value>, issues: &mut Vec<ServiceContractIssue>) {
    let Some(value) = value else {
        issues.push(ServiceContractIssue::new(
            "$.provider",
            "provider must be an object",
        ));
        return;
    };
    let Some(object) = value.as_object() else {
        issues.push(ServiceContractIssue::new(
            "$.provider",
            "provider must be an object",
        ));
        return;
    };
    require_non_empty_string(object.get("name"), "$.provider.name", issues);
    if object
        .get("servicePackage")
        .or_else(|| object.get("service_package"))
        .or_else(|| object.get("serviceManifest"))
        .or_else(|| object.get("service_manifest"))
        .and_then(Value::as_str)
        .map(str::trim)
        .is_none_or(str::is_empty)
    {
        issues.push(ServiceContractIssue::new(
            "$.provider.servicePackage",
            "field must be a non-empty string",
        ));
    }
}

fn validate_provider(value: Option<&Value>, issues: &mut Vec<ServiceContractIssue>) {
    let Some(value) = value else {
        return;
    };
    if !value.is_object() {
        issues.push(ServiceContractIssue::new(
            "$.provider",
            "provider must be an object",
        ));
        return;
    }
    require_non_empty_string(value.get("name"), "$.provider.name", issues);
}

fn validate_compatibility(value: Option<&Value>, issues: &mut Vec<ServiceContractIssue>) {
    let Some(value) = value else {
        return;
    };
    let Some(object) = value.as_object() else {
        issues.push(ServiceContractIssue::new(
            "$.compatibility",
            "compatibility must be an object",
        ));
        return;
    };
    validate_string_array(
        object
            .get("requiredHostFeatures")
            .or_else(|| object.get("required_host_features")),
        "$.compatibility.requiredHostFeatures",
        issues,
    );
}

fn validate_named_fields_array(
    value: Option<&Value>,
    path: &str,
    name_field: &str,
    issues: &mut Vec<ServiceContractIssue>,
) {
    let Some(value) = value else {
        return;
    };
    let Some(array) = value.as_array() else {
        issues.push(ServiceContractIssue::new(path, "field must be an array"));
        return;
    };
    for (index, item) in array.iter().enumerate() {
        if !item.is_object() {
            issues.push(ServiceContractIssue::new(
                format!("{path}[{index}]"),
                "entry must be an object",
            ));
            continue;
        }
        require_non_empty_string(
            item.get(name_field),
            &format!("{path}[{index}].{name_field}"),
            issues,
        );
    }
}

fn validate_local_process(
    value: Option<&Value>,
    path: &str,
    issues: &mut Vec<ServiceContractIssue>,
) {
    let Some(value) = value else {
        return;
    };
    if !value.is_object() {
        issues.push(ServiceContractIssue::new(
            path,
            "localProcess must be an object",
        ));
        return;
    }
    require_non_empty_string(value.get("command"), &format!("{path}.command"), issues);
}

fn validate_install(value: Option<&Value>, issues: &mut Vec<ServiceContractIssue>) {
    let Some(value) = value else {
        return;
    };
    let Some(object) = value.as_object() else {
        issues.push(ServiceContractIssue::new(
            "$.install",
            "install must be an object",
        ));
        return;
    };
    let Some(services) = object.get("services") else {
        return;
    };
    let Some(array) = services.as_array() else {
        issues.push(ServiceContractIssue::new(
            "$.install.services",
            "install services must be an array",
        ));
        return;
    };
    for (index, service) in array.iter().enumerate() {
        if !service.is_object() {
            issues.push(ServiceContractIssue::new(
                format!("$.install.services[{index}]"),
                "service must be an object",
            ));
            continue;
        }
        require_non_empty_string(
            service.get("name"),
            &format!("$.install.services[{index}].name"),
            issues,
        );
        require_non_empty_string(
            service.get("command"),
            &format!("$.install.services[{index}].command"),
            issues,
        );
    }
}

fn validate_modules(value: Option<&Value>, issues: &mut Vec<ServiceContractIssue>) {
    let Some(value) = value else {
        issues.push(ServiceContractIssue::new(
            "$.modules",
            "modules must be an array",
        ));
        return;
    };
    let Some(array) = value.as_array() else {
        issues.push(ServiceContractIssue::new(
            "$.modules",
            "modules must be an array",
        ));
        return;
    };
    if array.is_empty() {
        issues.push(ServiceContractIssue::new(
            "$.modules",
            "modules must not be empty",
        ));
        return;
    }

    let mut names = BTreeSet::new();
    for (index, module) in array.iter().enumerate() {
        let Some(object) = module.as_object() else {
            issues.push(ServiceContractIssue::new(
                format!("$.modules[{index}]"),
                "module must be an object",
            ));
            continue;
        };
        let Some(module_name) = non_empty_string(
            object.get("name"),
            &format!("$.modules[{index}].name"),
            issues,
        ) else {
            continue;
        };
        if !names.insert(module_name.to_owned()) {
            issues.push(ServiceContractIssue::new(
                format!("$.modules[{index}].name"),
                format!("module `{module_name}` is declared more than once"),
            ));
        }
        validate_string_array(
            object.get("capabilities"),
            &format!("$.modules[{index}].capabilities"),
            issues,
        );
        validate_string_array(
            object.get("dependencies"),
            &format!("$.modules[{index}].dependencies"),
            issues,
        );
    }
}

fn validate_service_package_modules(value: Option<&Value>, issues: &mut Vec<ServiceContractIssue>) {
    let Some(value) = value else {
        issues.push(ServiceContractIssue::new(
            "$.modules",
            "modules must be an array",
        ));
        return;
    };
    let Some(array) = value.as_array() else {
        issues.push(ServiceContractIssue::new(
            "$.modules",
            "modules must be an array",
        ));
        return;
    };
    if array.is_empty() {
        issues.push(ServiceContractIssue::new(
            "$.modules",
            "modules must not be empty",
        ));
        return;
    }
    let mut names = BTreeSet::new();
    for (index, module) in array.iter().enumerate() {
        let Some(module_name) =
            non_empty_string(Some(module), &format!("$.modules[{index}]"), issues)
        else {
            continue;
        };
        if !names.insert(module_name.to_owned()) {
            issues.push(ServiceContractIssue::new(
                format!("$.modules[{index}]"),
                format!("module `{module_name}` is declared more than once"),
            ));
        }
    }
}

fn validate_workspace_services(value: Option<&Value>, issues: &mut Vec<ServiceContractIssue>) {
    let Some(value) = value else {
        return;
    };
    let Some(array) = value.as_array() else {
        issues.push(ServiceContractIssue::new(
            "$.services",
            "services must be an array",
        ));
        return;
    };
    let mut names = BTreeSet::new();
    for (index, service) in array.iter().enumerate() {
        let Some(object) = service.as_object() else {
            issues.push(ServiceContractIssue::new(
                format!("$.services[{index}]"),
                "service must be an object",
            ));
            continue;
        };
        let name = non_empty_string(
            object.get("name"),
            &format!("$.services[{index}].name"),
            issues,
        );
        if let Some(name) = name {
            if !names.insert(name.to_owned()) {
                issues.push(ServiceContractIssue::new(
                    format!("$.services[{index}].name"),
                    format!("service `{name}` is declared more than once"),
                ));
            }
        }
        require_non_empty_string(
            object.get("lang"),
            &format!("$.services[{index}].lang"),
            issues,
        );
        require_non_empty_string(
            object.get("cwd"),
            &format!("$.services[{index}].cwd"),
            issues,
        );
        require_non_empty_string(
            object.get("manifest"),
            &format!("$.services[{index}].manifest"),
            issues,
        );
        require_non_empty_string(
            object.get("command"),
            &format!("$.services[{index}].command"),
            issues,
        );
        require_non_empty_string(
            object.get("readyUrl").or_else(|| object.get("ready_url")),
            &format!("$.services[{index}].readyUrl"),
            issues,
        );
        validate_string_array(
            object.get("modules"),
            &format!("$.services[{index}].modules"),
            issues,
        );
    }
}

fn validate_string_array(
    value: Option<&Value>,
    path: &str,
    issues: &mut Vec<ServiceContractIssue>,
) {
    let Some(value) = value else {
        return;
    };
    let Some(array) = value.as_array() else {
        issues.push(ServiceContractIssue::new(path, "field must be an array"));
        return;
    };
    for (index, item) in array.iter().enumerate() {
        require_non_empty_string(Some(item), &format!("{path}[{index}]"), issues);
    }
}

fn require_non_empty_string(
    value: Option<&Value>,
    path: &str,
    issues: &mut Vec<ServiceContractIssue>,
) {
    let _ = non_empty_string(value, path, issues);
}

fn non_empty_string<'a>(
    value: Option<&'a Value>,
    path: &str,
    issues: &mut Vec<ServiceContractIssue>,
) -> Option<&'a str> {
    match value.and_then(Value::as_str).map(str::trim) {
        Some(value) if !value.is_empty() => Some(value),
        _ => {
            issues.push(ServiceContractIssue::new(
                path,
                "field must be a non-empty string",
            ));
            None
        }
    }
}

fn service_base_url_from_url_suffix(value: &str, suffixes: &[&str]) -> Option<String> {
    let value = value.trim();
    if !(value.starts_with("http://") || value.starts_with("https://")) {
        return None;
    }
    let value = strip_query_fragment(value).trim_end_matches('/');
    suffixes.iter().find_map(|suffix| {
        value
            .strip_suffix(suffix)
            .map(|base_url| base_url.trim_end_matches('/'))
            .map(ToOwned::to_owned)
    })
}

fn strip_query_fragment(value: &str) -> &str {
    let query_index = value.find('?').unwrap_or(value.len());
    let fragment_index = value.find('#').unwrap_or(value.len());
    &value[..query_index.min(fragment_index)]
}

const fn service_release_risk_rank(risk: ServiceReleaseRisk) -> u8 {
    match risk {
        ServiceReleaseRisk::Safe => 0,
        ServiceReleaseRisk::NeedsAttention => 1,
        ServiceReleaseRisk::Breaking => 2,
        ServiceReleaseRisk::Blocked => 3,
    }
}

const fn is_false(value: &bool) -> bool {
    !*value
}

const fn default_service_auto_start() -> bool {
    true
}

const fn default_service_ready_timeout_ms() -> u64 {
    30_000
}

const fn default_workspace_service_ready_timeout_ms() -> u64 {
    10_000
}

fn default_service_manifest() -> String {
    "lenso.service.json".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn service_package_new_uses_v1_protocol() {
        let package = ServicePackage::new(
            "support-suite-provider",
            "0.2.0",
            vec!["support-ticket".to_owned()],
        );
        let value = serde_json::to_value(package).unwrap();

        assert_eq!(value["protocol"], SERVICE_PACKAGE_PROTOCOL);
        assert_eq!(value["serviceManifest"], "lenso.service.json");
        assert_eq!(value["modules"], json!(["support-ticket"]));
    }

    #[test]
    fn service_release_plan_uses_delivery_policy() {
        let diff = ServiceReleaseDiff {
            capabilities: vec![ServiceReleaseModuleChangeSet {
                module: "support-ticket".to_owned(),
                added: Vec::new(),
                removed: vec!["support_ticket.tickets.write".to_owned()],
            }],
            config: ServiceReleaseChangeSet {
                added: vec!["support.mode".to_owned()],
                removed: Vec::new(),
            },
            env: ServiceReleaseChangeSet {
                added: vec!["SUPPORT_API_KEY".to_owned()],
                removed: Vec::new(),
            },
            operations: vec![ServiceReleaseModuleChangeSet {
                module: "support-ticket".to_owned(),
                added: Vec::new(),
                removed: vec!["route:DELETE /tickets/{id}".to_owned()],
            }],
            ..ServiceReleaseDiff::default()
        };
        let current = ServiceReleaseManifestSummary {
            name: "support-suite-provider".to_owned(),
            version: Some("0.1.0".to_owned()),
            manifest_reference: "./support/v1/lenso.service.json".to_owned(),
            package_reference: None,
            input_reference: None,
            modules: vec!["support-ticket".to_owned()],
            compatibility_issue: None,
        };
        let candidate = ServiceReleaseManifestSummary {
            name: "support-suite-provider".to_owned(),
            version: Some("0.2.0".to_owned()),
            manifest_reference: "./support/v2/lenso.service.json".to_owned(),
            package_reference: Some("./support/v2/lenso.service-package.json".to_owned()),
            input_reference: None,
            modules: vec!["support-ticket".to_owned()],
            compatibility_issue: None,
        };

        let plan = ServiceReleasePlan::new("support-suite-provider", current, candidate, diff);
        let value = serde_json::to_value(plan).unwrap();

        assert_eq!(value["protocol"], SERVICE_RELEASE_PLAN_PROTOCOL);
        assert_eq!(value["policy"]["risk"], "breaking");
        assert_eq!(value["restartRequired"], true);
        assert_eq!(
            evaluate_service_release_policy(
                &ServiceReleaseDiff::default(),
                Some("remote protocol is newer"),
            )
            .risk,
            ServiceReleaseRisk::Blocked
        );
    }

    #[test]
    fn service_environment_round_trips_kubernetes_target() {
        let file = ServiceEnvironmentsFile {
            version: 1,
            environments: vec![ServiceEnvironment {
                namespace: Some("lenso-staging".to_owned()),
                kube_context: Some("staging".to_owned()),
                image: Some("ghcr.io/acme/support-suite-provider:0.4.0".to_owned()),
                public_base_url: Some("https://support-staging.example.com".to_owned()),
                release_track: Some("staging".to_owned()),
                config: Some(
                    KubernetesDeploymentConfig::new()
                        .replicas(2)
                        .port(4110)
                        .ingress_host("support-staging.example.com"),
                ),
                ..ServiceEnvironment::kubernetes("staging", "support-suite-provider")
            }],
        };

        let value = serde_json::to_value(&file).unwrap();
        assert_eq!(value["environments"][0]["target"], "kubernetes");
        assert_eq!(
            value["environments"][0]["serviceName"],
            "support-suite-provider"
        );
        assert_eq!(
            value["environments"][0]["config"]["ingressHost"],
            "support-staging.example.com"
        );

        let round_trip: ServiceEnvironmentsFile = serde_json::from_value(value).unwrap();
        assert_eq!(round_trip, file);
    }

    #[test]
    fn service_deployment_observation_uses_stable_state_names() {
        let observation = ServiceDeploymentObservation {
            service_name: "support-suite-provider".to_owned(),
            environment: "staging".to_owned(),
            target: ServiceDeploymentTarget::Kubernetes,
            observed_at_unix_ms: 1_803_744_000_000,
            state: ServiceDeploymentState::Ready,
            drift: ServiceDeploymentDrift::InSync,
            cluster: Some(KubernetesDeploymentObservation {
                namespace: "lenso-staging".to_owned(),
                deployment: "support-suite-provider".to_owned(),
                ready_replicas: Some(2),
                desired_replicas: Some(2),
                available_replicas: Some(2),
                image: Some("ghcr.io/acme/support-suite-provider:0.4.0".to_owned()),
                release_id: Some("rel_staging".to_owned()),
                manifest_reference: Some(
                    "https://support-staging.example.com/lenso/service/v1/manifest".to_owned(),
                ),
                service_endpoint: Some(
                    "support-suite-provider.lenso-staging.svc.cluster.local".to_owned(),
                ),
                ingress_host: Some("support-staging.example.com".to_owned()),
            }),
            host: Some(ServiceDeploymentHostObservation {
                release_id: Some("rel_staging".to_owned()),
                candidate_version: Some("0.4.0".to_owned()),
            }),
            checks: vec![ServiceDeploymentCheck {
                name: "deployment_rollout".to_owned(),
                status: "ok".to_owned(),
                detail: Some("2/2 replicas ready".to_owned()),
            }],
            next_action: Some("monitor rollout and Remote Calls".to_owned()),
        };

        let value = serde_json::to_value(&observation).unwrap();
        assert_eq!(value["state"], "ready");
        assert_eq!(value["drift"], "in_sync");
        assert_eq!(value["cluster"]["readyReplicas"], 2);

        let round_trip: ServiceDeploymentObservation = serde_json::from_value(value).unwrap();
        assert_eq!(round_trip, observation);
    }

    #[test]
    fn valid_service_package_has_no_issues() {
        let issues = validate_service_package_value(&json!({
            "protocol": "lenso.service-package.v1",
            "name": "support-suite-provider",
            "version": "0.2.0",
            "serviceManifest": "lenso.service.json",
            "modules": ["support-ticket", "support-inbox"]
        }));

        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn invalid_service_package_reports_protocol_and_modules() {
        let issues = validate_service_package_value(&json!({
            "protocol": "remote-module",
            "name": "support-suite-provider",
            "version": "0.2.0",
            "serviceManifest": "lenso.service.json",
            "modules": ["support-ticket", "support-ticket", ""]
        }));

        assert_eq!(
            issues
                .iter()
                .map(|issue| issue.path.as_str())
                .collect::<Vec<_>>(),
            vec!["$.protocol", "$.modules[1]", "$.modules[2]"]
        );
    }

    #[test]
    fn valid_service_workspace_has_no_issues() {
        let issues = validate_service_workspace_value(&json!({
            "protocol": "lenso.service-workspace.v1",
            "services": [
                {
                    "name": "support-suite-provider",
                    "lang": "ts",
                    "cwd": "services/support-suite-provider",
                    "manifest": "lenso.service.json",
                    "command": "pnpm start",
                    "readyUrl": "http://127.0.0.1:4110/lenso/service/v1/status",
                    "modules": ["support-ticket"]
                }
            ]
        }));

        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn service_workspace_exports_module_service_start_file() {
        let workspace = ServiceWorkspace::new(vec![ServiceWorkspaceService {
            name: "support-suite-provider".to_owned(),
            lang: "ts".to_owned(),
            cwd: "services/support-suite-provider".to_owned(),
            manifest: "lenso.service.json".to_owned(),
            command: "pnpm start".to_owned(),
            ready_url: "http://127.0.0.1:4110/lenso/service/v1/status".to_owned(),
            auto_start: true,
            ready_timeout_ms: 10_000,
            modules: vec!["support-ticket".to_owned()],
        }]);

        let value = serde_json::to_value(service_workspace_to_module_services(&workspace)).unwrap();

        assert_eq!(value["version"], 1);
        assert_eq!(value["modules"][0]["moduleName"], "support-suite-provider");
        assert_eq!(value["modules"][0]["services"][0]["command"], "pnpm start");
        assert_eq!(
            value["modules"][0]["services"][0]["readyUrl"],
            "http://127.0.0.1:4110/lenso/service/v1/status"
        );
    }

    #[test]
    fn service_workspace_infers_service_base_url() {
        assert_eq!(
            service_base_url_from_ready_url(
                "http://127.0.0.1:4110/lenso/service/v1/status?probe=1"
            )
            .as_deref(),
            Some("http://127.0.0.1:4110/lenso/service/v1")
        );
        assert_eq!(
            service_base_url_from_manifest_url("http://127.0.0.1:4110/lenso/service/v1/manifest")
                .as_deref(),
            Some("http://127.0.0.1:4110/lenso/service/v1")
        );
        assert_eq!(
            service_workspace_base_url(&ServiceWorkspaceService {
                name: "support-suite-provider".to_owned(),
                lang: "ts".to_owned(),
                cwd: "services/support-suite-provider".to_owned(),
                manifest: "lenso.service.json".to_owned(),
                command: "pnpm start".to_owned(),
                ready_url: "http://127.0.0.1:4110/lenso/service/v1/ready".to_owned(),
                auto_start: true,
                ready_timeout_ms: 10_000,
                modules: vec!["support-ticket".to_owned()],
            })
            .as_deref(),
            Some("http://127.0.0.1:4110/lenso/service/v1")
        );
        assert!(service_base_url_from_ready_url("not a url").is_none());
    }

    #[test]
    fn invalid_service_workspace_reports_service_paths() {
        let issues = validate_service_workspace_value(&json!({
            "protocol": "lenso.workspace",
            "services": [
                {
                    "name": "",
                    "modules": ["support-ticket", 42]
                }
            ]
        }));

        assert_eq!(
            issues
                .iter()
                .map(|issue| issue.path.as_str())
                .collect::<Vec<_>>(),
            vec![
                "$.protocol",
                "$.services[0].name",
                "$.services[0].lang",
                "$.services[0].cwd",
                "$.services[0].manifest",
                "$.services[0].command",
                "$.services[0].readyUrl",
                "$.services[0].modules[1]"
            ]
        );
    }

    #[test]
    fn module_contract_new_uses_v1_protocol() {
        let contract = ModuleContract::new("support-ticket", "0.2.0", "linked")
            .capabilities(vec!["support_ticket.tickets.read".to_owned()])
            .dependencies(vec!["auth".to_owned()]);
        let value = serde_json::to_value(contract).unwrap();

        assert_eq!(value["protocol"], MODULE_CONTRACT_PROTOCOL);
        assert_eq!(value["source"], "linked");
        assert_eq!(
            value["capabilities"],
            json!(["support_ticket.tickets.read"])
        );
        assert_eq!(value["dependencies"], json!(["auth"]));
        assert!(validate_module_contract_value(&value).is_empty());
    }

    #[test]
    fn invalid_module_contract_reports_protocol_source_and_arrays() {
        let issues = validate_module_contract_value(&json!({
            "protocol": "lenso.module",
            "name": "",
            "version": "",
            "source": "remote",
            "capabilities": ["support_ticket.read", 42],
            "manifest": []
        }));

        assert_eq!(
            issues
                .iter()
                .map(|issue| issue.path.as_str())
                .collect::<Vec<_>>(),
            vec![
                "$.protocol",
                "$.name",
                "$.version",
                "$.source",
                "$.capabilities[1]",
                "$.manifest"
            ]
        );
    }

    #[test]
    fn module_release_new_uses_v1_protocol() {
        let release = ModuleRelease::new("support-ticket", "0.2.0", "support-suite-provider")
            .capabilities(vec!["support_ticket.tickets.read".to_owned()])
            .dependencies(vec!["auth".to_owned()]);
        let value = serde_json::to_value(release).unwrap();

        assert_eq!(value["protocol"], MODULE_RELEASE_PROTOCOL);
        assert_eq!(value["source"], "service");
        assert_eq!(
            value["provider"]["servicePackage"],
            "lenso.service-package.json"
        );
        assert_eq!(
            value["capabilities"],
            json!(["support_ticket.tickets.read"])
        );
        assert_eq!(value["dependencies"], json!(["auth"]));
    }

    #[test]
    fn valid_module_release_has_no_issues() {
        let issues = validate_module_release_value(&json!({
            "protocol": "lenso.module-release.v1",
            "name": "support-ticket",
            "version": "0.2.0",
            "source": "service",
            "provider": {
                "name": "support-suite-provider",
                "serviceManifest": "https://example.test/lenso/service/v1/manifest"
            },
            "capabilities": ["support_ticket.tickets.read"]
        }));

        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn linked_module_release_does_not_require_provider() {
        let issues = validate_module_release_value(&json!({
            "protocol": "lenso.module-release.v1",
            "name": "auth-password",
            "version": "0.2.0",
            "source": "linked",
            "capabilities": ["auth.password.login"]
        }));

        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn invalid_module_release_reports_protocol_source_provider_and_capabilities() {
        let issues = validate_module_release_value(&json!({
            "protocol": "remote-module",
            "name": "",
            "version": "",
            "source": "remote",
            "provider": { "name": "" },
            "capabilities": ["support_ticket.read", 42]
        }));

        assert_eq!(
            issues
                .iter()
                .map(|issue| issue.path.as_str())
                .collect::<Vec<_>>(),
            vec![
                "$.protocol",
                "$.name",
                "$.version",
                "$.source",
                "$.capabilities[1]"
            ]
        );
    }
}
