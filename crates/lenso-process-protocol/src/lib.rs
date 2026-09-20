//! Language-neutral source types for `lenso-process-jsonrpc-http-v1`.

use std::cmp::Ordering;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod execution_target;
mod proof;
mod strict_json;

/// Runtime-neutral values shared by Authoring V2 execution profiles.
///
/// This module does not define a transport envelope. Process and Bun adapters
/// wrap these values in their own versioned protocols.
pub mod authoring;

pub use execution_target::{
    EXECUTION_TARGET_CAPABILITY_PROFILE, ExecutionTargetCapability,
    ExecutionTargetCapabilityProfile,
};
pub use proof::{
    AuthoringHandshakeProofInput, authoring_callback_proof_message, authoring_child_proof_message,
    authoring_handshake_proof_payload, authoring_host_proof_message, canonicalize_proof_value,
    child_proof_message, handshake_params_digest, host_proof_message,
};
pub use strict_json::{MAX_STRICT_JSON_NESTING, decode_strict, encode_compact};

/// Exact Process Protocol profile selected by an Adapter.
pub const PROCESS_PROFILE: &str = "lenso-process-jsonrpc-http-v1";
/// Portable JSON value profile used by request payloads.
pub const VALUE_PROFILE: &str = "lenso-json-value-v1";
/// Mandatory child-provides-request interaction profile.
pub const PROVIDE_REQUEST_PROFILE: &str = "provide-request-v1";
/// Control method used once during process startup.
pub const HANDSHAKE_METHOD: &str = "lenso.process.v1.handshake";
/// Data method used for one request interaction.
pub const REQUEST_METHOD: &str = "lenso.process.v1.request";
/// Control method used for idempotent cancellation.
pub const CANCEL_METHOD: &str = "lenso.process.v1.cancel";
/// Control method used for graceful child shutdown.
pub const SHUTDOWN_METHOD: &str = "lenso.process.v1.shutdown";

/// One bounded readiness record written on the dedicated inherited handle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadinessRecord {
    /// Exact Process Protocol profile implemented by both listeners.
    pub protocol: String,
    /// Ephemeral loopback data-listener port.
    pub data_port: u16,
    /// Distinct ephemeral loopback control-listener port.
    pub control_port: u16,
}

impl ReadinessRecord {
    /// Validates the exact profile and two non-zero, distinct ports.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.protocol != PROCESS_PROFILE {
            return Err(ProtocolError::new("unsupported Process Protocol profile"));
        }
        if self.data_port == 0 || self.control_port == 0 || self.data_port == self.control_port {
            return Err(ProtocolError::new(
                "readiness requires distinct non-zero data and control ports",
            ));
        }
        Ok(())
    }
}

/// Error returned when a Process Protocol document is malformed or inconsistent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolError {
    detail: String,
}

impl ProtocolError {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }

    /// Returns the stable human-readable validation detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for ProtocolError {}

/// Peer-confirmed limits that must match exactly during handshake.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeerLimits {
    /// Maximum encoded data request or response body.
    pub max_http_body_bytes: u64,
    /// Maximum encoded control request or response body.
    pub max_control_http_body_bytes: u64,
    /// Maximum concurrently active request handlers.
    pub max_concurrent_requests: u32,
    /// Maximum queued child data requests.
    pub child_request_queue_capacity: u32,
    /// Maximum completed correlation IDs retained in one process session.
    pub max_retired_correlation_ids: u32,
    /// Maximum queued control requests, with at least one reserved handler.
    pub control_queue_capacity: u32,
}

impl PeerLimits {
    /// Returns the V1 defaults.
    #[must_use]
    pub const fn v1_defaults() -> Self {
        Self {
            max_http_body_bytes: 65_536,
            max_control_http_body_bytes: 16_384,
            max_concurrent_requests: 32,
            child_request_queue_capacity: 32,
            max_retired_correlation_ids: 65_536,
            control_queue_capacity: 32,
        }
    }

    /// Enforces non-zero V1 hard maxima.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_limit("max_http_body_bytes", self.max_http_body_bytes, 1_048_576)?;
        validate_limit(
            "max_control_http_body_bytes",
            self.max_control_http_body_bytes,
            65_536,
        )?;
        validate_limit(
            "max_concurrent_requests",
            u64::from(self.max_concurrent_requests),
            256,
        )?;
        validate_limit(
            "child_request_queue_capacity",
            u64::from(self.child_request_queue_capacity),
            1_024,
        )?;
        validate_limit(
            "max_retired_correlation_ids",
            u64::from(self.max_retired_correlation_ids),
            1_048_576,
        )?;
        validate_limit(
            "control_queue_capacity",
            u64::from(self.control_queue_capacity),
            256,
        )
    }
}

/// One exact provided operation in handshake identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationDescriptor {
    /// Stable operation name.
    pub operation: String,
    /// Exact interaction kind. V1 supports request provision only.
    pub interaction: InteractionKind,
}

/// Interaction kinds admitted by the base V1 profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionKind {
    /// One request with one terminal outcome.
    Request,
}

/// One exact provided Capability Descriptor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityDescriptor {
    /// Stable `namespace.name@major` identity.
    pub capability_id: String,
    /// Exact Descriptor semantic version.
    pub descriptor_version: String,
    /// Canonical Descriptor SHA-256 digest.
    pub descriptor_digest: String,
    /// Canonical-sorted operation table.
    pub operations: Vec<OperationDescriptor>,
}

/// One exact outbound binding exposed only by a selected consume profile.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutboundBindingDescriptor {
    /// Stable binding identity within the Plugin Instance.
    pub binding_id: String,
    /// Required Capability identity.
    pub capability_id: String,
    /// Exact Descriptor version.
    pub descriptor_version: String,
    /// Canonical Descriptor digest.
    pub descriptor_digest: String,
    /// Explicit provider Plugin Instance key.
    pub provider_instance: String,
}

/// Immutable identity the child must echo exactly during handshake.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandshakeIdentity {
    /// Exact transport profile.
    pub protocol_profile: String,
    /// Exact portable value profile.
    pub value_profile: String,
    /// App-local Plugin Instance key.
    pub plugin_instance: String,
    /// Canonical decimal Plugin generation.
    pub plugin_generation: String,
    /// Exact App Generation Spec digest.
    pub generation_spec_digest: String,
    /// Exact executable Artifact digest.
    pub artifact_digest: String,
    /// Exact Effective Host Grant Set digest.
    pub effective_host_grant_set_digest: String,
    /// Canonical-sorted exact interaction profiles.
    pub interaction_profiles: Vec<String>,
    /// Canonical-sorted provided Capability Descriptors.
    pub provided_capabilities: Vec<CapabilityDescriptor>,
    /// Canonical-sorted explicit outbound bindings.
    pub outbound_bindings: Vec<OutboundBindingDescriptor>,
    /// Limits both peers must confirm exactly.
    pub peer_limits: PeerLimits,
}

/// Host-to-child handshake params.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandshakeParams {
    /// Exact identity to admit.
    pub identity: HandshakeIdentity,
    /// Canonical base64url encoding of 32 random bytes.
    pub host_nonce: String,
    /// Canonical base64url HMAC output.
    pub host_proof: String,
}

/// Child-to-host accepted handshake result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandshakeResult {
    /// Exact admitted identity echoed from the request.
    pub identity: HandshakeIdentity,
    /// Canonical base64url encoding of 32 random bytes.
    pub session: String,
    /// Canonical base64url HMAC output.
    pub child_proof: String,
}

/// Portable invocation extension transported without domain interpretation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvocationExtension {
    /// Stable unique extension key.
    pub key: String,
    /// Canonical padded Base64 bytes.
    pub value: String,
    /// Optional issuing authority.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    /// Canonical-sorted declared audiences.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audience: Vec<String>,
    /// Optional opaque proof.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof: Option<String>,
    /// Whether the extension value is sealed for its audience.
    #[serde(default, skip_serializing_if = "is_false")]
    pub sealed: bool,
}

/// Host-to-child request params for `provide-request-v1`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestParams {
    /// Authenticated process session.
    pub session: String,
    /// Opaque canonical decimal-string correlation ID.
    pub correlation_id: String,
    /// Selected Capability identity.
    pub capability_id: String,
    /// Exact selected Descriptor version.
    pub descriptor_version: String,
    /// Exact selected Descriptor digest.
    pub descriptor_digest: String,
    /// Selected operation.
    pub operation: String,
    /// Exact request interaction marker.
    pub interaction: InteractionKind,
    /// Host-established caller Plugin Instance, or `null` for host ingress.
    pub caller_instance: Option<String>,
    /// Relative remaining budget as decimal nanoseconds, or `null` for none.
    pub remaining_timeout_nanos: Option<String>,
    /// Structurally validated portable invocation extensions.
    #[serde(default)]
    pub extensions: Vec<InvocationExtension>,
    /// Capability-defined portable JSON payload.
    pub payload: Value,
}

/// Child-to-host request result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestResult {
    /// Authenticated process session.
    pub session: String,
    /// Correlation ID echoed from the request.
    pub correlation_id: String,
    /// Exactly one terminal application outcome.
    pub outcome: ProcessOutcome,
}

/// Child-originated request outcomes allowed by V1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProcessOutcome {
    /// Capability success payload.
    Success { value: Value },
    /// Capability-defined Domain Error payload.
    Domain { error: Value },
    /// Narrow child-originated Runtime Failure.
    Runtime { failure: ChildRuntimeFailure },
}

/// Runtime Failures the child is permitted to originate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChildRuntimeFailure {
    /// The child cannot admit this operation within its declared bound.
    ResourceExhausted { operation: String },
    /// The Plugin generation is no longer trustworthy and must be retired.
    PluginFailure { detail: String },
}

/// Idempotent cancel params.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelParams {
    /// Authenticated process session.
    pub session: String,
    /// Active, retired, or unknown correlation ID.
    pub correlation_id: String,
}

/// Graceful shutdown params.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShutdownParams {
    /// Authenticated process session.
    pub session: String,
}

/// Successful cancel or shutdown acknowledgement.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlAck {
    /// Authenticated process session.
    pub session: String,
    /// Always true; cancel does not reveal correlation existence.
    pub accepted: bool,
}

/// Strict JSON-RPC request envelope.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JsonRpcRequest<T> {
    /// Must equal `2.0`.
    pub jsonrpc: String,
    /// Canonical decimal-string JSON-RPC ID.
    pub id: String,
    /// Exact versioned method.
    pub method: String,
    /// Strict method params object.
    pub params: T,
}

/// Strict successful JSON-RPC response envelope.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JsonRpcSuccess<T> {
    /// Must equal `2.0`.
    pub jsonrpc: String,
    /// Exact request ID.
    pub id: String,
    /// Strict method result object.
    pub result: T,
}

/// Strict JSON-RPC 2.0 error object used before a method result exists.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JsonRpcErrorObject {
    /// Standard JSON-RPC error code.
    pub code: i32,
    /// Stable public error message without host or child internals.
    pub message: String,
}

/// Strict JSON-RPC 2.0 error response. Parse/invalid-request errors use a null ID.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JsonRpcError {
    /// Must equal `2.0`.
    pub jsonrpc: String,
    /// Exact request ID, or null when the request ID could not be authenticated.
    pub id: Option<String>,
    /// Standard error object.
    pub error: JsonRpcErrorObject,
}

impl HandshakeIdentity {
    /// Validates exact profiles, digests, canonical ordering, and V1 limits.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.protocol_profile != PROCESS_PROFILE {
            return Err(ProtocolError::new("unsupported Process Protocol profile"));
        }
        if self.value_profile != VALUE_PROFILE {
            return Err(ProtocolError::new("unsupported portable value profile"));
        }
        validate_token("plugin_instance", &self.plugin_instance)?;
        validate_decimal("plugin_generation", &self.plugin_generation)?;
        validate_digest("generation_spec_digest", &self.generation_spec_digest)?;
        validate_digest("artifact_digest", &self.artifact_digest)?;
        validate_digest(
            "effective_host_grant_set_digest",
            &self.effective_host_grant_set_digest,
        )?;
        validate_sorted_unique("interaction_profiles", &self.interaction_profiles, Ord::cmp)?;
        if self.interaction_profiles.as_slice() != [PROVIDE_REQUEST_PROFILE] {
            return Err(ProtocolError::new(
                "V1 requires exactly the provide-request-v1 interaction profile",
            ));
        }
        validate_sorted_unique(
            "provided_capabilities",
            &self.provided_capabilities,
            |left, right| left.capability_id.cmp(&right.capability_id),
        )?;
        if self.provided_capabilities.is_empty() {
            return Err(ProtocolError::new(
                "at least one provided Capability is required",
            ));
        }
        for descriptor in &self.provided_capabilities {
            descriptor.validate()?;
        }
        if !self.outbound_bindings.is_empty() {
            return Err(ProtocolError::new(
                "outbound bindings require a selected consume profile",
            ));
        }
        self.peer_limits.validate()
    }
}

impl CapabilityDescriptor {
    fn validate(&self) -> Result<(), ProtocolError> {
        validate_token("capability_id", &self.capability_id)?;
        validate_token("descriptor_version", &self.descriptor_version)?;
        validate_digest("descriptor_digest", &self.descriptor_digest)?;
        validate_sorted_unique("operations", &self.operations, |left, right| {
            left.operation.cmp(&right.operation)
        })?;
        if self.operations.is_empty() {
            return Err(ProtocolError::new(
                "provided Capability operations cannot be empty",
            ));
        }
        for operation in &self.operations {
            validate_token("operation", &operation.operation)?;
        }
        Ok(())
    }
}

impl HandshakeParams {
    /// Validates identity, nonce, and proof encoding.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.identity.validate()?;
        decode_base64url_32("host_nonce", &self.host_nonce)?;
        decode_base64url_32("host_proof", &self.host_proof)?;
        Ok(())
    }
}

impl HandshakeResult {
    /// Validates the child echo and proof encoding against the expected identity.
    pub fn validate_against(&self, expected: &HandshakeIdentity) -> Result<(), ProtocolError> {
        self.identity.validate()?;
        if &self.identity != expected {
            return Err(ProtocolError::new("child handshake identity mismatch"));
        }
        decode_base64url_32("session", &self.session)?;
        decode_base64url_32("child_proof", &self.child_proof)?;
        Ok(())
    }
}

impl RequestParams {
    /// Validates host-owned request envelope fields before payload Schema decoding.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        decode_base64url_32("session", &self.session)?;
        validate_decimal("correlation_id", &self.correlation_id)?;
        validate_token("capability_id", &self.capability_id)?;
        validate_token("descriptor_version", &self.descriptor_version)?;
        validate_digest("descriptor_digest", &self.descriptor_digest)?;
        validate_token("operation", &self.operation)?;
        if let Some(caller) = &self.caller_instance {
            validate_token("caller_instance", caller)?;
        }
        if let Some(timeout) = &self.remaining_timeout_nanos {
            validate_decimal("remaining_timeout_nanos", timeout)?;
        }
        validate_sorted_unique("extensions", &self.extensions, |left, right| {
            left.key.cmp(&right.key)
        })?;
        for extension in &self.extensions {
            extension.validate()?;
        }
        Ok(())
    }
}

impl InvocationExtension {
    fn validate(&self) -> Result<(), ProtocolError> {
        validate_token("extension key", &self.key)?;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&self.value)
            .map_err(|_| ProtocolError::new("extension value must be padded Base64"))?;
        if base64::engine::general_purpose::STANDARD.encode(decoded) != self.value {
            return Err(ProtocolError::new(
                "extension value must use canonical padded Base64",
            ));
        }
        validate_sorted_unique("extension audience", &self.audience, |left, right| {
            left.cmp(right)
        })?;
        Ok(())
    }
}

impl RequestResult {
    /// Validates session, correlation, and bounded child Runtime Failure fields.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        decode_base64url_32("session", &self.session)?;
        validate_decimal("correlation_id", &self.correlation_id)?;
        if let ProcessOutcome::Runtime { failure } = &self.outcome {
            match failure {
                ChildRuntimeFailure::ResourceExhausted { operation } => {
                    validate_token("operation", operation)?;
                }
                ChildRuntimeFailure::PluginFailure { detail } => {
                    if detail.is_empty() || detail.len() > 1_024 {
                        return Err(ProtocolError::new(
                            "Plugin Failure detail must contain 1..=1024 bytes",
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

impl CancelParams {
    /// Validates the authenticated cancel identity.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        decode_base64url_32("session", &self.session)?;
        validate_decimal("correlation_id", &self.correlation_id).map(|_| ())
    }
}

impl ShutdownParams {
    /// Validates the authenticated shutdown identity.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        decode_base64url_32("session", &self.session)?;
        Ok(())
    }
}

impl ControlAck {
    /// Validates an authenticated, affirmative control acknowledgement.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        decode_base64url_32("session", &self.session)?;
        if !self.accepted {
            return Err(ProtocolError::new(
                "control acknowledgement must be accepted",
            ));
        }
        Ok(())
    }
}

impl<T> JsonRpcRequest<T> {
    /// Validates the base JSON-RPC envelope and exact expected method.
    pub fn validate_envelope(&self, expected_method: &str) -> Result<(), ProtocolError> {
        if self.jsonrpc != "2.0" {
            return Err(ProtocolError::new("jsonrpc must equal 2.0"));
        }
        validate_decimal("JSON-RPC id", &self.id)?;
        if self.method != expected_method {
            return Err(ProtocolError::new("unexpected JSON-RPC method"));
        }
        Ok(())
    }
}

impl<T> JsonRpcSuccess<T> {
    /// Validates the base JSON-RPC success envelope and expected request ID.
    pub fn validate_envelope(&self, expected_id: &str) -> Result<(), ProtocolError> {
        if self.jsonrpc != "2.0" {
            return Err(ProtocolError::new("jsonrpc must equal 2.0"));
        }
        validate_decimal("JSON-RPC id", &self.id)?;
        if self.id != expected_id {
            return Err(ProtocolError::new("JSON-RPC response id mismatch"));
        }
        Ok(())
    }
}

impl JsonRpcError {
    /// Validates the closed V1 JSON-RPC error surface and optional request ID.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.jsonrpc != "2.0" {
            return Err(ProtocolError::new("jsonrpc must equal 2.0"));
        }
        if let Some(id) = &self.id {
            validate_decimal("JSON-RPC id", id)?;
        }
        if !matches!(self.error.code, -32700 | -32600 | -32601 | -32602 | -32603) {
            return Err(ProtocolError::new("unsupported JSON-RPC error code"));
        }
        if self.error.message.is_empty() || self.error.message.len() > 256 {
            return Err(ProtocolError::new(
                "JSON-RPC error message must contain 1..=256 bytes",
            ));
        }
        Ok(())
    }
}

fn validate_limit(name: &str, value: u64, maximum: u64) -> Result<(), ProtocolError> {
    if value == 0 || value > maximum {
        return Err(ProtocolError::new(format!(
            "{name} must be within 1..={maximum}"
        )));
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
        return Err(ProtocolError::new(format!(
            "{name} must be a 1..=256 byte portable token"
        )));
    }
    Ok(())
}

fn validate_decimal(name: &str, value: &str) -> Result<u64, ProtocolError> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(ProtocolError::new(format!(
            "{name} must be a canonical non-negative decimal string"
        )));
    }
    value.parse::<u64>().map_err(|_| {
        ProtocolError::new(format!(
            "{name} exceeds the unsigned 64-bit decimal profile"
        ))
    })
}

fn validate_digest(name: &str, value: &str) -> Result<(), ProtocolError> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(ProtocolError::new(format!(
            "{name} must use a sha256: prefix"
        )));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ProtocolError::new(format!(
            "{name} must contain 64 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

fn decode_base64url_32(name: &str, value: &str) -> Result<[u8; 32], ProtocolError> {
    let decoded = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| ProtocolError::new(format!("{name} must be canonical base64url")))?;
    let decoded: [u8; 32] = decoded
        .try_into()
        .map_err(|_| ProtocolError::new(format!("{name} must encode exactly 32 bytes")))?;
    if URL_SAFE_NO_PAD.encode(decoded) != value {
        return Err(ProtocolError::new(format!(
            "{name} must be canonical unpadded base64url"
        )));
    }
    Ok(decoded)
}

fn validate_sorted_unique<T>(
    name: &str,
    values: &[T],
    compare: impl Fn(&T, &T) -> Ordering,
) -> Result<(), ProtocolError> {
    if values
        .windows(2)
        .any(|pair| compare(&pair[0], &pair[1]) != Ordering::Less)
    {
        return Err(ProtocolError::new(format!(
            "{name} must be strictly canonical-sorted and unique"
        )));
    }
    Ok(())
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_false(value: &bool) -> bool {
    !*value
}
