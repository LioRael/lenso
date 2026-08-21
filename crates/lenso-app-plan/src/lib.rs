//! App Composition and immutable execution input for the Lenso vNext Kernel.

pub mod authoring;

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    time::Duration,
};

use serde::{Deserialize, Serialize};

mod execution;
mod resolution;

pub use execution::{ExecutionClassId, ExecutionLaneId, ExecutionLanePlan};
use resolution::{
    activation_order_for, resolve_parts, sort_bindings, sort_module_instances,
    sorted_execution_lanes, validate_execution_lanes,
};

/// The Resolved App Plan schema understood by this Kernel version.
pub const PLAN_SCHEMA_VERSION: u32 = 1;

/// Default maximum number of requests waiting for one Operation.
pub const DEFAULT_REQUEST_QUEUE_CAPACITY: usize = 16;

/// Default maximum concurrent executions for one Operation.
pub const DEFAULT_REQUEST_MAX_CONCURRENCY: usize = 1;

/// Default maximum number of accepted Events retained by one explicit binding.
pub const DEFAULT_EVENT_QUEUE_CAPACITY: usize = 16;

fn default_execution_lanes() -> Vec<ExecutionLanePlan> {
    vec![ExecutionLanePlan::new("main")]
}

/// The cardinality of one Module's Capability requirement.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityCardinality {
    /// Exactly one provider must be bound.
    One,
    /// Zero or one provider may be bound.
    Optional,
    /// Zero or more providers may be bound in deterministic order.
    Many,
}

/// The transport-independent interaction semantics of one Capability Operation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityOperationKind {
    /// One request produces one response or Domain Error.
    Request,
    /// One open establishes an ordered, bidirectional stream session.
    Stream,
    /// One publication is delivered to zero or more subscribers.
    Event,
}

/// The bounded admission policy materialized for one request Operation.
///
/// `queue_capacity` counts requests waiting for one of the
/// `max_concurrency` execution slots. A zero queue capacity is valid and
/// makes admission fail immediately while all execution slots are occupied.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RequestAdmissionPlan {
    queue_capacity: usize,
    max_concurrency: usize,
}

impl RequestAdmissionPlan {
    /// Creates a bounded request admission policy.
    pub const fn new(queue_capacity: usize, max_concurrency: usize) -> Self {
        Self {
            queue_capacity,
            max_concurrency,
        }
    }

    /// Returns the maximum number of requests waiting for an execution slot.
    pub const fn queue_capacity(self) -> usize {
        self.queue_capacity
    }

    /// Returns the maximum number of requests executing concurrently.
    pub const fn max_concurrency(self) -> usize {
        self.max_concurrency
    }

    fn validate(self, capability_id: &str, operation: &str) -> Result<(), PlanResolutionError> {
        if self.max_concurrency == 0 {
            return Err(PlanResolutionError::InvalidRequestAdmission {
                capability_id: capability_id.to_owned(),
                operation: operation.to_owned(),
                queue_capacity: self.queue_capacity,
                max_concurrency: self.max_concurrency,
            });
        }
        Ok(())
    }
}

impl Default for RequestAdmissionPlan {
    fn default() -> Self {
        Self::new(
            DEFAULT_REQUEST_QUEUE_CAPACITY,
            DEFAULT_REQUEST_MAX_CONCURRENCY,
        )
    }
}

/// The bounded volatile mailbox policy materialized for one Event binding.
///
/// Capacity counts all accepted Events that have not completed handling. Zero
/// is valid and makes every publication to the binding report exhausted.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EventAdmissionPlan {
    capacity: usize,
}

impl EventAdmissionPlan {
    /// Creates one Event mailbox policy.
    pub const fn new(capacity: usize) -> Self {
        Self { capacity }
    }

    /// Returns the maximum number of accepted Events retained by the binding.
    pub const fn capacity(self) -> usize {
        self.capacity
    }
}

impl Default for EventAdmissionPlan {
    fn default() -> Self {
        Self::new(DEFAULT_EVENT_QUEUE_CAPACITY)
    }
}

/// The finite restart mode selected for one Module Instance.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RestartMode {
    /// Do not recreate a failed generation.
    Never,
    /// Recreate failed generations within a bounded attempt window.
    OnFailure,
}

/// Bounded supervision settings materialized in the Resolved App Plan.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RestartPolicy {
    mode: RestartMode,
    max_attempts: usize,
    window: Duration,
    backoff: Duration,
    jitter: Duration,
    stability: Duration,
}

impl RestartPolicy {
    /// Creates a policy that never recreates a failed generation.
    pub const fn never() -> Self {
        Self {
            mode: RestartMode::Never,
            max_attempts: 0,
            window: Duration::ZERO,
            backoff: Duration::ZERO,
            jitter: Duration::ZERO,
            stability: Duration::ZERO,
        }
    }

    /// Creates a finite on-failure policy.
    pub const fn on_failure(
        max_attempts: usize,
        window: Duration,
        backoff: Duration,
        jitter: Duration,
        stability: Duration,
    ) -> Self {
        Self {
            mode: RestartMode::OnFailure,
            max_attempts,
            window,
            backoff,
            jitter,
            stability,
        }
    }

    /// Returns the selected restart mode.
    pub const fn mode(self) -> RestartMode {
        self.mode
    }

    /// Returns the maximum number of recreation attempts in one window.
    pub const fn max_attempts(self) -> usize {
        self.max_attempts
    }

    /// Returns the rolling attempt window.
    pub const fn window(self) -> Duration {
        self.window
    }

    /// Returns the initial exponential backoff duration.
    pub const fn backoff(self) -> Duration {
        self.backoff
    }

    /// Returns the maximum jitter requested from the Runtime Driver.
    pub const fn jitter(self) -> Duration {
        self.jitter
    }

    /// Returns the ready period after which the attempt budget becomes stable again.
    pub const fn stability(self) -> Duration {
        self.stability
    }

    fn validate(&self, instance_key: &str) -> Result<(), PlanResolutionError> {
        if self.mode == RestartMode::OnFailure && (self.max_attempts == 0 || self.window.is_zero())
        {
            return Err(PlanResolutionError::InvalidRestartPolicy {
                instance_key: instance_key.to_owned(),
                max_attempts: self.max_attempts,
                window: self.window,
            });
        }
        Ok(())
    }
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self::never()
    }
}

/// Whether a failed Module Instance is allowed to remain unavailable.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleCriticality {
    /// Exhaustion leaves this Module unavailable when it is not required by a `one` binding.
    #[default]
    NonCritical,
    /// Exhaustion fails the App even when no `one` binding reaches this Module.
    Critical,
}

impl ModuleCriticality {
    /// Returns whether this criticality requires a terminal App outcome on exhaustion.
    pub const fn is_critical(self) -> bool {
        matches!(self, Self::Critical)
    }
}

/// One Capability required by a Module Instance.
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

/// Exact Capability endpoint metadata expected from one Module Instance.
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

/// One exact App-local Module Instance selected by the resolved Plan.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModuleInstancePlan {
    instance_key: String,
    package_id: String,
    entrypoint: String,
    configuration: String,
    provided_capabilities: Vec<CapabilityEndpointPlan>,
    required_capabilities: Vec<CapabilityRequirementPlan>,
    execution_class: ExecutionClassId,
    package_revision: String,
    restart_policy: RestartPolicy,
    criticality: ModuleCriticality,
    #[serde(default)]
    execution_lane: ExecutionLaneId,
}

impl ModuleInstancePlan {
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
            criticality: ModuleCriticality::default(),
            execution_lane: ExecutionLaneId::default(),
        }
    }

    /// Selects the exact package entrypoint executed for this Instance.
    #[must_use]
    pub fn with_entrypoint(mut self, entrypoint: impl Into<String>) -> Self {
        self.entrypoint = entrypoint.into();
        self
    }

    /// Supplies opaque, non-secret configuration owned and decoded by the Module.
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

    /// Selects the host execution class for this Module Instance.
    #[must_use]
    pub fn with_execution_class(mut self, execution_class: ExecutionClassId) -> Self {
        self.execution_class = execution_class;
        self
    }

    /// Places this Module Instance on one Plan-declared Execution Lane.
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

    /// Selects the finite supervision policy for this Module Instance.
    #[must_use]
    pub fn with_restart_policy(mut self, restart_policy: RestartPolicy) -> Self {
        self.restart_policy = restart_policy;
        self
    }

    /// Marks this Module Instance critical for supervision exhaustion outcomes.
    #[must_use]
    pub fn with_criticality(mut self, criticality: ModuleCriticality) -> Self {
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

    /// Returns the Module-owned opaque configuration selected before boot.
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
    pub const fn criticality(&self) -> ModuleCriticality {
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
    module_instances: Vec<ModuleInstancePlan>,
    capability_bindings: Vec<CapabilityBinding>,
    #[serde(default = "default_execution_lanes")]
    execution_lanes: Vec<ExecutionLanePlan>,
}

impl AppComposition {
    /// Creates an App Composition with explicit Module Instances and bindings.
    pub fn new(
        module_instances: Vec<ModuleInstancePlan>,
        capability_bindings: Vec<CapabilityBinding>,
    ) -> Self {
        Self {
            module_instances,
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
        validate_execution_lanes(&self.execution_lanes, &self.module_instances)?;
        resolve_parts(&self.module_instances, &self.capability_bindings).map(
            |(module_instances, capability_bindings)| ResolvedAppPlan {
                schema_version: PLAN_SCHEMA_VERSION,
                module_instances,
                capability_bindings,
                execution_lanes: sorted_execution_lanes(&self.execution_lanes),
            },
        )
    }

    /// Returns the authoring Module Instances.
    pub fn module_instances(&self) -> &[ModuleInstancePlan] {
        &self.module_instances
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

/// A reason App Composition could not be materialized into a Plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanResolutionError {
    /// The Plan schema cannot be executed by this Kernel version.
    UnsupportedSchemaVersion { expected: u32, actual: u32 },
    /// Two Module Instances use the same App-local key.
    DuplicateModuleInstance { instance_key: String },
    /// Every App must declare at least one Execution Lane.
    MissingExecutionLane,
    /// An Execution Lane identity is empty or whitespace-only.
    InvalidExecutionLane { execution_lane: String },
    /// Two Execution Lanes use the same App-local identity.
    DuplicateExecutionLane { execution_lane: String },
    /// A Module Instance names an Execution Lane absent from the Plan.
    UndeclaredExecutionLane {
        instance_key: String,
        execution_lane: String,
    },
    /// A Module Instance has no executable entrypoint identity.
    InvalidModuleEntrypoint { instance_key: String },
    /// A Module declares the same provided Capability more than once.
    DuplicateProvidedCapability {
        provider_instance: String,
        capability_id: String,
    },
    /// One endpoint declares an Operation more than once.
    DuplicateOperation {
        provider_instance: String,
        capability_id: String,
        operation: String,
    },
    /// A Module declares the same required Capability more than once.
    DuplicateRequiredCapability {
        consumer_instance: String,
        capability_id: String,
    },
    /// A binding names a consumer that is not in the Composition.
    InvalidConsumerReference {
        consumer_instance: String,
        capability_id: String,
    },
    /// A binding names a provider that is not in the Composition or does not provide the Capability.
    InvalidProviderReference {
        consumer_instance: String,
        capability_id: String,
        provider_instance: String,
    },
    /// A binding exists without a matching consumer requirement.
    UndeclaredCapabilityRequirement {
        consumer_instance: String,
        capability_id: String,
    },
    /// The provider and consumer selected different exact Descriptor versions.
    IncompatibleCapabilityVersion {
        consumer_instance: String,
        capability_id: String,
        required: String,
        provided: String,
        provider_instance: String,
    },
    /// A binding crosses Execution Lanes but its generated contract types are not transferable.
    CrossLaneTransferUnsupported {
        consumer_instance: String,
        provider_instance: String,
        capability_id: String,
    },
    /// This Plan version has no native cross-lane transfer for the selected interaction kind.
    CrossLaneInteractionUnsupported {
        capability_id: String,
        operation: String,
        interaction: CapabilityOperationKind,
    },
    /// A `one` requirement has no explicit provider.
    MissingOneBinding {
        consumer_instance: String,
        capability_id: String,
    },
    /// A `one` requirement has more than one explicit provider.
    AmbiguousOneBinding {
        consumer_instance: String,
        capability_id: String,
        providers: usize,
    },
    /// An `optional` requirement has more than one explicit provider.
    AmbiguousOptionalBinding {
        consumer_instance: String,
        capability_id: String,
        providers: usize,
    },
    /// A provider is repeated for the same requirement.
    DuplicateBinding {
        consumer_instance: String,
        capability_id: String,
        provider_instance: String,
    },
    /// A request Operation has an invalid bounded admission policy.
    InvalidRequestAdmission {
        capability_id: String,
        operation: String,
        queue_capacity: usize,
        max_concurrency: usize,
    },
    /// Admission was configured for an Operation absent from the endpoint.
    UnknownAdmissionOperation {
        capability_id: String,
        operation: String,
    },
    /// Interaction metadata was configured for an absent Operation.
    UnknownOperationInteraction {
        capability_id: String,
        operation: String,
    },
    /// A Module Instance selected an unusable finite restart policy.
    InvalidRestartPolicy {
        instance_key: String,
        max_attempts: usize,
        window: Duration,
    },
    /// Explicit Capability activation dependencies contain a cycle.
    ActivationCycle { instances: Vec<String> },
}

impl fmt::Display for PlanResolutionError {
    #[allow(clippy::too_many_lines)]
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchemaVersion { expected, actual } => write!(
                formatter,
                "unsupported Plan schema version {actual}; expected {expected}"
            ),
            Self::DuplicateModuleInstance { instance_key } => {
                write!(formatter, "duplicate Module Instance `{instance_key}`")
            }
            Self::MissingExecutionLane => {
                formatter.write_str("Resolved App Plan declares no Execution Lanes")
            }
            Self::InvalidExecutionLane { execution_lane } => {
                write!(formatter, "invalid Execution Lane `{execution_lane}`")
            }
            Self::DuplicateExecutionLane { execution_lane } => {
                write!(formatter, "duplicate Execution Lane `{execution_lane}`")
            }
            Self::UndeclaredExecutionLane {
                instance_key,
                execution_lane,
            } => write!(
                formatter,
                "Module Instance `{instance_key}` is placed on undeclared Execution Lane `{execution_lane}`"
            ),
            Self::InvalidModuleEntrypoint { instance_key } => write!(
                formatter,
                "Module Instance `{instance_key}` has an empty entrypoint"
            ),
            Self::DuplicateProvidedCapability {
                provider_instance,
                capability_id,
            } => write!(
                formatter,
                "Module Instance `{provider_instance}` provides Capability `{capability_id}` more than once"
            ),
            Self::DuplicateOperation {
                provider_instance,
                capability_id,
                operation,
            } => write!(
                formatter,
                "Module Instance `{provider_instance}` Capability `{capability_id}` declares Operation `{operation}` more than once"
            ),
            Self::DuplicateRequiredCapability {
                consumer_instance,
                capability_id,
            } => write!(
                formatter,
                "Module Instance `{consumer_instance}` requires Capability `{capability_id}` more than once"
            ),
            Self::InvalidConsumerReference {
                consumer_instance,
                capability_id,
            } => write!(
                formatter,
                "Capability `{capability_id}` names missing consumer `{consumer_instance}`"
            ),
            Self::InvalidProviderReference {
                consumer_instance,
                capability_id,
                provider_instance,
            } => write!(
                formatter,
                "consumer `{consumer_instance}` names invalid provider `{provider_instance}` for Capability `{capability_id}`"
            ),
            Self::UndeclaredCapabilityRequirement {
                consumer_instance,
                capability_id,
            } => write!(
                formatter,
                "consumer `{consumer_instance}` has no declared requirement for Capability `{capability_id}`"
            ),
            Self::IncompatibleCapabilityVersion {
                consumer_instance,
                capability_id,
                required,
                provided,
                provider_instance,
            } => write!(
                formatter,
                "consumer `{consumer_instance}` requires Capability `{capability_id}` version `{required}`, but provider `{provider_instance}` provides `{provided}`"
            ),
            Self::CrossLaneTransferUnsupported {
                consumer_instance,
                provider_instance,
                capability_id,
            } => write!(
                formatter,
                "consumer `{consumer_instance}` binds Capability `{capability_id}` across Execution Lanes to provider `{provider_instance}`, but its contract types do not support cross-lane transfer"
            ),
            Self::CrossLaneInteractionUnsupported {
                capability_id,
                operation,
                interaction,
            } => write!(
                formatter,
                "Capability `{capability_id}` Operation `{operation}` uses {interaction:?}, which this Plan version cannot transfer across Execution Lanes"
            ),
            Self::MissingOneBinding {
                consumer_instance,
                capability_id,
            } => write!(
                formatter,
                "consumer `{consumer_instance}` is missing one binding for Capability `{capability_id}`"
            ),
            Self::AmbiguousOneBinding {
                consumer_instance,
                capability_id,
                providers,
            } => write!(
                formatter,
                "consumer `{consumer_instance}` has {providers} bindings for one Capability `{capability_id}`"
            ),
            Self::AmbiguousOptionalBinding {
                consumer_instance,
                capability_id,
                providers,
            } => write!(
                formatter,
                "consumer `{consumer_instance}` has {providers} bindings for optional Capability `{capability_id}`"
            ),
            Self::DuplicateBinding {
                consumer_instance,
                capability_id,
                provider_instance,
            } => write!(
                formatter,
                "consumer `{consumer_instance}` binds Capability `{capability_id}` to provider `{provider_instance}` more than once"
            ),
            Self::InvalidRequestAdmission {
                capability_id,
                operation,
                queue_capacity,
                max_concurrency,
            } => write!(
                formatter,
                "Capability `{capability_id}` Operation `{operation}` has invalid request admission (queue capacity {queue_capacity}, concurrency {max_concurrency})"
            ),
            Self::UnknownAdmissionOperation {
                capability_id,
                operation,
            } => write!(
                formatter,
                "Capability `{capability_id}` configures request admission for unknown Operation `{operation}`"
            ),
            Self::UnknownOperationInteraction {
                capability_id,
                operation,
            } => write!(
                formatter,
                "Capability `{capability_id}` configures interaction metadata for unknown Operation `{operation}`"
            ),
            Self::InvalidRestartPolicy {
                instance_key,
                max_attempts,
                window,
            } => write!(
                formatter,
                "Module Instance `{instance_key}` has invalid restart policy (attempts {max_attempts}, window {window:?})"
            ),
            Self::ActivationCycle { instances } => write!(
                formatter,
                "required Capability activation cycle: {}",
                instances.join(" -> ")
            ),
        }
    }
}

impl std::error::Error for PlanResolutionError {}

/// Exact, immutable execution input supplied to the Kernel.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResolvedAppPlan {
    schema_version: u32,
    module_instances: Vec<ModuleInstancePlan>,
    capability_bindings: Vec<CapabilityBinding>,
    #[serde(default = "default_execution_lanes")]
    execution_lanes: Vec<ExecutionLanePlan>,
}

impl ResolvedAppPlan {
    /// Creates a valid Plan containing no Module Instances.
    pub fn empty() -> Self {
        Self {
            schema_version: PLAN_SCHEMA_VERSION,
            module_instances: Vec::new(),
            capability_bindings: Vec::new(),
            execution_lanes: default_execution_lanes(),
        }
    }

    /// Creates a Plan with exact entries, retaining invalid entries for later validation.
    pub fn new(
        mut module_instances: Vec<ModuleInstancePlan>,
        mut capability_bindings: Vec<CapabilityBinding>,
    ) -> Self {
        sort_module_instances(&mut module_instances);
        sort_bindings(&mut capability_bindings);
        Self {
            schema_version: PLAN_SCHEMA_VERSION,
            module_instances,
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
            module_instances: Vec::new(),
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
        validate_execution_lanes(&self.execution_lanes, &self.module_instances)?;
        resolve_parts(&self.module_instances, &self.capability_bindings).map(|_| ())
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
        validate_execution_lanes(&self.execution_lanes, &self.module_instances)?;
        let (instances, bindings) =
            resolve_parts(&self.module_instances, &self.capability_bindings)?;
        activation_order_for(&instances, &bindings)
            .map_err(|instances| PlanResolutionError::ActivationCycle { instances })
    }

    /// Returns the Plan schema version.
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Returns the exact Module Instances in deterministic Plan order.
    pub fn module_instances(&self) -> &[ModuleInstancePlan] {
        &self.module_instances
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

        self.module_instances
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

        self.module_instances
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

    /// Returns the exact Module Instance selected by its App-local key.
    pub fn module_instance(&self, instance_key: &str) -> Option<&ModuleInstancePlan> {
        self.module_instances
            .iter()
            .find(|instance| instance.instance_key() == instance_key)
    }

    /// Returns the restart policy materialized for one Module Instance.
    pub fn restart_policy_for(&self, instance_key: &str) -> Option<RestartPolicy> {
        self.module_instance(instance_key)
            .map(ModuleInstancePlan::restart_policy)
    }

    /// Returns the criticality materialized for one Module Instance.
    pub fn criticality_for(&self, instance_key: &str) -> Option<ModuleCriticality> {
        self.module_instance(instance_key)
            .map(ModuleInstancePlan::criticality)
    }

    /// Returns whether a Module Instance is directly bound to a required `one` Capability path.
    pub fn module_instance_is_required(&self, instance_key: &str) -> bool {
        self.capability_bindings.iter().any(|binding| {
            binding.provider_instance() == instance_key
                && self
                    .module_instance(binding.consumer_instance())
                    .is_some_and(|consumer| {
                        consumer.required_capabilities().iter().any(|requirement| {
                            requirement.capability_id() == binding.capability_id()
                                && requirement.cardinality() == CapabilityCardinality::One
                        })
                    })
        })
    }
}
