use std::collections::{BTreeMap, BTreeSet};

use super::super::configuration::ConfigurationError;
use super::{
    CandidateInstance, CapabilityBinding, CapabilityCardinality, HostBinding, HostCatalog,
    HostDefaultPlugin, HostPluginConfiguration, HostPluginRelease, HostSlot, PluginDescriptor,
    PluginInstanceId, PluginInstancePlan, PluginInstanceSource, PluginRootInstance,
    PluginRootResolutionError, PluginRootSnapshot, ResolvedApp, ResolvedPluginInstance, Value,
    materialize_app, select_slot_candidates,
};

pub fn resolve_plugin_root(
    host: &HostCatalog,
    root: &PluginRootSnapshot,
) -> Result<ResolvedApp, PluginRootResolutionError> {
    resolve_root(host, root, false)
}

/// Proposes exact choices without reading or writing Root storage.
pub fn propose_plugin_root(
    host: &HostCatalog,
    root: &PluginRootSnapshot,
) -> Result<ResolvedApp, PluginRootResolutionError> {
    resolve_root(host, root, true)
}

fn resolve_root(
    host: &HostCatalog,
    root: &PluginRootSnapshot,
    propose: bool,
) -> Result<ResolvedApp, PluginRootResolutionError> {
    super::selection::validate_choices(root)?;
    let slots = index_slots(&host.slots)?;
    let releases = index_releases(&host.plugins, &root.releases)?;
    validate_release_slots(&releases, &slots)?;
    let defaults = index_defaults(&host.defaults)?;
    let configurations = index_configurations(&host.configurations, &defaults, &releases)?;
    let explicit = index_root_instances(&root.instances)?;
    let disabled = index_disabled(&root.disabled)?;
    validate_disabled(&defaults, &explicit, &disabled)?;
    let candidates = build_candidates(&defaults, &configurations, &explicit, &disabled, &releases)?;
    let selected = select_slot_candidates(&slots, candidates)?;
    materialize_app(
        selected,
        &host.bindings,
        &host.execution_lanes,
        root,
        propose,
    )
}

fn index_slots(slots: &[HostSlot]) -> Result<BTreeMap<&str, &HostSlot>, PluginRootResolutionError> {
    let mut indexed = BTreeMap::new();
    for slot in slots {
        if indexed.insert(slot.id.as_str(), slot).is_some() {
            return Err(PluginRootResolutionError::DuplicateHostSlot(
                slot.id.clone(),
            ));
        }
    }
    Ok(indexed)
}

fn index_releases<'a>(
    host: &'a [HostPluginRelease],
    root: &'a [PluginDescriptor],
) -> Result<BTreeMap<&'a str, &'a PluginDescriptor>, PluginRootResolutionError> {
    let mut indexed = BTreeMap::new();
    let mut overrides = BTreeMap::new();
    for release in host {
        let id = release.descriptor.plugin_id();
        if indexed.insert(id, &release.descriptor).is_some() {
            return Err(PluginRootResolutionError::DuplicatePluginRelease(
                id.to_owned(),
            ));
        }
        overrides.insert(id, release.allow_root_override);
    }
    let mut root_ids = BTreeSet::new();
    for descriptor in root {
        let id = descriptor.plugin_id();
        if !root_ids.insert(id) {
            return Err(PluginRootResolutionError::DuplicatePluginRelease(
                id.to_owned(),
            ));
        }
        if indexed.contains_key(id) && !overrides.get(id).copied().unwrap_or(false) {
            return Err(PluginRootResolutionError::RootReleaseOverrideDenied(
                id.to_owned(),
            ));
        }
        indexed.insert(id, descriptor);
    }
    Ok(indexed)
}

fn validate_release_slots(
    releases: &BTreeMap<&str, &PluginDescriptor>,
    slots: &BTreeMap<&str, &HostSlot>,
) -> Result<(), PluginRootResolutionError> {
    for descriptor in releases.values() {
        if !slots.contains_key(descriptor.root_slot()) {
            return Err(PluginRootResolutionError::UnknownRootSlot {
                plugin_id: descriptor.plugin_id().to_owned(),
                slot: descriptor.root_slot().to_owned(),
            });
        }
    }
    Ok(())
}

fn index_defaults(
    defaults: &[HostDefaultPlugin],
) -> Result<BTreeMap<&PluginInstanceId, &HostDefaultPlugin>, PluginRootResolutionError> {
    let mut indexed = BTreeMap::new();
    for default in defaults {
        if indexed.insert(&default.id, default).is_some() {
            return Err(PluginRootResolutionError::DuplicateInstance(
                default.id.clone(),
            ));
        }
    }
    Ok(indexed)
}

fn index_configurations<'a>(
    configurations: &'a [HostPluginConfiguration],
    defaults: &BTreeMap<&PluginInstanceId, &HostDefaultPlugin>,
    releases: &BTreeMap<&str, &PluginDescriptor>,
) -> Result<BTreeMap<&'a PluginInstanceId, &'a Value>, PluginRootResolutionError> {
    let mut indexed = BTreeMap::new();
    for configuration in configurations {
        if !releases.contains_key(configuration.id.plugin_id()) {
            return Err(PluginRootResolutionError::InvalidHostConfiguration(
                format!("`{}` has no exact Plugin Release", configuration.id),
            ));
        }
        if defaults.contains_key(&configuration.id) {
            return Err(PluginRootResolutionError::InvalidHostConfiguration(
                format!(
                    "`{}` duplicates configuration owned by a Host default",
                    configuration.id
                ),
            ));
        }
        if indexed
            .insert(&configuration.id, &configuration.configuration)
            .is_some()
        {
            return Err(PluginRootResolutionError::InvalidHostConfiguration(
                format!("duplicate configuration for `{}`", configuration.id),
            ));
        }
    }
    Ok(indexed)
}

fn index_root_instances(
    instances: &[PluginRootInstance],
) -> Result<BTreeMap<&PluginInstanceId, &PluginRootInstance>, PluginRootResolutionError> {
    let mut indexed = BTreeMap::new();
    for instance in instances {
        if indexed.insert(&instance.id, instance).is_some() {
            return Err(PluginRootResolutionError::DuplicateInstance(
                instance.id.clone(),
            ));
        }
    }
    Ok(indexed)
}

fn index_disabled(
    disabled: &[PluginInstanceId],
) -> Result<BTreeSet<&PluginInstanceId>, PluginRootResolutionError> {
    let mut indexed = BTreeSet::new();
    for instance in disabled {
        if !indexed.insert(instance) {
            return Err(PluginRootResolutionError::DuplicateDisabledMarker(
                instance.clone(),
            ));
        }
    }
    Ok(indexed)
}

fn validate_disabled(
    defaults: &BTreeMap<&PluginInstanceId, &HostDefaultPlugin>,
    explicit: &BTreeMap<&PluginInstanceId, &PluginRootInstance>,
    disabled: &BTreeSet<&PluginInstanceId>,
) -> Result<(), PluginRootResolutionError> {
    for id in disabled {
        if let Some(default) = defaults.get(id) {
            if !default.disableable {
                return Err(PluginRootResolutionError::RequiredInstanceDisabled(
                    (*id).clone(),
                ));
            }
        } else if !explicit.contains_key(id) {
            return Err(PluginRootResolutionError::UnknownDisabledInstance(
                (*id).clone(),
            ));
        }
    }
    Ok(())
}

fn build_candidates<'a>(
    defaults: &BTreeMap<&'a PluginInstanceId, &'a HostDefaultPlugin>,
    configurations: &BTreeMap<&'a PluginInstanceId, &'a Value>,
    explicit: &BTreeMap<&'a PluginInstanceId, &'a PluginRootInstance>,
    disabled: &BTreeSet<&PluginInstanceId>,
    releases: &BTreeMap<&'a str, &'a PluginDescriptor>,
) -> Result<Vec<CandidateInstance<'a>>, PluginRootResolutionError> {
    let ids = defaults
        .keys()
        .chain(explicit.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    let mut candidates = Vec::new();
    for id in ids {
        if disabled.contains(id) {
            continue;
        }
        let descriptor = releases
            .get(id.plugin_id())
            .copied()
            .ok_or_else(|| PluginRootResolutionError::UnknownPlugin(id.clone()))?;
        let host_default = defaults.get(id).copied();
        let root_instance = explicit.get(id).copied();
        let source = match (host_default, root_instance) {
            (Some(_), Some(_)) => PluginInstanceSource::HostDefaultConfiguredByRoot,
            (Some(_), None) => PluginInstanceSource::HostDefault,
            (None, Some(_)) => PluginInstanceSource::PluginRoot,
            (None, None) => unreachable!("candidate IDs come from default or explicit input"),
        };
        candidates.push(CandidateInstance {
            id: id.clone(),
            descriptor,
            host_configuration: host_default
                .map(|default| &default.configuration)
                .or_else(|| configurations.get(id).copied()),
            root_configuration: root_instance.map(|instance| &instance.configuration),
            source,
        });
    }
    Ok(candidates)
}

#[allow(
    clippy::too_many_lines,
    reason = "resolve constraints, saved intent and exact bindings in one deterministic pass"
)]
pub(super) fn derive_root_bindings(
    instances: &[PluginInstancePlan],
    resolved: &[ResolvedPluginInstance],
    plan_slots: &BTreeMap<String, String>,
    host_bindings: &[HostBinding],
    root: &PluginRootSnapshot,
    propose: bool,
) -> Result<(Vec<CapabilityBinding>, Vec<super::DependencyChoice>), PluginRootResolutionError> {
    let ids_by_plan_key = resolved
        .iter()
        .map(|instance| (instance.plan_key.as_str(), &instance.id))
        .collect::<BTreeMap<_, _>>();
    let plan_keys_by_id = resolved
        .iter()
        .map(|instance| (&instance.id, instance.plan_key.as_str()))
        .collect::<BTreeMap<_, _>>();
    let indexed = index_host_bindings(host_bindings)?;

    let mut consumed = BTreeSet::new();
    let mut choices = Vec::new();
    let mut bindings = Vec::new();
    for consumer in instances {
        let Some(consumer_id) = ids_by_plan_key.get(consumer.instance_key()) else {
            return Err(PluginRootResolutionError::InvalidResolvedApp(format!(
                "missing Plugin identity for `{}`",
                consumer.instance_key()
            )));
        };
        for requirement in consumer.required_capabilities() {
            let key = (*consumer_id, requirement.requirement_id().to_owned());
            let host_binding = indexed.get(&key).copied();
            if host_binding
                .is_some_and(|binding| binding.capability_id() != requirement.capability_id())
            {
                return Err(PluginRootResolutionError::InvalidHostBinding(format!(
                    "requirement `{}` Capability mismatch",
                    requirement.requirement_id()
                )));
            }
            if host_binding.is_some() {
                consumed.insert(key);
            }
            let mut candidates = instances
                .iter()
                .filter(|provider| {
                    provider.provided_capabilities().iter().any(|endpoint| {
                        endpoint.capability_id() == requirement.capability_id()
                            && endpoint.descriptor_version() == requirement.descriptor_version()
                    }) && host_binding.is_none_or(|binding| {
                        binding.provider_slot.as_ref().is_none_or(|slot| {
                            plan_slots.get(provider.instance_key()) == Some(slot)
                        }) && binding.provider_instance.as_ref().is_none_or(|id| {
                            plan_keys_by_id.get(id).copied() == Some(provider.instance_key())
                        }) && (binding.provider_instances.is_empty()
                            || binding.provider_instances.iter().any(|id| {
                                plan_keys_by_id.get(id).copied() == Some(provider.instance_key())
                            }))
                    })
                })
                .collect::<Vec<_>>();
            candidates.sort_by_key(|candidate| candidate.instance_key());
            if let Some(binding) = host_binding
                && binding.selection == super::DependencySelection::Fixed
                && !binding.provider_instances.is_empty()
                && candidates.len() != binding.provider_instances.len()
            {
                return Err(PluginRootResolutionError::InvalidHostBinding(format!(
                    "attachment for `{consumer_id}` Capability `{}` does not resolve every selected provider Instance",
                    requirement.capability_id()
                )));
            }
            let selected_choice = super::selection::select_requirement(
                consumer_id,
                requirement,
                host_binding,
                &candidates,
                &ids_by_plan_key,
                root,
                propose,
            )?;
            if let Some(choice) = selected_choice {
                candidates.retain(|candidate| {
                    choice
                        .provider
                        .as_ref()
                        .is_some_and(|provider| provider.plan_key() == candidate.instance_key())
                });
                choices.push(choice);
            }
            let selected =
                select_cardinality(consumer_id, requirement, candidates, &ids_by_plan_key)?;
            for provider in selected {
                let mut binding = CapabilityBinding::new(
                    consumer.instance_key(),
                    requirement.capability_id(),
                    requirement.descriptor_version(),
                    provider.instance_key(),
                )
                .with_requirement_id(requirement.requirement_id());
                if let Some(admission) = host_binding.and_then(HostBinding::admission) {
                    binding = binding.with_admission(admission);
                }
                bindings.push(binding);
            }
        }
    }
    validate_host_bindings_consumed(host_bindings, &plan_keys_by_id, &consumed)?;
    for choice in &root.dependency_choices {
        if plan_keys_by_id.contains_key(&choice.consumer)
            && !choices.iter().any(|selected| {
                selected.consumer == choice.consumer
                    && selected.requirement_id == choice.requirement_id
            })
        {
            return Err(PluginRootResolutionError::InvalidHostBinding(format!(
                "saved choice for `{}` requirement `{}` is not selectable",
                choice.consumer, choice.requirement_id
            )));
        }
        if !plan_keys_by_id.contains_key(&choice.consumer) {
            choices.push(choice.clone());
        }
    }
    choices.sort_by(|left, right| {
        (&left.consumer, &left.requirement_id).cmp(&(&right.consumer, &right.requirement_id))
    });
    Ok((bindings, choices))
}

type IndexedHostBindings<'a> = BTreeMap<(&'a PluginInstanceId, String), &'a HostBinding>;

fn index_host_bindings(
    host_bindings: &[HostBinding],
) -> Result<IndexedHostBindings<'_>, PluginRootResolutionError> {
    let mut indexed = BTreeMap::new();
    for binding in host_bindings {
        if binding
            .requirement_id
            .as_deref()
            .is_some_and(|id| !crate::schema::valid_requirement_id(id))
            || (binding.selection == super::DependencySelection::Fixed
                && binding.default_provider.is_some())
        {
            return Err(PluginRootResolutionError::InvalidHostBinding(format!(
                "invalid selection declaration for `{}`",
                binding.consumer
            )));
        }
        let key = (&binding.consumer, binding.requirement_id().into_owned());
        if indexed.insert(key, binding).is_some() {
            return Err(PluginRootResolutionError::InvalidHostBinding(format!(
                "duplicate attachment for `{}` Capability `{}`",
                binding.consumer, binding.capability_id
            )));
        }
        let selectors = usize::from(binding.provider_slot.is_some())
            + usize::from(binding.provider_instance.is_some())
            + usize::from(!binding.provider_instances.is_empty());
        if selectors != 1 {
            return Err(PluginRootResolutionError::InvalidHostBinding(format!(
                "attachment for `{}` must select exactly one provider Slot, Instance, or Instance set",
                binding.consumer
            )));
        }
        if binding
            .provider_instances
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            != binding.provider_instances.len()
        {
            return Err(PluginRootResolutionError::InvalidHostBinding(format!(
                "attachment for `{}` contains a duplicate provider Instance",
                binding.consumer
            )));
        }
    }
    Ok(indexed)
}

fn validate_host_bindings_consumed(
    host_bindings: &[HostBinding],
    plan_keys_by_id: &BTreeMap<&PluginInstanceId, &str>,
    consumed: &BTreeSet<(&PluginInstanceId, String)>,
) -> Result<(), PluginRootResolutionError> {
    let Some(binding) = host_bindings.iter().find(|binding| {
        plan_keys_by_id.contains_key(&binding.consumer)
            && !consumed.contains(&(&binding.consumer, binding.requirement_id().into_owned()))
    }) else {
        return Ok(());
    };
    let consumer_plan_key = plan_keys_by_id
        .get(&binding.consumer)
        .copied()
        .unwrap_or("<missing>");
    Err(PluginRootResolutionError::InvalidHostBinding(format!(
        "attachment for `{consumer_plan_key}` Capability `{}` matches no requirement",
        binding.capability_id
    )))
}

pub(super) fn map_configuration_error(
    instance: &PluginInstanceId,
    error: ConfigurationError,
) -> PluginRootResolutionError {
    PluginRootResolutionError::InvalidConfiguration {
        instance: instance.clone(),
        detail: error.detail,
    }
}

fn select_cardinality<'a>(
    consumer_id: &PluginInstanceId,
    requirement: &crate::CapabilityRequirementPlan,
    candidates: Vec<&'a PluginInstancePlan>,
    ids_by_plan_key: &BTreeMap<&str, &PluginInstanceId>,
) -> Result<Vec<&'a PluginInstancePlan>, PluginRootResolutionError> {
    match requirement.cardinality() {
        CapabilityCardinality::Many => Ok(candidates),
        CapabilityCardinality::One if candidates.len() == 1 => Ok(candidates),
        CapabilityCardinality::Optional if candidates.len() <= 1 => Ok(candidates),
        CapabilityCardinality::One if candidates.is_empty() => {
            Err(PluginRootResolutionError::MissingCapability {
                consumer: consumer_id.clone(),
                capability_id: requirement.capability_id().to_owned(),
                descriptor_version: requirement.descriptor_version().to_owned(),
            })
        }
        CapabilityCardinality::Optional | CapabilityCardinality::One => {
            Err(PluginRootResolutionError::AmbiguousCapability {
                consumer: consumer_id.clone(),
                capability_id: requirement.capability_id().to_owned(),
                candidates: candidates
                    .iter()
                    .filter_map(|candidate| {
                        ids_by_plan_key
                            .get(candidate.instance_key())
                            .copied()
                            .cloned()
                    })
                    .collect(),
            })
        }
    }
}
