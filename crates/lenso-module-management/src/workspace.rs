use crate::{
    APPLICATION_MODULE_LOCK_PROTOCOL, ApplicationModuleLock, DESIRED_MODULE_COMPOSITION_PROTOCOL,
    DesiredModuleComposition, MODULE_MANAGEMENT_SNAPSHOT_PROTOCOL,
    MODULE_PLANNING_CONTEXT_PROTOCOL, ModuleChangePlan, ModuleChangePlanRequest,
    ModuleChangePlanner, ModuleChangePlannerError, ModuleManagementSnapshot,
    ModuleManagementSnapshotStatus, ModulePlanningContext, ModuleRootChange,
    application_module_lock_digest, desired_composition_digest,
};
use chrono::{DateTime, Utc};
use lenso_contracts::digest_json;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct WorkspaceModuleManagement {
    root: PathBuf,
    desired_path: PathBuf,
    lock_path: PathBuf,
    planning_context_path: PathBuf,
    environment_policy_path: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceModuleManagementError {
    #[error("Module management workspace I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Module management workspace JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Module planning context is unavailable")]
    PlanningUnavailable,
    #[error("Application Module Lock is unavailable")]
    ApplicationLockUnavailable,
    #[error("Module management workspace contract is invalid: {0}")]
    InvalidContract(String),
    #[error(transparent)]
    Planning(#[from] ModuleChangePlannerError),
    #[error(transparent)]
    ProviderRuntime(#[from] crate::ProviderRuntimePlanError),
    #[error(transparent)]
    ServiceInstallation(#[from] crate::ServiceInstallationError),
}

impl WorkspaceModuleManagement {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            desired_path: PathBuf::from("lenso.modules.json"),
            lock_path: PathBuf::from("lenso.modules.lock.json"),
            planning_context_path: PathBuf::from(".lenso/module-planning-context.json"),
            environment_policy_path: PathBuf::from(".lenso/module-environment-policy.json"),
        }
    }

    #[must_use]
    pub fn with_planning_context_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.planning_context_path = path.into();
        self
    }

    pub fn snapshot(&self) -> Result<ModuleManagementSnapshot, WorkspaceModuleManagementError> {
        let mut issues = Vec::new();
        let desired = read_optional::<DesiredModuleComposition>(
            &self.root.join(&self.desired_path),
            "desired_composition_invalid",
            &mut issues,
        );
        let application_lock = read_optional::<ApplicationModuleLock>(
            &self.root.join(&self.lock_path),
            "application_lock_invalid",
            &mut issues,
        );
        let planning_context = read_optional::<ModulePlanningContext>(
            &self.root.join(&self.planning_context_path),
            "planning_context_invalid",
            &mut issues,
        );
        let environment_policy = read_optional::<crate::ModuleEnvironmentPolicy>(
            &self.root.join(&self.environment_policy_path),
            "environment_policy_invalid",
            &mut issues,
        );
        if desired
            .as_ref()
            .is_some_and(|value| value.protocol != DESIRED_MODULE_COMPOSITION_PROTOCOL)
        {
            issues.push("desired_composition_protocol_unsupported".to_owned());
        }
        if application_lock
            .as_ref()
            .is_some_and(|value| value.protocol != APPLICATION_MODULE_LOCK_PROTOCOL)
        {
            issues.push("application_lock_protocol_unsupported".to_owned());
        }
        if planning_context
            .as_ref()
            .is_some_and(|value| value.protocol != MODULE_PLANNING_CONTEXT_PROTOCOL)
        {
            issues.push("planning_context_protocol_unsupported".to_owned());
        }
        if environment_policy
            .as_ref()
            .is_some_and(|value| value.protocol != crate::MODULE_ENVIRONMENT_POLICY_PROTOCOL)
        {
            issues.push("environment_policy_protocol_unsupported".to_owned());
        }
        let identities = desired
            .as_ref()
            .map(|value| value.application_id.as_str())
            .into_iter()
            .chain(
                application_lock
                    .as_ref()
                    .map(|value| value.application_id.as_str()),
            )
            .chain(
                planning_context
                    .as_ref()
                    .map(|value| value.application_id.as_str()),
            )
            .collect::<std::collections::BTreeSet<_>>();
        if identities.len() > 1 {
            issues.push("application_identity_mismatch".to_owned());
        }
        issues.sort();
        issues.dedup();
        let application_id = identities.first().map(|value| (*value).to_owned());
        let desired_digest = desired
            .as_ref()
            .map(desired_composition_digest)
            .transpose()?;
        let application_lock_digest = application_lock
            .as_ref()
            .map(application_module_lock_digest)
            .transpose()?;
        let planning_context_digest = planning_context.as_ref().map(digest_json).transpose()?;
        let status = if !issues.is_empty() {
            ModuleManagementSnapshotStatus::Invalid
        } else if planning_context.is_none() || environment_policy.is_none() {
            ModuleManagementSnapshotStatus::Unconfigured
        } else {
            ModuleManagementSnapshotStatus::Ready
        };
        Ok(ModuleManagementSnapshot {
            protocol: MODULE_MANAGEMENT_SNAPSHOT_PROTOCOL.to_owned(),
            status,
            application_id,
            desired,
            desired_digest,
            application_lock,
            application_lock_digest,
            planning_available: planning_context.is_some() && issues.is_empty(),
            planning_context_digest,
            execution_available: environment_policy
                .as_ref()
                .is_some_and(|policy| policy.mode == crate::EnvironmentManagementMode::Full)
                && issues.is_empty(),
            environment_policy,
            issues,
        })
    }

    pub fn preview(
        &self,
        change: ModuleRootChange,
        created_at: DateTime<Utc>,
    ) -> Result<ModuleChangePlan, WorkspaceModuleManagementError> {
        let snapshot = self.snapshot()?;
        if snapshot.status == ModuleManagementSnapshotStatus::Invalid {
            return Err(WorkspaceModuleManagementError::InvalidContract(
                snapshot.issues.join(","),
            ));
        }
        let planning_context: ModulePlanningContext = read_required(
            &self.root.join(&self.planning_context_path),
        )
        .map_err(|error| match error {
            WorkspaceModuleManagementError::Io(ref io)
                if io.kind() == std::io::ErrorKind::NotFound =>
            {
                WorkspaceModuleManagementError::PlanningUnavailable
            }
            other => other,
        })?;
        if planning_context.protocol != MODULE_PLANNING_CONTEXT_PROTOCOL {
            return Err(WorkspaceModuleManagementError::InvalidContract(
                "unsupported planning context protocol".to_owned(),
            ));
        }
        let current_desired = read_optional_strict(&self.root.join(&self.desired_path))?
            .unwrap_or_else(|| DesiredModuleComposition {
                protocol: DESIRED_MODULE_COMPOSITION_PROTOCOL.to_owned(),
                application_id: planning_context.application_id.clone(),
                revision: 0,
                selected: Vec::new(),
                local_overrides: Vec::new(),
            });
        let current_lock = read_optional_strict(&self.root.join(&self.lock_path))?;
        let current_service_installations = crate::WorkspaceServiceInstallationManager::new(
            &self.root,
            &planning_context.system_id,
            &planning_context.environment_id,
        )
        .snapshot()
        .map_err(|error| WorkspaceModuleManagementError::InvalidContract(error.to_string()))?;
        if current_desired.application_id != planning_context.application_id
            || current_lock
                .as_ref()
                .is_some_and(|lock: &ApplicationModuleLock| {
                    lock.application_id != planning_context.application_id
                })
        {
            return Err(WorkspaceModuleManagementError::InvalidContract(
                "application identity differs from planning context".to_owned(),
            ));
        }
        ModuleChangePlanner::new(&self.root)
            .plan(&ModuleChangePlanRequest {
                current_desired,
                current_lock,
                change,
                catalog_snapshot_digest: planning_context.catalog_snapshot_digest,
                trust_policy_digest: planning_context.trust_policy_digest,
                compatibility_evidence_digest: planning_context.compatibility_evidence_digest,
                resolver_version: planning_context.resolver_version,
                environment_id: planning_context.environment_id,
                expected_target_revision: planning_context.expected_target_revision,
                candidates: planning_context.candidates,
                current_service_installations,
                service_deployments: planning_context.service_deployments,
                cargo_offline: planning_context.cargo_offline,
                created_at,
            })
            .map_err(Into::into)
    }

    /// Loads the exact reviewed workspace artifacts and compiles the sole
    /// Provider transport input. No live Provider endpoint participates in
    /// Module discovery or selection.
    pub fn provider_runtime_plan(
        &self,
    ) -> Result<crate::ProviderRuntimePlan, WorkspaceModuleManagementError> {
        let module_lock: ApplicationModuleLock = read_required(&self.root.join(&self.lock_path))
            .map_err(|error| match error {
                WorkspaceModuleManagementError::Io(ref io)
                    if io.kind() == std::io::ErrorKind::NotFound =>
                {
                    WorkspaceModuleManagementError::ApplicationLockUnavailable
                }
                other => other,
            })?;
        let planning_context: ModulePlanningContext = read_required(
            &self.root.join(&self.planning_context_path),
        )
        .map_err(|error| match error {
            WorkspaceModuleManagementError::Io(ref io)
                if io.kind() == std::io::ErrorKind::NotFound =>
            {
                WorkspaceModuleManagementError::PlanningUnavailable
            }
            other => other,
        })?;
        let installations = crate::WorkspaceServiceInstallationManager::new(
            &self.root,
            &planning_context.system_id,
            &planning_context.environment_id,
        )
        .snapshot()?;

        Ok(crate::compile_provider_runtime_plan(
            &module_lock,
            &planning_context,
            &installations,
        )?)
    }
}

fn read_optional<T: serde::de::DeserializeOwned>(
    path: &Path,
    issue: &str,
    issues: &mut Vec<String>,
) -> Option<T> {
    match fs::read(path) {
        Ok(bytes) => {
            if let Ok(value) = serde_json::from_slice(&bytes) {
                Some(value)
            } else {
                issues.push(issue.to_owned());
                None
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => {
            issues.push(format!("{issue}_read_failed"));
            None
        }
    }
}

fn read_required<T: serde::de::DeserializeOwned>(
    path: &Path,
) -> Result<T, WorkspaceModuleManagementError> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn read_optional_strict<T: serde::de::DeserializeOwned>(
    path: &Path,
) -> Result<Option<T>, WorkspaceModuleManagementError> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
