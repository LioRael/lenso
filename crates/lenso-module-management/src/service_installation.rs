use chrono::{DateTime, Utc};
use fs2::FileExt as _;
use lenso_contracts::{ServiceResponsibilityProfile, digest_json};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const SERVICE_INSTALLATION_SET_PROTOCOL: &str = "lenso.service-installations.v1";
pub const SERVICE_INSTALLATION_PLAN_PROTOCOL: &str = "lenso.service-install-plan.v1";
pub const SERVICE_INSTALLATION_RECEIPT_PROTOCOL: &str = "lenso.service-install-receipt.v1";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceReference {
    pub system_id: String,
    pub service_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServiceDesiredMode {
    Active,
    Inactive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InstalledServiceRelease {
    pub version: String,
    pub digest: String,
    pub immutable_locator: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InstalledServiceExport {
    pub export_key: String,
    pub module_id: String,
    pub module_version: String,
    pub module_release_digest: String,
    pub manifest_digest: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contract_digests: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConfigActivationIntent {
    Prepare,
    Activate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceConfigBinding {
    pub owner_id: String,
    pub config_contract_digest: String,
    pub config_revision_id: String,
    pub config_revision_digest: String,
    pub activation: ConfigActivationIntent,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secret_references: Vec<String>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ServiceTransportBinding {
    ProviderHttpJson,
    ProviderGrpc,
    DirectHttp,
    DirectGrpc,
    Event,
    SystemPlane,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StaticEndpointDeclaration {
    pub address: String,
    pub binding: ServiceTransportBinding,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_domain: Option<String>,
    #[serde(default)]
    pub priority: u32,
    #[serde(default = "default_weight")]
    pub weight: u32,
}

const fn default_weight() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EndpointResolverSource {
    Static {
        endpoints: Vec<StaticEndpointDeclaration>,
    },
    LocalProcess {
        source_id: String,
    },
    Adapter {
        adapter_id: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        public_config: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        secret_references: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceIdentityPolicy {
    pub principal: String,
    pub audience: String,
    pub trust_profile: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credential_references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EndpointSelectionPolicy {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preferred_regions: Vec<String>,
    #[serde(default)]
    pub require_distinct_failure_domains: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EndpointCachePolicy {
    pub maximum_age_seconds: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_if_source_unavailable_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EndpointBinding {
    pub binding_id: String,
    pub service_ref: ServiceReference,
    pub resolver_source: EndpointResolverSource,
    pub allowed_bindings: Vec<ServiceTransportBinding>,
    pub identity_policy: ServiceIdentityPolicy,
    #[serde(default)]
    pub selection_policy: EndpointSelectionPolicy,
    pub cache_policy: EndpointCachePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServiceLifecycleBinding {
    External {
        deployment_reference: String,
        observation_adapter_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        operation_adapter_id: Option<String>,
    },
    LocalSupervisor {
        supervisor_id: String,
        workload_artifact_digests: Vec<String>,
        working_directory: String,
        readiness_timeout_seconds: u64,
        shutdown_timeout_seconds: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceInstallation {
    pub service_ref: ServiceReference,
    pub profile: ServiceResponsibilityProfile,
    pub desired_mode: ServiceDesiredMode,
    pub service_release: InstalledServiceRelease,
    pub exports: Vec<InstalledServiceExport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config_bindings: Vec<ServiceConfigBinding>,
    pub endpoint_binding: EndpointBinding,
    pub lifecycle_binding: ServiceLifecycleBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceInstallationSet {
    pub protocol: String,
    pub system_id: String,
    pub environment_id: String,
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_state_digest: Option<String>,
    pub services: Vec<ServiceInstallation>,
}

impl ServiceInstallationSet {
    #[must_use]
    pub fn empty(system_id: impl Into<String>, environment_id: impl Into<String>) -> Self {
        Self {
            protocol: SERVICE_INSTALLATION_SET_PROTOCOL.to_owned(),
            system_id: system_id.into(),
            environment_id: environment_id.into(),
            revision: 0,
            previous_state_digest: None,
            services: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServiceInstallationChange {
    Install { installation: ServiceInstallation },
    Uninstall { service_ref: ServiceReference },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServiceInstallationPlanKind {
    Install,
    Update,
    Reuse,
    Uninstall,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceInstallationPlan {
    pub protocol: String,
    pub plan_id: String,
    pub plan_digest: String,
    pub system_id: String,
    pub environment_id: String,
    pub kind: ServiceInstallationPlanKind,
    pub change: ServiceInstallationChange,
    pub expected_revision: u64,
    pub expected_state_digest: String,
    pub target_revision: u64,
    pub target_state_digest: String,
    pub target: ServiceInstallationSet,
    pub required_authority: String,
    pub next_actions: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServiceInstallationOutcome {
    AppliedNeedsAttention,
    Reused,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceInstallationReceipt {
    pub protocol: String,
    pub receipt_id: String,
    pub operation_id: String,
    pub plan_id: String,
    pub plan_digest: String,
    pub actor_id: String,
    pub verified_authorities: Vec<String>,
    pub system_id: String,
    pub environment_id: String,
    pub service_ref: ServiceReference,
    pub prior_revision: u64,
    pub target_revision: u64,
    pub prior_state_digest: String,
    pub target_state_digest: String,
    pub outcome: ServiceInstallationOutcome,
    pub reasons: Vec<String>,
    pub next_actions: Vec<String>,
    pub committed_at: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub enum ServiceInstallationError {
    #[error("Service Installation contract is invalid: {0}")]
    InvalidContract(String),
    #[error("Service Installation state is stale")]
    StaleState,
    #[error("Service Installation operation requires authority `{0}`")]
    MissingAuthority(String),
    #[error("Service Installation operation identity is unsafe")]
    UnsafeOperationIdentity,
    #[error("Service Installation I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Service Installation JSON failed: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct WorkspaceServiceInstallationManager {
    root: PathBuf,
    system_id: String,
    environment_id: String,
}

impl WorkspaceServiceInstallationManager {
    pub fn new(
        root: impl Into<PathBuf>,
        system_id: impl Into<String>,
        environment_id: impl Into<String>,
    ) -> Self {
        Self {
            root: root.into(),
            system_id: system_id.into(),
            environment_id: environment_id.into(),
        }
    }

    pub fn snapshot(&self) -> Result<ServiceInstallationSet, ServiceInstallationError> {
        let path = self.state_path()?;
        match fs::read(path) {
            Ok(bytes) => {
                let state: ServiceInstallationSet = serde_json::from_slice(&bytes)?;
                validate_service_installation_set(&state)?;
                if state.system_id != self.system_id || state.environment_id != self.environment_id
                {
                    return invalid("Service Installation Set scope differs from target scope");
                }
                Ok(state)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(
                ServiceInstallationSet::empty(&self.system_id, &self.environment_id),
            ),
            Err(error) => Err(error.into()),
        }
    }

    pub fn preview(
        &self,
        change: ServiceInstallationChange,
        created_at: DateTime<Utc>,
    ) -> Result<ServiceInstallationPlan, ServiceInstallationError> {
        let current = self.snapshot()?;
        plan_service_installation(&current, change, created_at)
    }

    pub fn apply(
        &self,
        operation_id: &str,
        plan: &ServiceInstallationPlan,
        actor_id: &str,
        authorities: &BTreeSet<String>,
        now: DateTime<Utc>,
    ) -> Result<ServiceInstallationReceipt, ServiceInstallationError> {
        if !safe_id(operation_id) {
            return Err(ServiceInstallationError::UnsafeOperationIdentity);
        }
        if !authorities.contains(&plan.required_authority) {
            return Err(ServiceInstallationError::MissingAuthority(
                plan.required_authority.clone(),
            ));
        }
        validate_service_installation_plan(plan)?;
        let receipt_path = self.receipt_path(operation_id)?;
        if let Ok(bytes) = fs::read(&receipt_path) {
            let receipt: ServiceInstallationReceipt = serde_json::from_slice(&bytes)?;
            if receipt.protocol != SERVICE_INSTALLATION_RECEIPT_PROTOCOL
                || receipt.operation_id != operation_id
                || receipt.plan_id != plan.plan_id
                || receipt.plan_digest != plan.plan_digest
                || receipt.target_state_digest != plan.target_state_digest
            {
                return Err(ServiceInstallationError::StaleState);
            }
            return Ok(receipt);
        }

        let lock_path = self.environment_root()?.join("service-installations.lock");
        fs::create_dir_all(self.environment_root()?)?;
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(lock_path)?;
        lock.lock_exclusive()?;
        let result = self.apply_locked(operation_id, plan, actor_id, authorities, now);
        let unlock_result = lock.unlock();
        result.and_then(|receipt| {
            unlock_result?;
            Ok(receipt)
        })
    }

    fn apply_locked(
        &self,
        operation_id: &str,
        plan: &ServiceInstallationPlan,
        actor_id: &str,
        authorities: &BTreeSet<String>,
        now: DateTime<Utc>,
    ) -> Result<ServiceInstallationReceipt, ServiceInstallationError> {
        let current = self.snapshot()?;
        let current_digest = service_installation_set_digest(&current)?;
        if current.revision == plan.target_revision && current_digest == plan.target_state_digest {
            let receipt =
                service_installation_receipt(operation_id, plan, actor_id, authorities, now)?;
            atomic_write_json(&self.receipt_path(operation_id)?, &receipt)?;
            return Ok(receipt);
        }
        if current.revision != plan.expected_revision
            || current_digest != plan.expected_state_digest
        {
            return Err(ServiceInstallationError::StaleState);
        }
        let verified = plan_service_installation(&current, plan.change.clone(), plan.created_at)?;
        if &verified != plan {
            return Err(ServiceInstallationError::StaleState);
        }
        if plan.kind != ServiceInstallationPlanKind::Reuse {
            atomic_write_json(&self.state_path()?, &plan.target)?;
        }
        let receipt = service_installation_receipt(operation_id, plan, actor_id, authorities, now)?;
        atomic_write_json(&self.receipt_path(operation_id)?, &receipt)?;
        Ok(receipt)
    }

    fn environment_root(&self) -> Result<PathBuf, ServiceInstallationError> {
        if !safe_segment(&self.environment_id) {
            return invalid("environment identity is unsafe");
        }
        Ok(self
            .root
            .join(".lenso/environments")
            .join(&self.environment_id))
    }

    fn state_path(&self) -> Result<PathBuf, ServiceInstallationError> {
        Ok(self.environment_root()?.join("service-installations.json"))
    }

    fn receipt_path(&self, operation_id: &str) -> Result<PathBuf, ServiceInstallationError> {
        if !safe_id(operation_id) {
            return Err(ServiceInstallationError::UnsafeOperationIdentity);
        }
        Ok(self
            .environment_root()?
            .join("service-install-receipts")
            .join(format!("{operation_id}.json")))
    }
}

pub fn plan_service_installation(
    current: &ServiceInstallationSet,
    change: ServiceInstallationChange,
    created_at: DateTime<Utc>,
) -> Result<ServiceInstallationPlan, ServiceInstallationError> {
    validate_service_installation_set(current)?;
    let expected_state_digest = service_installation_set_digest(current)?;
    let mut target = current.clone();
    let kind = match &change {
        ServiceInstallationChange::Install { installation } => {
            validate_service_installation(installation)?;
            if installation.service_ref.system_id != current.system_id {
                return invalid("Service Reference system differs from Installation Set");
            }
            match target
                .services
                .binary_search_by(|candidate| candidate.service_ref.cmp(&installation.service_ref))
            {
                Ok(index) if target.services[index] == *installation => {
                    ServiceInstallationPlanKind::Reuse
                }
                Ok(index) => {
                    if target.services[index].profile != installation.profile {
                        return invalid(
                            "Service responsibility profile replacement requires a separate plan",
                        );
                    }
                    target.services[index] = installation.clone();
                    ServiceInstallationPlanKind::Update
                }
                Err(index) => {
                    target.services.insert(index, installation.clone());
                    ServiceInstallationPlanKind::Install
                }
            }
        }
        ServiceInstallationChange::Uninstall { service_ref } => {
            if service_ref.system_id != current.system_id {
                return invalid("Service Reference system differs from Installation Set");
            }
            let index = target
                .services
                .binary_search_by(|candidate| candidate.service_ref.cmp(service_ref))
                .map_err(|_| {
                    ServiceInstallationError::InvalidContract("Service is not installed".to_owned())
                })?;
            target.services.remove(index);
            ServiceInstallationPlanKind::Uninstall
        }
    };
    if kind != ServiceInstallationPlanKind::Reuse {
        target.previous_state_digest = Some(expected_state_digest.clone());
        target.revision = current.revision.saturating_add(1);
    }
    let target_state_digest = service_installation_set_digest(&target)?;
    let identity = digest_json(&(
        current.system_id.as_str(),
        current.environment_id.as_str(),
        &change,
        current.revision,
        expected_state_digest.as_str(),
        target_state_digest.as_str(),
    ))?;
    let mut plan = ServiceInstallationPlan {
        protocol: SERVICE_INSTALLATION_PLAN_PROTOCOL.to_owned(),
        plan_id: format!("service-install-plan:{}", &identity[7..23]),
        plan_digest: String::new(),
        system_id: current.system_id.clone(),
        environment_id: current.environment_id.clone(),
        kind,
        change,
        expected_revision: current.revision,
        expected_state_digest,
        target_revision: target.revision,
        target_state_digest,
        target,
        required_authority: "service.manage".to_owned(),
        next_actions: vec![
            "review_service_installation_plan".to_owned(),
            "apply_service_installation_plan".to_owned(),
        ],
        created_at,
    };
    plan.plan_digest = service_installation_plan_digest(&plan)?;
    validate_service_installation_plan(&plan)?;
    Ok(plan)
}

pub fn service_installation_set_digest(
    state: &ServiceInstallationSet,
) -> Result<String, serde_json::Error> {
    digest_json(state)
}

pub fn service_installation_plan_digest(
    plan: &ServiceInstallationPlan,
) -> Result<String, serde_json::Error> {
    let mut content = plan.clone();
    content.plan_digest.clear();
    digest_json(&content)
}

pub fn validate_service_installation_plan(
    plan: &ServiceInstallationPlan,
) -> Result<(), ServiceInstallationError> {
    if plan.protocol != SERVICE_INSTALLATION_PLAN_PROTOCOL
        || service_installation_plan_digest(plan)? != plan.plan_digest
        || plan.target_state_digest != service_installation_set_digest(&plan.target)?
        || plan.target.system_id != plan.system_id
        || plan.target.environment_id != plan.environment_id
        || plan.required_authority != "service.manage"
    {
        return invalid("Service Installation Plan identity or digest is invalid");
    }
    validate_service_installation_set(&plan.target)
}

pub fn validate_service_installation_set(
    state: &ServiceInstallationSet,
) -> Result<(), ServiceInstallationError> {
    if state.protocol != SERVICE_INSTALLATION_SET_PROTOCOL
        || !safe_identity(&state.system_id)
        || !safe_segment(&state.environment_id)
        || state
            .previous_state_digest
            .as_deref()
            .is_some_and(|digest| !valid_digest(digest))
    {
        return invalid("Service Installation Set identity, protocol, or digest is invalid");
    }
    require_sorted_unique(
        state.services.iter().map(|service| &service.service_ref),
        "Service References",
    )?;
    for service in &state.services {
        validate_service_installation(service)?;
        if service.service_ref.system_id != state.system_id {
            return invalid("installed Service belongs to another System");
        }
    }
    Ok(())
}

pub fn validate_service_installation(
    installation: &ServiceInstallation,
) -> Result<(), ServiceInstallationError> {
    if !safe_identity(&installation.service_ref.system_id)
        || !safe_identity(&installation.service_ref.service_id)
        || semver::Version::parse(&installation.service_release.version).is_err()
        || !valid_digest(&installation.service_release.digest)
        || installation
            .service_release
            .immutable_locator
            .trim()
            .is_empty()
        || installation.endpoint_binding.service_ref != installation.service_ref
        || installation.exports.is_empty()
        || installation.endpoint_binding.binding_id.trim().is_empty()
        || installation.endpoint_binding.allowed_bindings.is_empty()
        || installation
            .endpoint_binding
            .cache_policy
            .maximum_age_seconds
            == 0
        || installation
            .endpoint_binding
            .identity_policy
            .principal
            .trim()
            .is_empty()
        || installation
            .endpoint_binding
            .identity_policy
            .audience
            .trim()
            .is_empty()
        || installation
            .endpoint_binding
            .identity_policy
            .trust_profile
            .trim()
            .is_empty()
    {
        return invalid(
            "installed Service identity, release, endpoint, or identity policy is invalid",
        );
    }
    require_sorted_unique(
        installation
            .exports
            .iter()
            .map(|export| export.export_key.as_str()),
        "Service export keys",
    )?;
    require_sorted_unique(
        installation.endpoint_binding.allowed_bindings.iter(),
        "allowed endpoint bindings",
    )?;
    for export in &installation.exports {
        if export.export_key.trim().is_empty()
            || !valid_module_id(&export.module_id)
            || semver::Version::parse(&export.module_version).is_err()
            || !valid_digest(&export.module_release_digest)
            || !valid_digest(&export.manifest_digest)
            || export
                .contract_digests
                .iter()
                .any(|digest| !valid_digest(digest))
        {
            return invalid("installed Service export is invalid");
        }
        require_sorted_unique(
            export.contract_digests.iter(),
            "Service export contract digests",
        )?;
    }
    for binding in &installation.config_bindings {
        if binding.owner_id.trim().is_empty()
            || !valid_digest(&binding.config_contract_digest)
            || binding.config_revision_id.trim().is_empty()
            || !valid_digest(&binding.config_revision_digest)
        {
            return invalid("Service Config binding is invalid");
        }
        require_sorted_unique(binding.secret_references.iter(), "Secret References")?;
    }
    match &installation.endpoint_binding.resolver_source {
        EndpointResolverSource::Static { endpoints } => {
            if endpoints.is_empty()
                || endpoints.iter().any(|endpoint| {
                    endpoint.address.trim().is_empty()
                        || endpoint.weight == 0
                        || !installation
                            .endpoint_binding
                            .allowed_bindings
                            .contains(&endpoint.binding)
                })
            {
                return invalid("static Endpoint source is invalid");
            }
        }
        EndpointResolverSource::LocalProcess { source_id } => {
            if source_id.trim().is_empty() {
                return invalid("local process Endpoint source is invalid");
            }
        }
        EndpointResolverSource::Adapter {
            adapter_id,
            public_config,
            secret_references,
        } => {
            if adapter_id.trim().is_empty()
                || public_config.iter().any(|(key, value)| {
                    key.trim().is_empty()
                        || value.trim().is_empty()
                        || secret_shaped(key)
                        || secret_shaped(value)
                })
            {
                return invalid("external Endpoint adapter configuration is invalid");
            }
            require_sorted_unique(secret_references.iter(), "Endpoint Secret References")?;
        }
    }
    Ok(())
}

fn service_installation_receipt(
    operation_id: &str,
    plan: &ServiceInstallationPlan,
    actor_id: &str,
    authorities: &BTreeSet<String>,
    now: DateTime<Utc>,
) -> Result<ServiceInstallationReceipt, ServiceInstallationError> {
    let service_ref = match &plan.change {
        ServiceInstallationChange::Install { installation } => installation.service_ref.clone(),
        ServiceInstallationChange::Uninstall { service_ref } => service_ref.clone(),
    };
    let (outcome, reasons, next_actions) = match plan.kind {
        ServiceInstallationPlanKind::Reuse => (
            ServiceInstallationOutcome::Reused,
            vec!["desired_service_installation_already_matches".to_owned()],
            Vec::new(),
        ),
        ServiceInstallationPlanKind::Uninstall => (
            ServiceInstallationOutcome::Removed,
            vec!["desired_service_installation_removed".to_owned()],
            vec!["preserve_deployment_data_and_observations".to_owned()],
        ),
        ServiceInstallationPlanKind::Install | ServiceInstallationPlanKind::Update => (
            ServiceInstallationOutcome::AppliedNeedsAttention,
            vec!["runtime_readiness_requires_fresh_observation".to_owned()],
            vec!["observe_service_identity_and_readiness".to_owned()],
        ),
    };
    let mut verified_authorities = authorities.iter().cloned().collect::<Vec<_>>();
    verified_authorities.sort();
    let receipt_seed = digest_json(&(operation_id, plan.plan_digest.as_str(), actor_id, now))?;
    Ok(ServiceInstallationReceipt {
        protocol: SERVICE_INSTALLATION_RECEIPT_PROTOCOL.to_owned(),
        receipt_id: format!("service-install-receipt:{}", &receipt_seed[7..23]),
        operation_id: operation_id.to_owned(),
        plan_id: plan.plan_id.clone(),
        plan_digest: plan.plan_digest.clone(),
        actor_id: actor_id.to_owned(),
        verified_authorities,
        system_id: plan.system_id.clone(),
        environment_id: plan.environment_id.clone(),
        service_ref,
        prior_revision: plan.expected_revision,
        target_revision: plan.target_revision,
        prior_state_digest: plan.expected_state_digest.clone(),
        target_state_digest: plan.target_state_digest.clone(),
        outcome,
        reasons,
        next_actions,
        committed_at: now,
    })
}

fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), ServiceInstallationError> {
    let parent = path.parent().ok_or_else(|| {
        ServiceInstallationError::InvalidContract("state path has no parent".to_owned())
    })?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension(format!("json.tmp-{}", std::process::id()));
    let bytes = serde_json::to_vec_pretty(value)?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    fs::rename(&temporary, path)?;
    OpenOptions::new().read(true).open(parent)?.sync_all()?;
    Ok(())
}

fn require_sorted_unique<'a, T: Ord + ?Sized + 'a>(
    values: impl IntoIterator<Item = &'a T>,
    field: &str,
) -> Result<(), ServiceInstallationError> {
    let values = values.into_iter().collect::<Vec<_>>();
    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return invalid(format!("{field} must be sorted and unique"));
    }
    Ok(())
}

fn valid_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn safe_identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | '/')
        })
}

fn safe_segment(value: &str) -> bool {
    safe_identity(value) && !value.contains('/') && value != "." && value != ".."
}

fn safe_id(value: &str) -> bool {
    safe_segment(value) && value.len() <= 180
}

fn valid_module_id(value: &str) -> bool {
    value.split_once('/').is_some_and(|(namespace, name)| {
        !namespace.is_empty()
            && !name.is_empty()
            && !name.contains('/')
            && safe_identity(namespace)
            && safe_identity(name)
    })
}

fn secret_shaped(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    ["secret", "password", "credential", "privatekey", "token"]
        .iter()
        .any(|needle| normalized.contains(needle))
}

fn invalid<T>(message: impl Into<String>) -> Result<T, ServiceInstallationError> {
    Err(ServiceInstallationError::InvalidContract(message.into()))
}
