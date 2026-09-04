//! Pure selection within Host-granted dependency constraints.

use super::{HostBinding, PluginInstanceId, PluginRootResolutionError, PluginRootSnapshot};
use crate::{CapabilityCardinality, CapabilityRequirementPlan, PluginInstancePlan};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencySelection {
    #[default]
    Fixed,
    Selectable,
}

/// One persisted consumer-local choice. `None` explicitly selects optional absence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyChoice {
    pub consumer: PluginInstanceId,
    pub requirement_id: String,
    pub provider: Option<PluginInstanceId>,
}

fn invalid(detail: impl Into<String>) -> PluginRootResolutionError {
    PluginRootResolutionError::InvalidHostBinding(detail.into())
}

pub(super) fn validate_choices(root: &PluginRootSnapshot) -> Result<(), PluginRootResolutionError> {
    if !root.dependency_selection_adopted && !root.dependency_choices.is_empty() {
        return Err(invalid(
            "saved choices require adoption of named dependency selection",
        ));
    }
    let mut seen = BTreeSet::new();
    for choice in &root.dependency_choices {
        if !crate::schema::valid_requirement_id(&choice.requirement_id)
            || !seen.insert((&choice.consumer, &choice.requirement_id))
        {
            return Err(invalid(format!(
                "invalid or duplicate saved requirement `{}` for `{}`",
                choice.requirement_id, choice.consumer
            )));
        }
        if choice.consumer.plugin_id().is_empty()
            || choice.consumer.instance_key().is_empty()
            || choice.provider.as_ref().is_some_and(|provider| {
                provider.plugin_id().is_empty() || provider.instance_key().is_empty()
            })
        {
            return Err(invalid(
                "saved choice contains an empty Plugin Instance identity",
            ));
        }
    }
    Ok(())
}

#[allow(
    clippy::too_many_arguments,
    reason = "selection receives only explicit pure resolution inputs"
)]
pub(super) fn select_requirement(
    consumer: &PluginInstanceId,
    requirement: &CapabilityRequirementPlan,
    rule: Option<&HostBinding>,
    candidates: &[&PluginInstancePlan],
    ids: &BTreeMap<&str, &PluginInstanceId>,
    root: &PluginRootSnapshot,
    propose: bool,
) -> Result<Option<DependencyChoice>, PluginRootResolutionError> {
    let Some(rule) = rule.filter(|rule| rule.selection == DependencySelection::Selectable) else {
        return Ok(None);
    };
    let id = requirement.requirement_id();
    if !crate::schema::valid_requirement_id(id)
        || requirement.cardinality() == CapabilityCardinality::Many
        || rule.provider_instance.is_some()
        || (rule.provider_slot.is_none() && rule.provider_instances.is_empty())
    {
        return Err(invalid(format!(
            "requirement `{id}` cannot grant selectable binding"
        )));
    }
    let allowed = |provider: &PluginInstanceId| {
        candidates.iter().any(|candidate| {
            ids.get(candidate.instance_key())
                .is_some_and(|id| *id == provider)
        })
    };
    if rule
        .default_provider
        .as_ref()
        .is_some_and(|provider| !allowed(provider))
    {
        return Err(invalid(format!(
            "default provider for requirement `{id}` is unavailable or forbidden"
        )));
    }
    let saved = root
        .dependency_choices
        .iter()
        .find(|choice| &choice.consumer == consumer && choice.requirement_id == id);
    if let Some(saved) = saved {
        if saved
            .provider
            .as_ref()
            .is_some_and(|provider| !allowed(provider))
            || (saved.provider.is_none()
                && requirement.cardinality() != CapabilityCardinality::Optional)
        {
            return Err(invalid(format!(
                "saved provider for requirement `{id}` is unavailable or forbidden"
            )));
        }
        return Ok(Some(saved.clone()));
    }
    if !propose {
        return Err(invalid(format!(
            "requirement `{id}` needs a materialized dependency choice before activation"
        )));
    }
    let provider = if let Some(default) = &rule.default_provider {
        Some(default.clone())
    } else if candidates.len() == 1 {
        ids.get(candidates[0].instance_key())
            .map(|id| (*id).clone())
    } else if candidates.is_empty() && requirement.cardinality() == CapabilityCardinality::Optional
    {
        None
    } else {
        return Err(invalid(format!(
            "requirement `{id}` needs an explicit provider selection"
        )));
    };
    Ok(Some(DependencyChoice {
        consumer: consumer.clone(),
        requirement_id: id.to_owned(),
        provider,
    }))
}
