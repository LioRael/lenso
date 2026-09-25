//! Target admission for the exact Capability bindings in one resolved App.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use anyhow::{Context, bail};
use lenso_app_plan::{CapabilityOperationKind, ExecutionClassId, authoring::ResolvedApp};
use lenso_plugin_bundle::{ExecutionTargetCapability, ExecutionTargetCapabilityProfile};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct InventoryTarget {
    plugin_id: String,
    artifact_digest: String,
    execution_class: String,
    runtime_profile: String,
    target_capability_profile: ExecutionTargetCapabilityProfile,
}

/// Check consumer-side interactions after the one resolver has fixed every
/// provider. A consumer's own Bundle cannot infer the Operation kinds of a
/// Capability it requires; those are known only from the bound provider.
pub(super) fn admit(resolved: &ResolvedApp, inventory: &[Value]) -> anyhow::Result<()> {
    let mut profiles = BTreeMap::new();
    for value in inventory {
        let record: InventoryTarget = serde_json::from_value(value.clone())
            .context("invalid target admission in Host bundle inventory")?;
        record
            .target_capability_profile
            .validate()
            .map_err(|error| anyhow::anyhow!(error.to_string()))
            .with_context(|| format!("Plugin `{}` target profile", record.plugin_id))?;
        let concrete = match record.execution_class.as_str() {
            "lenso.bun-process@1"
                if record.runtime_profile == lenso_bun_adapter::BUN_AUTHORING_RUNTIME_PROFILE =>
            {
                crate::target_profile::bun_admission()?
            }
            "lenso.process@1"
                if matches!(
                    record.runtime_profile.as_str(),
                    lenso_process_adapter::RUNTIME_PROFILE_V1
                        | lenso_process_adapter::RUNTIME_PROFILE_V2
                ) =>
            {
                crate::target_profile::request_native_process_admission(
                    ExecutionClassId::new(&record.execution_class),
                    &record.runtime_profile,
                )
            }
            "lenso.wasm-component@1"
                if record.runtime_profile == lenso_wasm_component_adapter::RUNTIME_PROFILE =>
            {
                crate::target_profile::request_wasm_component_admission(
                    ExecutionClassId::new(&record.execution_class),
                    &record.runtime_profile,
                )
            }
            _ => bail!(
                "Plugin `{}` has no concrete Host Adapter admission for `{}` / `{}`",
                record.plugin_id,
                record.execution_class,
                record.runtime_profile,
            ),
        };
        if record.target_capability_profile != concrete.capability_profile() {
            bail!(
                "Plugin `{}` target profile differs from concrete Host Adapter admission",
                record.plugin_id
            );
        }
        let identity = (
            record.plugin_id.clone(),
            record.artifact_digest.clone(),
            record.execution_class.clone(),
            record.runtime_profile.clone(),
        );
        if let Some(previous) = profiles.insert(identity, record.clone())
            && previous != record
        {
            bail!(
                "Plugin `{}` artifact `{}` has conflicting target admissions",
                record.plugin_id,
                record.artifact_digest,
            );
        }
    }

    let plan = resolved.plan();
    let instance_ids = resolved
        .instances()
        .iter()
        .map(|instance| (instance.plan_key(), instance.id()))
        .collect::<BTreeMap<_, _>>();
    let identities = resolved
        .instances()
        .iter()
        .map(|instance| (instance.plan_key(), instance.id().to_string()))
        .collect::<BTreeMap<_, _>>();
    let mut selected_profiles = BTreeMap::new();
    for instance in plan.plugin_instances() {
        if instance.execution_class().as_str() == "lenso.native-rust@1" {
            continue;
        }
        let id = instance_ids
            .get(instance.instance_key())
            .context("selected Plan Instance has no Plugin identity")?;
        let identity = (
            id.plugin_id().to_owned(),
            instance.package_revision().to_owned(),
            instance.execution_class().as_str().to_owned(),
            instance.runtime_profile().to_owned(),
        );
        let record = profiles.get(&identity).with_context(|| {
            format!("selected Plugin `{id}` has no target admission in Host inventory")
        })?;
        if record.execution_class != instance.execution_class().as_str()
            || record.runtime_profile != instance.runtime_profile()
            || record.target_capability_profile.target_profile != instance.runtime_profile()
        {
            bail!("selected Plugin `{id}` differs from its Host target admission");
        }
        selected_profiles.insert(instance.instance_key(), &record.target_capability_profile);
    }

    for binding in plan.capability_bindings() {
        let Some(profile) = selected_profiles.get(binding.consumer_instance()) else {
            continue;
        };
        let provider = plan
            .plugin_instance(binding.provider_instance())
            .context("resolved binding has no provider Instance")?;
        let endpoint = provider
            .provided_capabilities()
            .iter()
            .find(|endpoint| {
                endpoint.capability_id() == binding.capability_id()
                    && endpoint.descriptor_version() == binding.descriptor_version()
            })
            .context("resolved binding has no exact provider Capability")?;
        for operation in endpoint.operations() {
            let feature = match endpoint.operation_kind(operation) {
                Some(CapabilityOperationKind::Request) => ExecutionTargetCapability::Request,
                Some(CapabilityOperationKind::Stream) => ExecutionTargetCapability::Stream,
                Some(CapabilityOperationKind::Event) => ExecutionTargetCapability::Event,
                None => bail!("bound provider has an undeclared Operation `{operation}`"),
            };
            if !profile.supports(feature) {
                let path = dependency_path(resolved, &identities, binding);
                bail!(
                    "target admission denied for dependency path {path}: consumer target profile `{}` does not admit `{}` for `{}.{operation}`; no alternate provider or runtime was selected",
                    profile.target_profile,
                    feature.as_str(),
                    endpoint.capability_id(),
                );
            }
        }
    }
    Ok(())
}

fn dependency_path(
    resolved: &ResolvedApp,
    identities: &BTreeMap<&str, String>,
    failing: &lenso_app_plan::CapabilityBinding,
) -> String {
    let bindings = resolved.plan().capability_bindings();
    let providers = bindings
        .iter()
        .map(|binding| binding.provider_instance())
        .collect::<BTreeSet<_>>();
    let mut roots = identities
        .keys()
        .copied()
        .filter(|key| !providers.contains(key))
        .collect::<Vec<_>>();
    roots.sort_unstable();
    let mut queue = roots
        .into_iter()
        .map(|key| (key, identities[key].clone()))
        .collect::<VecDeque<_>>();
    let mut visited = BTreeSet::new();
    while let Some((current, path)) = queue.pop_front() {
        if !visited.insert(current) {
            continue;
        }
        if current == failing.consumer_instance() {
            return format!(
                "{path} --{}--> {}",
                failing.requirement_id(),
                identities
                    .get(failing.provider_instance())
                    .map(String::as_str)
                    .unwrap_or(failing.provider_instance())
            );
        }
        let mut edges = bindings
            .iter()
            .filter(|binding| binding.consumer_instance() == current)
            .collect::<Vec<_>>();
        edges.sort_unstable_by_key(|binding| {
            (binding.requirement_id(), binding.provider_instance())
        });
        for edge in edges {
            let provider = edge.provider_instance();
            let provider_id = identities
                .get(provider)
                .map(String::as_str)
                .unwrap_or(provider);
            queue.push_back((
                provider,
                format!("{path} --{}--> {provider_id}", edge.requirement_id()),
            ));
        }
    }
    format!(
        "{} --{}--> {}",
        identities
            .get(failing.consumer_instance())
            .map(String::as_str)
            .unwrap_or(failing.consumer_instance()),
        failing.requirement_id(),
        identities
            .get(failing.provider_instance())
            .map(String::as_str)
            .unwrap_or(failing.provider_instance())
    )
}

/// Re-check changed Plugin Root choices against the profiles recorded by a
/// generated Host. Legacy hand-authored Catalogs have no such inventory.
pub(super) fn check_generated_host(
    root: &std::path::Path,
    resolved: &ResolvedApp,
) -> anyhow::Result<()> {
    if !root.join(".lenso/host-build.json").is_file() {
        return Ok(());
    }
    let inventory: Vec<Value> = serde_json::from_slice(
        &std::fs::read(root.join("bundles.json")).context("read Host bundle inventory")?,
    )
    .context("invalid Host bundle inventory")?;
    admit(resolved, &inventory)
}

#[cfg(test)]
mod tests;
