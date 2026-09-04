//! Versioned wire decoding and provenance validation for immutable Plans.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    CapabilityBinding, CapabilityCardinality, CapabilityRequirementPlan, EventAdmissionPlan,
    ExecutionClassId, ExecutionLanePlan, PLAN_SCHEMA_VERSION, PlanResolutionError,
    PluginInstancePlan, RequestAdmissionPlan, ResolvedAppPlan, default_execution_lanes,
};

pub(crate) const fn old_authoring_version() -> u32 {
    1
}

/// App terminal semantics selected before activation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TerminalPolicy {
    /// Preserve the existing required Capability path failure policy.
    RequiredPath,
    /// Host-selected essential roots and their exact resolved dependency closure.
    /// Execution support is intentionally gated independently of wire recognition.
    HostEssential {
        roots: Vec<String>,
        closure: Vec<String>,
    },
}

impl TerminalPolicy {
    pub(super) fn validate(
        &self,
        instances: &[PluginInstancePlan],
        bindings: &[CapabilityBinding],
    ) -> Result<(), PlanResolutionError> {
        match self {
            Self::RequiredPath => Ok(()),
            Self::HostEssential { roots, closure } => {
                validate_sorted_unique("roots", roots)?;
                validate_sorted_unique("closure", closure)?;
                let selected = instances
                    .iter()
                    .map(PluginInstancePlan::instance_key)
                    .collect::<BTreeSet<_>>();
                for instance in roots.iter().chain(closure) {
                    if !selected.contains(instance.as_str()) {
                        return Err(PlanResolutionError::InvalidTerminalPolicy {
                            detail: format!("unknown Plugin Instance `{instance}`"),
                        });
                    }
                }
                let mut expected = roots.iter().cloned().collect::<BTreeSet<_>>();
                let mut pending = roots.clone();
                while let Some(consumer) = pending.pop() {
                    let Some(instance) = instances
                        .iter()
                        .find(|instance| instance.instance_key() == consumer)
                    else {
                        continue;
                    };
                    for requirement in
                        instance
                            .required_capabilities()
                            .iter()
                            .filter(|requirement| {
                                requirement.cardinality() == CapabilityCardinality::One
                            })
                    {
                        for provider in bindings.iter().filter(|binding| {
                            binding.consumer_instance() == consumer
                                && binding.requirement_id() == requirement.requirement_id()
                        }) {
                            if expected.insert(provider.provider_instance().to_owned()) {
                                pending.push(provider.provider_instance().to_owned());
                            }
                        }
                    }
                }
                let expected = expected.into_iter().collect::<Vec<_>>();
                if *closure != expected {
                    return Err(PlanResolutionError::InvalidTerminalPolicy {
                        detail: format!(
                            "materialized closure {closure:?} does not match recomputed closure {expected:?}"
                        ),
                    });
                }
                Ok(())
            }
        }
    }
}

fn validate_sorted_unique(field: &str, values: &[String]) -> Result<(), PlanResolutionError> {
    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(PlanResolutionError::InvalidTerminalPolicy {
            detail: format!("{field} must be sorted and unique"),
        });
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RequirementWire {
    requirement_id: Option<String>,
    capability_id: String,
    descriptor_version: String,
    cardinality: CapabilityCardinality,
}

impl From<RequirementWire> for CapabilityRequirementPlan {
    fn from(wire: RequirementWire) -> Self {
        Self {
            requirement_id: wire
                .requirement_id
                .unwrap_or_else(|| format!("~{}", wire.capability_id)),
            capability_id: wire.capability_id,
            descriptor_version: wire.descriptor_version,
            cardinality: wire.cardinality,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BindingWire {
    requirement_id: Option<String>,
    consumer_instance: String,
    capability_id: String,
    descriptor_version: String,
    provider_instance: String,
    provider_order: usize,
    admission: RequestAdmissionPlan,
    admission_explicit: bool,
    event_admission: EventAdmissionPlan,
    event_admission_explicit: bool,
}

impl From<BindingWire> for CapabilityBinding {
    fn from(wire: BindingWire) -> Self {
        Self {
            requirement_id: wire
                .requirement_id
                .unwrap_or_else(|| format!("~{}", wire.capability_id)),
            consumer_instance: wire.consumer_instance,
            capability_id: wire.capability_id,
            descriptor_version: wire.descriptor_version,
            provider_instance: wire.provider_instance,
            provider_order: wire.provider_order,
            admission: wire.admission,
            admission_explicit: wire.admission_explicit,
            event_admission: wire.event_admission,
            event_admission_explicit: wire.event_admission_explicit,
        }
    }
}

#[derive(Deserialize)]
#[serde(transparent)]
pub(super) struct PlanWire(Value);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DecodedPlan {
    schema_version: u32,
    terminal_policy: Option<TerminalPolicy>,
    plugin_instances: Vec<PluginInstancePlan>,
    capability_bindings: Vec<CapabilityBinding>,
    #[serde(default = "default_execution_lanes")]
    execution_lanes: Vec<ExecutionLanePlan>,
}

impl TryFrom<PlanWire> for ResolvedAppPlan {
    type Error = String;

    fn try_from(wire: PlanWire) -> Result<Self, String> {
        let version = wire
            .0
            .get("schema_version")
            .and_then(Value::as_u64)
            .ok_or("missing Plan schema_version")?;
        if version != 2 && version != u64::from(PLAN_SCHEMA_VERSION) {
            return Err(format!("unsupported Plan schema version {version}"));
        }
        let modern = version == u64::from(PLAN_SCHEMA_VERSION);
        check_field(&wire.0, "terminal_policy", modern)?;
        for instance in wire
            .0
            .get("plugin_instances")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            check_field(instance, "authoring_version", modern)?;
            check_field(instance, "runtime_profile", modern)?;
            for requirement in instance
                .get("required_capabilities")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                check_field(requirement, "requirement_id", modern)?;
            }
        }
        for binding in wire
            .0
            .get("capability_bindings")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            check_field(binding, "requirement_id", modern)?;
        }
        let mut decoded: DecodedPlan =
            serde_json::from_value(wire.0).map_err(|error| error.to_string())?;
        if decoded.schema_version == 2 {
            for instance in &mut decoded.plugin_instances {
                instance.runtime_profile = old_runtime_profile(&instance.execution_class);
            }
        }
        Ok(Self {
            schema_version: PLAN_SCHEMA_VERSION,
            terminal_policy: decoded
                .terminal_policy
                .unwrap_or(TerminalPolicy::RequiredPath),
            plugin_instances: decoded.plugin_instances,
            capability_bindings: decoded.capability_bindings,
            execution_lanes: decoded.execution_lanes,
        })
    }
}

fn check_field(value: &Value, field: &str, expected: bool) -> Result<(), String> {
    if expected && value.get(field).is_some_and(Value::is_null) {
        return Err(format!("Plan schema requires non-null {field}"));
    }
    if value.get(field).is_some() != expected {
        return Err(format!(
            "Plan schema {} {field}",
            if expected { "requires" } else { "forbids" }
        ));
    }
    Ok(())
}

pub(crate) fn old_runtime_profile(class: &ExecutionClassId) -> String {
    match class.as_str() {
        "lenso.native-rust@1" => "lenso.native-authoring@1".to_owned(),
        // An old execution class is preserved as opaque legacy provenance. It
        // must never be upgraded to a newer Adapter profile by the decoder.
        other => other.to_owned(),
    }
}

pub(super) fn validate_authoring(instance: &PluginInstancePlan) -> Result<(), PlanResolutionError> {
    if !matches!(instance.authoring_version, 1 | 2) || instance.runtime_profile.trim().is_empty() {
        return Err(PlanResolutionError::InvalidAuthoring {
            instance_key: instance.instance_key.clone(),
        });
    }
    for requirement in &instance.required_capabilities {
        let id = requirement.requirement_id();
        let valid = if instance.authoring_version == 1 {
            id == format!("~{}", requirement.capability_id())
        } else {
            valid_requirement_id(id)
        };
        if !valid {
            return Err(PlanResolutionError::InvalidRequirementId {
                consumer_instance: instance.instance_key.clone(),
                requirement_id: id.to_owned(),
            });
        }
    }
    Ok(())
}

pub(crate) fn valid_requirement_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id.as_bytes()[0].is_ascii_lowercase()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}
