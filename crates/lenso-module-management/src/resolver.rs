use crate::{
    APPLICATION_MODULE_LOCK_PROTOCOL, ApplicationModuleLock, DesiredModuleComposition,
    DesiredModuleSelection, LockedCapabilityBinding, LockedConsoleUiArtifact, LockedModule,
    LockedModuleReason, ManagedDeliveryKind, ModuleRootChange,
};
use lenso_contracts::{
    ModuleDelivery, ModuleEligibility, ModuleEligibilityState, ModuleRelease,
    ModuleVerificationCell, digest_json,
};
use schemars::JsonSchema;
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

pub const MODULE_RESOLUTION_CONFLICT_PROTOCOL: &str = "lenso.module-resolution-conflict.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleResolutionCandidate {
    pub catalog_snapshot_digest: String,
    pub release_digest: String,
    pub release: ModuleRelease,
    pub eligibility: ModuleEligibility,
    pub verification_cell: ModuleVerificationCell,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleResolutionRequest {
    pub current_desired: DesiredModuleComposition,
    pub current_lock: Option<ApplicationModuleLock>,
    pub change: ModuleRootChange,
    pub catalog_snapshot_digest: String,
    pub trust_policy_digest: String,
    pub resolver_version: String,
    pub candidates: Vec<ModuleResolutionCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleResolution {
    pub target_desired: DesiredModuleComposition,
    pub target_lock: ApplicationModuleLock,
    pub removed_orphan_module_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleResolutionConflict {
    pub protocol: String,
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependency_paths: Vec<Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub eligible_alternatives: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
#[error("Module graph resolution failed: {conflict:?}")]
pub struct ModuleResolutionError {
    pub conflict: Box<ModuleResolutionConflict>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ModuleGraphResolver;

#[derive(Debug, Clone)]
struct RequirementConstraint {
    requirement: VersionReq,
    requirement_text: String,
    capabilities: Vec<String>,
    exact_release_digest: Option<String>,
    delivery: Option<ManagedDeliveryKind>,
    path: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct SolverState {
    constraints: BTreeMap<String, Vec<RequirementConstraint>>,
    selections: BTreeMap<String, usize>,
    dependencies: BTreeMap<String, BTreeSet<String>>,
}

impl ModuleGraphResolver {
    pub fn resolve(
        &self,
        request: &ModuleResolutionRequest,
    ) -> Result<ModuleResolution, ModuleResolutionError> {
        let mut target_desired = request.current_desired.clone();
        apply_root_change(&mut target_desired, &request.change)?;
        normalize_desired(&mut target_desired);

        let mut candidates = request.candidates.clone();
        validate_candidates(&request.catalog_snapshot_digest, &candidates)?;
        candidates.sort_by(|left, right| {
            left.release
                .module_id
                .cmp(&right.release.module_id)
                .then_with(|| left.release_digest.cmp(&right.release_digest))
        });

        let selected_optional = target_desired
            .selected
            .iter()
            .map(|selection| {
                (
                    selection.module_id.clone(),
                    selection
                        .optional_requirements
                        .iter()
                        .cloned()
                        .collect::<BTreeSet<_>>(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let direct_ids = target_desired
            .selected
            .iter()
            .map(|selection| selection.module_id.clone())
            .collect::<BTreeSet<_>>();
        let current_locked = request
            .current_lock
            .as_ref()
            .map(|lock| {
                lock.modules
                    .iter()
                    .map(|module| (module.module_id.clone(), module.release_digest.clone()))
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();

        let mut state = SolverState::default();
        for selection in &target_desired.selected {
            let requirement = parse_requirement(
                &selection.module_id,
                &selection.version_requirement,
                vec![selection.module_id.clone()],
            )?;
            state
                .constraints
                .entry(selection.module_id.clone())
                .or_default()
                .push(RequirementConstraint {
                    requirement,
                    requirement_text: selection.version_requirement.clone(),
                    capabilities: Vec::new(),
                    exact_release_digest: selection.exact_release_digest.clone(),
                    delivery: selection.delivery_preference,
                    path: vec![selection.module_id.clone()],
                });
        }

        let solved = solve(state, &candidates, &selected_optional, &current_locked)?;
        validate_optional_selections(&target_desired, &candidates, &solved)?;
        let desired_digest = digest_json(&target_desired)
            .map_err(|_| conflict("desired_composition_not_canonical", None))?;
        let target_lock = build_lock(
            request,
            &target_desired,
            &desired_digest,
            &candidates,
            &solved,
            &direct_ids,
            &selected_optional,
        );
        let target_ids = target_lock
            .modules
            .iter()
            .map(|module| module.module_id.clone())
            .collect::<BTreeSet<_>>();
        let mut removed_orphan_module_ids = request
            .current_lock
            .as_ref()
            .into_iter()
            .flat_map(|lock| lock.modules.iter())
            .filter(|module| !target_ids.contains(&module.module_id))
            .map(|module| module.module_id.clone())
            .collect::<Vec<_>>();
        removed_orphan_module_ids.sort();

        Ok(ModuleResolution {
            target_desired,
            target_lock,
            removed_orphan_module_ids,
        })
    }
}

fn apply_root_change(
    desired: &mut DesiredModuleComposition,
    change: &ModuleRootChange,
) -> Result<(), ModuleResolutionError> {
    let before = desired.clone();
    match change {
        ModuleRootChange::Install { selection } => {
            if desired
                .selected
                .iter()
                .any(|current| current.module_id == selection.module_id)
            {
                return Err(conflict(
                    "module_already_selected",
                    Some(&selection.module_id),
                ));
            }
            desired.selected.push(selection.clone());
        }
        ModuleRootChange::Update {
            module_id,
            version_requirement,
        } => find_selection_mut(desired, module_id)?
            .version_requirement
            .clone_from(version_requirement),
        ModuleRootChange::Uninstall { module_id } => {
            let old_len = desired.selected.len();
            desired.selected.retain(|item| item.module_id != *module_id);
            if desired.selected.len() == old_len {
                return Err(conflict("module_not_selected", Some(module_id)));
            }
        }
        ModuleRootChange::SelectOptional {
            module_id,
            requirement,
            selected,
        } => {
            let selection = find_selection_mut(desired, module_id)?;
            if *selected {
                selection.optional_requirements.push(requirement.clone());
            } else {
                selection
                    .optional_requirements
                    .retain(|item| item != requirement);
            }
        }
        ModuleRootChange::SwitchDelivery {
            module_id,
            delivery,
        } => {
            find_selection_mut(desired, module_id)?.delivery_preference = Some(*delivery);
        }
        ModuleRootChange::Restore { .. } | ModuleRootChange::Repair { .. } => {
            return Err(conflict("change_requires_exact_lock", None));
        }
    }
    if *desired != before {
        desired.revision = desired.revision.saturating_add(1);
    }
    Ok(())
}

fn find_selection_mut<'a>(
    desired: &'a mut DesiredModuleComposition,
    module_id: &str,
) -> Result<&'a mut DesiredModuleSelection, ModuleResolutionError> {
    desired
        .selected
        .iter_mut()
        .find(|selection| selection.module_id == module_id)
        .ok_or_else(|| conflict("module_not_selected", Some(module_id)))
}

fn normalize_desired(desired: &mut DesiredModuleComposition) {
    for selection in &mut desired.selected {
        selection.optional_requirements.sort();
        selection.optional_requirements.dedup();
    }
    desired
        .selected
        .sort_by(|left, right| left.module_id.cmp(&right.module_id));
    desired
        .local_overrides
        .sort_by(|left, right| left.module_id.cmp(&right.module_id));
}

fn validate_candidates(
    snapshot_digest: &str,
    candidates: &[ModuleResolutionCandidate],
) -> Result<(), ModuleResolutionError> {
    let mut identities = BTreeSet::new();
    for candidate in candidates {
        let release = &candidate.release;
        if candidate.catalog_snapshot_digest != snapshot_digest {
            return Err(conflict(
                "candidate_snapshot_mismatch",
                Some(&release.module_id),
            ));
        }
        if digest_json(release).ok().as_deref() != Some(candidate.release_digest.as_str()) {
            return Err(conflict(
                "candidate_release_digest_mismatch",
                Some(&release.module_id),
            ));
        }
        if !release.validate().is_empty() {
            return Err(conflict(
                "candidate_release_invalid",
                Some(&release.module_id),
            ));
        }
        if candidate.verification_cell.module_release_digest != candidate.release_digest {
            return Err(conflict(
                "candidate_verification_cell_mismatch",
                Some(&release.module_id),
            ));
        }
        let identity = (
            release.module_id.clone(),
            release.version.clone(),
            ManagedDeliveryKind::from(&release.delivery),
        );
        if !identities.insert(identity) {
            return Err(conflict(
                "duplicate_release_identity",
                Some(&release.module_id),
            ));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn solve(
    state: SolverState,
    candidates: &[ModuleResolutionCandidate],
    selected_optional: &BTreeMap<String, BTreeSet<String>>,
    current_locked: &BTreeMap<String, String>,
) -> Result<SolverState, ModuleResolutionError> {
    if let Some((selected_id, _selected_index)) = state
        .selections
        .iter()
        .find(|(id, index)| !candidate_matches(&candidates[**index], &state.constraints[*id]))
    {
        return Err(unsatisfied_conflict(
            selected_id,
            &state.constraints[selected_id],
            candidates,
        ));
    }

    let unresolved = state
        .constraints
        .iter()
        .filter(|(module_id, _)| !state.selections.contains_key(*module_id))
        .map(|(module_id, constraints)| {
            let options = ranked_options(module_id, constraints, candidates, current_locked);
            (module_id.clone(), options)
        })
        .min_by(|left, right| {
            left.1
                .len()
                .cmp(&right.1.len())
                .then_with(|| left.0.cmp(&right.0))
        });
    let Some((module_id, options)) = unresolved else {
        return Ok(state);
    };
    if options.is_empty() {
        return Err(unsatisfied_conflict(
            &module_id,
            &state.constraints[&module_id],
            candidates,
        ));
    }

    let mut first_error = None;
    for candidate_index in options {
        let candidate = &candidates[candidate_index];
        let mut next = state.clone();
        let mut invalid_candidate = false;
        next.selections.insert(module_id.clone(), candidate_index);
        let parent_path = next.constraints[&module_id]
            .iter()
            .map(|constraint| constraint.path.clone())
            .min()
            .unwrap_or_else(|| vec![module_id.clone()]);
        for requirement in candidate
            .release
            .manifest
            .requires
            .iter()
            .filter(|requirement| {
                !requirement.optional
                    || selected_optional
                        .get(&module_id)
                        .is_some_and(|selected| selected.contains(&requirement.module_id))
            })
        {
            if path_exists(&next.dependencies, &requirement.module_id, &module_id) {
                let mut path = parent_path.clone();
                path.push(requirement.module_id.clone());
                let mut error = conflict("dependency_cycle", Some(&requirement.module_id));
                error.conflict.dependency_paths.push(path);
                first_error.get_or_insert(error);
                invalid_candidate = true;
                continue;
            }
            next.dependencies
                .entry(module_id.clone())
                .or_default()
                .insert(requirement.module_id.clone());
            let mut path = parent_path.clone();
            path.push(requirement.module_id.clone());
            let parsed = match parse_requirement(
                &requirement.module_id,
                &requirement.version_requirement,
                path.clone(),
            ) {
                Ok(parsed) => parsed,
                Err(error) => {
                    first_error.get_or_insert(error);
                    invalid_candidate = true;
                    continue;
                }
            };
            next.constraints
                .entry(requirement.module_id.clone())
                .or_default()
                .push(RequirementConstraint {
                    requirement: parsed,
                    requirement_text: requirement.version_requirement.clone(),
                    capabilities: requirement.capabilities.clone(),
                    exact_release_digest: None,
                    delivery: None,
                    path,
                });
        }
        if invalid_candidate {
            continue;
        }
        match solve(next, candidates, selected_optional, current_locked) {
            Ok(solved) => return Ok(solved),
            Err(error) => first_error.get_or_insert(error),
        };
    }
    Err(first_error.unwrap_or_else(|| {
        unsatisfied_conflict(&module_id, &state.constraints[&module_id], candidates)
    }))
}

fn path_exists(graph: &BTreeMap<String, BTreeSet<String>>, start: &str, target: &str) -> bool {
    if start == target {
        return true;
    }
    graph.get(start).is_some_and(|children| {
        children
            .iter()
            .any(|child| path_exists(graph, child, target))
    })
}

fn ranked_options(
    module_id: &str,
    constraints: &[RequirementConstraint],
    candidates: &[ModuleResolutionCandidate],
    current_locked: &BTreeMap<String, String>,
) -> Vec<usize> {
    let mut options = candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| {
            candidate.release.module_id == module_id && candidate_matches(candidate, constraints)
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    options.sort_by(|left, right| {
        compare_candidates(
            &candidates[*left],
            &candidates[*right],
            current_locked.get(module_id).map(String::as_str),
        )
    });
    options
}

fn compare_candidates(
    left: &ModuleResolutionCandidate,
    right: &ModuleResolutionCandidate,
    current_digest: Option<&str>,
) -> Ordering {
    let left_current = current_digest == Some(left.release_digest.as_str());
    let right_current = current_digest == Some(right.release_digest.as_str());
    right_current
        .cmp(&left_current)
        .then_with(|| {
            let left_linked = matches!(left.release.delivery, ModuleDelivery::Linked(_));
            let right_linked = matches!(right.release.delivery, ModuleDelivery::Linked(_));
            right_linked.cmp(&left_linked)
        })
        .then_with(|| {
            Version::parse(&right.release.version)
                .expect("validated release version")
                .cmp(&Version::parse(&left.release.version).expect("validated release version"))
        })
        .then_with(|| left.release_digest.cmp(&right.release_digest))
}

fn candidate_matches(
    candidate: &ModuleResolutionCandidate,
    constraints: &[RequirementConstraint],
) -> bool {
    if !matches!(
        candidate.eligibility.state,
        ModuleEligibilityState::Eligible | ModuleEligibilityState::EligibleWithWarning
    ) {
        return false;
    }
    let Ok(version) = Version::parse(&candidate.release.version) else {
        return false;
    };
    constraints.iter().all(|constraint| {
        constraint.requirement.matches(&version)
            && constraint
                .exact_release_digest
                .as_deref()
                .is_none_or(|digest| digest == candidate.release_digest)
            && constraint.delivery.is_none_or(|delivery| {
                delivery == ManagedDeliveryKind::from(&candidate.release.delivery)
            })
            && constraint
                .capabilities
                .iter()
                .all(|capability| candidate.release.manifest.capabilities.contains(capability))
    })
}

fn parse_requirement(
    module_id: &str,
    requirement: &str,
    path: Vec<String>,
) -> Result<VersionReq, ModuleResolutionError> {
    VersionReq::parse(requirement).map_err(|_| {
        let mut error = conflict("invalid_version_requirement", Some(module_id));
        error.conflict.constraints.push(requirement.to_owned());
        error.conflict.dependency_paths.push(path);
        error
    })
}

fn unsatisfied_conflict(
    module_id: &str,
    constraints: &[RequirementConstraint],
    candidates: &[ModuleResolutionCandidate],
) -> ModuleResolutionError {
    let mut error = conflict("unsatisfiable_module", Some(module_id));
    error.conflict.dependency_paths = constraints
        .iter()
        .map(|constraint| constraint.path.clone())
        .collect();
    error.conflict.dependency_paths.sort();
    error.conflict.dependency_paths.dedup();
    error.conflict.constraints = constraints
        .iter()
        .flat_map(|constraint| {
            let mut values = vec![constraint.requirement_text.clone()];
            values.extend(
                constraint
                    .capabilities
                    .iter()
                    .map(|capability| format!("capability:{capability}")),
            );
            if let Some(digest) = &constraint.exact_release_digest {
                values.push(format!("release:{digest}"));
            }
            if let Some(delivery) = constraint.delivery {
                values.push(format!("delivery:{delivery:?}"));
            }
            values
        })
        .collect();
    error.conflict.constraints.sort();
    error.conflict.constraints.dedup();
    error.conflict.eligible_alternatives = candidates
        .iter()
        .filter(|candidate| {
            candidate.release.module_id == module_id
                && matches!(
                    candidate.eligibility.state,
                    ModuleEligibilityState::Eligible | ModuleEligibilityState::EligibleWithWarning
                )
        })
        .map(|candidate| {
            format!(
                "{}@{}#{:?}:{}",
                candidate.release.module_id,
                candidate.release.version,
                ManagedDeliveryKind::from(&candidate.release.delivery),
                candidate.release_digest
            )
        })
        .collect();
    error.conflict.eligible_alternatives.sort();
    error
}

#[allow(clippy::too_many_lines)]
fn build_lock(
    request: &ModuleResolutionRequest,
    desired: &DesiredModuleComposition,
    desired_digest: &str,
    candidates: &[ModuleResolutionCandidate],
    solved: &SolverState,
    direct_ids: &BTreeSet<String>,
    selected_optional: &BTreeMap<String, BTreeSet<String>>,
) -> ApplicationModuleLock {
    let overrides = desired
        .local_overrides
        .iter()
        .map(|item| (item.module_id.as_str(), item.content_digest.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut modules = solved
        .selections
        .iter()
        .map(|(module_id, index)| {
            let candidate = &candidates[*index];
            let release = &candidate.release;
            let dependency_module_ids = release
                .manifest
                .requires
                .iter()
                .filter(|requirement| {
                    !requirement.optional
                        || selected_optional
                            .get(module_id)
                            .is_some_and(|selected| selected.contains(&requirement.module_id))
                })
                .map(|requirement| requirement.module_id.clone())
                .collect::<Vec<_>>();
            let (crate_features, migration_artifacts) = match &release.delivery {
                ModuleDelivery::Linked(linked) => {
                    (linked.features.clone(), linked.migrations.clone())
                }
                ModuleDelivery::Service(_) => (Vec::new(), Vec::new()),
            };
            LockedModule {
                module_id: module_id.clone(),
                version: release.version.clone(),
                release_digest: candidate.release_digest.clone(),
                manifest_digest: release.manifest_digest.clone(),
                delivery: release.delivery.clone(),
                reason: if direct_ids.contains(module_id) {
                    LockedModuleReason::Direct
                } else {
                    LockedModuleReason::Transitive
                },
                dependency_module_ids,
                crate_features,
                migration_artifacts,
                console_ui_artifact: release.console_ui_artifact.as_ref().map(|artifact| {
                    LockedConsoleUiArtifact {
                        locator: artifact.artifact.locator.clone(),
                        digest: artifact.artifact.digest.clone(),
                        format: artifact.format.clone(),
                        protocol_major: artifact.protocol_major,
                        entry: artifact.entry.clone(),
                        entries: artifact.entries.clone(),
                        style_assets: artifact.style_assets.clone(),
                        manifest: artifact.manifest.clone(),
                        requested_permissions: artifact.requested_permissions.clone(),
                    }
                }),
                verification: candidate.eligibility.verification.clone(),
                verification_cell: candidate.verification_cell.clone(),
                lifecycle: candidate.eligibility.lifecycle.clone(),
                local_override_digest: overrides.get(module_id.as_str()).cloned(),
            }
        })
        .collect::<Vec<_>>();
    modules.sort_by(|left, right| left.module_id.cmp(&right.module_id));

    let mut capability_bindings = solved
        .selections
        .iter()
        .flat_map(|(consumer, index)| {
            candidates[*index]
                .release
                .manifest
                .requires
                .iter()
                .filter(|requirement| {
                    !requirement.optional
                        || selected_optional
                            .get(consumer)
                            .is_some_and(|selected| selected.contains(&requirement.module_id))
                })
                .flat_map(move |requirement| {
                    requirement
                        .capabilities
                        .iter()
                        .map(move |capability| LockedCapabilityBinding {
                            capability: capability.clone(),
                            provider_module_id: requirement.module_id.clone(),
                            consumer_module_id: consumer.clone(),
                        })
                })
        })
        .collect::<Vec<_>>();
    capability_bindings.sort_by(|left, right| {
        left.consumer_module_id
            .cmp(&right.consumer_module_id)
            .then_with(|| left.capability.cmp(&right.capability))
            .then_with(|| left.provider_module_id.cmp(&right.provider_module_id))
    });

    ApplicationModuleLock {
        protocol: APPLICATION_MODULE_LOCK_PROTOCOL.to_owned(),
        application_id: desired.application_id.clone(),
        desired_composition_digest: desired_digest.to_owned(),
        catalog_snapshot_digest: request.catalog_snapshot_digest.clone(),
        trust_policy_digest: request.trust_policy_digest.clone(),
        resolver_version: request.resolver_version.clone(),
        modules,
        capability_bindings,
    }
}

fn validate_optional_selections(
    desired: &DesiredModuleComposition,
    candidates: &[ModuleResolutionCandidate],
    solved: &SolverState,
) -> Result<(), ModuleResolutionError> {
    for selection in &desired.selected {
        let Some(candidate_index) = solved.selections.get(&selection.module_id) else {
            continue;
        };
        let declared = candidates[*candidate_index]
            .release
            .manifest
            .requires
            .iter()
            .filter(|requirement| requirement.optional)
            .map(|requirement| requirement.module_id.as_str())
            .collect::<BTreeSet<_>>();
        if let Some(requirement) = selection
            .optional_requirements
            .iter()
            .find(|requirement| !declared.contains(requirement.as_str()))
        {
            let mut error = conflict(
                "optional_requirement_not_declared",
                Some(&selection.module_id),
            );
            error.conflict.constraints.push(requirement.clone());
            error
                .conflict
                .dependency_paths
                .push(vec![selection.module_id.clone(), requirement.clone()]);
            return Err(error);
        }
    }
    Ok(())
}

fn conflict(code: &str, module_id: Option<&str>) -> ModuleResolutionError {
    ModuleResolutionError {
        conflict: Box::new(ModuleResolutionConflict {
            protocol: MODULE_RESOLUTION_CONFLICT_PROTOCOL.to_owned(),
            code: code.to_owned(),
            module_id: module_id.map(str::to_owned),
            dependency_paths: Vec::new(),
            constraints: Vec::new(),
            eligible_alternatives: Vec::new(),
        }),
    }
}
