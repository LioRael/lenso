use crate::{
    ApplicationModuleLock, CargoLockGenerator, DesiredModuleComposition, IsolatedCargoLockResolver,
    LinkedWorkspaceError, LinkedWorkspacePlanner, MODULE_CHANGE_PLAN_PROTOCOL,
    MigrationExecutionMode, ModuleApprovalBoundary, ModuleChangePlan, ModuleGraphResolver,
    ModuleManagementError, ModulePlanEffect, ModuleResolutionCandidate, ModuleResolutionError,
    ModuleResolutionRequest, ModuleRiskClass, ModuleRootChange, application_module_lock_digest,
    desired_composition_digest, module_change_plan_digest, validate_change_plan,
};
use chrono::{DateTime, Utc};
use lenso_contracts::{ModuleDelivery, digest_json};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ModuleChangePlanRequest {
    pub current_desired: DesiredModuleComposition,
    pub current_lock: Option<ApplicationModuleLock>,
    pub change: ModuleRootChange,
    pub catalog_snapshot_digest: String,
    pub trust_policy_digest: String,
    pub compatibility_evidence_digest: String,
    pub resolver_version: String,
    pub environment_id: String,
    pub expected_target_revision: u64,
    pub candidates: Vec<ModuleResolutionCandidate>,
    pub current_service_installations: crate::ServiceInstallationSet,
    pub service_deployments: Vec<crate::ServiceDeploymentBinding>,
    pub cargo_offline: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum ModuleChangePlannerError {
    #[error(transparent)]
    Resolution(#[from] ModuleResolutionError),
    #[error(transparent)]
    Linked(#[from] LinkedWorkspaceError),
    #[error(transparent)]
    Cargo(#[from] crate::CargoLockResolutionError),
    #[error(transparent)]
    Management(#[from] ModuleManagementError),
    #[error("Module change plan JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    ServiceInstallation(#[from] crate::ServiceInstallationError),
    #[error("isolated Cargo produced a non-UTF-8 lockfile")]
    NonUtf8CargoLock,
}

#[derive(Debug, Clone)]
pub struct ModuleChangePlanner<G = crate::CargoGenerateLockfile> {
    graph: ModuleGraphResolver,
    linked: LinkedWorkspacePlanner,
    cargo: IsolatedCargoLockResolver<G>,
}

impl ModuleChangePlanner<crate::CargoGenerateLockfile> {
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            graph: ModuleGraphResolver,
            linked: LinkedWorkspacePlanner::new(workspace_root),
            cargo: IsolatedCargoLockResolver::default(),
        }
    }
}

impl<G> ModuleChangePlanner<G>
where
    G: CargoLockGenerator,
{
    pub fn with_cargo_generator(workspace_root: impl Into<PathBuf>, generator: G) -> Self {
        Self {
            graph: ModuleGraphResolver,
            linked: LinkedWorkspacePlanner::new(workspace_root),
            cargo: IsolatedCargoLockResolver::new(generator),
        }
    }

    pub fn plan(
        &self,
        request: &ModuleChangePlanRequest,
    ) -> Result<ModuleChangePlan, ModuleChangePlannerError> {
        let resolution = self.graph.resolve(&ModuleResolutionRequest {
            current_desired: request.current_desired.clone(),
            current_lock: request.current_lock.clone(),
            change: request.change.clone(),
            catalog_snapshot_digest: request.catalog_snapshot_digest.clone(),
            trust_policy_digest: request.trust_policy_digest.clone(),
            resolver_version: request.resolver_version.clone(),
            candidates: request.candidates.clone(),
        })?;
        let reviewed_desired_document = format!(
            "{}\n",
            serde_json::to_string_pretty(&resolution.target_desired)?
        );
        let cargo_preparation = self.linked.prepare_cargo_resolution(
            &resolution.target_desired,
            request.current_lock.as_ref(),
            &resolution.target_lock,
            &reviewed_desired_document,
            request.cargo_offline,
        )?;
        let cargo = self.cargo.resolve(&cargo_preparation.cargo_request)?;
        let candidate_cargo_lock = String::from_utf8(cargo.candidate_lock)
            .map_err(|_| ModuleChangePlannerError::NonUtf8CargoLock)?;
        let workspace = self.linked.plan(
            &resolution.target_desired,
            &resolution.target_lock,
            &reviewed_desired_document,
            Some(&candidate_cargo_lock),
        )?;

        let current_desired_digest = desired_composition_digest(&request.current_desired)?;
        let target_desired_digest = desired_composition_digest(&resolution.target_desired)?;
        let current_lock_digest = request
            .current_lock
            .as_ref()
            .map(application_module_lock_digest)
            .transpose()?;
        let target_lock_digest = application_module_lock_digest(&resolution.target_lock)?;
        let plan_identity = digest_json(&json!({
            "application_id": resolution.target_desired.application_id,
            "environment_id": request.environment_id,
            "expected_target_revision": request.expected_target_revision,
            "request": request.change,
            "current_desired_digest": current_desired_digest,
            "current_lock_digest": current_lock_digest,
            "target_lock_digest": target_lock_digest,
        }))?;
        let mut effects = workspace.effects;
        effects.extend(non_workspace_effects(
            request.current_lock.as_ref(),
            &resolution.target_lock,
            &request.candidates,
            &target_lock_digest,
            &cargo.evidence.candidate_lock_digest,
            &request.current_service_installations,
            &request.service_deployments,
            request.created_at,
        )?);
        effects.sort_by(|left, right| left.effect_id().cmp(right.effect_id()));
        let approval_boundaries = destructive_boundaries(&effects);
        let mut next_actions = vec!["review_plan".to_owned(), "apply_plan".to_owned()];
        if !approval_boundaries.is_empty() {
            next_actions.insert(1, "approve_destructive_migrations".to_owned());
        }
        let validation_commands = vec!["cargo check --locked".to_owned()];
        let mut plan = ModuleChangePlan {
            protocol: MODULE_CHANGE_PLAN_PROTOCOL.to_owned(),
            plan_id: format!("module-plan-{}", &plan_identity[7..23]),
            plan_digest: String::new(),
            application_id: resolution.target_desired.application_id.clone(),
            environment_id: request.environment_id.clone(),
            expected_target_revision: request.expected_target_revision,
            request: request.change.clone(),
            current_desired_digest,
            target_desired: resolution.target_desired,
            target_desired_digest,
            current_lock_digest,
            target_lock: resolution.target_lock,
            target_lock_digest,
            catalog_snapshot_digest: request.catalog_snapshot_digest.clone(),
            resolver_version: request.resolver_version.clone(),
            trust_policy_digest: request.trust_policy_digest.clone(),
            compatibility_evidence_digest: request.compatibility_evidence_digest.clone(),
            cargo_lock_candidate: Some(cargo.evidence),
            read_set: workspace.read_set,
            effects,
            approval_boundaries,
            validation_commands,
            next_actions,
            created_at: request.created_at,
        };
        plan.plan_digest = module_change_plan_digest(&plan)?;
        validate_change_plan(&plan)?;
        Ok(plan)
    }
}

#[allow(clippy::too_many_lines)]
fn non_workspace_effects(
    current_lock: Option<&ApplicationModuleLock>,
    target_lock: &ApplicationModuleLock,
    candidates: &[ModuleResolutionCandidate],
    target_lock_digest: &str,
    cargo_lock_digest: &str,
    current_service_installations: &crate::ServiceInstallationSet,
    service_deployments: &[crate::ServiceDeploymentBinding],
    created_at: DateTime<Utc>,
) -> Result<Vec<ModulePlanEffect>, ModuleChangePlannerError> {
    let releases = candidates
        .iter()
        .map(|candidate| (candidate.release_digest.as_str(), &candidate.release))
        .collect::<BTreeMap<_, _>>();
    let mut effects = Vec::new();
    let current_modules = current_lock
        .map(|lock| {
            lock.modules
                .iter()
                .map(|module| (module.module_id.as_str(), module))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let changed_target_modules = target_lock
        .modules
        .iter()
        .filter(|module| {
            current_modules
                .get(module.module_id.as_str())
                .is_none_or(|current| {
                    current.release_digest != module.release_digest
                        || current.delivery != module.delivery
                        || current.crate_features != module.crate_features
                        || current.local_override_digest != module.local_override_digest
                })
        })
        .collect::<Vec<_>>();
    let current_services = current_lock
        .into_iter()
        .flat_map(|lock| &lock.modules)
        .filter_map(|module| match &module.delivery {
            ModuleDelivery::Service(service) => Some((
                service.service_id.clone(),
                service.service_release_digest.clone(),
            )),
            ModuleDelivery::Linked(_) => None,
        })
        .collect::<BTreeSet<_>>();
    let target_services = target_lock
        .modules
        .iter()
        .filter_map(|module| match &module.delivery {
            ModuleDelivery::Service(service) => Some((
                service.service_id.clone(),
                service.service_release_digest.clone(),
            )),
            ModuleDelivery::Linked(_) => None,
        })
        .collect::<BTreeSet<_>>();
    let mut installation_state = current_service_installations.clone();
    for (service_id, service_release_digest) in target_services.difference(&current_services) {
        let binding = service_binding(service_deployments, service_id, service_release_digest);
        if let Some(installation) = binding.and_then(|binding| binding.installation.as_ref()) {
            if installation.service_ref.service_id != *service_id
                || installation.service_release.digest != *service_release_digest
            {
                return Err(crate::ServiceInstallationError::InvalidContract(
                    "Service Installation binding differs from the resolved Service release"
                        .to_owned(),
                )
                .into());
            }
            let installed_exports = installation
                .exports
                .iter()
                .map(|export| export.module_id.as_str())
                .collect::<BTreeSet<_>>();
            if target_lock.modules.iter().any(|module| {
                matches!(
                    &module.delivery,
                    ModuleDelivery::Service(service)
                        if service.service_id == *service_id
                            && service.service_release_digest == *service_release_digest
                            && !installed_exports.contains(module.module_id.as_str())
                )
            }) {
                return Err(crate::ServiceInstallationError::InvalidContract(
                    "Service Installation does not declare every resolved Module export".to_owned(),
                )
                .into());
            }
        }
        let installation_plan = binding
            .and_then(|binding| binding.installation.clone())
            .map(|installation| {
                crate::plan_service_installation(
                    &installation_state,
                    crate::ServiceInstallationChange::Install { installation },
                    created_at,
                )
            })
            .transpose()?;
        if let Some(plan) = &installation_plan {
            installation_state = plan.target.clone();
        }
        effects.push(ModulePlanEffect::ServiceInstallation {
            effect_id: format!(
                "20-service-install:{}:{}",
                safe_id(service_id),
                &service_release_digest[7..23]
            ),
            service_id: service_id.clone(),
            service_release_digest: service_release_digest.clone(),
            installation_plan,
            adapter: binding.map(|binding| binding.adapter),
            action: binding.and_then(|binding| binding.install.clone()),
        });
    }
    for module in &changed_target_modules {
        match &module.delivery {
            ModuleDelivery::Service(_) => {}
            ModuleDelivery::Linked(_) => {
                if current_modules
                    .get(module.module_id.as_str())
                    .is_some_and(|current| current.release_digest == module.release_digest)
                {
                    continue;
                }
                let Some(release) = releases.get(module.release_digest.as_str()) else {
                    continue;
                };
                for (declaration, artifact) in release
                    .manifest
                    .migrations
                    .iter()
                    .zip(&module.migration_artifacts)
                {
                    effects.push(ModulePlanEffect::Migration {
                        effect_id: format!(
                            "30-migration:{}:{:08}:{}",
                            safe_id(&module.module_id),
                            declaration.order,
                            safe_id(&declaration.migration_id)
                        ),
                        module_id: module.module_id.clone(),
                        release_digest: module.release_digest.clone(),
                        migration_id: declaration.migration_id.clone(),
                        artifact_locator: artifact.locator.clone(),
                        artifact_digest: artifact.digest.clone(),
                        store_scope: declaration.store.clone(),
                        execution: MigrationExecutionMode::Transactional,
                        risk_class: if declaration.destructive {
                            ModuleRiskClass::DestructiveMigration
                        } else {
                            ModuleRiskClass::Ordinary
                        },
                    });
                }
            }
        }
    }
    effects.push(ModulePlanEffect::Validate {
        effect_id: "80-validate:cargo-check".to_owned(),
        command: "cargo check --locked".to_owned(),
        expected_evidence: cargo_lock_digest.to_owned(),
    });
    if changed_target_modules
        .iter()
        .any(|module| matches!(module.delivery, ModuleDelivery::Linked(_)))
    {
        effects.push(ModulePlanEffect::Restart {
            effect_id: "99-restart:host".to_owned(),
            target: "host".to_owned(),
        });
    }
    for (service_id, service_release_digest) in &target_services {
        if !changed_target_modules.iter().any(|module| {
            matches!(&module.delivery, ModuleDelivery::Service(service) if service.service_id == *service_id && service.service_release_digest == *service_release_digest)
        }) {
            continue;
        }
        let binding = service_binding(service_deployments, service_id, service_release_digest);
        let Some((adapter, action)) = binding.and_then(|binding| {
            binding
                .restart
                .clone()
                .map(|action| (binding.adapter, action))
        }) else {
            continue;
        };
        effects.push(ModulePlanEffect::ServiceRestart {
            effect_id: format!("99-service-restart:{}", safe_id(service_id)),
            service_id: service_id.clone(),
            service_release_digest: service_release_digest.clone(),
            adapter: Some(adapter),
            action: Some(action),
        });
    }
    effects.push(ModulePlanEffect::Activate {
        effect_id: "90-activate:application-lock".to_owned(),
        target_lock_digest: target_lock_digest.to_owned(),
    });
    effects.sort_by(|left, right| left.effect_id().cmp(right.effect_id()));
    Ok(effects)
}

fn service_binding<'a>(
    bindings: &'a [crate::ServiceDeploymentBinding],
    service_id: &str,
    release_digest: &str,
) -> Option<&'a crate::ServiceDeploymentBinding> {
    bindings.iter().find(|binding| {
        binding.service_id == service_id && binding.service_release_digest == release_digest
    })
}

fn destructive_boundaries(effects: &[ModulePlanEffect]) -> Vec<ModuleApprovalBoundary> {
    let effect_ids = effects
        .iter()
        .filter(|effect| effect.risk_class() == ModuleRiskClass::DestructiveMigration)
        .map(|effect| effect.effect_id().to_owned())
        .collect::<Vec<_>>();
    if effect_ids.is_empty() {
        Vec::new()
    } else {
        vec![ModuleApprovalBoundary {
            boundary_id: "destructive-migrations".to_owned(),
            risk_class: ModuleRiskClass::DestructiveMigration,
            required_authority: "module.migrate.destructive".to_owned(),
            effect_ids,
            backup_evidence_digest: None,
        }]
    }
}

fn safe_id(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect()
}
