use std::{fmt, time::Duration};

use super::CapabilityOperationKind;

/// A reason App Composition could not be materialized into a Plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanResolutionError {
    /// This Kernel does not yet execute the selected terminal policy.
    UnsupportedTerminalPolicy,
    /// Host-essential roots or their materialized closure are invalid.
    InvalidTerminalPolicy { detail: String },
    /// Contract version or selected runtime profile is invalid.
    InvalidAuthoring { instance_key: String },
    /// A requirement identity is invalid for the selected authoring version.
    InvalidRequirementId {
        consumer_instance: String,
        requirement_id: String,
    },
    /// Two declarations share one consumer-local identity.
    DuplicateRequirementId {
        consumer_instance: String,
        requirement_id: String,
    },
    /// The Plan schema cannot be executed by this Kernel version.
    UnsupportedSchemaVersion { expected: u32, actual: u32 },
    /// Two Plugin Instances use the same App-local key.
    DuplicatePluginInstance { instance_key: String },
    /// Every App must declare at least one Execution Lane.
    MissingExecutionLane,
    /// An Execution Lane identity is empty or whitespace-only.
    InvalidExecutionLane { execution_lane: String },
    /// Two Execution Lanes use the same App-local identity.
    DuplicateExecutionLane { execution_lane: String },
    /// A Plugin Instance names an Execution Lane absent from the Plan.
    UndeclaredExecutionLane {
        instance_key: String,
        execution_lane: String,
    },
    /// A Plugin Instance has no executable entrypoint identity.
    InvalidPluginEntrypoint { instance_key: String },
    /// A Plugin declares the same provided Capability more than once.
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
    /// A Plugin declares the same required Capability more than once.
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
    /// A host Runtime does not implement cross-lane transfer for the selected interaction kind.
    ///
    /// Retained so downstream code can classify failures produced by older Runtime versions.
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
    /// A Plugin Instance selected an unusable finite restart policy.
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
            Self::UnsupportedTerminalPolicy => {
                formatter.write_str("unsupported host_essential terminal policy")
            }
            Self::InvalidTerminalPolicy { detail } => {
                write!(formatter, "invalid terminal policy: {detail}")
            }
            Self::InvalidAuthoring { instance_key } => write!(
                formatter,
                "invalid authoring version or runtime profile for `{instance_key}`"
            ),
            Self::InvalidRequirementId {
                consumer_instance,
                requirement_id,
            } => write!(
                formatter,
                "invalid requirement `{requirement_id}` for `{consumer_instance}`"
            ),
            Self::DuplicateRequirementId {
                consumer_instance,
                requirement_id,
            } => write!(
                formatter,
                "duplicate requirement `{requirement_id}` for `{consumer_instance}`"
            ),
            Self::UnsupportedSchemaVersion { expected, actual } => write!(
                formatter,
                "unsupported Plan schema version {actual}; expected {expected}"
            ),
            Self::DuplicatePluginInstance { instance_key } => {
                write!(formatter, "duplicate Plugin Instance `{instance_key}`")
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
                "Plugin Instance `{instance_key}` is placed on undeclared Execution Lane `{execution_lane}`"
            ),
            Self::InvalidPluginEntrypoint { instance_key } => write!(
                formatter,
                "Plugin Instance `{instance_key}` has an empty entrypoint"
            ),
            Self::DuplicateProvidedCapability {
                provider_instance,
                capability_id,
            } => write!(
                formatter,
                "Plugin Instance `{provider_instance}` provides Capability `{capability_id}` more than once"
            ),
            Self::DuplicateOperation {
                provider_instance,
                capability_id,
                operation,
            } => write!(
                formatter,
                "Plugin Instance `{provider_instance}` Capability `{capability_id}` declares Operation `{operation}` more than once"
            ),
            Self::DuplicateRequiredCapability {
                consumer_instance,
                capability_id,
            } => write!(
                formatter,
                "Plugin Instance `{consumer_instance}` requires Capability `{capability_id}` more than once"
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
                "Plugin Instance `{instance_key}` has invalid restart policy (attempts {max_attempts}, window {window:?})"
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
