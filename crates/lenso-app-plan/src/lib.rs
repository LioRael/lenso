//! App Composition and immutable execution input for the Lenso vNext Kernel.

pub mod authoring;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

mod error;
mod execution;
mod policy;
mod resolution;

pub use error::PlanResolutionError;
pub use execution::{ExecutionClassId, ExecutionLaneId, ExecutionLanePlan};
pub use policy::{
    CapabilityCardinality, CapabilityOperationKind, EventAdmissionPlan, PluginCriticality,
    RequestAdmissionPlan, RestartMode, RestartPolicy,
};
use resolution::{
    activation_order_for, resolve_parts, sort_bindings, sort_plugin_instances,
    sorted_execution_lanes, validate_execution_lanes,
};

/// The Resolved App Plan schema understood by this Kernel version.
pub const PLAN_SCHEMA_VERSION: u32 = 2;

/// Default maximum number of requests waiting for one Operation.
pub const DEFAULT_REQUEST_QUEUE_CAPACITY: usize = 16;

/// Default maximum concurrent executions for one Operation.
pub const DEFAULT_REQUEST_MAX_CONCURRENCY: usize = 1;

/// Default maximum number of accepted Events retained by one explicit binding.
pub const DEFAULT_EVENT_QUEUE_CAPACITY: usize = 16;

fn default_execution_lanes() -> Vec<ExecutionLanePlan> {
    vec![ExecutionLanePlan::new("main")]
}

/// One Capability required by a Plugin Instance.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CapabilityRequirementPlan {
    capability_id: String,
    descriptor_version: String,
    cardinality: CapabilityCardinality,
}

impl CapabilityRequirementPlan {
    /// Declares one exact Capability Descriptor and its binding cardinality.
    pub fn new(
        capability_id: impl Into<String>,
        descriptor_version: impl Into<String>,
        cardinality: CapabilityCardinality,
    ) -> Self {
        Self {
            capability_id: capability_id.into(),
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

/// Exact Capability endpoint metadata expected from one Plugin Instance.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CapabilityEndpointPlan {
    capability_id: String,
    descriptor_version: String,
    operations: Vec<String>,
    operation_kinds: BTreeMap<String, CapabilityOperationKind>,
    default_admission: Option<RequestAdmissionPlan>,
    operation_admissions: BTreeMap<String, RequestAdmissionPlan>,
    event_admission: Option<EventAdmissionPlan>,
    #[serde(default)]
    cross_lane_transfer: bool,
}

impl CapabilityEndpointPlan {
    /// Declares one exact Capability Descriptor and its stable Operation table.
    pub fn new(
        capability_id: impl Into<String>,
        descriptor_version: impl Into<String>,
        operations: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            capability_id: capability_id.into(),
            descriptor_version: descriptor_version.into(),
            operations: operations.into_iter().map(Into::into).collect(),
            operation_kinds: BTreeMap::new(),
            default_admission: None,
            operation_admissions: BTreeMap::new(),
            event_admission: None,
            cross_lane_transfer: false,
        }
    }

    /// Applies one bounded admission policy to every Operation on this endpoint.
    #[must_use]
    pub fn with_admission(mut self, admission: RequestAdmissionPlan) -> Self {
        self.default_admission = Some(admission);
        self
    }

    /// Applies queue and concurrency limits to every Operation on this endpoint.
    #[must_use]
    pub fn with_limits(self, queue_capacity: usize, max_concurrency: usize) -> Self {
        self.with_admission(RequestAdmissionPlan::new(queue_capacity, max_concurrency))
    }

    /// Marks one declared Operation with its transport-independent interaction kind.
    #[must_use]
    pub fn with_operation_kind(
        mut self,
        operation: impl Into<String>,
        kind: CapabilityOperationKind,
    ) -> Self {
        self.operation_kinds.insert(operation.into(), kind);
        self
    }

    /// Marks one declared Operation as a bidirectional stream.
    #[must_use]
    pub fn with_stream_operation(self, operation: impl Into<String>) -> Self {
        self.with_operation_kind(operation, CapabilityOperationKind::Stream)
    }

    /// Marks one declared Operation as an ephemeral Event.
    #[must_use]
    pub fn with_event_operation(self, operation: impl Into<String>) -> Self {
        self.with_operation_kind(operation, CapabilityOperationKind::Event)
    }

    /// Applies one volatile mailbox policy to every Event Operation on this endpoint.
    #[must_use]
    pub fn with_event_admission(mut self, admission: EventAdmissionPlan) -> Self {
        self.event_admission = Some(admission);
        self
    }

    /// Applies one volatile mailbox capacity to every Event Operation on this endpoint.
    #[must_use]
    pub fn with_event_capacity(self, capacity: usize) -> Self {
        self.with_event_admission(EventAdmissionPlan::new(capacity))
    }

    /// Marks the generated Capability value types as safe for native cross-lane transfer.
    #[must_use]
    pub const fn with_cross_lane_transfer(mut self) -> Self {
        self.cross_lane_transfer = true;
        self
    }

    /// Applies one bounded admission policy to a named Operation.
    #[must_use]
    pub fn with_operation_admission(
        mut self,
        operation: impl Into<String>,
        admission: RequestAdmissionPlan,
    ) -> Self {
        self.operation_admissions
            .insert(operation.into(), admission);
        self
    }

    /// Applies queue and concurrency limits to a named Operation.
    #[must_use]
    pub fn with_operation_limits(
        self,
        operation: impl Into<String>,
        queue_capacity: usize,
        max_concurrency: usize,
    ) -> Self {
        self.with_operation_admission(
            operation,
            RequestAdmissionPlan::new(queue_capacity, max_concurrency),
        )
    }

    /// Returns the Capability series identity.
    pub fn capability_id(&self) -> &str {
        &self.capability_id
    }

    /// Returns the exact Descriptor version.
    pub fn descriptor_version(&self) -> &str {
        &self.descriptor_version
    }

    /// Returns the exact stable Operation table.
    pub fn operations(&self) -> &[String] {
        &self.operations
    }

    /// Returns the interaction kind of one declared Operation.
    pub fn operation_kind(&self, operation: &str) -> Option<CapabilityOperationKind> {
        self.operations
            .iter()
            .any(|declared| declared == operation)
            .then(|| {
                self.operation_kinds
                    .get(operation)
                    .copied()
                    .unwrap_or(CapabilityOperationKind::Request)
            })
    }

    /// Returns the declared stream Operation names in Descriptor order.
    pub fn stream_operations(&self) -> Vec<&str> {
        self.operations
            .iter()
            .filter(|operation| {
                self.operation_kind(operation) == Some(CapabilityOperationKind::Stream)
            })
            .map(String::as_str)
            .collect()
    }

    /// Returns the declared request Operation names in Descriptor order.
    pub fn request_operations(&self) -> Vec<&str> {
        self.operations
            .iter()
            .filter(|operation| {
                self.operation_kind(operation) == Some(CapabilityOperationKind::Request)
            })
            .map(String::as_str)
            .collect()
    }

    /// Returns the declared ephemeral Event Operation names in Descriptor order.
    pub fn event_operations(&self) -> Vec<&str> {
        self.operations
            .iter()
            .filter(|operation| {
                self.operation_kind(operation) == Some(CapabilityOperationKind::Event)
            })
            .map(String::as_str)
            .collect()
    }

    /// Returns the endpoint-wide Event mailbox policy, when one was authored.
    pub const fn event_admission(&self) -> Option<EventAdmissionPlan> {
        self.event_admission
    }

    /// Returns whether generated values may cross native Execution Lanes without serialization.
    pub const fn supports_cross_lane_transfer(&self) -> bool {
        self.cross_lane_transfer
    }

    /// Returns the endpoint-wide admission policy, when one was authored.
    pub fn default_admission(&self) -> Option<RequestAdmissionPlan> {
        self.default_admission
    }

    /// Returns the Operation-specific admission policies.
    pub fn operation_admissions(&self) -> &BTreeMap<String, RequestAdmissionPlan> {
        &self.operation_admissions
    }

    /// Returns the effective policy for one Operation, if one was authored.
    pub fn operation_admission(&self, operation: &str) -> Option<RequestAdmissionPlan> {
        self.operation_admissions
            .get(operation)
            .copied()
            .or(self.default_admission)
    }
}

/// One exact App-local Plugin Instance selected by the resolved Plan.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PluginInstancePlan {
    instance_key: String,
    package_id: String,
    entrypoint: String,
    configuration: String,
    provided_capabilities: Vec<CapabilityEndpointPlan>,
    required_capabilities: Vec<CapabilityRequirementPlan>,
    execution_class: ExecutionClassId,
    package_revision: String,
    restart_policy: RestartPolicy,
    criticality: PluginCriticality,
    #[serde(default)]
    execution_lane: ExecutionLaneId,
}

impl PluginInstancePlan {
    /// Selects one statically linked package under an App-local Instance key.
    pub fn new(instance_key: impl Into<String>, package_id: impl Into<String>) -> Self {
        Self {
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
pub struct CapabilityBinding {
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

impl CapabilityBinding {
    /// Binds one consumer to one provider at an exact Descriptor version.
    pub fn new(
        consumer_instance: impl Into<String>,
        capability_id: impl Into<String>,
        descriptor_version: impl Into<String>,
        provider_instance: impl Into<String>,
    ) -> Self {
        Self {
            consumer_instance: consumer_instance.into(),
            capability_id: capability_id.into(),
            descriptor_version: descriptor_version.into(),
            provider_instance: provider_instance.into(),
            provider_order: 0,
            admission: RequestAdmissionPlan::default(),
            admission_explicit: false,
            event_admission: EventAdmissionPlan::default(),
            event_admission_explicit: false,
        }
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

    fn with_provider_order(mut self, provider_order: usize) -> Self {
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

/// Declarative, language-independent authoring input for one App.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppComposition {
    plugin_instances: Vec<PluginInstancePlan>,
    capability_bindings: Vec<CapabilityBinding>,
    #[serde(default = "default_execution_lanes")]
    execution_lanes: Vec<ExecutionLanePlan>,
}

impl AppComposition {
    /// Creates an App Composition with explicit Plugin Instances and bindings.
    pub fn new(
        plugin_instances: Vec<PluginInstancePlan>,
        capability_bindings: Vec<CapabilityBinding>,
    ) -> Self {
        Self {
            plugin_instances,
            capability_bindings,
            execution_lanes: default_execution_lanes(),
        }
    }

    /// Replaces the declared Execution Lane set.
    #[must_use]
    pub fn with_execution_lanes(mut self, execution_lanes: Vec<ExecutionLanePlan>) -> Self {
        self.execution_lanes = execution_lanes;
        self
    }

    /// Materializes one deterministic, validated Resolved App Plan.
    pub fn resolve(&self) -> Result<ResolvedAppPlan, PlanResolutionError> {
        validate_execution_lanes(&self.execution_lanes, &self.plugin_instances)?;
        resolve_parts(&self.plugin_instances, &self.capability_bindings).map(
            |(plugin_instances, capability_bindings)| ResolvedAppPlan {
                schema_version: PLAN_SCHEMA_VERSION,
                plugin_instances,
                capability_bindings,
                execution_lanes: sorted_execution_lanes(&self.execution_lanes),
            },
        )
    }

    /// Returns the authoring Plugin Instances.
    pub fn plugin_instances(&self) -> &[PluginInstancePlan] {
        &self.plugin_instances
    }

    /// Returns the authoring bindings.
    pub fn capability_bindings(&self) -> &[CapabilityBinding] {
        &self.capability_bindings
    }

    /// Returns the authored Execution Lanes.
    pub fn execution_lanes(&self) -> &[ExecutionLanePlan] {
        &self.execution_lanes
    }
}

/// Exact, immutable execution input supplied to the Kernel.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResolvedAppPlan {
    schema_version: u32,
    plugin_instances: Vec<PluginInstancePlan>,
    capability_bindings: Vec<CapabilityBinding>,
    #[serde(default = "default_execution_lanes")]
    execution_lanes: Vec<ExecutionLanePlan>,
}

impl ResolvedAppPlan {
    /// Creates a valid Plan containing no Plugin Instances.
    pub fn empty() -> Self {
        Self {
            schema_version: PLAN_SCHEMA_VERSION,
            plugin_instances: Vec::new(),
            capability_bindings: Vec::new(),
            execution_lanes: default_execution_lanes(),
        }
    }

    /// Creates a Plan with exact entries, retaining invalid entries for later validation.
    pub fn new(
        mut plugin_instances: Vec<PluginInstancePlan>,
        mut capability_bindings: Vec<CapabilityBinding>,
    ) -> Self {
        sort_plugin_instances(&mut plugin_instances);
        sort_bindings(&mut capability_bindings);
        Self {
            schema_version: PLAN_SCHEMA_VERSION,
            plugin_instances,
            capability_bindings,
            execution_lanes: default_execution_lanes(),
        }
    }

    /// Creates a Plan with an explicit schema version.
    ///
    /// This is primarily useful to decode authoring-tool output before validation.
    pub const fn with_schema_version(schema_version: u32) -> Self {
        Self {
            schema_version,
            plugin_instances: Vec::new(),
            capability_bindings: Vec::new(),
            execution_lanes: Vec::new(),
        }
    }

    /// Validates the immutable Plan graph before a Runtime Driver or Adapter boots it.
    pub fn validate(&self) -> Result<(), PlanResolutionError> {
        if self.schema_version != PLAN_SCHEMA_VERSION {
            return Err(PlanResolutionError::UnsupportedSchemaVersion {
                expected: PLAN_SCHEMA_VERSION,
                actual: self.schema_version,
            });
        }
        validate_execution_lanes(&self.execution_lanes, &self.plugin_instances)?;
        resolve_parts(&self.plugin_instances, &self.capability_bindings).map(|_| ())
    }

    /// Returns the deterministic provider-before-consumer lifecycle order.
    ///
    /// Every explicit binding is an activation dependency, including an
    /// optional or many binding when one is present in the resolved Plan.
    pub fn activation_order(&self) -> Result<Vec<String>, PlanResolutionError> {
        if self.schema_version != PLAN_SCHEMA_VERSION {
            return Err(PlanResolutionError::UnsupportedSchemaVersion {
                expected: PLAN_SCHEMA_VERSION,
                actual: self.schema_version,
            });
        }
        validate_execution_lanes(&self.execution_lanes, &self.plugin_instances)?;
        let (instances, bindings) =
            resolve_parts(&self.plugin_instances, &self.capability_bindings)?;
        activation_order_for(&instances, &bindings)
            .map_err(|instances| PlanResolutionError::ActivationCycle { instances })
    }

    /// Returns the Plan schema version.
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Returns the exact Plugin Instances in deterministic Plan order.
    pub fn plugin_instances(&self) -> &[PluginInstancePlan] {
        &self.plugin_instances
    }

    /// Returns the exact Capability bindings in deterministic Plan order.
    pub fn capability_bindings(&self) -> &[CapabilityBinding] {
        &self.capability_bindings
    }

    /// Returns the Plan-declared Execution Lanes in deterministic identity order.
    pub fn execution_lanes(&self) -> &[ExecutionLanePlan] {
        &self.execution_lanes
    }

    /// Returns the bounded admission policy materialized for one binding Operation.
    pub fn request_admission_for(
        &self,
        binding: &CapabilityBinding,
        operation: &str,
    ) -> RequestAdmissionPlan {
        if binding.has_explicit_admission() {
            return binding.admission();
        }

        self.plugin_instances
            .iter()
            .find(|instance| instance.instance_key() == binding.provider_instance())
            .and_then(|provider| {
                provider
                    .provided_capabilities()
                    .iter()
                    .find(|endpoint| endpoint.capability_id() == binding.capability_id())
            })
            .and_then(|endpoint| endpoint.operation_admission(operation))
            .unwrap_or_else(|| binding.admission())
    }

    /// Returns the bounded volatile mailbox policy for one Event binding.
    pub fn event_admission_for(&self, binding: &CapabilityBinding) -> EventAdmissionPlan {
        if binding.has_explicit_event_admission() {
            return binding.event_admission();
        }

        self.plugin_instances
            .iter()
            .find(|instance| instance.instance_key() == binding.provider_instance())
            .and_then(|provider| {
                provider
                    .provided_capabilities()
                    .iter()
                    .find(|endpoint| endpoint.capability_id() == binding.capability_id())
            })
            .and_then(CapabilityEndpointPlan::event_admission)
            .unwrap_or_else(|| binding.event_admission())
    }

    /// Returns the exact Plugin Instance selected by its App-local key.
    pub fn plugin_instance(&self, instance_key: &str) -> Option<&PluginInstancePlan> {
        self.plugin_instances
            .iter()
            .find(|instance| instance.instance_key() == instance_key)
    }

    /// Returns the restart policy materialized for one Plugin Instance.
    pub fn restart_policy_for(&self, instance_key: &str) -> Option<RestartPolicy> {
        self.plugin_instance(instance_key)
            .map(PluginInstancePlan::restart_policy)
    }

    /// Returns the criticality materialized for one Plugin Instance.
    pub fn criticality_for(&self, instance_key: &str) -> Option<PluginCriticality> {
        self.plugin_instance(instance_key)
            .map(PluginInstancePlan::criticality)
    }

    /// Returns whether a Plugin Instance is directly bound to a required `one` Capability path.
    pub fn plugin_instance_is_required(&self, instance_key: &str) -> bool {
        self.capability_bindings.iter().any(|binding| {
            binding.provider_instance() == instance_key
                && self
                    .plugin_instance(binding.consumer_instance())
                    .is_some_and(|consumer| {
                        consumer.required_capabilities().iter().any(|requirement| {
                            requirement.capability_id() == binding.capability_id()
                                && requirement.cardinality() == CapabilityCardinality::One
                        })
                    })
        })
    }
}
