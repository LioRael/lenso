//! Versioned execution-target capability profiles.
//!
//! A profile is an Adapter or Driver's explicit, portable declaration of the
//! interactions and host facilities it implements. It is not an App Plan, a
//! Plugin requirement, or a provider-selection mechanism. Hosts use it before
//! composition to reject a selected implementation whose declared target lacks
//! a required feature.

use serde::{Deserialize, Serialize, de::Error as _};

use super::ProtocolError;

/// Exact versioned profile identity for execution-target capabilities.
pub const EXECUTION_TARGET_CAPABILITY_PROFILE: &str = "lenso.execution-target-capability-profile@1";

/// Closed feature vocabulary for an execution target.
///
/// A future feature is intentionally not accepted by this version. Callers
/// that ask for an unknown feature therefore receive `false` from
/// [`ExecutionTargetCapabilityProfile::supports_named`].
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ExecutionTargetCapability {
    /// One request with one terminal outcome.
    #[serde(rename = "request")]
    Request,
    /// One bounded bidirectional Stream session.
    #[serde(rename = "stream")]
    Stream,
    /// Independently admitted volatile Event fan-out.
    #[serde(rename = "event")]
    Event,
    /// WebSocket ingress and lifecycle handling.
    #[serde(rename = "websocket")]
    WebSocket,
    /// Plan-bound guest-to-host imports.
    #[serde(rename = "host-imports")]
    HostImports,
    /// Native child-process execution.
    #[serde(rename = "native-process")]
    NativeProcess,
    /// Wasm component execution.
    #[serde(rename = "wasm-component")]
    WasmComponent,
    /// Remote execution through an Adapter-owned transport.
    #[serde(rename = "remote")]
    Remote,
    /// Browser execution.
    #[serde(rename = "browser")]
    Browser,
    /// Cloudflare Workers execution.
    #[serde(rename = "workers")]
    Workers,
}

impl ExecutionTargetCapability {
    /// Every feature known to profile version 1, in canonical wire order.
    pub const ALL: [Self; 10] = [
        Self::Browser,
        Self::Event,
        Self::HostImports,
        Self::NativeProcess,
        Self::Remote,
        Self::Request,
        Self::Stream,
        Self::WasmComponent,
        Self::WebSocket,
        Self::Workers,
    ];

    /// Returns the exact portable feature spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Stream => "stream",
            Self::Event => "event",
            Self::WebSocket => "websocket",
            Self::HostImports => "host-imports",
            Self::NativeProcess => "native-process",
            Self::WasmComponent => "wasm-component",
            Self::Remote => "remote",
            Self::Browser => "browser",
            Self::Workers => "workers",
        }
    }

    /// Parses one feature spelling from an untyped request.
    #[must_use]
    pub fn from_name(value: &str) -> Option<Self> {
        match value {
            "request" => Some(Self::Request),
            "stream" => Some(Self::Stream),
            "event" => Some(Self::Event),
            "websocket" => Some(Self::WebSocket),
            "host-imports" => Some(Self::HostImports),
            "native-process" => Some(Self::NativeProcess),
            "wasm-component" => Some(Self::WasmComponent),
            "remote" => Some(Self::Remote),
            "browser" => Some(Self::Browser),
            "workers" => Some(Self::Workers),
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for ExecutionTargetCapability {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_name(&value)
            .ok_or_else(|| D::Error::custom("unknown execution target capability"))
    }
}

/// Machine-checkable declaration of one exact Adapter or Driver target.
///
/// `target_profile` identifies the selected runtime profile, for example
/// `lenso.bun-authoring@2`. It deliberately does not imply any feature: every
/// supported feature is explicit in `capabilities`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionTargetCapabilityProfile {
    /// Exact versioned profile contract identity.
    pub profile: String,
    /// Exact Adapter or Driver runtime profile this declaration describes.
    pub target_profile: String,
    /// Canonical-sorted, unique, explicit target features. An empty list is an
    /// explicit declaration that this target supports no V1 features.
    pub capabilities: Vec<ExecutionTargetCapability>,
}

impl ExecutionTargetCapabilityProfile {
    /// Validates the closed vocabulary, exact profile identity, and canonical
    /// feature ordering required by the portable profile contract.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.profile != EXECUTION_TARGET_CAPABILITY_PROFILE {
            return Err(ProtocolError::new(
                "unsupported execution target capability profile",
            ));
        }
        validate_target_profile(&self.target_profile)?;
        if self
            .capabilities
            .windows(2)
            .any(|pair| pair[0].as_str() >= pair[1].as_str())
        {
            return Err(ProtocolError::new(
                "capabilities must be strictly canonical-sorted and unique",
            ));
        }
        Ok(())
    }

    /// Returns whether a validated profile explicitly supports this feature.
    ///
    /// An invalid or incomplete profile is never trusted, so this method fails
    /// closed rather than treating its feature list as authoritative.
    #[must_use]
    pub fn supports(&self, capability: ExecutionTargetCapability) -> bool {
        self.validate().is_ok() && self.capabilities.contains(&capability)
    }

    /// Returns whether a validated profile supports an untyped requested
    /// feature. Unknown feature names return `false`.
    #[must_use]
    pub fn supports_named(&self, capability: &str) -> bool {
        ExecutionTargetCapability::from_name(capability).is_some_and(|known| self.supports(known))
    }

    /// Reports every required feature the profile cannot prove it supports.
    ///
    /// The result preserves the caller's requirement order. Unknown feature
    /// names remain missing instead of being silently dropped.
    #[must_use]
    pub fn missing_capabilities<'a>(
        &self,
        required: impl IntoIterator<Item = &'a str>,
    ) -> Vec<String> {
        required
            .into_iter()
            .filter(|capability| !self.supports_named(capability))
            .map(ToOwned::to_owned)
            .collect()
    }
}

fn validate_target_profile(value: &str) -> Result<(), ProtocolError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'@'))
    {
        return Err(ProtocolError::new(
            "target_profile must be a 1..=128 byte portable token",
        ));
    }
    Ok(())
}
