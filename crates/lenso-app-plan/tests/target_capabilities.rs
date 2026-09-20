use lenso_app_plan::{
    ExecutionClassId, ExecutionTargetCapability,
    authoring::{
        HostCatalog, HostDefaultPlugin, HostPluginRelease, HostSlot, PluginContract,
        PluginImplementation, PluginRootSnapshot, resolve_plugin_root,
    },
};
use serde_json::json;

fn implementation() -> PluginImplementation {
    PluginImplementation::new(
        "example.target",
        "sha256:implementation",
        "plugin.js",
        ExecutionClassId::new("example.adapter@1"),
    )
    .with_runtime_profile("example.authoring@2")
    .with_required_target_capabilities([
        ExecutionTargetCapability::Workers,
        ExecutionTargetCapability::WebSocket,
        ExecutionTargetCapability::Browser,
        ExecutionTargetCapability::NativeProcess,
        ExecutionTargetCapability::HostImports,
        ExecutionTargetCapability::Remote,
        ExecutionTargetCapability::WasmComponent,
        ExecutionTargetCapability::Event,
        ExecutionTargetCapability::Stream,
        ExecutionTargetCapability::Request,
        ExecutionTargetCapability::Workers,
    ])
}

#[test]
fn implementation_descriptor_and_plan_preserve_canonical_target_requirements() {
    let implementation = implementation();
    assert_eq!(
        implementation.required_target_capabilities(),
        ExecutionTargetCapability::ALL.as_slice()
    );

    let contract =
        PluginContract::new("example.target", "1.0.0", "tools").with_authoring_version(2);
    let descriptor = contract.resolve(&implementation);
    assert_eq!(
        descriptor.required_target_capabilities(),
        ExecutionTargetCapability::ALL
    );
    assert_eq!(descriptor.implementation(), implementation);

    let host = HostCatalog::new(
        [HostSlot::one("tools")],
        [HostPluginRelease::new(descriptor)],
        [HostDefaultPlugin::new("example.target", "default")],
    );
    let resolved = resolve_plugin_root(&host, &PluginRootSnapshot::default()).unwrap();
    let instance = resolved
        .plan()
        .plugin_instance("example.target/default")
        .unwrap();
    assert_eq!(
        instance.required_target_capabilities(),
        ExecutionTargetCapability::ALL
    );

    let wire = serde_json::to_value(resolved.plan()).unwrap();
    assert_eq!(wire["schema_version"], json!(4));
    assert_eq!(
        wire["plugin_instances"][0]["required_target_capabilities"],
        json!([
            "browser",
            "event",
            "host-imports",
            "native-process",
            "remote",
            "request",
            "stream",
            "wasm-component",
            "websocket",
            "workers",
        ])
    );
    assert_eq!(
        serde_json::from_value::<lenso_app_plan::ResolvedAppPlan>(wire).unwrap(),
        *resolved.plan()
    );
}

#[test]
fn v2_and_v3_plans_default_target_requirements_to_empty() {
    let implementation = implementation();
    let contract =
        PluginContract::new("example.target", "1.0.0", "tools").with_authoring_version(2);
    let descriptor = contract.resolve(&implementation);
    let host = HostCatalog::new(
        [HostSlot::one("tools")],
        [HostPluginRelease::new(descriptor)],
        [HostDefaultPlugin::new("example.target", "default")],
    );
    let resolved = resolve_plugin_root(&host, &PluginRootSnapshot::default()).unwrap();

    let mut v3 = serde_json::to_value(resolved.plan()).unwrap();
    v3["schema_version"] = json!(3);
    v3["plugin_instances"][0]
        .as_object_mut()
        .unwrap()
        .remove("required_target_capabilities");
    let v3 = serde_json::from_value::<lenso_app_plan::ResolvedAppPlan>(v3).unwrap();
    assert!(
        v3.plugin_instances()[0]
            .required_target_capabilities()
            .is_empty()
    );
    assert_eq!(v3.schema_version(), 4);

    let mut v2 = serde_json::to_value(resolved.plan()).unwrap();
    v2["schema_version"] = json!(2);
    v2.as_object_mut().unwrap().remove("terminal_policy");
    for instance in v2["plugin_instances"].as_array_mut().unwrap() {
        let instance = instance.as_object_mut().unwrap();
        instance.remove("required_target_capabilities");
        instance.remove("authoring_version");
        instance.remove("runtime_profile");
    }
    let v2 = serde_json::from_value::<lenso_app_plan::ResolvedAppPlan>(v2).unwrap();
    assert!(
        v2.plugin_instances()[0]
            .required_target_capabilities()
            .is_empty()
    );
    assert_eq!(v2.schema_version(), 4);
}

#[test]
fn unknown_target_requirement_is_rejected_before_plan_construction() {
    let implementation = json!({
        "runtime_package_id": "example.target",
        "runtime_package_revision": "sha256:implementation",
        "entrypoint": "plugin.js",
        "execution_class": "example.adapter@1",
        "required_target_capabilities": ["request", "future-host-feature"],
    });
    assert!(serde_json::from_value::<PluginImplementation>(implementation).is_err());
}
