use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{PluginDescriptor, empty_configuration};
use crate::{
    CapabilityEndpointPlan, CapabilityRequirementPlan, ExecutionClassId, PluginCriticality,
    RestartPolicy,
};

/// Runtime-independent contract shared by every implementation of one Plugin Release.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginContract {
    plugin_id: String,
    release_version: String,
    root_slot: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    configuration_schema: Option<Value>,
    #[serde(
        default = "empty_configuration",
        skip_serializing_if = "super::is_empty_configuration"
    )]
    configuration_defaults: Value,
    provided_capabilities: Vec<CapabilityEndpointPlan>,
    required_capabilities: Vec<CapabilityRequirementPlan>,
    restart_policy: RestartPolicy,
    criticality: PluginCriticality,
}

impl PluginContract {
    pub fn new(
        plugin_id: impl Into<String>,
        release_version: impl Into<String>,
        root_slot: impl Into<String>,
    ) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            release_version: release_version.into(),
            root_slot: root_slot.into(),
            configuration_schema: None,
            configuration_defaults: empty_configuration(),
            provided_capabilities: Vec::new(),
            required_capabilities: Vec::new(),
            restart_policy: RestartPolicy::default(),
            criticality: PluginCriticality::default(),
        }
    }

    #[must_use]
    pub fn with_configuration_schema(mut self, schema: Value) -> Self {
        self.configuration_schema = Some(schema);
        self
    }

    #[must_use]
    pub fn with_configuration_defaults(mut self, defaults: Value) -> Self {
        self.configuration_defaults = defaults;
        self
    }

    #[must_use]
    pub fn with_capability(mut self, capability: CapabilityEndpointPlan) -> Self {
        self.provided_capabilities.push(capability);
        self
    }

    #[must_use]
    pub fn with_requirement(mut self, requirement: CapabilityRequirementPlan) -> Self {
        self.required_capabilities.push(requirement);
        self
    }

    #[must_use]
    pub const fn with_restart_policy(mut self, restart_policy: RestartPolicy) -> Self {
        self.restart_policy = restart_policy;
        self
    }

    #[must_use]
    pub const fn with_criticality(mut self, criticality: PluginCriticality) -> Self {
        self.criticality = criticality;
        self
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    pub fn release_version(&self) -> &str {
        &self.release_version
    }

    pub fn root_slot(&self) -> &str {
        &self.root_slot
    }

    pub const fn configuration_schema(&self) -> Option<&Value> {
        self.configuration_schema.as_ref()
    }

    pub const fn configuration_defaults(&self) -> &Value {
        &self.configuration_defaults
    }

    pub fn provided_capabilities(&self) -> &[CapabilityEndpointPlan] {
        &self.provided_capabilities
    }

    pub fn required_capabilities(&self) -> &[CapabilityRequirementPlan] {
        &self.required_capabilities
    }

    pub const fn restart_policy(&self) -> RestartPolicy {
        self.restart_policy
    }

    pub const fn criticality(&self) -> PluginCriticality {
        self.criticality
    }

    /// Resolves this stable contract against one exact executable implementation.
    pub fn resolve(&self, implementation: &PluginImplementation) -> PluginDescriptor {
        let mut descriptor =
            PluginDescriptor::new(&self.plugin_id, &self.release_version, &self.root_slot)
                .with_runtime_package(
                    &implementation.runtime_package_id,
                    &implementation.runtime_package_revision,
                )
                .with_entrypoint(&implementation.entrypoint)
                .with_configuration_defaults(self.configuration_defaults.clone())
                .with_execution_class(implementation.execution_class.clone())
                .with_restart_policy(self.restart_policy)
                .with_criticality(self.criticality);
        if let Some(schema) = &self.configuration_schema {
            descriptor = descriptor.with_configuration_schema(schema.clone());
        }
        for capability in &self.provided_capabilities {
            descriptor = descriptor.with_capability(capability.clone());
        }
        for requirement in &self.required_capabilities {
            descriptor = descriptor.with_requirement(requirement.clone());
        }
        descriptor
    }
}

/// Exact runtime selection used to execute one Plugin Contract.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginImplementation {
    runtime_package_id: String,
    runtime_package_revision: String,
    entrypoint: String,
    execution_class: ExecutionClassId,
}

impl PluginImplementation {
    pub fn new(
        runtime_package_id: impl Into<String>,
        runtime_package_revision: impl Into<String>,
        entrypoint: impl Into<String>,
        execution_class: ExecutionClassId,
    ) -> Self {
        Self {
            runtime_package_id: runtime_package_id.into(),
            runtime_package_revision: runtime_package_revision.into(),
            entrypoint: entrypoint.into(),
            execution_class,
        }
    }

    pub fn runtime_package_id(&self) -> &str {
        &self.runtime_package_id
    }

    pub fn runtime_package_revision(&self) -> &str {
        &self.runtime_package_revision
    }

    pub fn entrypoint(&self) -> &str {
        &self.entrypoint
    }

    pub const fn execution_class(&self) -> &ExecutionClassId {
        &self.execution_class
    }
}

impl PluginDescriptor {
    /// Projects the runtime-independent Contract from this resolved descriptor.
    pub fn contract(&self) -> PluginContract {
        PluginContract {
            plugin_id: self.plugin_id.clone(),
            release_version: self.release_version.clone(),
            root_slot: self.root_slot.clone(),
            configuration_schema: self.configuration_schema.clone(),
            configuration_defaults: self.configuration_defaults.clone(),
            provided_capabilities: self.provided_capabilities.clone(),
            required_capabilities: self.required_capabilities.clone(),
            restart_policy: self.restart_policy,
            criticality: self.criticality,
        }
    }

    /// Projects the exact runtime selection from this resolved descriptor.
    pub fn implementation(&self) -> PluginImplementation {
        PluginImplementation {
            runtime_package_id: self.runtime_package_id.clone(),
            runtime_package_revision: self.runtime_package_revision.clone(),
            entrypoint: self.entrypoint.clone(),
            execution_class: self.execution_class.clone(),
        }
    }
}
