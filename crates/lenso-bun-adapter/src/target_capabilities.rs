//! Explicit capability evidence for the Bun Authoring V2 execution target.
//!
//! The list is kept next to the Adapter rather than inferred by a Host from a
//! runtime-profile string. `adapter_v2` owns request, stream, Event, and host
//! import bridges; `authoring_v2` owns the supervised Bun child-process
//! lifecycle. Features absent here are intentionally unsupported.

pub use lenso_process_protocol::{
    EXECUTION_TARGET_CAPABILITY_PROFILE as EXECUTION_TARGET_CAPABILITY_PROFILE_CONTRACT,
    ExecutionTargetCapability as BunExecutionTargetCapability,
    ExecutionTargetCapabilityProfile as BunExecutionTargetCapabilityProfile,
};

use crate::BUN_AUTHORING_RUNTIME_PROFILE;

/// Returns the current Bun Authoring V2 capability profile.
///
/// WebSocket, Wasm component, remote, browser, and Workers are deliberately
/// absent: they have no corresponding Bun Adapter implementation today.
pub fn bun_authoring_target_capability_profile() -> BunExecutionTargetCapabilityProfile {
    BunExecutionTargetCapabilityProfile {
        profile: EXECUTION_TARGET_CAPABILITY_PROFILE_CONTRACT.to_owned(),
        target_profile: BUN_AUTHORING_RUNTIME_PROFILE.to_owned(),
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
        assert!(profile.validate().is_ok());
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
