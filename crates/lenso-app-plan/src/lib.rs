//! App Composition and immutable execution input for the Lenso vNext Kernel.

pub mod authoring;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

mod contract;
pub use contract::{CapabilityBinding, CapabilityRequirementPlan, PluginInstancePlan};
mod error;
mod execution;
mod policy;
mod resolution;
mod schema;
pub use schema::TerminalPolicy;

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
pub const PLAN_SCHEMA_VERSION: u32 = 3;

/// Portable lifecycle and dependency semantics used by authoring version 2 Plugins.
pub const PLUGIN_AUTHORING_V2_RUNTIME_PROFILE: &str = "lenso.plugin-authoring@2";

/// Default maximum number of requests waiting for one Operation.
pub const DEFAULT_REQUEST_QUEUE_CAPACITY: usize = 16;

/// Default maximum concurrent executions for one Operation.
pub const DEFAULT_REQUEST_MAX_CONCURRENCY: usize = 1;

/// Default maximum number of accepted Events retained by one explicit binding.
pub const DEFAULT_EVENT_QUEUE_CAPACITY: usize = 16;

fn default_execution_lanes() -> Vec<ExecutionLanePlan> {
    vec![ExecutionLanePlan::new("main")]
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
                terminal_policy: TerminalPolicy::RequiredPath,
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
#[serde(try_from = "schema::PlanWire")]
pub struct ResolvedAppPlan {
    terminal_policy: TerminalPolicy,
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
            terminal_policy: TerminalPolicy::RequiredPath,
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
            terminal_policy: TerminalPolicy::RequiredPath,
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
            terminal_policy: TerminalPolicy::RequiredPath,
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
        let (instances, bindings) =
            resolve_parts(&self.plugin_instances, &self.capability_bindings)?;
        self.terminal_policy.validate(&instances, &bindings)
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
        self.terminal_policy.validate(&instances, &bindings)?;
        activation_order_for(&instances, &bindings)
            .map_err(|instances| PlanResolutionError::ActivationCycle { instances })
    }

    /// Returns the Plan schema version.
    pub const fn terminal_policy(&self) -> &TerminalPolicy {
        &self.terminal_policy
    }

    /// Selects an explicit terminal policy; unsupported policies fail validation.
    #[must_use]
    pub fn with_terminal_policy(mut self, policy: TerminalPolicy) -> Self {
        self.terminal_policy = policy;
        self
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
                            requirement.requirement_id() == binding.requirement_id()
                                && requirement.cardinality() == CapabilityCardinality::One
                        })
                    })
        })
    }

    /// Returns whether exhaustion of this Plugin Instance is terminal under
    /// the Plan-selected failure policy.
    pub fn plugin_instance_is_terminal(&self, instance_key: &str) -> bool {
        match &self.terminal_policy {
            TerminalPolicy::RequiredPath => self.plugin_instance_is_required(instance_key),
            TerminalPolicy::HostEssential { closure, .. } => closure
                .binary_search_by(|candidate| candidate.as_str().cmp(instance_key))
                .is_ok(),
        }
    }
}
