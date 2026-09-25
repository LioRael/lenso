use super::*;
use lenso_app_authoring::host_authoring::{GeneratedHostBuild, HostPluginInput};
use lenso_app_plan::{
    CapabilityEndpointPlan, CapabilityRequirementPlan, ExecutionClassId,
    authoring::{
        HostCatalog, HostDefaultPlugin, HostPluginRelease, HostSlot, PluginContract,
        PluginDescriptor, PluginImplementation, PluginRootSnapshot, resolve_plugin_root,
    },
};
use lenso_plugin_bundle::ExecutionTargetCapabilities;
use serde_json::json;

fn descriptors(kind: CapabilityOperationKind) -> Vec<PluginDescriptor> {
    let revision = format!("sha256:{}", "a".repeat(64));
    let descriptor = |contract: PluginContract, class: &str, profile: &str| {
        let id = contract.plugin_id().to_owned();
        contract.resolve(
            &PluginImplementation::new(id, &revision, "plugin", ExecutionClassId::new(class))
                .with_runtime_profile(profile),
        )
    };
    vec![
        descriptor(
            PluginContract::new("example.root", "1.0.0", "tools")
                .with_authoring_version(2)
                .with_requirement(
                    CapabilityRequirementPlan::one("example.middle@1", "1")
                        .with_requirement_id("middle"),
                ),
            "lenso.bun-process@1",
            "lenso.bun-authoring@2",
        ),
        descriptor(
            PluginContract::new("example.middle", "1.0.0", "tools")
                .with_authoring_version(2)
                .with_capability(CapabilityEndpointPlan::new(
                    "example.middle@1",
                    "1",
                    ["get"],
                ))
                .with_requirement(
                    CapabilityRequirementPlan::one("example.leaf@1", "1")
                        .with_requirement_id("leaf"),
                ),
            "lenso.process@1",
            "lenso.process-stdio@2",
        ),
        descriptor(
            PluginContract::new("example.leaf", "1.0.0", "tools")
                .with_authoring_version(2)
                .with_capability(
                    CapabilityEndpointPlan::new("example.leaf@1", "1", ["open"])
                        .with_operation_kind("open", kind),
                ),
            "lenso.native-rust@1",
            "lenso.native-rust@1",
        ),
    ]
}

fn inventory() -> Vec<Value> {
    let artifact_digest = format!("sha256:{}", "a".repeat(64));
    let profile = |runtime: &str| {
        ExecutionTargetCapabilities::new([
            ExecutionTargetCapability::NativeProcess,
            ExecutionTargetCapability::Request,
        ])
        .profile_for(runtime)
    };
    vec![
        json!({
            "plugin_id":"example.root", "execution_class":"lenso.bun-process@1",
            "artifact_digest":artifact_digest,
            "runtime_profile":"lenso.bun-authoring@2",
            "target_capability_profile":profile("lenso.bun-authoring@2"),
        }),
        json!({
            "plugin_id":"example.middle", "execution_class":"lenso.process@1",
            "artifact_digest":artifact_digest,
            "runtime_profile":"lenso.process-stdio@2",
            "target_capability_profile":profile("lenso.process-stdio@2"),
        }),
    ]
}

fn resolved(kind: CapabilityOperationKind) -> ResolvedApp {
    let descriptors = descriptors(kind);
    let host = HostCatalog::new(
        [HostSlot::many("tools")],
        descriptors.into_iter().map(HostPluginRelease::new),
        [
            HostDefaultPlugin::new("example.root", "default"),
            HostDefaultPlugin::new("example.middle", "default"),
            HostDefaultPlugin::new("example.leaf", "default"),
        ],
    );
    resolve_plugin_root(&host, &PluginRootSnapshot::default()).unwrap()
}

#[test]
fn transitive_stream_and_event_requirements_reject_the_consumer_target_with_path() {
    for (kind, feature) in [
        (CapabilityOperationKind::Stream, "stream"),
        (CapabilityOperationKind::Event, "event"),
    ] {
        let error = admit(&resolved(kind), &inventory()).unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("example.root/default --middle--> example.middle/default --leaf--> example.leaf/default"), "{message}");
        assert!(
            message.contains(&format!("does not admit `{feature}`")),
            "{message}"
        );
        assert!(message.contains("example.leaf@1.open"), "{message}");
        assert!(
            message.contains("no alternate provider or runtime was selected"),
            "{message}"
        );
    }
}

#[test]
fn request_only_dependency_closure_is_admitted() {
    admit(&resolved(CapabilityOperationKind::Request), &inventory()).unwrap();
}

#[test]
fn unselected_variant_of_same_plugin_does_not_replace_selected_target_profile() {
    let mut inventory = inventory();
    inventory.push(json!({
        "plugin_id":"example.middle",
        "artifact_digest":format!("sha256:{}", "b".repeat(64)),
        "execution_class":"lenso.wasm-component@1",
        "runtime_profile":lenso_wasm_component_adapter::RUNTIME_PROFILE,
        "target_capability_profile":ExecutionTargetCapabilities::new([
            ExecutionTargetCapability::Request,
            ExecutionTargetCapability::WasmComponent,
        ]).profile_for(lenso_wasm_component_adapter::RUNTIME_PROFILE),
    }));
    admit(&resolved(CapabilityOperationKind::Request), &inventory).unwrap();
    let error = admit(&resolved(CapabilityOperationKind::Stream), &inventory).unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("lenso.process-stdio@2"), "{message}");
    assert!(message.contains("does not admit `stream`"), "{message}");
}

#[test]
fn unselected_revision_cannot_stand_in_for_the_selected_artifact() {
    let mut inventory = inventory();
    inventory[1]["artifact_digest"] = json!(format!("sha256:{}", "b".repeat(64)));
    let error = admit(&resolved(CapabilityOperationKind::Request), &inventory).unwrap_err();
    assert!(
        format!("{error:#}")
            .contains("example.middle/default` has no target admission in Host inventory"),
        "{error:#}"
    );
}

#[test]
fn selected_portable_consumer_without_a_target_profile_fails_closed() {
    let mut inventory = inventory();
    inventory.retain(|record| record["plugin_id"] != "example.middle");
    let error = admit(&resolved(CapabilityOperationKind::Request), &inventory).unwrap_err();
    assert!(format!("{error:#}").contains("example.middle/default` has no target admission"));
}

#[test]
fn forged_stream_admission_cannot_make_the_dependency_closure_pass() {
    let mut inventory = inventory();
    inventory[1]["target_capability_profile"]["capabilities"] =
        json!(["native-process", "request", "stream"]);
    let error = admit(&resolved(CapabilityOperationKind::Stream), &inventory).unwrap_err();
    assert!(
        format!("{error:#}")
            .contains("target profile differs from concrete Host Adapter admission")
    );
}

#[test]
fn app_check_rechecks_the_generated_host_dependency_closure() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".lenso")).unwrap();
    let inputs = descriptors(CapabilityOperationKind::Stream)
        .into_iter()
        .map(|descriptor| HostPluginInput {
            descriptor,
            instance: "default".to_owned(),
            configuration: json!({}),
            source: "test".to_owned(),
        })
        .collect();
    let host =
        GeneratedHostBuild::lower("example.app", inputs, [HostSlot::many("tools")].into()).unwrap();
    std::fs::write(
        root.path().join(".lenso/host-build.json"),
        serde_json::to_vec(&host).unwrap(),
    )
    .unwrap();
    std::fs::write(
        root.path().join("bundles.json"),
        serde_json::to_vec(&inventory()).unwrap(),
    )
    .unwrap();

    let error = crate::app::inspect_app_check(root.path()).unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("example.root/default"), "{message}");
    assert!(message.contains("does not admit `stream`"), "{message}");
}
