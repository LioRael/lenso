//! Read-only, structured App admission and binding evidence.
//!
//! This module projects persisted Host and Runtime facts. It never invokes a
//! second implementation or dependency resolver.
//! Execution/cache lifecycle facts belong to an active `lenso_engine::Session`;
//! this static inspection reports that absence explicitly rather than guessing.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use clap::Args;
use lenso_app_plan::{CapabilityCardinality, authoring::HostBinding};
use lenso_plugin_bundle::ExecutionTargetCapabilityProfile;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::target_profile::ImplementationSelectionEvidence;

/// Explain target admission and resolved consumer bindings without executing
/// an App or resolving an alternate implementation.
#[derive(Args, Clone, Debug)]
pub struct ExplainArgs {
    /// Built Host authoring root. Defaults to the current directory.
    #[arg(long)]
    pub root: Option<PathBuf>,
    /// Emit the stable `lenso.app-explain.v1` JSON contract.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Deserialize)]
struct BundleSelectionRecord {
    plugin_id: String,
    target_capability_profile: ExecutionTargetCapabilityProfile,
    selection: ImplementationSelectionEvidence,
}

pub(super) fn run(args: ExplainArgs) -> anyhow::Result<()> {
    let root = crate::plugins::project_root(args.root)?;
    let report = report(&root)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        let implementations = report["implementation_selection"]
            .as_array()
            .context("invalid implementation explanation")?;
        let requirements = report["consumer_requirements"]
            .as_array()
            .context("invalid consumer explanation")?;
        println!(
            "Host admission: {} implementation selection(s), {} consumer requirement(s). Use --json for lenso.app-explain.v1.",
            implementations.len(),
            requirements.len(),
        );
    }
    Ok(())
}

/// Produces the JSON-ready App contract from the current persisted Host output.
pub(super) fn report(root: &Path) -> anyhow::Result<Value> {
    let state = lenso_app_authoring::inspect_plugin_root(root)?;
    let host_path = root.join(".lenso/host-build.json");
    let host: lenso_app_authoring::host_authoring::GeneratedHostBuild = serde_json::from_slice(
        &fs::read(&host_path)
            .with_context(|| format!("read generated Host authority {}", host_path.display()))?,
    )
    .context("invalid generated Host authority")?;
    host.validate()?;

    let inventory_path = root.join("bundles.json");
    let inventory: Vec<Value> = serde_json::from_slice(
        &fs::read(&inventory_path)
            .with_context(|| format!("read Host bundle inventory {}", inventory_path.display()))?,
    )
    .context("invalid Host bundle inventory")?;
    let mut bundles = inventory
        .into_iter()
        .map(|value| {
            serde_json::from_value::<BundleSelectionRecord>(value)
                .context("Host bundle inventory is missing Runtime selection evidence")
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    if bundles.len() > 256 {
        bail!("Host bundle inventory exceeds 256 entries");
    }
    bundles.sort_by(|left, right| left.plugin_id.cmp(&right.plugin_id));

    let resolved = state.resolved();
    let plan_instance_ids = resolved
        .instances()
        .iter()
        .map(|instance| (instance.plan_key().to_owned(), instance.id().to_string()))
        .collect::<BTreeMap<_, _>>();
    let enabled_instances = resolved
        .instances()
        .iter()
        .map(|instance| instance.id().to_string())
        .collect::<BTreeSet<_>>();
    let disabled_instances = state
        .plugins()
        .iter()
        .flat_map(|plugin| plugin.instances())
        .filter(|instance| instance.is_disabled_by_root())
        .map(|instance| instance.id().to_string())
        .collect::<BTreeSet<_>>();

    let mut target_profiles = BTreeMap::new();
    let implementations = bundles
        .iter()
        .map(|bundle| {
            target_profiles.insert(
                serde_json::to_string(&bundle.target_capability_profile)?,
                serde_json::to_value(&bundle.target_capability_profile)?,
            );
            Ok(json!({
                "plugin_id": bundle.plugin_id,
                "selected": bundle.selection.selected,
                "target_capability_profile": bundle.target_capability_profile,
                "rejected": bundle.selection.rejected,
            }))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let mut consumer_requirements = Vec::new();
    let mut unmet_consumer_requirements = Vec::new();
    for consumer in resolved.plan().plugin_instances() {
        let consumer_id = plan_instance_ids
            .get(consumer.instance_key())
            .context("resolved Plan consumer is missing its Plugin Instance identity")?;
        for requirement in consumer.required_capabilities() {
            let selected_providers = resolved
                .plan()
                .capability_bindings()
                .iter()
                .filter(|binding| {
                    binding.consumer_instance() == consumer.instance_key()
                        && binding.requirement_id() == requirement.requirement_id()
                })
                .map(|binding| {
                    Ok(json!({
                        "provider_instance": plan_instance_ids
                            .get(binding.provider_instance())
                            .cloned()
                            .unwrap_or_else(|| binding.provider_instance().to_owned()),
                        "provider_order": binding.provider_order(),
                    }))
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            let selected_ids = selected_providers
                .iter()
                .filter_map(|provider| provider["provider_instance"].as_str())
                .collect::<BTreeSet<_>>();
            // `load_resolved_app` normally rejects this state before an
            // explanation exists. Keep the projection explicit so embedding
            // callers never have to infer an unmet mandatory demand from an
            // empty provider array, and so a future permissive inspection path
            // has a stable representation rather than inventing one.
            let unmet = requirement.cardinality() == CapabilityCardinality::One
                && selected_providers.is_empty();
            if unmet {
                unmet_consumer_requirements.push(json!({
                    "consumer_instance": consumer_id,
                    "requirement_id": requirement.requirement_id(),
                    "capability_id": requirement.capability_id(),
                    "descriptor_version": requirement.descriptor_version(),
                    "cardinality": requirement.cardinality(),
                    "reason": "no_selected_provider",
                }));
            }
            let host_binding = host.catalog().bindings().iter().find(|binding| {
                binding.consumer().to_string() == *consumer_id
                    && binding.requirement_id() == requirement.requirement_id()
                    && binding.capability_id() == requirement.capability_id()
            });
            let non_selected_candidates = host_binding
                .map(|binding| {
                    non_selected_direct_candidates(
                        binding,
                        &selected_ids,
                        &enabled_instances,
                        &disabled_instances,
                        resolved.dependency_choices(),
                    )
                })
                .unwrap_or_default();
            consumer_requirements.push(json!({
                "consumer_instance": consumer_id,
                "requirement_id": requirement.requirement_id(),
                "capability_id": requirement.capability_id(),
                "descriptor_version": requirement.descriptor_version(),
                "cardinality": requirement.cardinality(),
                "selected_providers": selected_providers,
                "satisfied": !unmet,
                "host_binding_scope": host_binding.map(host_binding_scope),
                "non_selected_host_provider_candidates": non_selected_candidates,
            }));
        }
    }

    Ok(json!({
        "schema": "lenso.app-explain.v1",
        "host": { "id": host.host_id() },
        "target_capability_profiles": target_profiles.into_values().collect::<Vec<_>>(),
        "implementation_selection": implementations,
        "consumer_requirements": consumer_requirements,
        "unmet_consumer_requirements": unmet_consumer_requirements,
        "engine_execution": {
            "schema": "lenso.engine-explain.v1",
            "availability": {
                "kind": "unavailable",
                "reason": "no_active_engine_session",
            },
        },
    }))
}

fn host_binding_scope(binding: &HostBinding) -> Value {
    json!({
        "provider_slot": binding.provider_slot(),
        "provider_instance": binding.provider_instance().map(ToString::to_string),
        "provider_instances": binding
            .provider_instances()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
    })
}

fn non_selected_direct_candidates(
    binding: &HostBinding,
    selected: &BTreeSet<&str>,
    enabled: &BTreeSet<String>,
    disabled: &BTreeSet<String>,
    choices: &[lenso_app_plan::authoring::DependencyChoice],
) -> Vec<Value> {
    let mut candidates = binding
        .provider_instance()
        .into_iter()
        .chain(binding.provider_instances().iter())
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>();
    candidates.retain(|candidate| !selected.contains(candidate.as_str()));
    candidates
        .into_iter()
        .map(|candidate| {
            let reason = if disabled.contains(&candidate) {
                "disabled_by_root"
            } else if !enabled.contains(&candidate) {
                "not_present_in_resolved_app"
            } else if choices.iter().any(|choice| {
                choice.consumer == *binding.consumer()
                    && choice.requirement_id == binding.requirement_id()
                    && choice
                        .provider
                        .as_ref()
                        .is_some_and(|provider| provider.to_string() != candidate)
            }) {
                "not_selected_by_persisted_dependency_choice"
            } else {
                "not_in_resolved_binding"
            };
            json!({ "provider_instance": candidate, "reason": reason })
        })
        .collect()
}
