//! Consumer declarations and exact Plugin execution and binding metadata.

use super::{
    CapabilityCardinality, CapabilityEndpointPlan, EventAdmissionPlan, ExecutionClassId,
    ExecutionLaneId, PluginCriticality, RequestAdmissionPlan, RestartPolicy, schema,
};
use serde::{Deserialize, Serialize};

/// One Capability required by a Plugin Instance.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(from = "schema::RequirementWire")]
pub struct CapabilityRequirementPlan {
    #[serde(default)]
    pub(super) requirement_id: String,
    pub(super) capability_id: String,
    pub(super) descriptor_version: String,
    pub(super) cardinality: CapabilityCardinality,
}

impl CapabilityRequirementPlan {
    /// Declares one exact Capability Descriptor and its binding cardinality.
    pub fn new(
        capability_id: impl Into<String>,
        descriptor_version: impl Into<String>,
        cardinality: CapabilityCardinality,
    ) -> Self {
        let capability_id = capability_id.into();
        Self {
            requirement_id: format!("~{capability_id}"),
            capability_id,
            descriptor_version: descriptor_version.into(),
            cardinality,
        }
    }

    /// Declares a required one-provider Capability.
    pub fn one(capability_id: impl Into<String>, descriptor_version: impl Into<String>) -> Self {
        Self::new(
            capability_id,
            descriptor_version,
            CapabilityCardinality::One,
        )
    }

    /// Declares an optional zero-or-one-provider Capability.
    pub fn optional(
        capability_id: impl Into<String>,
        descriptor_version: impl Into<String>,
    ) -> Self {
        Self::new(
            capability_id,
            descriptor_version,
            CapabilityCardinality::Optional,
        )
    }

    /// Declares a many-provider Capability.
    pub fn many(capability_id: impl Into<String>, descriptor_version: impl Into<String>) -> Self {
        Self::new(
            capability_id,
            descriptor_version,
            CapabilityCardinality::Many,
        )
    }

    /// Names this dependency within its consumer's version 2 contract.
    #[must_use]
    pub fn with_requirement_id(mut self, requirement_id: impl Into<String>) -> Self {
        self.requirement_id = requirement_id.into();
        self
    }

    /// Returns the consumer-local identity, including normalized old declarations.
    pub fn requirement_id(&self) -> &str {
        &self.requirement_id
    }

    /// Returns the Capability series identity.
    pub fn capability_id(&self) -> &str {
        &self.capability_id
    }

    /// Returns the exact Descriptor version selected by Composition.
    pub fn descriptor_version(&self) -> &str {
        &self.descriptor_version
    }

    /// Returns the requirement cardinality.
    pub const fn cardinality(&self) -> CapabilityCardinality {
        self.cardinality
    }
}

/// One exact App-local Plugin Instance selected by the resolved Plan.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PluginInstancePlan {
    #[serde(default = "schema::old_authoring_version")]
    pub(super) authoring_version: u32,
    #[serde(default)]
    pub(super) runtime_profile: String,
    pub(super) instance_key: String,
    pub(super) package_id: String,
    pub(super) entrypoint: String,
    pub(super) configuration: String,
    pub(super) provided_capabilities: Vec<CapabilityEndpointPlan>,
    pub(super) required_capabilities: Vec<CapabilityRequirementPlan>,
    pub(super) execution_class: ExecutionClassId,
    pub(super) package_revision: String,
    pub(super) restart_policy: RestartPolicy,
    pub(super) criticality: PluginCriticality,
    #[serde(default)]
    pub(super) execution_lane: ExecutionLaneId,
}

impl PluginInstancePlan {
    /// Selects one statically linked package under an App-local Instance key.
    pub fn new(instance_key: impl Into<String>, package_id: impl Into<String>) -> Self {
        Self {
            authoring_version: 1,
            runtime_profile: "lenso.native-authoring@1".to_owned(),
            instance_key: instance_key.into(),
            package_id: package_id.into(),
            entrypoint: "default".to_owned(),
            configuration: "{}".to_owned(),
            provided_capabilities: Vec::new(),
            required_capabilities: Vec::new(),
            execution_class: ExecutionClassId::native_rust(),
            package_revision: String::new(),
            restart_policy: RestartPolicy::default(),
            criticality: PluginCriticality::default(),
            execution_lane: ExecutionLaneId::default(),
        }
    }

    /// Selects the exact package entrypoint executed for this Instance.
    #[must_use]
    pub fn with_authoring(mut self, version: u32, runtime_profile: impl Into<String>) -> Self {
        self.authoring_version = version;
        self.runtime_profile = runtime_profile.into();
        self
    }

    /// Returns the selected Plugin contract version.
    pub const fn authoring_version(&self) -> u32 {
        self.authoring_version
    }

    /// Returns the exact opaque execution profile, independently of execution class.
    pub fn runtime_profile(&self) -> &str {
        &self.runtime_profile
    }

    /// Selects the exact package entrypoint executed for this Instance.
    #[must_use]
    pub fn with_entrypoint(mut self, entrypoint: impl Into<String>) -> Self {
        self.entrypoint = entrypoint.into();
        self
    }

    /// Supplies opaque, non-secret configuration owned and decoded by the Plugin.
    #[must_use]
    pub fn with_configuration(mut self, configuration: impl Into<String>) -> Self {
        self.configuration = configuration.into();
        self
    }

    /// Declares one exact endpoint this Instance must prepare.
    #[must_use]
    pub fn with_capability(mut self, capability: CapabilityEndpointPlan) -> Self {
        self.provided_capabilities.push(capability);
        self
    }

    /// Declares one Capability dependency for this Instance.
    #[must_use]
    pub fn with_requirement(mut self, requirement: CapabilityRequirementPlan) -> Self {
        self.required_capabilities.push(requirement);
        self
    }

    /// Alias that makes the authoring direction explicit at the call site.
    #[must_use]
    pub fn with_required_capability(self, requirement: CapabilityRequirementPlan) -> Self {
        self.with_requirement(requirement)
    }

    /// Selects the host execution class for this Plugin Instance.
    #[must_use]
    pub fn with_execution_class(mut self, execution_class: ExecutionClassId) -> Self {
        if self.authoring_version == 1
            && self.runtime_profile == schema::old_runtime_profile(&self.execution_class)
        {
            self.runtime_profile = schema::old_runtime_profile(&execution_class);
        }
        self.execution_class = execution_class;
        self
    }

    /// Places this Plugin Instance on one Plan-declared Execution Lane.
    #[must_use]
    pub fn with_execution_lane(mut self, execution_lane: ExecutionLaneId) -> Self {
        self.execution_lane = execution_lane;
        self
    }

    /// Records the exact opaque package-manager lock selection before boot.
    #[must_use]
    pub fn with_package_revision(mut self, revision: impl Into<String>) -> Self {
        self.package_revision = revision.into();
        self
    }

    /// Selects the finite supervision policy for this Plugin Instance.
    #[must_use]
    pub fn with_restart_policy(mut self, restart_policy: RestartPolicy) -> Self {
        self.restart_policy = restart_policy;
        self
    }

    /// Marks this Plugin Instance critical for supervision exhaustion outcomes.
    #[must_use]
    pub fn with_criticality(mut self, criticality: PluginCriticality) -> Self {
        self.criticality = criticality;
        self
    }

    /// Returns the App-local Instance key.
    pub fn instance_key(&self) -> &str {
        &self.instance_key
    }

    /// Returns the selected package identity.
    pub fn package_id(&self) -> &str {
        &self.package_id
    }

    /// Returns the exact package entrypoint selected before boot.
    pub fn entrypoint(&self) -> &str {
        &self.entrypoint
    }

    /// Returns the Plugin-owned opaque configuration selected before boot.
    pub fn configuration(&self) -> &str {
        &self.configuration
    }

    /// Returns the exact endpoint set this Instance must prepare.
    pub fn provided_capabilities(&self) -> &[CapabilityEndpointPlan] {
        &self.provided_capabilities
    }

    /// Returns the exact Capability requirements this Instance receives.
    pub fn required_capabilities(&self) -> &[CapabilityRequirementPlan] {
        &self.required_capabilities
    }

    /// Returns the host execution class selected for this Instance.
    pub fn execution_class(&self) -> &ExecutionClassId {
        &self.execution_class
    }

    /// Returns the Plan-declared Execution Lane for this Instance.
    pub const fn execution_lane(&self) -> &ExecutionLaneId {
        &self.execution_lane
    }

    /// Returns the exact opaque package-manager lock selection.
    pub fn package_revision(&self) -> &str {
        &self.package_revision
    }

    /// Returns the supervision policy selected for this Instance.
    pub const fn restart_policy(&self) -> RestartPolicy {
        self.restart_policy
    }

    /// Returns the criticality selected for this Instance.
    pub const fn criticality(&self) -> PluginCriticality {
        self.criticality
    }
}

/// One exact consumer-to-provider Capability binding.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(from = "schema::BindingWire")]
pub struct CapabilityBinding {
    #[serde(default)]
    pub(super) requirement_id: String,
    pub(super) consumer_instance: String,
    pub(super) capability_id: String,
    pub(super) descriptor_version: String,
    pub(super) provider_instance: String,
    pub(super) provider_order: usize,
    pub(super) admission: RequestAdmissionPlan,
    pub(super) admission_explicit: bool,
    pub(super) event_admission: EventAdmissionPlan,
    pub(super) event_admission_explicit: bool,
}

impl CapabilityBinding {
    /// Binds one consumer to one provider at an exact Descriptor version.
    pub fn new(
        consumer_instance: impl Into<String>,
        capability_id: impl Into<String>,
        descriptor_version: impl Into<String>,
        provider_instance: impl Into<String>,
    ) -> Self {
        let capability_id = capability_id.into();
        Self {
            requirement_id: format!("~{capability_id}"),
            consumer_instance: consumer_instance.into(),
            capability_id,
            descriptor_version: descriptor_version.into(),
            provider_instance: provider_instance.into(),
            provider_order: 0,
            admission: RequestAdmissionPlan::default(),
            admission_explicit: false,
            event_admission: EventAdmissionPlan::default(),
            event_admission_explicit: false,
        }
    }

    /// Selects the consumer-local named requirement.
    #[must_use]
    pub fn with_requirement_id(mut self, requirement_id: impl Into<String>) -> Self {
        self.requirement_id = requirement_id.into();
        self
    }

    /// Returns the consumer-local requirement selected by this binding.
    pub fn requirement_id(&self) -> &str {
        &self.requirement_id
    }

    /// Overrides the provider Operation admission policy for this binding.
    #[must_use]
    pub fn with_admission(mut self, admission: RequestAdmissionPlan) -> Self {
        self.admission = admission;
        self.admission_explicit = true;
        self
    }

    /// Overrides queue and concurrency limits for this binding.
    #[must_use]
    pub fn with_limits(self, queue_capacity: usize, max_concurrency: usize) -> Self {
        self.with_admission(RequestAdmissionPlan::new(queue_capacity, max_concurrency))
    }

    /// Overrides the Event mailbox policy for this binding.
    #[must_use]
    pub fn with_event_admission(mut self, admission: EventAdmissionPlan) -> Self {
        self.event_admission = admission;
        self.event_admission_explicit = true;
        self
    }

    /// Overrides the Event mailbox capacity for this binding.
    #[must_use]
    pub fn with_event_capacity(self, capacity: usize) -> Self {
        self.with_event_admission(EventAdmissionPlan::new(capacity))
    }

    pub(super) fn with_provider_order(mut self, provider_order: usize) -> Self {
        self.provider_order = provider_order;
        self
    }

    /// Returns the consumer Instance key.
    pub fn consumer_instance(&self) -> &str {
        &self.consumer_instance
    }

    /// Returns the Capability series identity.
    pub fn capability_id(&self) -> &str {
        &self.capability_id
    }

    /// Returns the exact Descriptor version.
    pub fn descriptor_version(&self) -> &str {
        &self.descriptor_version
    }

    /// Returns the provider Instance key.
    pub fn provider_instance(&self) -> &str {
        &self.provider_instance
    }

    /// Returns the deterministic zero-based order within a `many` requirement.
    pub const fn provider_order(&self) -> usize {
        self.provider_order
    }

    /// Returns the binding's effective fallback admission policy.
    pub const fn admission(&self) -> RequestAdmissionPlan {
        self.admission
    }

    /// Returns whether this binding explicitly overrides the provider policy.
    pub const fn has_explicit_admission(&self) -> bool {
        self.admission_explicit
    }

    /// Returns the binding's effective fallback Event mailbox policy.
    pub const fn event_admission(&self) -> EventAdmissionPlan {
        self.event_admission
    }

    /// Returns whether this binding explicitly overrides the provider Event policy.
    pub const fn has_explicit_event_admission(&self) -> bool {
        self.event_admission_explicit
    }
}
