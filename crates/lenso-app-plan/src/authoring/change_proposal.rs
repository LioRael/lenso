use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    HostCatalog, PluginInstanceId, PluginRootResolutionError, PluginRootSnapshot, ResolvedApp,
    resolve_plugin_root,
};

/// Whether a deterministic Desired State proposal can be applied.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeProposalStatus {
    Ready,
    NeedsDecision,
    Rejected,
}

/// The runtime mechanism required after a proposal passes its Ready Gate.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeApplication {
    Noop,
    AppGeneration,
    Blocked,
}

/// One stable, machine-readable reason a proposal cannot proceed automatically.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChangeDiagnostic {
    code: String,
    detail: String,
}

impl ChangeDiagnostic {
    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }
}

/// One user-visible difference between the current and candidate Apps.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginChange {
    Added { instance: PluginInstanceId },
    Removed { instance: PluginInstanceId },
    Reconfigured { instance: PluginInstanceId },
    Replaced { instance: PluginInstanceId },
    Rebound { consumer: PluginInstanceId },
}

/// Deterministic explanation of one exact Plugin Root change.
///
/// This is review evidence, not runtime authority. A product Host still closes
/// the exact Generation authorities and passes the Ready Gate before switching.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChangeProposal {
    schema: String,
    digest: String,
    status: ChangeProposalStatus,
    application: ChangeApplication,
    changes: Vec<PluginChange>,
    diagnostics: Vec<ChangeDiagnostic>,
}

impl ChangeProposal {
    pub fn digest(&self) -> &str {
        &self.digest
    }

    pub const fn status(&self) -> ChangeProposalStatus {
        self.status
    }

    pub const fn application(&self) -> ChangeApplication {
        self.application
    }

    pub fn changes(&self) -> &[PluginChange] {
        &self.changes
    }

    pub fn diagnostics(&self) -> &[ChangeDiagnostic] {
        &self.diagnostics
    }
}

/// Resolves both states and explains their exact deterministic difference.
pub fn propose_plugin_root_change(
    host: &HostCatalog,
    current: &PluginRootSnapshot,
    candidate: &PluginRootSnapshot,
) -> Result<ChangeProposal, PluginRootResolutionError> {
    let current_app = resolve_plugin_root(host, current)?;
    let candidate_app = resolve_plugin_root(host, candidate)?;
    let authority = serde_json::to_vec(&(host, current, candidate)).map_err(|error| {
        PluginRootResolutionError::InvalidResolvedApp(format!(
            "failed to encode Change Proposal authority: {error}"
        ))
    })?;
    let digest = format!("{:x}", Sha256::digest(authority));
    let changes = diff_apps(&current_app, &candidate_app);
    Ok(ChangeProposal {
        schema: "lenso.change-proposal.v1".to_owned(),
        digest: format!("sha256:{digest}"),
        status: ChangeProposalStatus::Ready,
        application: if changes.is_empty() {
            ChangeApplication::Noop
        } else {
            // Hot Plan Transition remains opt-in until the full whitelist and
            // product conformance are available. Generation replacement is the
            // safe common denominator for every resolved structural change.
            ChangeApplication::AppGeneration
        },
        changes,
        diagnostics: Vec::new(),
    })
}

/// Evaluates one candidate as stable review data, including rejected states.
pub fn evaluate_plugin_root_change(
    host: &HostCatalog,
    current: &PluginRootSnapshot,
    candidate: &PluginRootSnapshot,
) -> ChangeProposal {
    match propose_plugin_root_change(host, current, candidate) {
        Ok(proposal) => proposal,
        Err(error) => {
            let status = if matches!(
                error,
                PluginRootResolutionError::AmbiguousSlot { .. }
                    | PluginRootResolutionError::AmbiguousCapability { .. }
            ) {
                ChangeProposalStatus::NeedsDecision
            } else {
                ChangeProposalStatus::Rejected
            };
            let authority = serde_json::to_vec(&(host, current, candidate)).unwrap_or_default();
            let digest = format!("{:x}", Sha256::digest(authority));
            ChangeProposal {
                schema: "lenso.change-proposal.v1".to_owned(),
                digest: format!("sha256:{digest}"),
                status,
                application: ChangeApplication::Blocked,
                changes: Vec::new(),
                diagnostics: vec![ChangeDiagnostic {
                    code: resolution_error_code(&error).to_owned(),
                    detail: error.to_string(),
                }],
            }
        }
    }
}

fn resolution_error_code(error: &PluginRootResolutionError) -> &'static str {
    match error {
        PluginRootResolutionError::AmbiguousSlot { .. } => "ambiguous_slot",
        PluginRootResolutionError::AmbiguousCapability { .. } => "ambiguous_capability",
        PluginRootResolutionError::InvalidConfiguration { .. } => "invalid_configuration",
        PluginRootResolutionError::MissingRequiredSlot(_) => "missing_required_slot",
        PluginRootResolutionError::MissingCapability { .. } => "missing_capability",
        PluginRootResolutionError::RequiredInstanceDisabled(_) => "required_instance_disabled",
        PluginRootResolutionError::UnknownPlugin(_) => "unknown_plugin",
        PluginRootResolutionError::UnknownDisabledInstance(_) => "unknown_disabled_instance",
        _ => "invalid_plugin_root",
    }
}

fn diff_apps(current: &ResolvedApp, candidate: &ResolvedApp) -> Vec<PluginChange> {
    let current_instances = current
        .instances()
        .iter()
        .map(|instance| (instance.id(), instance))
        .collect::<BTreeMap<_, _>>();
    let candidate_instances = candidate
        .instances()
        .iter()
        .map(|instance| (instance.id(), instance))
        .collect::<BTreeMap<_, _>>();
    let current_plans = current
        .plan()
        .plugin_instances()
        .iter()
        .map(|instance| (instance.instance_key(), instance))
        .collect::<BTreeMap<_, _>>();
    let candidate_plans = candidate
        .plan()
        .plugin_instances()
        .iter()
        .map(|instance| (instance.instance_key(), instance))
        .collect::<BTreeMap<_, _>>();
    let ids = current_instances
        .keys()
        .chain(candidate_instances.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    let mut changes = Vec::new();
    for id in ids {
        match (current_instances.get(id), candidate_instances.get(id)) {
            (None, Some(_)) => changes.push(PluginChange::Added {
                instance: id.clone(),
            }),
            (Some(_), None) => changes.push(PluginChange::Removed {
                instance: id.clone(),
            }),
            (Some(current_instance), Some(candidate_instance)) => {
                let current_plan = current_plans[current_instance.plan_key()];
                let candidate_plan = candidate_plans[candidate_instance.plan_key()];
                if current_plan == candidate_plan
                    && current_instance.source() == candidate_instance.source()
                {
                    continue;
                }
                if current_plan.configuration() != candidate_plan.configuration()
                    && same_contract_except_configuration(current_plan, candidate_plan)
                {
                    changes.push(PluginChange::Reconfigured {
                        instance: id.clone(),
                    });
                } else {
                    changes.push(PluginChange::Replaced {
                        instance: id.clone(),
                    });
                }
            }
            (None, None) => unreachable!("identity came from one resolved App"),
        }
    }
    let current_bindings = current.plan().capability_bindings();
    let candidate_bindings = candidate.plan().capability_bindings();
    if current_bindings != candidate_bindings {
        let consumers = current_bindings
            .iter()
            .map(crate::CapabilityBinding::consumer_instance)
            .chain(
                candidate_bindings
                    .iter()
                    .map(crate::CapabilityBinding::consumer_instance),
            )
            .collect::<BTreeSet<_>>();
        for consumer in consumers {
            let current = current_bindings
                .iter()
                .filter(|binding| binding.consumer_instance() == consumer)
                .collect::<Vec<_>>();
            let candidate = candidate_bindings
                .iter()
                .filter(|binding| binding.consumer_instance() == consumer)
                .collect::<Vec<_>>();
            if current != candidate
                && let Some(instance) = candidate_instances
                    .values()
                    .chain(current_instances.values())
                    .find(|instance| instance.plan_key() == consumer)
            {
                changes.push(PluginChange::Rebound {
                    consumer: instance.id().clone(),
                });
            }
        }
    }
    changes
}

fn same_contract_except_configuration(
    left: &crate::PluginInstancePlan,
    right: &crate::PluginInstancePlan,
) -> bool {
    left.instance_key() == right.instance_key()
        && left.package_id() == right.package_id()
        && left.entrypoint() == right.entrypoint()
        && left.provided_capabilities() == right.provided_capabilities()
        && left.required_capabilities() == right.required_capabilities()
        && left.execution_class() == right.execution_class()
        && left.package_revision() == right.package_revision()
        && left.restart_policy() == right.restart_policy()
        && left.criticality() == right.criticality()
        && left.execution_lane() == right.execution_lane()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authoring::{HostPluginRelease, HostSlot, PluginDescriptor, PluginRootInstance};

    #[test]
    fn proposal_explains_an_added_optional_plugin() {
        let host = HostCatalog::new(
            [HostSlot::optional("tools")],
            [HostPluginRelease::new(PluginDescriptor::new(
                "company.uppercase",
                "1.0.0",
                "tools",
            ))],
            [],
        );
        let current = PluginRootSnapshot::default();
        let candidate = PluginRootSnapshot::new(
            [],
            [PluginRootInstance::new("company.uppercase", "default")],
            [],
        );
        let proposal = propose_plugin_root_change(&host, &current, &candidate).unwrap();
        assert_eq!(proposal.status(), ChangeProposalStatus::Ready);
        assert_eq!(proposal.application(), ChangeApplication::AppGeneration);
        assert_eq!(
            proposal.changes(),
            [PluginChange::Added {
                instance: PluginInstanceId::new("company.uppercase", "default"),
            }]
        );
        assert!(proposal.digest().starts_with("sha256:"));
    }

    #[test]
    fn evaluation_preserves_a_rejected_candidate_as_structured_data() {
        let host = HostCatalog::new(
            [HostSlot::one("agent")],
            [HostPluginRelease::new(PluginDescriptor::new(
                "company.agent",
                "1.0.0",
                "agent",
            ))],
            [],
        );
        let proposal = evaluate_plugin_root_change(
            &host,
            &PluginRootSnapshot::new(
                [],
                [PluginRootInstance::new("company.agent", "current")],
                [],
            ),
            &PluginRootSnapshot::default(),
        );
        assert_eq!(proposal.status(), ChangeProposalStatus::Rejected);
        assert_eq!(proposal.application(), ChangeApplication::Blocked);
        assert_eq!(proposal.diagnostics()[0].code(), "missing_required_slot");
    }
}
