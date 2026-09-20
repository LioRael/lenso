//! Explicit capability evidence for the Bun Authoring V2 execution target.
//!
//! The list is kept next to the Adapter rather than inferred by a Host from a
//! runtime-profile string. `adapter_v2` owns request, stream, Event, and host
//! import bridges; `authoring_v2` owns the supervised Bun child-process
//! lifecycle. Features absent here are intentionally unsupported.

use serde::Serialize;

use crate::BUN_AUTHORING_RUNTIME_PROFILE;

/// Stable identity of the protocol contract this profile implements.
pub const EXECUTION_TARGET_CAPABILITY_PROFILE_CONTRACT: &str =
    "lenso.execution-target-capability-profile@1";

/// Target-owned feature declarations for Bun Authoring V2.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BunExecutionTargetCapability {
    Request,
    Stream,
    Event,
    HostImports,
    NativeProcess,
}

impl BunExecutionTargetCapability {
    /// Returns the portable profile-contract spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Stream => "stream",
            Self::Event => "event",
            Self::HostImports => "host-imports",
            Self::NativeProcess => "native-process",
        }
    }
}

/// A serializable, fail-closed profile for the actual Bun Adapter surface.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BunExecutionTargetCapabilityProfile {
    pub profile: &'static str,
    pub target_profile: &'static str,
    pub capabilities: Vec<BunExecutionTargetCapability>,
}

impl BunExecutionTargetCapabilityProfile {
    /// Returns whether the Adapter explicitly supports one feature.
    pub fn supports(&self, capability: BunExecutionTargetCapability) -> bool {
        self.capabilities.contains(&capability)
    }

    /// Validates the shared profile wire invariants without granting a fallback.
    pub fn is_valid(&self) -> bool {
        self.profile == EXECUTION_TARGET_CAPABILITY_PROFILE_CONTRACT
            && self
                .capabilities
                .windows(2)
                .all(|pair| pair[0].as_str() < pair[1].as_str())
    }
}

/// Returns the current Bun Authoring V2 capability profile.
///
/// WebSocket, Wasm component, remote, browser, and Workers are deliberately
/// absent: they have no corresponding Bun Adapter implementation today.
pub fn bun_authoring_target_capability_profile() -> BunExecutionTargetCapabilityProfile {
    BunExecutionTargetCapabilityProfile {
        profile: EXECUTION_TARGET_CAPABILITY_PROFILE_CONTRACT,
        target_profile: BUN_AUTHORING_RUNTIME_PROFILE,
        capabilities: vec![
            BunExecutionTargetCapability::Event,
            BunExecutionTargetCapability::HostImports,
            BunExecutionTargetCapability::NativeProcess,
            BunExecutionTargetCapability::Request,
            BunExecutionTargetCapability::Stream,
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bun_authoring_v2_declares_only_implemented_features() {
        let profile = bun_authoring_target_capability_profile();
        assert_eq!(
            profile.profile,
            EXECUTION_TARGET_CAPABILITY_PROFILE_CONTRACT
        );
        assert_eq!(profile.target_profile, BUN_AUTHORING_RUNTIME_PROFILE);
        assert!(profile.is_valid());
        assert!(profile.supports(BunExecutionTargetCapability::Request));
        assert!(profile.supports(BunExecutionTargetCapability::Stream));
        assert!(profile.supports(BunExecutionTargetCapability::Event));
        assert!(profile.supports(BunExecutionTargetCapability::HostImports));
        assert!(profile.supports(BunExecutionTargetCapability::NativeProcess));
        assert_eq!(
            serde_json::to_value(profile).unwrap(),
            serde_json::json!({
                "profile": "lenso.execution-target-capability-profile@1",
                "target_profile": "lenso.bun-authoring@2",
                "capabilities": [
                    "event", "host-imports", "native-process", "request", "stream"
                ],
            })
        );
    }
}
