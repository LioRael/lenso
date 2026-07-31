use crate::{
    APPLICATION_MODULE_LOCK_PROTOCOL, ApplicationModuleLock, EndpointBinding,
    EndpointResolverSource, InstalledServiceRelease, ModulePlanningContext,
    SERVICE_INSTALLATION_SET_PROTOCOL, ServiceDesiredMode, ServiceInstallation,
    ServiceInstallationSet, ServiceReference, ServiceTransportBinding,
};
use lenso_contracts::{ModuleDelivery, ModuleManifest, ServiceResponsibilityProfile, digest_json};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const PROVIDER_RUNTIME_PLAN_PROTOCOL: &str = "lenso.provider-runtime-plan.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProviderRuntimePlan {
    pub protocol: String,
    pub system_id: String,
    pub application_id: String,
    pub environment_id: String,
    pub application_lock_digest: String,
    pub service_installation_revision: u64,
    pub providers: Vec<ProviderRuntimeService>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProviderRuntimeService {
    pub service_ref: ServiceReference,
    pub service_release: InstalledServiceRelease,
    pub endpoint_binding: EndpointBinding,
    pub modules: Vec<ProviderRuntimeModule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProviderRuntimeModule {
    pub export_key: String,
    pub module_id: String,
    pub module_version: String,
    pub module_release_digest: String,
    pub manifest_digest: String,
    pub contract_digests: Vec<String>,
    pub manifest: ModuleManifest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProviderRuntimePlanIssueCode {
    UnsupportedProtocol,
    IdentityMismatch,
    InvalidApplicationLock,
    ReleaseMissing,
    ReleaseMismatch,
    ManifestMismatch,
    InstallationMissing,
    InstallationInactive,
    ProfileMismatch,
    ServiceReleaseMismatch,
    ExportMissing,
    ExportMismatch,
    ProviderTransportUnavailable,
    ProviderEndpointUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProviderRuntimePlanIssue {
    pub code: ProviderRuntimePlanIssueCode,
    pub path: String,
    pub message: String,
    pub next_action: String,
}

#[derive(Debug, thiserror::Error)]
#[error("Provider runtime plan could not be compiled: {issues:?}")]
pub struct ProviderRuntimePlanError {
    pub issues: Vec<ProviderRuntimePlanIssue>,
}

/// Compiles the only runtime input accepted by a Provider transport adapter.
///
/// Canonical Manifests come from the exact Module Releases retained in the
/// planning context. Environment state contributes endpoints and identity
/// policy only. No live Provider response can add or replace a Module.
pub fn compile_provider_runtime_plan(
    module_lock: &ApplicationModuleLock,
    planning_context: &ModulePlanningContext,
    installations: &ServiceInstallationSet,
) -> Result<ProviderRuntimePlan, ProviderRuntimePlanError> {
    let mut issues = validate_roots(module_lock, planning_context, installations);
    let application_lock_digest = match digest_json(module_lock) {
        Ok(digest) => digest,
        Err(error) => {
            issues.push(issue(
                ProviderRuntimePlanIssueCode::InvalidApplicationLock,
                "$.application_lock",
                format!("Application Module Lock cannot be canonicalized: {error}"),
                "regenerate the Application Module Lock from a reviewed plan",
            ));
            String::new()
        }
    };

    let candidates = planning_context
        .candidates
        .iter()
        .map(|candidate| (candidate.release_digest.as_str(), candidate))
        .collect::<BTreeMap<_, _>>();
    let installation_index = installations
        .services
        .iter()
        .map(|installation| (installation.service_ref.clone(), installation))
        .collect::<BTreeMap<_, _>>();
    let mut providers = BTreeMap::<ServiceReference, ProviderRuntimeService>::new();

    for (index, locked) in module_lock.modules.iter().enumerate() {
        let ModuleDelivery::Service(delivery) = &locked.delivery else {
            continue;
        };
        if delivery.responsibility_profile != ServiceResponsibilityProfile::Provider {
            continue;
        }
        let path = format!("$.application_lock.modules[{index}]");
        let Some(candidate) = candidates.get(locked.release_digest.as_str()) else {
            issues.push(issue(
                ProviderRuntimePlanIssueCode::ReleaseMissing,
                &path,
                format!(
                    "locked Provider Module {} has no exact immutable Module Release",
                    locked.module_id
                ),
                "restore the exact planning context used to create the Application Module Lock",
            ));
            continue;
        };
        let release = &candidate.release;
        if release.module_id != locked.module_id
            || release.version != locked.version
            || release.delivery != locked.delivery
        {
            issues.push(issue(
                ProviderRuntimePlanIssueCode::ReleaseMismatch,
                &path,
                format!(
                    "locked Provider Module {} does not match its immutable Module Release",
                    locked.module_id
                ),
                "re-resolve the Module graph from the verified Catalog Snapshot",
            ));
            continue;
        }
        if release.manifest_digest != locked.manifest_digest
            || digest_json(&release.manifest).ok().as_deref() != Some(&locked.manifest_digest)
        {
            issues.push(issue(
                ProviderRuntimePlanIssueCode::ManifestMismatch,
                &format!("{path}.manifest_digest"),
                format!(
                    "locked Manifest digest for {} does not match canonical Release bytes",
                    locked.module_id
                ),
                "restore the exact verified Module Release and regenerate the lock",
            ));
            continue;
        }

        let service_ref = ServiceReference {
            system_id: planning_context.system_id.clone(),
            service_id: delivery.service_id.clone(),
        };
        let Some(installation) = installation_index.get(&service_ref) else {
            issues.push(issue(
                ProviderRuntimePlanIssueCode::InstallationMissing,
                &path,
                format!("Provider Service {} is not installed", delivery.service_id),
                "apply the exact Service Installation plan before activating this Module",
            ));
            continue;
        };
        validate_installation(installation, delivery, &path, &mut issues);
        let Some(export) = installation
            .exports
            .iter()
            .find(|export| export.export_key == delivery.export)
        else {
            issues.push(issue(
                ProviderRuntimePlanIssueCode::ExportMissing,
                &path,
                format!(
                    "Provider Service {} does not install export {}",
                    delivery.service_id, delivery.export
                ),
                "install the exact Service Release that owns the locked export",
            ));
            continue;
        };
        if export.module_id != locked.module_id
            || export.module_version != locked.version
            || export.module_release_digest != locked.release_digest
            || export.manifest_digest != locked.manifest_digest
            || export.contract_digests != delivery.contract_digests
        {
            issues.push(issue(
                ProviderRuntimePlanIssueCode::ExportMismatch,
                &path,
                format!(
                    "installed export {} does not match locked Module {}",
                    delivery.export, locked.module_id
                ),
                "re-plan the Service Installation and Module graph as one exact target",
            ));
            continue;
        }

        providers
            .entry(service_ref.clone())
            .or_insert_with(|| ProviderRuntimeService {
                service_ref,
                service_release: installation.service_release.clone(),
                endpoint_binding: installation.endpoint_binding.clone(),
                modules: Vec::new(),
            })
            .modules
            .push(ProviderRuntimeModule {
                export_key: delivery.export.clone(),
                module_id: locked.module_id.clone(),
                module_version: locked.version.clone(),
                module_release_digest: locked.release_digest.clone(),
                manifest_digest: locked.manifest_digest.clone(),
                contract_digests: delivery.contract_digests.clone(),
                manifest: release.manifest.clone(),
            });
    }

    if !issues.is_empty() {
        issues.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then_with(|| format!("{:?}", left.code).cmp(&format!("{:?}", right.code)))
        });
        return Err(ProviderRuntimePlanError { issues });
    }

    let mut providers = providers.into_values().collect::<Vec<_>>();
    for provider in &mut providers {
        provider.modules.sort_by(|left, right| {
            left.module_id
                .cmp(&right.module_id)
                .then_with(|| left.export_key.cmp(&right.export_key))
        });
    }
    Ok(ProviderRuntimePlan {
        protocol: PROVIDER_RUNTIME_PLAN_PROTOCOL.to_owned(),
        system_id: planning_context.system_id.clone(),
        application_id: module_lock.application_id.clone(),
        environment_id: planning_context.environment_id.clone(),
        application_lock_digest,
        service_installation_revision: installations.revision,
        providers,
    })
}

fn validate_roots(
    module_lock: &ApplicationModuleLock,
    planning_context: &ModulePlanningContext,
    installations: &ServiceInstallationSet,
) -> Vec<ProviderRuntimePlanIssue> {
    let mut issues = Vec::new();
    if module_lock.protocol != APPLICATION_MODULE_LOCK_PROTOCOL
        || planning_context.protocol != crate::MODULE_PLANNING_CONTEXT_PROTOCOL
        || installations.protocol != SERVICE_INSTALLATION_SET_PROTOCOL
    {
        issues.push(issue(
            ProviderRuntimePlanIssueCode::UnsupportedProtocol,
            "$",
            "Provider runtime inputs use an unsupported protocol",
            "regenerate management artifacts with the current Lenso version",
        ));
    }
    if module_lock.application_id != planning_context.application_id
        || planning_context.system_id != installations.system_id
        || planning_context.environment_id != installations.environment_id
    {
        issues.push(issue(
            ProviderRuntimePlanIssueCode::IdentityMismatch,
            "$",
            "Provider runtime inputs belong to different application, system, or environment identities",
            "load all runtime inputs from the same reviewed environment target",
        ));
    }
    if let Err(error) = crate::validate_application_module_lock(module_lock) {
        issues.push(issue(
            ProviderRuntimePlanIssueCode::InvalidApplicationLock,
            "$.application_lock",
            error.to_string(),
            "regenerate the Application Module Lock from a reviewed plan",
        ));
    }
    issues
}

fn validate_installation(
    installation: &ServiceInstallation,
    delivery: &lenso_contracts::ServiceModuleDelivery,
    path: &str,
    issues: &mut Vec<ProviderRuntimePlanIssue>,
) {
    if installation.desired_mode != ServiceDesiredMode::Active {
        issues.push(issue(
            ProviderRuntimePlanIssueCode::InstallationInactive,
            path,
            format!(
                "Provider Service {} is installed but inactive",
                delivery.service_id
            ),
            "activate the Service Installation before activating its Modules",
        ));
    }
    if installation.profile != ServiceResponsibilityProfile::Provider {
        issues.push(issue(
            ProviderRuntimePlanIssueCode::ProfileMismatch,
            path,
            format!(
                "Service {} is not installed with the Provider profile",
                delivery.service_id
            ),
            "replace the Service Installation with the exact Provider profile",
        ));
    }
    if installation.service_release.version != delivery.service_release_version
        || installation.service_release.digest != delivery.service_release_digest
    {
        issues.push(issue(
            ProviderRuntimePlanIssueCode::ServiceReleaseMismatch,
            path,
            format!(
                "installed Service Release for {} differs from the locked release",
                delivery.service_id
            ),
            "apply the exact co-resolved Service Installation update",
        ));
    }
    let provider_bindings = installation
        .endpoint_binding
        .allowed_bindings
        .iter()
        .copied()
        .filter(is_provider_binding)
        .collect::<Vec<_>>();
    if provider_bindings.is_empty() {
        issues.push(issue(
            ProviderRuntimePlanIssueCode::ProviderTransportUnavailable,
            path,
            format!(
                "Provider Service {} allows no Provider V1 transport",
                delivery.service_id
            ),
            "configure Provider HTTP/JSON or Provider gRPC in the Endpoint Binding",
        ));
    } else if let EndpointResolverSource::Static { endpoints } =
        &installation.endpoint_binding.resolver_source
        && !endpoints.iter().any(|endpoint| {
            provider_bindings.contains(&endpoint.binding) && !endpoint.address.trim().is_empty()
        })
    {
        issues.push(issue(
            ProviderRuntimePlanIssueCode::ProviderEndpointUnavailable,
            path,
            format!(
                "Provider Service {} has no eligible static Provider endpoint",
                delivery.service_id
            ),
            "configure an endpoint matching an allowed Provider V1 transport",
        ));
    }
}

fn is_provider_binding(binding: &ServiceTransportBinding) -> bool {
    matches!(
        binding,
        ServiceTransportBinding::ProviderHttpJson | ServiceTransportBinding::ProviderGrpc
    )
}

fn issue(
    code: ProviderRuntimePlanIssueCode,
    path: impl Into<String>,
    message: impl Into<String>,
    next_action: impl Into<String>,
) -> ProviderRuntimePlanIssue {
    ProviderRuntimePlanIssue {
        code,
        path: path.into(),
        message: message.into(),
        next_action: next_action.into(),
    }
}
