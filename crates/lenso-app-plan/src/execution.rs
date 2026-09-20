//! Execution Adapter class identities preserved in the Resolved App Plan.

use std::{collections::BTreeSet, fmt};

use serde::{Deserialize, Deserializer, Serialize};

use crate::CapabilityOperationKind;

/// Stable App-local identity of one single-owner Kernel execution lane.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ExecutionLaneId(String);

impl ExecutionLaneId {
    /// Creates an App-local lane identity selected by App Composition.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the conventional one-lane placement used when none is authored.
    pub fn main() -> Self {
        Self::new("main")
    }

    /// Returns the stable App-local identity.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ExecutionLaneId {
    fn default() -> Self {
        Self::main()
    }
}

impl fmt::Display for ExecutionLaneId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One Plan-declared single-owner Kernel lane.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExecutionLanePlan {
    id: ExecutionLaneId,
}

impl ExecutionLanePlan {
    /// Declares one execution lane.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: ExecutionLaneId::new(id),
        }
    }

    /// Returns the App-local lane identity.
    pub const fn id(&self) -> &ExecutionLaneId {
        &self.id
    }
}

/// Stable, open identity of the execution mechanism selected for a Plugin
/// Instance.
///
/// Execution Adapter packages own these IDs. The Plan preserves them as opaque
/// authoring data so third-party Adapters do not require changes to this crate.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ExecutionClassId(String);

impl ExecutionClassId {
    /// Creates an execution-class identity selected by App Composition.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the official statically linked Rust execution class.
    pub fn native_rust() -> Self {
        Self::new("lenso.native-rust@1")
    }

    /// Returns the official trusted Bun child-process execution class.
    pub fn bun_child_process() -> Self {
        Self::new("lenso.bun-process@1")
    }

    /// Returns the stable execution-class identity.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ExecutionClassId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// An execution-target feature that a Host Adapter explicitly declares.
///
/// These are target mechanics, not Plugin dependencies and not Infrastructure
/// selections. A Host uses them while admitting an implementation; the
/// immutable Plan remains unchanged after the selection succeeds.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionTargetCapability {
    /// One request produces one terminal response or domain error.
    Request,
    /// A Capability operation opens an ordered bidirectional stream.
    Stream,
    /// A Capability operation publishes an ephemeral event.
    Event,
    /// A Host ingress can keep a WebSocket session.
    #[serde(rename = "websocket")]
    WebSocket,
    /// The target can supply explicit Host imports to an admitted Plugin.
    HostImports,
    /// The target can own a native child process lifecycle.
    NativeProcess,
    /// The target can admit a Wasm Component execution.
    WasmComponent,
    /// The target can route a remote Adapter execution.
    Remote,
    /// The target runs in a browser execution environment.
    Browser,
    /// The target runs in a Cloudflare Workers execution environment.
    Workers,
}

impl ExecutionTargetCapability {
    /// Every feature known to the portable V1 target-profile vocabulary, in
    /// canonical wire order. Keep this list aligned with
    /// `lenso.execution-target-capability-profile@1`; an unknown feature must
    /// be represented by a later version of that contract rather than inferred
    /// by a Host.
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

    /// Maps one portable Capability operation semantics to the required target
    /// feature. WebSocket is a Web transport feature, so it is requested by a
    /// Web Host rather than inferred from a generic Capability operation.
    pub const fn for_operation_kind(kind: CapabilityOperationKind) -> Self {
        match kind {
            CapabilityOperationKind::Request => Self::Request,
            CapabilityOperationKind::Stream => Self::Stream,
            CapabilityOperationKind::Event => Self::Event,
        }
    }
}

/// Returns a canonical, duplicate-free target-requirement list.
///
/// Runtime profiles are an Adapter concern, but an implementation's required
/// target facilities are immutable Plugin selection data. Keeping the list in
/// canonical wire order makes independently produced Descriptors and Plans
/// compare byte-for-byte without relying on authoring input order.
pub(crate) fn normalize_target_capabilities(
    capabilities: impl IntoIterator<Item = ExecutionTargetCapability>,
) -> Vec<ExecutionTargetCapability> {
    let mut capabilities = capabilities.into_iter().collect::<Vec<_>>();
    capabilities.sort_unstable_by_key(|capability| capability.as_str());
    capabilities.dedup();
    capabilities
}

/// Decodes an optional target-requirement list into its canonical form.
///
/// V2 and V3 Plans intentionally omit this V4 field. `#[serde(default)]` on
/// the owning field supplies an empty, fail-closed requirement set for those
/// historic snapshots; when the field is present, duplicate and unordered
/// authoring input is normalized before it can enter a Plan.
pub(crate) fn deserialize_normalized_target_capabilities<'de, D>(
    deserializer: D,
) -> Result<Vec<ExecutionTargetCapability>, D::Error>
where
    D: Deserializer<'de>,
{
    Vec::<ExecutionTargetCapability>::deserialize(deserializer).map(normalize_target_capabilities)
}

/// Machine-checkable target features declared by exactly one Host Adapter.
///
/// An empty set is valid and fails every feature-dependent admission. This is
/// intentional: an unknown or legacy target must not silently receive a
/// Request, Stream, Event, or WebSocket assumption.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ExecutionTargetCapabilities(BTreeSet<ExecutionTargetCapability>);

impl ExecutionTargetCapabilities {
    /// Creates a target profile from its complete supported feature set.
    pub fn new(features: impl IntoIterator<Item = ExecutionTargetCapability>) -> Self {
        Self(features.into_iter().collect())
    }

    /// Returns a profile that declares no capabilities.
    pub fn none() -> Self {
        Self::default()
    }

    /// Returns whether this target explicitly supports a feature.
    pub fn supports(&self, capability: ExecutionTargetCapability) -> bool {
        self.0.contains(&capability)
    }

    /// Returns missing features in the caller's requirement order. This keeps
    /// Host rejection reports actionable and intentionally retains duplicate
    /// or future requirements rather than inventing an implicit fallback.
    pub fn missing(
        &self,
        required: impl IntoIterator<Item = ExecutionTargetCapability>,
    ) -> Vec<ExecutionTargetCapability> {
        required
            .into_iter()
            .filter(|capability| !self.supports(*capability))
            .collect()
    }

    /// Returns the complete feature set in canonical wire order.
    pub fn features(&self) -> Vec<ExecutionTargetCapability> {
        ExecutionTargetCapability::ALL
            .into_iter()
            .filter(|capability| self.supports(*capability))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_target_features_fail_closed_in_stable_order() {
        let profile = ExecutionTargetCapabilities::new([ExecutionTargetCapability::Request]);

        assert!(profile.supports(ExecutionTargetCapability::Request));
        assert_eq!(
            profile.missing([
                ExecutionTargetCapability::Stream,
                ExecutionTargetCapability::Request,
                ExecutionTargetCapability::WebSocket,
            ]),
            vec![
                ExecutionTargetCapability::Stream,
                ExecutionTargetCapability::WebSocket,
            ]
        );
        assert_eq!(
            serde_json::to_string(&ExecutionTargetCapability::HostImports).unwrap(),
            "\"host-imports\""
        );
        assert_eq!(profile.features(), vec![ExecutionTargetCapability::Request]);
    }

    #[test]
    fn target_capability_vocabulary_and_normalization_are_canonical() {
        for capability in ExecutionTargetCapability::ALL {
            let wire = serde_json::to_string(&capability).unwrap();
            assert_eq!(
                serde_json::from_str::<ExecutionTargetCapability>(&wire).unwrap(),
                capability
            );
        }

        assert_eq!(
            normalize_target_capabilities([
                ExecutionTargetCapability::Workers,
                ExecutionTargetCapability::Browser,
                ExecutionTargetCapability::Workers,
                ExecutionTargetCapability::HostImports,
            ]),
            vec![
                ExecutionTargetCapability::Browser,
                ExecutionTargetCapability::HostImports,
                ExecutionTargetCapability::Workers,
            ]
        );
    }
}
