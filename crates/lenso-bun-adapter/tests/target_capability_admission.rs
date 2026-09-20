//! Regression coverage for the real Bun Adapter profile at the Host-selection
//! boundary. This constructs a Bundle manifest and asks the Bundle resolver to
//! admit it; it deliberately does not claim that a hand-built `ResolvedAppPlan`
//! exercised implementation selection.

use lenso_app_plan::{
    CapabilityEndpointPlan, ExecutionClassId, ExecutionTargetCapability,
    authoring::{PluginContract, PluginImplementation},
};
use lenso_bun_adapter::{
    BUN_AUTHORING_RUNTIME_PROFILE, BunExecutionTargetCapability,
    bun_authoring_target_capability_profile,
};
use lenso_plugin_bundle::{
    ExecutionTargetCapabilities, ImplementationPolicy, ImplementationRejectionReason,
    PluginArtifactV2, PluginImplementationV4, PluginManifest, PluginManifestV4,
    RejectedPluginImplementation, RuntimeAdmission, TargetCapabilityRequirement,
    explain_implementation, resolve_implementation,
};

const PLUGIN_ID: &str = "example.bun-target";
const CAPABILITY_ID: &str = "example.bun-target@1";
const ARTIFACT_DIGEST: &str =
    "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn bun_manifest(
    required_target_capabilities: impl IntoIterator<Item = ExecutionTargetCapability>,
) -> PluginManifest {
    PluginManifest::V4(PluginManifestV4 {
        schema_version: 4,
        contract: PluginContract::new(PLUGIN_ID, "1.0.0", "tools")
            .with_authoring_version(2)
            .with_capability(
                CapabilityEndpointPlan::new(CAPABILITY_ID, "1.0.0", ["echo", "chat", "notify"])
                    .with_stream_operation("chat")
                    .with_event_operation("notify"),
            ),
        implementations: vec![PluginImplementationV4 {
            id: "bun".to_owned(),
            host_targets: vec!["*".to_owned()],
            artifact: PluginArtifactV2 {
                path: "plugin.js".to_owned(),
                digest: ARTIFACT_DIGEST.to_owned(),
                size: 0,
                media_type: "application/javascript".to_owned(),
                target: "javascript-bun".to_owned(),
            },
            runtime: PluginImplementation::new(
                PLUGIN_ID,
                ARTIFACT_DIGEST,
                "plugin.js",
                ExecutionClassId::new("lenso.bun-process@1"),
            )
            .with_runtime_profile(BUN_AUTHORING_RUNTIME_PROFILE)
            .with_required_target_capabilities(required_target_capabilities),
        }],
    })
}

fn bun_policy() -> ImplementationPolicy {
    let profile = bun_authoring_target_capability_profile();
    ImplementationPolicy {
        host_target: "test-host".to_owned(),
        runtimes: vec![RuntimeAdmission::new(
            ExecutionClassId::new("lenso.bun-process@1"),
            BUN_AUTHORING_RUNTIME_PROFILE,
            ExecutionTargetCapabilities::new(profile.capabilities),
        )],
    }
}

#[test]
fn actual_bun_profile_admits_its_supported_contract_and_rejects_workers() {
    let profile = bun_authoring_target_capability_profile();
    assert!(profile.validate().is_ok());
    for capability in [
        BunExecutionTargetCapability::Request,
        BunExecutionTargetCapability::Stream,
        BunExecutionTargetCapability::Event,
        BunExecutionTargetCapability::HostImports,
        BunExecutionTargetCapability::NativeProcess,
    ] {
        assert!(profile.supports(capability));
    }
    assert!(!profile.supports(BunExecutionTargetCapability::Workers));

    let policy = bun_policy();
    let admitted = bun_manifest([
        // This is the minimum pair Engine must be able to admit together for
        // a real Bun child: a request route and its owned child lifecycle.
        ExecutionTargetCapability::Request,
        ExecutionTargetCapability::HostImports,
        ExecutionTargetCapability::NativeProcess,
    ]);
    let selected = resolve_implementation(&admitted, &policy)
        .expect("the Host must admit only the complete, actual Bun target profile");
    assert_eq!(selected.implementation_id, "bun");
    assert_eq!(
        selected.descriptor.runtime_profile(),
        BUN_AUTHORING_RUNTIME_PROFILE
    );

    let workers_only = bun_manifest([
        ExecutionTargetCapability::Request,
        ExecutionTargetCapability::HostImports,
        ExecutionTargetCapability::NativeProcess,
        ExecutionTargetCapability::Workers,
    ]);
    let explanation = explain_implementation(&workers_only, &policy)
        .expect("the resolver must explain a target mismatch without creating a Plan");
    assert!(matches!(
        explanation.rejected.as_slice(),
        [RejectedPluginImplementation {
            implementation_id,
            reason: ImplementationRejectionReason::MissingTargetCapabilities { requirements },
            ..
        }] if implementation_id == "bun"
            && requirements == &vec![TargetCapabilityRequirement {
                capability_id: PLUGIN_ID.to_owned(),
                operation: "<implementation>".to_owned(),
                feature: BunExecutionTargetCapability::Workers,
            }]
    ));
    assert!(
        resolve_implementation(&workers_only, &policy)
            .expect_err("Bun must not be selected for an explicit Workers requirement")
            .to_string()
            .contains("workers")
    );
}
