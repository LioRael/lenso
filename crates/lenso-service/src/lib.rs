use axum::{Json, Router, routing::get};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub use lenso_contracts::ModuleManifest;

pub const SERVICE_CONTRACT_PROTOCOL: &str = "lenso.service.v1";
pub const AUTONOMOUS_SERVICE_PROTOCOL: &str = "lenso.service.v2";
pub const SERVICE_PACKAGE_PROTOCOL: &str = "lenso.service-package.v1";
pub const SERVICE_WORKSPACE_PROTOCOL: &str = "lenso.service-workspace.v1";
pub const SERVICE_RELEASE_PLAN_PROTOCOL: &str = "lenso.service-release-plan.v1";
pub const SERVICE_SYSTEM_PROTOCOL: &str = "lenso.system.v1";
pub const MODULE_CONTRACT_PROTOCOL: &str = "lenso.module.v1";
pub const MODULE_RELEASE_PROTOCOL: &str = "lenso.module-release.v1";
pub const SERVICE_CONTRACT_SCHEMA_JSON: &str =
    include_str!("../schemas/lenso-service.v1.schema.json");
pub const SERVICE_V2_CONTRACT_SCHEMA_JSON: &str =
    include_str!("../schemas/lenso-service.v2.schema.json");
pub const SERVICE_PACKAGE_SCHEMA_JSON: &str =
    include_str!("../schemas/lenso-service-package.v1.schema.json");
pub const SERVICE_WORKSPACE_SCHEMA_JSON: &str =
    include_str!("../schemas/lenso-service-workspace.v1.schema.json");
pub const SERVICE_SYSTEM_SCHEMA_JSON: &str = include_str!("../schemas/lenso-system.v1.schema.json");
pub const MODULE_CONTRACT_SCHEMA_JSON: &str =
    include_str!("../schemas/lenso-module.v1.schema.json");
pub const MODULE_RELEASE_SCHEMA_JSON: &str =
    include_str!("../schemas/lenso-module-release.v1.schema.json");
pub const LEGACY_SERVICE_V1_FIXTURE_JSON: &str =
    include_str!("../fixtures/contracts/v1/service-provider.json");
pub const LEGACY_SYSTEM_V1_FIXTURE_JSON: &str =
    include_str!("../fixtures/contracts/v1/system-provider.json");
pub const AUTONOMOUS_SERVICE_V2_FIXTURE_JSON: &str =
    include_str!("../fixtures/contracts/v2/autonomous-service.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractArtifactKind {
    Service,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractSemanticKind {
    Provider,
    ProviderSystem,
    AutonomousService,
}

impl ContractSemanticKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Provider => "provider",
            Self::ProviderSystem => "provider_system",
            Self::AutonomousService => "autonomous_service",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractOwner {
    Host,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSemantics {
    pub providers: Vec<String>,
    pub auth_owner: ContractOwner,
    pub proxy_policy_owner: ContractOwner,
    pub retry_owner: ContractOwner,
    pub runtime_queue_owner: ContractOwner,
    pub outbox_owner: ContractOwner,
    pub story_owner: ContractOwner,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractArtifactCheck {
    pub detected_protocol: String,
    pub artifact_kind: ContractArtifactKind,
    pub semantic_kind: ContractSemanticKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_semantics: Option<ProviderSemantics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autonomous_service: Option<AutonomousServiceSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutonomousServiceSummary {
    pub service_id: String,
    pub workloads: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractArtifactCheckErrorCode {
    AmbiguousProtocol,
    UnsupportedProtocol,
    InvalidArtifact,
    UnknownField,
    InvalidProtocol,
    InvalidServiceIdentity,
    InvalidWorkloadIdentity,
    WorkloadOwnerMismatch,
    DuplicateWorkloadIdentity,
    InvalidWorkloadRole,
    InvalidModuleIdentity,
    DuplicateModuleIdentity,
    InvalidStoreIdentity,
    StoreOwnerMismatch,
    DuplicateStoreIdentity,
    InvalidTenancyMode,
    InvalidOperatingRegion,
    DuplicateOperatingRegion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractArtifactCheckError {
    pub code: ContractArtifactCheckErrorCode,
    pub path: String,
    pub message: String,
    pub next_action: String,
}

impl std::fmt::Display for ContractArtifactCheckError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let payload = serde_json::to_string(self).map_err(|_| std::fmt::Error)?;
        formatter.write_str(&payload)
    }
}

impl std::error::Error for ContractArtifactCheckError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContractFixture {
    pub name: &'static str,
    pub protocol: &'static str,
    pub semantic_kind: ContractSemanticKind,
    pub json: &'static str,
}

pub const LEGACY_CONTRACT_FIXTURES: &[ContractFixture] = &[
    ContractFixture {
        name: "service-provider-v1",
        protocol: SERVICE_CONTRACT_PROTOCOL,
        semantic_kind: ContractSemanticKind::Provider,
        json: LEGACY_SERVICE_V1_FIXTURE_JSON,
    },
    ContractFixture {
        name: "system-provider-v1",
        protocol: SERVICE_SYSTEM_PROTOCOL,
        semantic_kind: ContractSemanticKind::ProviderSystem,
        json: LEGACY_SYSTEM_V1_FIXTURE_JSON,
    },
];

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSystem {
    pub protocol: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environments: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<ServiceSystemService>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<ServiceSystemModule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<ServiceSystemDependency>,
}

impl ServiceSystem {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            protocol: SERVICE_SYSTEM_PROTOCOL.to_owned(),
            name: name.into(),
            environments: Vec::new(),
            services: Vec::new(),
            modules: Vec::new(),
            dependencies: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSystemService {
    pub name: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSystemModule {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_to: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSystemDependency {
    pub from: String,
    pub capability: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSystemGraph {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environments: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<ServiceSystemGraphService>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<ServiceSystemGraphModule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<ServiceSystemGraphDependency>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<ServiceSystemGraphIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSystemGraphService {
    pub name: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSystemGraphModule {
    pub name: String,
    pub owner: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSystemGraphDependency {
    pub from: String,
    pub capability: String,
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSystemGraphIssue {
    pub code: String,
    pub message: String,
}

#[must_use]
pub fn service_system_graph(system: &ServiceSystem) -> ServiceSystemGraph {
    let services_by_name = system
        .services
        .iter()
        .map(|service| (service.name.as_str(), service))
        .collect::<BTreeMap<_, _>>();
    let modules_by_name = system
        .modules
        .iter()
        .map(|module| (module.name.as_str(), module))
        .collect::<BTreeMap<_, _>>();
    let mut module_owner = BTreeMap::new();
    let mut issues = Vec::new();
    for service in &system.services {
        for module_name in &service.modules {
            if !modules_by_name.contains_key(module_name.as_str()) {
                issues.push(ServiceSystemGraphIssue {
                    code: "module_not_declared".to_owned(),
                    message: format!(
                        "Service `{}` references undeclared module `{module_name}`.",
                        service.name
                    ),
                });
            }
            if let Some(existing) = module_owner.insert(module_name.as_str(), service.name.as_str())
            {
                issues.push(ServiceSystemGraphIssue {
                    code: "module_owned_twice".to_owned(),
                    message: format!(
                        "Module `{module_name}` is assigned to both `{existing}` and `{}`.",
                        service.name
                    ),
                });
            }
        }
    }
    for module in &system.modules {
        if let Some(service_name) = module
            .install_to
            .as_deref()
            .and_then(|install_to| install_to.strip_prefix("service:"))
            && !services_by_name.contains_key(service_name)
        {
            issues.push(ServiceSystemGraphIssue {
                code: "install_target_missing".to_owned(),
                message: format!(
                    "Module `{}` installs to missing service `{service_name}`.",
                    module.name
                ),
            });
        }
    }

    let capability_owners = service_system_capability_owners(system, &module_owner);
    let mut dependencies = Vec::new();
    for module in &system.modules {
        let from = service_system_module_owner(module, &module_owner);
        for capability in &module.dependencies {
            dependencies.push(service_system_dependency_edge(
                from,
                capability,
                capability_owners
                    .get(capability.as_str())
                    .map(Vec::as_slice),
            ));
        }
    }
    for dependency in &system.dependencies {
        if let Some(to) = dependency.to.as_deref() {
            let target_exists =
                services_by_name.contains_key(to) || modules_by_name.contains_key(to);
            let target_has_capability = service_system_target_owns_capability(
                to,
                &dependency.capability,
                &capability_owners,
                &modules_by_name,
            );
            dependencies.push(ServiceSystemGraphDependency {
                from: dependency.from.clone(),
                capability: dependency.capability.clone(),
                state: if !target_exists {
                    "unresolved".to_owned()
                } else if target_has_capability {
                    "resolved".to_owned()
                } else {
                    "missing_capability".to_owned()
                },
                to: Some(to.to_owned()),
            });
        } else {
            dependencies.push(service_system_dependency_edge(
                &dependency.from,
                &dependency.capability,
                capability_owners
                    .get(dependency.capability.as_str())
                    .map(Vec::as_slice),
            ));
        }
    }
    for dependency in &dependencies {
        if dependency.state != "resolved" {
            issues.push(ServiceSystemGraphIssue {
                code: format!("dependency_{}", dependency.state),
                message: format!(
                    "`{}` depends on `{}`, but it is {}.",
                    dependency.from, dependency.capability, dependency.state
                ),
            });
        }
    }

    ServiceSystemGraph {
        name: system.name.clone(),
        environments: system.environments.clone(),
        services: system
            .services
            .iter()
            .map(|service| ServiceSystemGraphService {
                name: service.name.clone(),
                target: service.target.clone(),
                modules: service.modules.clone(),
            })
            .collect(),
        modules: system
            .modules
            .iter()
            .map(|module| ServiceSystemGraphModule {
                name: module.name.clone(),
                owner: service_system_module_owner(module, &module_owner).to_owned(),
                capabilities: module.capabilities.clone(),
                dependencies: module.dependencies.clone(),
            })
            .collect(),
        dependencies,
        issues,
    }
}

fn service_system_install_owner(module: &ServiceSystemModule) -> Option<&str> {
    let install_to = module.install_to.as_deref()?;
    install_to.strip_prefix("service:").or(Some(install_to))
}

fn service_system_module_owner<'a>(
    module: &'a ServiceSystemModule,
    module_owner: &BTreeMap<&'a str, &'a str>,
) -> &'a str {
    module_owner
        .get(module.name.as_str())
        .copied()
        .or_else(|| service_system_install_owner(module))
        .unwrap_or("host")
}

fn service_system_capability_owners<'a>(
    system: &'a ServiceSystem,
    module_owner: &BTreeMap<&'a str, &'a str>,
) -> BTreeMap<&'a str, Vec<&'a str>> {
    let mut owners: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for module in &system.modules {
        let owner = service_system_module_owner(module, module_owner);
        for capability in &module.capabilities {
            owners.entry(capability.as_str()).or_default().push(owner);
        }
    }
    owners
}

fn service_system_target_owns_capability(
    target: &str,
    capability: &str,
    capability_owners: &BTreeMap<&str, Vec<&str>>,
    modules_by_name: &BTreeMap<&str, &ServiceSystemModule>,
) -> bool {
    capability_owners
        .get(capability)
        .is_some_and(|owners| owners.iter().any(|owner| *owner == target))
        || modules_by_name.get(target).is_some_and(|module| {
            module
                .capabilities
                .iter()
                .any(|provided| provided == capability)
        })
}

fn service_system_dependency_edge(
    from: &str,
    capability: &str,
    owners: Option<&[&str]>,
) -> ServiceSystemGraphDependency {
    let (state, to) = match owners {
        Some(owners) if owners.len() == 1 => ("resolved", Some(owners[0].to_owned())),
        Some(owners) if owners.len() > 1 => ("ambiguous", Some(owners.join(","))),
        _ => ("unresolved", None),
    };
    ServiceSystemGraphDependency {
        from: from.to_owned(),
        capability: capability.to_owned(),
        state: state.to_owned(),
        to,
    }
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceTenancyMode {
    None,
    Optional,
    Required,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum WorkloadRole {
    Api,
    Worker,
    Migration,
    Other(String),
}

impl WorkloadRole {
    pub const API: Self = Self::Api;
    pub const WORKER: Self = Self::Worker;
    pub const MIGRATION: Self = Self::Migration;

    #[must_use]
    pub fn new(role: impl Into<String>) -> Self {
        match role.into().as_str() {
            "api" => Self::Api,
            "worker" => Self::Worker,
            "migration" => Self::Migration,
            role => Self::Other(role.to_owned()),
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Api => "api",
            Self::Worker => "worker",
            Self::Migration => "migration",
            Self::Other(role) => role,
        }
    }
}

impl Serialize for WorkloadRole {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for WorkloadRole {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::new)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousServiceWorkload {
    pub workload_id: String,
    pub service_id: String,
    pub role: WorkloadRole,
}

impl AutonomousServiceWorkload {
    #[must_use]
    pub fn new(
        workload_id: impl Into<String>,
        service_id: impl Into<String>,
        role: WorkloadRole,
    ) -> Self {
        Self {
            workload_id: workload_id.into(),
            service_id: service_id.into(),
            role,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousServiceStore {
    pub store_id: String,
    pub service_id: String,
}

impl AutonomousServiceStore {
    #[must_use]
    pub fn new(store_id: impl Into<String>, service_id: impl Into<String>) -> Self {
        Self {
            store_id: store_id.into(),
            service_id: service_id.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousServiceContract {
    pub protocol: String,
    pub service_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub workloads: Vec<AutonomousServiceWorkload>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stores: Vec<AutonomousServiceStore>,
    pub tenancy_mode: ServiceTenancyMode,
    pub operating_regions: Vec<String>,
}

impl AutonomousServiceContract {
    #[must_use]
    pub fn new(
        service_id: impl Into<String>,
        workloads: Vec<AutonomousServiceWorkload>,
        tenancy_mode: ServiceTenancyMode,
        operating_regions: Vec<String>,
    ) -> Self {
        Self {
            protocol: AUTONOMOUS_SERVICE_PROTOCOL.to_owned(),
            service_id: service_id.into(),
            version: None,
            workloads,
            modules: Vec::new(),
            stores: Vec::new(),
            tenancy_mode,
            operating_regions,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousServiceIssueCode {
    UnknownField,
    InvalidProtocol,
    InvalidServiceIdentity,
    InvalidWorkloadIdentity,
    WorkloadOwnerMismatch,
    DuplicateWorkloadIdentity,
    InvalidWorkloadRole,
    InvalidModuleIdentity,
    DuplicateModuleIdentity,
    InvalidStoreIdentity,
    StoreOwnerMismatch,
    DuplicateStoreIdentity,
    InvalidTenancyMode,
    InvalidOperatingRegion,
    DuplicateOperatingRegion,
}

impl From<AutonomousServiceIssueCode> for ContractArtifactCheckErrorCode {
    fn from(code: AutonomousServiceIssueCode) -> Self {
        match code {
            AutonomousServiceIssueCode::UnknownField => Self::UnknownField,
            AutonomousServiceIssueCode::InvalidProtocol => Self::InvalidProtocol,
            AutonomousServiceIssueCode::InvalidServiceIdentity => Self::InvalidServiceIdentity,
            AutonomousServiceIssueCode::InvalidWorkloadIdentity => Self::InvalidWorkloadIdentity,
            AutonomousServiceIssueCode::WorkloadOwnerMismatch => Self::WorkloadOwnerMismatch,
            AutonomousServiceIssueCode::DuplicateWorkloadIdentity => {
                Self::DuplicateWorkloadIdentity
            }
            AutonomousServiceIssueCode::InvalidWorkloadRole => Self::InvalidWorkloadRole,
            AutonomousServiceIssueCode::InvalidModuleIdentity => Self::InvalidModuleIdentity,
            AutonomousServiceIssueCode::DuplicateModuleIdentity => Self::DuplicateModuleIdentity,
            AutonomousServiceIssueCode::InvalidStoreIdentity => Self::InvalidStoreIdentity,
            AutonomousServiceIssueCode::StoreOwnerMismatch => Self::StoreOwnerMismatch,
            AutonomousServiceIssueCode::DuplicateStoreIdentity => Self::DuplicateStoreIdentity,
            AutonomousServiceIssueCode::InvalidTenancyMode => Self::InvalidTenancyMode,
            AutonomousServiceIssueCode::InvalidOperatingRegion => Self::InvalidOperatingRegion,
            AutonomousServiceIssueCode::DuplicateOperatingRegion => Self::DuplicateOperatingRegion,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutonomousServiceIssue {
    pub code: AutonomousServiceIssueCode,
    pub path: String,
    pub message: String,
    pub next_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceContract {
    #[serde(default = "default_service_contract_protocol")]
    pub protocol: String,
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
            protocol: SERVICE_CONTRACT_PROTOCOL.to_owned(),
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

fn default_service_contract_protocol() -> String {
    SERVICE_CONTRACT_PROTOCOL.to_owned()
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

/// Checks a versioned Provider-era contract artifact and projects its semantic meaning.
///
/// The returned read model is separate from the source JSON so compatibility checks never
/// rewrite a legacy artifact or reinterpret Provider declarations as Autonomous Services.
pub fn check_contract_artifact_value(
    value: &Value,
) -> Result<ContractArtifactCheck, ContractArtifactCheckError> {
    let Some(object) = value.as_object() else {
        return Err(ambiguous_protocol_error(
            "artifact must be an object with an explicit versioned protocol",
        ));
    };
    let Some(protocol) = object
        .get("protocol")
        .and_then(Value::as_str)
        .filter(|protocol| !protocol.trim().is_empty())
    else {
        return Err(ambiguous_protocol_error(
            "artifact protocol is required to determine its semantic kind",
        ));
    };

    if protocol == AUTONOMOUS_SERVICE_PROTOCOL {
        let issues = validate_autonomous_service_contract_value(value);
        if let Some(issue) = issues.first() {
            return Err(ContractArtifactCheckError {
                code: issue.code.into(),
                path: issue.path.clone(),
                message: issue.message.clone(),
                next_action: issue.next_action.clone(),
            });
        }
        let contract: AutonomousServiceContract =
            serde_json::from_value(value.clone()).map_err(|error| ContractArtifactCheckError {
                code: ContractArtifactCheckErrorCode::InvalidArtifact,
                path: "$".to_owned(),
                message: error.to_string(),
                next_action: "Fix the reported contract field and run the check again.".to_owned(),
            })?;
        return Ok(ContractArtifactCheck {
            detected_protocol: protocol.to_owned(),
            artifact_kind: ContractArtifactKind::Service,
            semantic_kind: ContractSemanticKind::AutonomousService,
            provider_semantics: None,
            autonomous_service: Some(AutonomousServiceSummary {
                service_id: contract.service_id,
                workloads: {
                    let mut workloads = contract
                        .workloads
                        .into_iter()
                        .map(|workload| workload.workload_id)
                        .collect::<Vec<_>>();
                    workloads.sort();
                    workloads
                },
            }),
        });
    }

    let (artifact_kind, semantic_kind, issues) = match protocol {
        SERVICE_CONTRACT_PROTOCOL => (
            ContractArtifactKind::Service,
            ContractSemanticKind::Provider,
            validate_service_contract_value(value),
        ),
        SERVICE_SYSTEM_PROTOCOL => (
            ContractArtifactKind::System,
            ContractSemanticKind::ProviderSystem,
            validate_service_system_value(value),
        ),
        _ => {
            return Err(ContractArtifactCheckError {
                code: ContractArtifactCheckErrorCode::UnsupportedProtocol,
                path: "$.protocol".to_owned(),
                message: format!("unsupported artifact protocol `{protocol}`"),
                next_action: "Use a supported protocol or upgrade Lenso for this artifact version."
                    .to_owned(),
            });
        }
    };

    if let Some(issue) = issues.first() {
        return Err(ContractArtifactCheckError {
            code: ContractArtifactCheckErrorCode::InvalidArtifact,
            path: issue.path.clone(),
            message: issue.message.clone(),
            next_action: "Fix the reported contract field and run the check again.".to_owned(),
        });
    }

    let mut providers: Vec<String> = match artifact_kind {
        ContractArtifactKind::Service => object
            .get("name")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .into_iter()
            .collect(),
        ContractArtifactKind::System => object
            .get("services")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|service| service.get("name").and_then(Value::as_str))
            .map(ToOwned::to_owned)
            .collect(),
    };
    providers.sort();
    providers.dedup();

    Ok(ContractArtifactCheck {
        detected_protocol: protocol.to_owned(),
        artifact_kind,
        semantic_kind,
        provider_semantics: Some(ProviderSemantics {
            providers,
            auth_owner: ContractOwner::Host,
            proxy_policy_owner: ContractOwner::Host,
            retry_owner: ContractOwner::Host,
            runtime_queue_owner: ContractOwner::Host,
            outbox_owner: ContractOwner::Host,
            story_owner: ContractOwner::Host,
        }),
        autonomous_service: None,
    })
}

fn ambiguous_protocol_error(message: &str) -> ContractArtifactCheckError {
    ContractArtifactCheckError {
        code: ContractArtifactCheckErrorCode::AmbiguousProtocol,
        path: "$.protocol".to_owned(),
        message: message.to_owned(),
        next_action: "Set `protocol` to a supported Provider-era artifact protocol.".to_owned(),
    }
}

#[must_use]
pub fn validate_autonomous_service_contract(
    contract: &AutonomousServiceContract,
) -> Vec<AutonomousServiceIssue> {
    validate_autonomous_service_contract_value(
        &serde_json::to_value(contract).expect("AutonomousServiceContract must serialize"),
    )
}

#[must_use]
pub fn validate_autonomous_service_contract_value(value: &Value) -> Vec<AutonomousServiceIssue> {
    let mut issues = Vec::new();
    let Some(object) = value.as_object() else {
        push_autonomous_issue(
            &mut issues,
            AutonomousServiceIssueCode::InvalidServiceIdentity,
            "$",
            "service contract must be an object",
            "Use a JSON object for the Service declaration.",
        );
        return issues;
    };
    validate_unknown_fields(
        object,
        "$",
        &[
            "protocol",
            "serviceId",
            "version",
            "workloads",
            "modules",
            "stores",
            "tenancyMode",
            "operatingRegions",
        ],
        &mut issues,
    );
    if object.get("protocol").and_then(Value::as_str) != Some(AUTONOMOUS_SERVICE_PROTOCOL) {
        push_autonomous_issue(
            &mut issues,
            AutonomousServiceIssueCode::InvalidProtocol,
            "$.protocol",
            "protocol must be `lenso.service.v2`",
            "Set `protocol` to `lenso.service.v2`.",
        );
    }
    let service_id = object
        .get("serviceId")
        .and_then(Value::as_str)
        .unwrap_or("");
    if service_id.trim().is_empty() {
        push_autonomous_issue(
            &mut issues,
            AutonomousServiceIssueCode::InvalidServiceIdentity,
            "$.serviceId",
            "serviceId must be a non-empty string",
            "Assign one stable logical Service identity.",
        );
    }
    let mut workload_ids = BTreeSet::new();
    match object.get("workloads").and_then(Value::as_array) {
        Some(workloads) if !workloads.is_empty() => {
            for (index, workload) in workloads.iter().enumerate() {
                let path = format!("$.workloads[{index}]");
                if let Some(object) = workload.as_object() {
                    validate_unknown_fields(
                        object,
                        &path,
                        &["workloadId", "serviceId", "role"],
                        &mut issues,
                    );
                }
                let id = workload
                    .get("workloadId")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if id.trim().is_empty() {
                    push_autonomous_issue(
                        &mut issues,
                        AutonomousServiceIssueCode::InvalidWorkloadIdentity,
                        format!("{path}.workloadId"),
                        "workloadId must be a non-empty string",
                        "Assign a unique identity to this Workload.",
                    );
                } else if !workload_ids.insert(id) {
                    push_autonomous_issue(
                        &mut issues,
                        AutonomousServiceIssueCode::DuplicateWorkloadIdentity,
                        format!("{path}.workloadId"),
                        "workloadId must be unique within the Service",
                        "Rename this Workload so each workloadId is unique.",
                    );
                }
                if workload.get("serviceId").and_then(Value::as_str) != Some(service_id) {
                    push_autonomous_issue(
                        &mut issues,
                        AutonomousServiceIssueCode::WorkloadOwnerMismatch,
                        format!("{path}.serviceId"),
                        "Workload owner must match the enclosing serviceId",
                        "Set the Workload serviceId to the enclosing Service identity.",
                    );
                }
                if workload
                    .get("role")
                    .and_then(Value::as_str)
                    .is_none_or(|role| role.trim().is_empty())
                {
                    push_autonomous_issue(
                        &mut issues,
                        AutonomousServiceIssueCode::InvalidWorkloadRole,
                        format!("{path}.role"),
                        "role must be a non-empty string",
                        "Use `api`, `worker`, `migration`, or a stable extension role.",
                    );
                }
            }
        }
        _ => push_autonomous_issue(
            &mut issues,
            AutonomousServiceIssueCode::InvalidWorkloadIdentity,
            "$.workloads",
            "workloads must contain at least one Workload",
            "Declare at least one API, Worker, Migration, or extension Workload.",
        ),
    }
    validate_owned_identities(
        object.get("stores"),
        "stores",
        "storeId",
        service_id,
        AutonomousServiceIssueCode::InvalidStoreIdentity,
        AutonomousServiceIssueCode::StoreOwnerMismatch,
        AutonomousServiceIssueCode::DuplicateStoreIdentity,
        &mut issues,
    );
    validate_unique_strings(
        object.get("modules"),
        "modules",
        AutonomousServiceIssueCode::InvalidModuleIdentity,
        AutonomousServiceIssueCode::DuplicateModuleIdentity,
        &mut issues,
    );
    match object.get("tenancyMode").and_then(Value::as_str) {
        Some("none" | "optional" | "required") => {}
        _ => push_autonomous_issue(
            &mut issues,
            AutonomousServiceIssueCode::InvalidTenancyMode,
            "$.tenancyMode",
            "tenancyMode must be `none`, `optional`, or `required`",
            "Choose one supported Tenancy Mode.",
        ),
    }
    validate_unique_strings(
        object.get("operatingRegions"),
        "operatingRegions",
        AutonomousServiceIssueCode::InvalidOperatingRegion,
        AutonomousServiceIssueCode::DuplicateOperatingRegion,
        &mut issues,
    );
    if object
        .get("operatingRegions")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty)
    {
        push_autonomous_issue(
            &mut issues,
            AutonomousServiceIssueCode::InvalidOperatingRegion,
            "$.operatingRegions",
            "at least one Operating Region is required",
            "Declare at least one logical Operating Region.",
        );
    }
    issues
}

fn push_autonomous_issue(
    issues: &mut Vec<AutonomousServiceIssue>,
    code: AutonomousServiceIssueCode,
    path: impl Into<String>,
    message: impl Into<String>,
    next_action: impl Into<String>,
) {
    issues.push(AutonomousServiceIssue {
        code,
        path: path.into(),
        message: message.into(),
        next_action: next_action.into(),
    });
}

fn validate_unknown_fields(
    object: &serde_json::Map<String, Value>,
    path: &str,
    allowed: &[&str],
    issues: &mut Vec<AutonomousServiceIssue>,
) {
    let mut unknown = object
        .keys()
        .filter(|key| !allowed.contains(&key.as_str()))
        .collect::<Vec<_>>();
    unknown.sort();
    for field in unknown {
        push_autonomous_issue(
            issues,
            AutonomousServiceIssueCode::UnknownField,
            format!("{path}.{field}"),
            format!("unknown field `{field}`"),
            "Remove the field or upgrade to a contract version that declares it.",
        );
    }
}

fn validate_unique_strings(
    value: Option<&Value>,
    field: &str,
    invalid: AutonomousServiceIssueCode,
    duplicate: AutonomousServiceIssueCode,
    issues: &mut Vec<AutonomousServiceIssue>,
) {
    let Some(values) = value.and_then(Value::as_array) else {
        return;
    };
    let mut seen = BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        let path = format!("$.{field}[{index}]");
        let Some(identity) = value
            .as_str()
            .filter(|identity| !identity.trim().is_empty())
        else {
            push_autonomous_issue(
                issues,
                invalid,
                path,
                format!("{field} identity must be a non-empty string"),
                format!("Assign a non-empty {field} identity."),
            );
            continue;
        };
        if !seen.insert(identity) {
            push_autonomous_issue(
                issues,
                duplicate,
                path,
                format!("{field} identities must be unique"),
                format!("Remove or rename the duplicate {field} identity."),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_owned_identities(
    value: Option<&Value>,
    field: &str,
    identity_field: &str,
    service_id: &str,
    invalid: AutonomousServiceIssueCode,
    owner_mismatch: AutonomousServiceIssueCode,
    duplicate: AutonomousServiceIssueCode,
    issues: &mut Vec<AutonomousServiceIssue>,
) {
    let Some(values) = value.and_then(Value::as_array) else {
        return;
    };
    let mut seen = BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        let base = format!("$.{field}[{index}]");
        if let Some(object) = value.as_object() {
            validate_unknown_fields(object, &base, &[identity_field, "serviceId"], issues);
        }
        let identity = value
            .get(identity_field)
            .and_then(Value::as_str)
            .unwrap_or("");
        if identity.trim().is_empty() {
            push_autonomous_issue(
                issues,
                invalid,
                format!("{base}.{identity_field}"),
                "identity must be a non-empty string",
                "Assign a stable logical identity.",
            );
        }
        if value.get("serviceId").and_then(Value::as_str) != Some(service_id) {
            push_autonomous_issue(
                issues,
                owner_mismatch,
                format!("{base}.serviceId"),
                "owner must match the enclosing serviceId",
                "Set serviceId to the enclosing Service identity.",
            );
        }
        if !identity.is_empty() && !seen.insert(identity) {
            push_autonomous_issue(
                issues,
                duplicate,
                format!("{base}.{identity_field}"),
                "identity must be unique within the Service",
                "Rename the duplicate identity.",
            );
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
    if let Some(protocol) = object.get("protocol") {
        match protocol.as_str() {
            Some(SERVICE_CONTRACT_PROTOCOL) => {}
            Some(_) => issues.push(ServiceContractIssue::new(
                "$.protocol",
                format!("protocol must be `{SERVICE_CONTRACT_PROTOCOL}`"),
            )),
            None => issues.push(ServiceContractIssue::new(
                "$.protocol",
                "field must be a non-empty string",
            )),
        }
    }
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
pub fn validate_service_system_value(value: &Value) -> Vec<ServiceContractIssue> {
    let Some(object) = value.as_object() else {
        return vec![ServiceContractIssue::new(
            "$",
            "service system must be an object",
        )];
    };

    let mut issues = Vec::new();
    match object.get("protocol").and_then(Value::as_str) {
        Some(SERVICE_SYSTEM_PROTOCOL) => {}
        Some(_) => issues.push(ServiceContractIssue::new(
            "$.protocol",
            format!("protocol must be `{SERVICE_SYSTEM_PROTOCOL}`"),
        )),
        None => issues.push(ServiceContractIssue::new(
            "$.protocol",
            "field must be a non-empty string",
        )),
    }
    require_non_empty_string(object.get("name"), "$.name", &mut issues);
    validate_string_array(object.get("environments"), "$.environments", &mut issues);
    validate_system_services(object.get("services"), &mut issues);
    validate_system_modules(object.get("modules"), &mut issues);
    validate_system_dependencies(object.get("dependencies"), &mut issues);
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

fn validate_system_services(value: Option<&Value>, issues: &mut Vec<ServiceContractIssue>) {
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
        if let Some(name) = non_empty_string(
            object.get("name"),
            &format!("$.services[{index}].name"),
            issues,
        ) && !names.insert(name.to_owned())
        {
            issues.push(ServiceContractIssue::new(
                format!("$.services[{index}].name"),
                format!("service `{name}` is declared more than once"),
            ));
        }
        require_non_empty_string(
            object.get("target"),
            &format!("$.services[{index}].target"),
            issues,
        );
        validate_string_array(
            object.get("modules"),
            &format!("$.services[{index}].modules"),
            issues,
        );
    }
}

fn validate_system_modules(value: Option<&Value>, issues: &mut Vec<ServiceContractIssue>) {
    let Some(value) = value else {
        return;
    };
    let Some(array) = value.as_array() else {
        issues.push(ServiceContractIssue::new(
            "$.modules",
            "modules must be an array",
        ));
        return;
    };
    let mut names = BTreeSet::new();
    for (index, module) in array.iter().enumerate() {
        let Some(object) = module.as_object() else {
            issues.push(ServiceContractIssue::new(
                format!("$.modules[{index}]"),
                "module must be an object",
            ));
            continue;
        };
        if let Some(name) = non_empty_string(
            object.get("name"),
            &format!("$.modules[{index}].name"),
            issues,
        ) && !names.insert(name.to_owned())
        {
            issues.push(ServiceContractIssue::new(
                format!("$.modules[{index}].name"),
                format!("module `{name}` is declared more than once"),
            ));
        }
        if let Some(install_to) = object.get("installTo").or_else(|| object.get("install_to")) {
            require_non_empty_string(
                Some(install_to),
                &format!("$.modules[{index}].installTo"),
                issues,
            );
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

fn validate_system_dependencies(value: Option<&Value>, issues: &mut Vec<ServiceContractIssue>) {
    let Some(value) = value else {
        return;
    };
    let Some(array) = value.as_array() else {
        issues.push(ServiceContractIssue::new(
            "$.dependencies",
            "dependencies must be an array",
        ));
        return;
    };
    for (index, dependency) in array.iter().enumerate() {
        let Some(object) = dependency.as_object() else {
            issues.push(ServiceContractIssue::new(
                format!("$.dependencies[{index}]"),
                "dependency must be an object",
            ));
            continue;
        };
        require_non_empty_string(
            object.get("from"),
            &format!("$.dependencies[{index}].from"),
            issues,
        );
        require_non_empty_string(
            object.get("capability"),
            &format!("$.dependencies[{index}].capability"),
            issues,
        );
        if let Some(to) = object.get("to") {
            require_non_empty_string(Some(to), &format!("$.dependencies[{index}].to"), issues);
        }
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
