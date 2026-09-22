//! Runtime-neutral Authoring V2 values and validation.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use super::{InvocationExtension, ProtocolError, VALUE_PROFILE};

/// First interoperable Authoring value API.
pub const AUTHORING_API_VERSION: u32 = 2;
/// Default maximum encoded frame size.
pub const DEFAULT_MAX_FRAME_BYTES: u64 = 1_048_576;

/// Decodes one bounded strict Authoring value.
pub fn decode_frame<T: DeserializeOwned>(
    wire: &[u8],
    max_frame_bytes: u64,
) -> Result<T, ProtocolError> {
    if u64::try_from(wire.len()).map_or(true, |length| length > max_frame_bytes) {
        return Err(error("Authoring frame exceeds max_frame_bytes"));
    }
    super::decode_strict(wire)
}

/// Immutable identity of one admitted Plugin generation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionIdentity {
    pub session: String,
    pub plugin_instance: String,
    pub plugin_generation: String,
    pub artifact_digest: String,
    pub contract_digest: String,
    pub runtime_profile: String,
    pub value_profile: String,
}

/// Host bounds confirmed during initialization.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringLimits {
    pub max_frame_bytes: u64,
    pub max_active_invocations: u32,
    pub max_active_outbound_calls: u32,
    pub max_queued_calls: u32,
    pub max_unfinished_executions: u32,
    pub max_retired_ids: u32,
}

impl AuthoringLimits {
    #[must_use]
    pub const fn defaults() -> Self {
        Self {
            max_frame_bytes: DEFAULT_MAX_FRAME_BYTES,
            max_active_invocations: 32,
            max_active_outbound_calls: 32,
            max_queued_calls: 32,
            max_unfinished_executions: 32,
            max_retired_ids: 1_024,
        }
    }
}

/// Cardinality of one named requirement.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequirementCardinality {
    One,
    Optional,
    Many,
}

/// One source-declared dependency, keyed independently from its Capability.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementDeclaration {
    pub requirement_id: String,
    pub capability_id: String,
    pub descriptor_version: String,
    pub descriptor_digest: String,
    pub cardinality: RequirementCardinality,
}

/// One exact provider selected for a named requirement.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteDescriptor {
    pub route_id: String,
    pub requirement_id: String,
    pub capability_id: String,
    pub descriptor_version: String,
    pub descriptor_digest: String,
    pub provider_instance: String,
    pub provider_order: u32,
}

/// One endpoint exported by the admitted Plugin instance.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvidedEndpoint {
    pub endpoint_id: String,
    pub capability_id: String,
    pub descriptor_version: String,
    pub descriptor_digest: String,
}

/// Host-to-runtime initialization value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitializeParams {
    pub api_version: u32,
    pub identity: SessionIdentity,
    pub config: Value,
    pub required_declarations: Vec<RequirementDeclaration>,
    pub routes: Vec<RouteDescriptor>,
    pub provided_endpoints: Vec<ProvidedEndpoint>,
    pub limits: AuthoringLimits,
}

/// Runtime echo proving that it admitted exactly the Host initialization.
pub type InitializedResult = InitializeParams;

/// Host-to-runtime factory invocation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructParams {
    pub session: String,
    pub lifecycle_scope_id: String,
    pub remaining_budget_nanos: String,
}

/// Result of transferring the constructed object to the runtime.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FactoryOutcome {
    Constructed,
    Failed { detail: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructedResult {
    pub session: String,
    pub lifecycle_scope_id: String,
    pub outcome: FactoryOutcome,
}

/// Invocation ancestry and ambient values that outbound calls must preserve.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvocationScope {
    pub scope_id: String,
    pub parent_scope_id: Option<String>,
    pub remaining_budget_nanos: String,
    pub permissions: Vec<String>,
    pub extensions: Vec<InvocationExtension>,
}

/// Host-to-plugin operation invocation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvokeParams {
    pub session: String,
    pub correlation_id: String,
    pub endpoint_id: String,
    pub capability_id: String,
    pub descriptor_version: String,
    pub descriptor_digest: String,
    pub operation: String,
    pub scope: InvocationScope,
    pub payload: Value,
}

/// Portable operation outcome. It is distinct from execution settlement.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuntimeFailure {
    Unavailable {
        capability: String,
    },
    UnknownOperation {
        capability: String,
        operation: String,
    },
    AmbiguousBinding {
        capability: String,
        providers: u32,
    },
    ProtocolViolation {
        capability: String,
    },
    MissingPluginFactory {
        instance: String,
        package_id: String,
    },
    UnavailableExecutionClass {
        instance_key: String,
        execution_class: String,
    },
    InvalidResolvedPlan {
        detail: String,
    },
    AdmissionClosed,
    ResourceExhausted {
        capability: String,
        operation: String,
    },
    DeadlineExceeded {
        request_id: String,
    },
    Cancelled {
        request_id: String,
    },
    Internal {
        detail: String,
    },
    PluginFailure {
        detail: String,
    },
    PluginRestartExhausted {
        instance: String,
        attempts: u32,
    },
}

impl RuntimeFailure {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Self::Unavailable { capability } | Self::ProtocolViolation { capability } => {
                validate_token("capability", capability)
            }
            Self::UnknownOperation {
                capability,
                operation,
            }
            | Self::ResourceExhausted {
                capability,
                operation,
            } => {
                validate_token("capability", capability)?;
                validate_token("operation", operation)
            }
            Self::AmbiguousBinding {
                capability,
                providers,
            } => {
                validate_token("capability", capability)?;
                validate_limit("providers", u64::from(*providers), 1_048_576)
            }
            Self::MissingPluginFactory {
                instance,
                package_id,
            } => {
                validate_token("instance", instance)?;
                validate_token("package_id", package_id)
            }
            Self::UnavailableExecutionClass {
                instance_key,
                execution_class,
            } => {
                validate_token("instance_key", instance_key)?;
                validate_token("execution_class", execution_class)
            }
            Self::InvalidResolvedPlan { detail }
            | Self::Internal { detail }
            | Self::PluginFailure { detail } => validate_detail(detail),
            Self::AdmissionClosed => Ok(()),
            Self::DeadlineExceeded { request_id } | Self::Cancelled { request_id } => {
                validate_decimal("request_id", request_id).map(|_| ())
            }
            Self::PluginRestartExhausted { instance, attempts } => {
                validate_token("instance", instance)?;
                validate_limit("attempts", u64::from(*attempts), 1_048_576)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InvocationOutcome {
    Success { value: Value },
    Domain { error: Value },
    Runtime { failure: RuntimeFailure },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvocationResult {
    pub session: String,
    pub correlation_id: String,
    pub outcome: InvocationOutcome,
}

/// Plugin-to-Host call through one exact Plan route.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutboundCallParams {
    pub session: String,
    pub correlation_id: String,
    pub requirement_id: String,
    pub route_id: String,
    pub operation: String,
    pub scope: InvocationScope,
    pub payload: Value,
}

pub type OutboundCallResult = InvocationResult;

/// Host-to-plugin publication for one provided Event operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventPublishParams {
    pub session: String,
    pub correlation_id: String,
    pub endpoint_id: String,
    pub capability_id: String,
    pub descriptor_version: String,
    pub descriptor_digest: String,
    pub operation: String,
    pub scope: InvocationScope,
    pub event: Value,
}

/// Plugin-to-Host Event publication through one exact Plan route.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutboundEventPublishParams {
    pub session: String,
    pub correlation_id: String,
    pub requirement_id: String,
    pub route_id: String,
    pub operation: String,
    pub scope: InvocationScope,
    pub event: Value,
}

/// Commit acknowledgement for one bounded Event admission.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EventPublishOutcome {
    Accepted,
    Runtime { failure: RuntimeFailure },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventPublishResult {
    pub session: String,
    pub correlation_id: String,
    pub outcome: EventPublishOutcome,
}

pub type OutboundEventPublishResult = EventPublishResult;

/// Host-to-plugin open request for one provided bidirectional Stream operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamOpenParams {
    pub session: String,
    pub correlation_id: String,
    pub endpoint_id: String,
    pub capability_id: String,
    pub descriptor_version: String,
    pub descriptor_digest: String,
    pub operation: String,
    pub scope: InvocationScope,
    pub request: Value,
}

/// Plugin-to-Host Stream open through one exact Plan route.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutboundStreamOpenParams {
    pub session: String,
    pub correlation_id: String,
    pub requirement_id: String,
    pub route_id: String,
    pub operation: String,
    pub scope: InvocationScope,
    pub request: Value,
}

/// Result of opening a Stream. The receiver owns the returned stream identity.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StreamOpenOutcome {
    Opened { stream_id: String },
    Domain { error: Value },
    Runtime { failure: RuntimeFailure },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamOpenResult {
    pub session: String,
    pub correlation_id: String,
    pub outcome: StreamOpenOutcome,
}

pub type OutboundStreamOpenResult = StreamOpenResult;

/// One ordered message submitted to an open Stream session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamSendParams {
    pub session: String,
    pub correlation_id: String,
    pub stream_id: String,
    pub sequence: String,
    pub message: Value,
}

/// One pull for the next observable item from an open Stream session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamReceiveParams {
    pub session: String,
    pub correlation_id: String,
    pub stream_id: String,
}

/// Half-closes the sender without closing its receive direction.
pub type StreamCloseSendParams = StreamReceiveParams;

/// Cancels a Stream session idempotently.
pub type StreamCancelParams = StreamReceiveParams;

/// Result of a Stream mutation after it was admitted by the receiver.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StreamActionOutcome {
    Accepted,
    Runtime { failure: RuntimeFailure },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamActionResult {
    pub session: String,
    pub correlation_id: String,
    pub stream_id: String,
    pub outcome: StreamActionOutcome,
}

/// Terminal result belongs to the Stream operation, not to one receive call.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StreamTerminalOutcome {
    Success,
    Domain { error: Value },
}

/// Next ordered item observed from a Stream session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StreamReceiveOutcome {
    Message { sequence: String, message: Value },
    PeerHalfClosed,
    Terminal { outcome: StreamTerminalOutcome },
    Runtime { failure: RuntimeFailure },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamReceiveResult {
    pub session: String,
    pub correlation_id: String,
    pub stream_id: String,
    pub outcome: StreamReceiveOutcome,
}

/// Cancellation request. Acknowledgement does not imply execution settlement.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelParams {
    pub session: String,
    pub scope_id: String,
    pub correlation_id: String,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelAck {
    pub session: String,
    pub scope_id: String,
    pub correlation_id: String,
    pub accepted: bool,
}

/// Runtime observation that one execution has actually stopped.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settlement {
    pub session: String,
    pub scope_id: String,
    pub correlation_id: String,
    pub state: SettlementState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementState {
    Completed,
    Cancelled,
    Abandoned,
}

/// Fresh cleanup scope with the remaining shared cleanup budget.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopParams {
    pub session: String,
    pub cleanup_scope_id: String,
    pub remaining_budget_nanos: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanupDiagnostic {
    pub code: String,
    pub detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoppedResult {
    pub session: String,
    pub cleanup_scope_id: String,
    pub hook: StopHookOutcome,
    pub diagnostics: Vec<CleanupDiagnostic>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopHookOutcome {
    NotDeclared,
    Completed,
    Failed,
}

impl SessionIdentity {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_token("session", &self.session)?;
        validate_token("plugin_instance", &self.plugin_instance)?;
        validate_decimal("plugin_generation", &self.plugin_generation)?;
        validate_digest("artifact_digest", &self.artifact_digest)?;
        validate_digest("contract_digest", &self.contract_digest)?;
        validate_token("runtime_profile", &self.runtime_profile)?;
        if self.value_profile != VALUE_PROFILE {
            return Err(error("unsupported portable value profile"));
        }
        Ok(())
    }
}

impl AuthoringLimits {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_limit(
            "max_frame_bytes",
            self.max_frame_bytes,
            DEFAULT_MAX_FRAME_BYTES,
        )?;
        validate_limit(
            "max_active_invocations",
            u64::from(self.max_active_invocations),
            1_024,
        )?;
        validate_limit(
            "max_active_outbound_calls",
            u64::from(self.max_active_outbound_calls),
            1_024,
        )?;
        validate_limit("max_queued_calls", u64::from(self.max_queued_calls), 65_536)?;
        validate_limit(
            "max_unfinished_executions",
            u64::from(self.max_unfinished_executions),
            65_536,
        )?;
        validate_limit(
            "max_retired_ids",
            u64::from(self.max_retired_ids),
            1_048_576,
        )
    }
}

impl InitializeParams {
    /// Validates initialization and exact requirement-to-route bindings.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.api_version != AUTHORING_API_VERSION {
            return Err(error("unsupported Authoring API version"));
        }
        self.identity.validate()?;
        self.limits.validate()?;
        validate_sorted(
            &self.required_declarations,
            |value| &value.requirement_id,
            "required_declarations",
        )?;
        validate_sorted(
            &self.provided_endpoints,
            |value| &value.endpoint_id,
            "provided_endpoints",
        )?;

        let mut declarations = BTreeMap::new();
        for declaration in &self.required_declarations {
            declaration.validate()?;
            declarations.insert(declaration.requirement_id.as_str(), declaration);
        }
        let mut counts = BTreeMap::<&str, usize>::new();
        let mut next_orders = BTreeMap::<&str, u32>::new();
        let mut route_ids = BTreeSet::new();
        let mut providers = BTreeSet::new();
        let mut previous_route: Option<(&str, u32, &str)> = None;
        for route in &self.routes {
            route.validate()?;
            if !route_ids.insert(route.route_id.as_str()) {
                return Err(error("route_id must be unique within a session"));
            }
            let sort_key = (
                route.requirement_id.as_str(),
                route.provider_order,
                route.provider_instance.as_str(),
            );
            if previous_route.is_some_and(|previous| previous >= sort_key) {
                return Err(error(
                    "routes must be sorted by requirement_id, provider_order and provider_instance",
                ));
            }
            previous_route = Some(sort_key);
            let declaration = declarations
                .get(route.requirement_id.as_str())
                .ok_or_else(|| error("route references an unknown requirement_id"))?;
            if route.capability_id != declaration.capability_id
                || route.descriptor_version != declaration.descriptor_version
                || route.descriptor_digest != declaration.descriptor_digest
            {
                return Err(error(
                    "route descriptor does not match its named requirement",
                ));
            }
            let expected = next_orders
                .entry(route.requirement_id.as_str())
                .or_default();
            if route.provider_order != *expected {
                return Err(error(
                    "requirement routes must use contiguous provider_order values",
                ));
            }
            if !providers.insert((
                route.requirement_id.as_str(),
                route.provider_instance.as_str(),
            )) {
                return Err(error("one requirement cannot bind the same provider twice"));
            }
            *expected += 1;
            *counts.entry(route.requirement_id.as_str()).or_default() += 1;
        }
        for declaration in &self.required_declarations {
            let count = counts
                .get(declaration.requirement_id.as_str())
                .copied()
                .unwrap_or_default();
            match declaration.cardinality {
                RequirementCardinality::One if count != 1 => {
                    return Err(error("one requirement must bind exactly one route"));
                }
                RequirementCardinality::Optional if count > 1 => {
                    return Err(error("optional requirement may bind at most one route"));
                }
                RequirementCardinality::Many
                | RequirementCardinality::One
                | RequirementCardinality::Optional => {}
            }
        }
        for endpoint in &self.provided_endpoints {
            endpoint.validate()?;
        }
        Ok(())
    }

    /// Validates initialization for one exact Adapter-selected runtime profile.
    pub fn validate_for_runtime_profile(&self, expected: &str) -> Result<(), ProtocolError> {
        self.validate()?;
        if self.identity.runtime_profile != expected {
            return Err(error(
                "runtime profile does not match the selected Adapter profile",
            ));
        }
        Ok(())
    }

    /// Requires an exact initialized echo; partial admission is forbidden.
    pub fn validate_initialized(
        &self,
        initialized: &InitializedResult,
    ) -> Result<(), ProtocolError> {
        initialized.validate()?;
        if initialized != self {
            return Err(error(
                "initialized result does not exactly echo initialization",
            ));
        }
        Ok(())
    }
}

impl RequirementDeclaration {
    fn validate(&self) -> Result<(), ProtocolError> {
        validate_requirement_id(&self.requirement_id)?;
        validate_descriptor(
            &self.capability_id,
            &self.descriptor_version,
            &self.descriptor_digest,
        )
    }
}

impl RouteDescriptor {
    fn validate(&self) -> Result<(), ProtocolError> {
        validate_token("route_id", &self.route_id)?;
        validate_requirement_id(&self.requirement_id)?;
        validate_token("provider_instance", &self.provider_instance)?;
        validate_descriptor(
            &self.capability_id,
            &self.descriptor_version,
            &self.descriptor_digest,
        )
    }
}

impl ProvidedEndpoint {
    fn validate(&self) -> Result<(), ProtocolError> {
        validate_token("endpoint_id", &self.endpoint_id)?;
        validate_descriptor(
            &self.capability_id,
            &self.descriptor_version,
            &self.descriptor_digest,
        )
    }
}

impl InvocationScope {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_token("scope_id", &self.scope_id)?;
        if let Some(parent) = &self.parent_scope_id {
            validate_token("parent_scope_id", parent)?;
        }
        validate_decimal("remaining_budget_nanos", &self.remaining_budget_nanos)?;
        validate_sorted_strings(&self.permissions, "permissions")?;
        let mut keys = BTreeSet::new();
        for extension in &self.extensions {
            extension.validate()?;
            if !keys.insert(extension.key.as_str()) {
                return Err(error("extensions must be sorted and unique"));
            }
        }
        if self
            .extensions
            .windows(2)
            .any(|pair| pair[0].key >= pair[1].key)
        {
            return Err(error("extensions must be sorted and unique"));
        }
        Ok(())
    }

    /// Validates an outbound child scope against its parent invocation.
    pub fn validate_child_of(&self, parent: &Self) -> Result<(), ProtocolError> {
        self.validate()?;
        parent.validate()?;
        if self.parent_scope_id.as_deref() != Some(parent.scope_id.as_str()) {
            return Err(error("outbound scope must name its exact parent scope"));
        }
        if decimal_value(&self.remaining_budget_nanos)?
            > decimal_value(&parent.remaining_budget_nanos)?
        {
            return Err(error("outbound remaining budget may not increase"));
        }
        if self.permissions != parent.permissions || self.extensions != parent.extensions {
            return Err(error(
                "outbound scope must preserve permissions and extensions",
            ));
        }
        Ok(())
    }
}

impl ConstructParams {
    pub fn validate_for(&self, identity: &SessionIdentity) -> Result<(), ProtocolError> {
        validate_session(&self.session, identity)?;
        validate_token("lifecycle_scope_id", &self.lifecycle_scope_id)?;
        validate_decimal("remaining_budget_nanos", &self.remaining_budget_nanos).map(|_| ())
    }
}
impl ConstructedResult {
    pub fn validate_for(&self, request: &ConstructParams) -> Result<(), ProtocolError> {
        validate_echo(
            &self.session,
            &self.lifecycle_scope_id,
            &request.session,
            &request.lifecycle_scope_id,
        )?;
        if let FactoryOutcome::Failed { detail } = &self.outcome {
            validate_detail(detail)?;
        }
        Ok(())
    }
}
impl InvokeParams {
    pub fn validate_for(&self, identity: &SessionIdentity) -> Result<(), ProtocolError> {
        validate_session(&self.session, identity)?;
        validate_decimal("correlation_id", &self.correlation_id)?;
        validate_token("endpoint_id", &self.endpoint_id)?;
        validate_descriptor(
            &self.capability_id,
            &self.descriptor_version,
            &self.descriptor_digest,
        )?;
        validate_token("operation", &self.operation)?;
        self.scope.validate()
    }
}
impl InvokeParams {
    /// Validates the endpoint identity against the admitted initialization.
    pub fn validate_against(&self, initialize: &InitializeParams) -> Result<(), ProtocolError> {
        self.validate_for(&initialize.identity)?;
        let endpoint = initialize
            .provided_endpoints
            .iter()
            .find(|endpoint| endpoint.endpoint_id == self.endpoint_id)
            .ok_or_else(|| error("invoke references an unknown endpoint_id"))?;
        if endpoint.capability_id != self.capability_id
            || endpoint.descriptor_version != self.descriptor_version
            || endpoint.descriptor_digest != self.descriptor_digest
        {
            return Err(error(
                "invoke descriptor does not match its admitted endpoint",
            ));
        }
        Ok(())
    }
}
impl InvocationResult {
    pub fn validate_for(&self, request: &InvokeParams) -> Result<(), ProtocolError> {
        validate_echo(
            &self.session,
            &self.correlation_id,
            &request.session,
            &request.correlation_id,
        )?;
        if let InvocationOutcome::Runtime { failure } = &self.outcome {
            failure.validate()?;
        }
        Ok(())
    }

    /// Validates an outbound-call result against its exact request identity.
    pub fn validate_for_outbound(&self, request: &OutboundCallParams) -> Result<(), ProtocolError> {
        validate_echo(
            &self.session,
            &self.correlation_id,
            &request.session,
            &request.correlation_id,
        )?;
        if let InvocationOutcome::Runtime { failure } = &self.outcome {
            failure.validate()?;
        }
        Ok(())
    }
}
impl OutboundCallParams {
    pub fn validate_for(
        &self,
        identity: &SessionIdentity,
        parent: &InvocationScope,
    ) -> Result<(), ProtocolError> {
        validate_session(&self.session, identity)?;
        validate_decimal("correlation_id", &self.correlation_id)?;
        validate_requirement_id(&self.requirement_id)?;
        validate_token("route_id", &self.route_id)?;
        validate_token("operation", &self.operation)?;
        self.scope.validate_child_of(parent)
    }
}
impl OutboundCallParams {
    /// Validates the opaque route against this exact session and named requirement.
    pub fn validate_against(
        &self,
        initialize: &InitializeParams,
        parent: &InvocationScope,
        parent_active: bool,
    ) -> Result<(), ProtocolError> {
        self.validate_for(&initialize.identity, parent)?;
        if !parent_active {
            return Err(error("closed parent scope cannot start an outbound call"));
        }
        let route = initialize
            .routes
            .iter()
            .find(|route| route.route_id == self.route_id)
            .ok_or_else(|| error("outbound call references an unknown route_id"))?;
        if route.requirement_id != self.requirement_id {
            return Err(error("outbound route belongs to another requirement"));
        }
        Ok(())
    }
}
impl EventPublishParams {
    pub fn validate_against(&self, initialize: &InitializeParams) -> Result<(), ProtocolError> {
        validate_provided_operation(
            &self.session,
            &self.correlation_id,
            &self.endpoint_id,
            &self.capability_id,
            &self.descriptor_version,
            &self.descriptor_digest,
            &self.operation,
            &self.scope,
            initialize,
            "event publication",
        )
    }
}
impl OutboundEventPublishParams {
    pub fn validate_against(
        &self,
        initialize: &InitializeParams,
        parent: &InvocationScope,
        parent_active: bool,
    ) -> Result<(), ProtocolError> {
        validate_outbound_operation(
            &self.session,
            &self.correlation_id,
            &self.requirement_id,
            &self.route_id,
            &self.operation,
            &self.scope,
            initialize,
            parent,
            parent_active,
            "event publication",
        )
    }
}
impl EventPublishResult {
    pub fn validate_for(&self, request: &EventPublishParams) -> Result<(), ProtocolError> {
        self.validate_identity(&request.session, &request.correlation_id)
    }

    pub fn validate_for_outbound(
        &self,
        request: &OutboundEventPublishParams,
    ) -> Result<(), ProtocolError> {
        self.validate_identity(&request.session, &request.correlation_id)
    }

    fn validate_identity(&self, session: &str, correlation_id: &str) -> Result<(), ProtocolError> {
        validate_echo(&self.session, &self.correlation_id, session, correlation_id)?;
        if let EventPublishOutcome::Runtime { failure } = &self.outcome {
            failure.validate()?;
        }
        Ok(())
    }
}
impl StreamOpenParams {
    pub fn validate_against(&self, initialize: &InitializeParams) -> Result<(), ProtocolError> {
        validate_provided_operation(
            &self.session,
            &self.correlation_id,
            &self.endpoint_id,
            &self.capability_id,
            &self.descriptor_version,
            &self.descriptor_digest,
            &self.operation,
            &self.scope,
            initialize,
            "stream open",
        )
    }
}
impl OutboundStreamOpenParams {
    pub fn validate_against(
        &self,
        initialize: &InitializeParams,
        parent: &InvocationScope,
        parent_active: bool,
    ) -> Result<(), ProtocolError> {
        validate_outbound_operation(
            &self.session,
            &self.correlation_id,
            &self.requirement_id,
            &self.route_id,
            &self.operation,
            &self.scope,
            initialize,
            parent,
            parent_active,
            "stream open",
        )
    }
}
impl StreamOpenResult {
    pub fn validate_for(&self, request: &StreamOpenParams) -> Result<(), ProtocolError> {
        self.validate_identity(&request.session, &request.correlation_id)
    }

    pub fn validate_for_outbound(
        &self,
        request: &OutboundStreamOpenParams,
    ) -> Result<(), ProtocolError> {
        self.validate_identity(&request.session, &request.correlation_id)
    }

    fn validate_identity(&self, session: &str, correlation_id: &str) -> Result<(), ProtocolError> {
        validate_echo(&self.session, &self.correlation_id, session, correlation_id)?;
        match &self.outcome {
            StreamOpenOutcome::Opened { stream_id } => {
                validate_decimal("stream_id", stream_id).map(|_| ())
            }
            StreamOpenOutcome::Domain { .. } => Ok(()),
            StreamOpenOutcome::Runtime { failure } => failure.validate(),
        }
    }
}
impl StreamSendParams {
    pub fn validate_for(&self, identity: &SessionIdentity) -> Result<(), ProtocolError> {
        validate_stream_action(
            &self.session,
            &self.correlation_id,
            &self.stream_id,
            identity,
        )?;
        validate_decimal("sequence", &self.sequence).map(|_| ())
    }
}
impl StreamReceiveParams {
    pub fn validate_for(&self, identity: &SessionIdentity) -> Result<(), ProtocolError> {
        validate_stream_action(
            &self.session,
            &self.correlation_id,
            &self.stream_id,
            identity,
        )
    }
}
impl StreamActionResult {
    pub fn validate_for(&self, request: &StreamReceiveParams) -> Result<(), ProtocolError> {
        validate_stream_result(
            &self.session,
            &self.correlation_id,
            &self.stream_id,
            &request.session,
            &request.correlation_id,
            &request.stream_id,
        )?;
        if let StreamActionOutcome::Runtime { failure } = &self.outcome {
            failure.validate()?;
        }
        Ok(())
    }

    pub fn validate_for_send(&self, request: &StreamSendParams) -> Result<(), ProtocolError> {
        validate_stream_result(
            &self.session,
            &self.correlation_id,
            &self.stream_id,
            &request.session,
            &request.correlation_id,
            &request.stream_id,
        )?;
        if let StreamActionOutcome::Runtime { failure } = &self.outcome {
            failure.validate()?;
        }
        Ok(())
    }
}
impl StreamReceiveResult {
    pub fn validate_for(&self, request: &StreamReceiveParams) -> Result<(), ProtocolError> {
        validate_stream_result(
            &self.session,
            &self.correlation_id,
            &self.stream_id,
            &request.session,
            &request.correlation_id,
            &request.stream_id,
        )?;
        match &self.outcome {
            StreamReceiveOutcome::Message { sequence, .. } => {
                validate_decimal("sequence", sequence).map(|_| ())
            }
            StreamReceiveOutcome::Runtime { failure } => failure.validate(),
            StreamReceiveOutcome::PeerHalfClosed | StreamReceiveOutcome::Terminal { .. } => Ok(()),
        }
    }
}
impl CancelParams {
    pub fn validate_for(&self, identity: &SessionIdentity) -> Result<(), ProtocolError> {
        validate_session(&self.session, identity)?;
        validate_token("scope_id", &self.scope_id)?;
        validate_decimal("correlation_id", &self.correlation_id)?;
        validate_detail(&self.reason)
    }
}
impl CancelAck {
    pub fn validate_for(&self, request: &CancelParams) -> Result<(), ProtocolError> {
        validate_echo(
            &self.session,
            &self.scope_id,
            &request.session,
            &request.scope_id,
        )?;
        if self.correlation_id != request.correlation_id || !self.accepted {
            return Err(error("cancel acknowledgement mismatch"));
        }
        Ok(())
    }
}
impl Settlement {
    pub fn validate_for(&self, identity: &SessionIdentity) -> Result<(), ProtocolError> {
        validate_session(&self.session, identity)?;
        validate_token("scope_id", &self.scope_id)?;
        validate_decimal("correlation_id", &self.correlation_id).map(|_| ())
    }
}
impl StopParams {
    pub fn validate_for(&self, identity: &SessionIdentity) -> Result<(), ProtocolError> {
        validate_session(&self.session, identity)?;
        validate_token("cleanup_scope_id", &self.cleanup_scope_id)?;
        validate_decimal("remaining_budget_nanos", &self.remaining_budget_nanos).map(|_| ())
    }
}
impl StoppedResult {
    pub fn validate_for(&self, request: &StopParams) -> Result<(), ProtocolError> {
        validate_echo(
            &self.session,
            &self.cleanup_scope_id,
            &request.session,
            &request.cleanup_scope_id,
        )?;
        for diagnostic in &self.diagnostics {
            validate_token("cleanup diagnostic code", &diagnostic.code)?;
            validate_detail(&diagnostic.detail)?;
        }
        Ok(())
    }
}

fn validate_descriptor(capability: &str, version: &str, digest: &str) -> Result<(), ProtocolError> {
    validate_token("capability_id", capability)?;
    validate_token("descriptor_version", version)?;
    validate_digest("descriptor_digest", digest)
}

#[expect(
    clippy::too_many_arguments,
    reason = "wire identity fields remain explicit"
)]
fn validate_provided_operation(
    session: &str,
    correlation_id: &str,
    endpoint_id: &str,
    capability_id: &str,
    descriptor_version: &str,
    descriptor_digest: &str,
    operation: &str,
    scope: &InvocationScope,
    initialize: &InitializeParams,
    interaction: &str,
) -> Result<(), ProtocolError> {
    validate_session(session, &initialize.identity)?;
    validate_decimal("correlation_id", correlation_id)?;
    validate_token("endpoint_id", endpoint_id)?;
    validate_descriptor(capability_id, descriptor_version, descriptor_digest)?;
    validate_token("operation", operation)?;
    scope.validate()?;
    let endpoint = initialize
        .provided_endpoints
        .iter()
        .find(|endpoint| endpoint.endpoint_id == endpoint_id)
        .ok_or_else(|| error(format!("{interaction} references an unknown endpoint_id")))?;
    if endpoint.capability_id != capability_id
        || endpoint.descriptor_version != descriptor_version
        || endpoint.descriptor_digest != descriptor_digest
    {
        return Err(error(format!(
            "{interaction} descriptor does not match its admitted endpoint"
        )));
    }
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "wire identity fields remain explicit"
)]
fn validate_outbound_operation(
    session: &str,
    correlation_id: &str,
    requirement_id: &str,
    route_id: &str,
    operation: &str,
    scope: &InvocationScope,
    initialize: &InitializeParams,
    parent: &InvocationScope,
    parent_active: bool,
    interaction: &str,
) -> Result<(), ProtocolError> {
    validate_session(session, &initialize.identity)?;
    validate_decimal("correlation_id", correlation_id)?;
    validate_requirement_id(requirement_id)?;
    validate_token("route_id", route_id)?;
    validate_token("operation", operation)?;
    scope.validate_child_of(parent)?;
    if !parent_active {
        return Err(error(format!(
            "closed parent scope cannot start an outbound {interaction}"
        )));
    }
    let route = initialize
        .routes
        .iter()
        .find(|route| route.route_id == route_id)
        .ok_or_else(|| {
            error(format!(
                "outbound {interaction} references an unknown route_id"
            ))
        })?;
    if route.requirement_id != requirement_id {
        return Err(error("outbound route belongs to another requirement"));
    }
    Ok(())
}

fn validate_stream_action(
    session: &str,
    correlation_id: &str,
    stream_id: &str,
    identity: &SessionIdentity,
) -> Result<(), ProtocolError> {
    validate_session(session, identity)?;
    validate_decimal("correlation_id", correlation_id)?;
    validate_decimal("stream_id", stream_id).map(|_| ())
}

fn validate_stream_result(
    session: &str,
    correlation_id: &str,
    stream_id: &str,
    expected_session: &str,
    expected_correlation_id: &str,
    expected_stream_id: &str,
) -> Result<(), ProtocolError> {
    validate_echo(
        session,
        correlation_id,
        expected_session,
        expected_correlation_id,
    )?;
    if stream_id != expected_stream_id {
        return Err(error("stream result identity mismatch"));
    }
    validate_decimal("stream_id", stream_id).map(|_| ())
}
fn validate_session(session: &str, identity: &SessionIdentity) -> Result<(), ProtocolError> {
    if session != identity.session {
        return Err(error("message session does not match admitted session"));
    }
    Ok(())
}
fn validate_echo(
    session: &str,
    id: &str,
    expected_session: &str,
    expected_id: &str,
) -> Result<(), ProtocolError> {
    if session != expected_session || id != expected_id {
        return Err(error("response identity mismatch"));
    }
    Ok(())
}
fn validate_detail(value: &str) -> Result<(), ProtocolError> {
    if value.is_empty() || value.len() > 1_024 {
        return Err(error("detail must contain 1..=1024 bytes"));
    }
    Ok(())
}
fn validate_limit(name: &str, value: u64, maximum: u64) -> Result<(), ProtocolError> {
    if value == 0 || value > maximum {
        return Err(error(format!("{name} must be within 1..={maximum}")));
    }
    Ok(())
}
fn validate_token(name: &str, value: &str) -> Result<(), ProtocolError> {
    if value.is_empty()
        || value.len() > 256
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'@' | b'/' | b':')
        })
    {
        return Err(error(format!("{name} must be a portable token")));
    }
    Ok(())
}
fn validate_requirement_id(value: &str) -> Result<(), ProtocolError> {
    if value.is_empty()
        || value.len() > 64
        || !value.as_bytes()[0].is_ascii_lowercase()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(error("requirement_id must match ^[a-z][a-z0-9_]{0,63}$"));
    }
    Ok(())
}
fn validate_digest(name: &str, value: &str) -> Result<(), ProtocolError> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(error(format!("{name} must be a canonical SHA-256 digest")));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error(format!("{name} must be a canonical SHA-256 digest")));
    }
    Ok(())
}
fn validate_decimal(name: &str, value: &str) -> Result<u64, ProtocolError> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(error(format!("{name} must be a canonical decimal string")));
    }
    decimal_value(value)
}
fn decimal_value(value: &str) -> Result<u64, ProtocolError> {
    value
        .parse()
        .map_err(|_| error("decimal string exceeds unsigned 64-bit range"))
}
fn validate_sorted<T>(
    values: &[T],
    key: impl Fn(&T) -> &str,
    name: &str,
) -> Result<(), ProtocolError> {
    if values.windows(2).any(|pair| key(&pair[0]) >= key(&pair[1])) {
        return Err(error(format!("{name} must be sorted and unique")));
    }
    Ok(())
}
fn validate_sorted_strings(values: &[String], name: &str) -> Result<(), ProtocolError> {
    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(error(format!("{name} must be sorted and unique")));
    }
    for value in values {
        validate_token(name, value)?;
    }
    Ok(())
}
fn error(detail: impl Into<String>) -> ProtocolError {
    ProtocolError::new(detail)
}
