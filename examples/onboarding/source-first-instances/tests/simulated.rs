//! A thin deterministic platform entry consuming the normal CLI's exact App.
//! No routes, Instances, configurations or dependency bindings are assembled here.
use lenso_kernel::{CancellationToken, DeterministicDriver, Kernel, ShutdownOutcome};
use lenso_native_adapter::NativePluginRegistry;
use lenso_web_ingress_plugin::WebIngressEventFactory;
use std::{path::PathBuf, time::Duration};

#[test]
#[ignore = "requires LENSO_SOURCE_INSTANCE_DISTRIBUTION from a normal CLI App build"]
fn the_same_resolved_source_app_runs_with_the_simulated_driver() {
    let root = PathBuf::from(std::env::var_os("LENSO_SOURCE_INSTANCE_DISTRIBUTION").unwrap());
    let resolution = lenso_app_authoring::resolve_runtime_app(
        &root.join("intent"),
        &root.join(".lenso/host-build.json"),
    )
    .unwrap();
    let plan: lenso_app_plan::ResolvedAppPlan =
        serde_json::from_value(serde_json::to_value(resolution).unwrap()["plan"].clone()).unwrap();
    assert_eq!(plan.plugin_instances().len(), 4);
    assert_eq!(plan.capability_bindings().len(), 3);
    lenso_source_first_instances::pair::link_plugin();
    lenso_source_first_instances::label::link_plugin();
    // The same event Ingress used by Workers replaces only the socket entry.
    let ingress = WebIngressEventFactory::new();
    let registry = NativePluginRegistry::new()
        .with_linked_factories()
        .with_factory(ingress.clone());
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(plan, driver.clone(), registry))
        .unwrap();
    for expected in [
        ["left:1:hello", "right:1:hello"],
        ["left:2:hello", "right:2:hello"],
    ] {
        let request = http::Request::builder()
            .uri("/instances")
            .body(bytes::Bytes::new())
            .unwrap();
        let response = driver
            .run(ingress.handle(request, CancellationToken::new()))
            .unwrap();
        assert_eq!(response.status(), http::StatusCode::OK);
        let values: Vec<String> = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(values, expected);
    }
    assert_eq!(
        driver.run(app.shutdown(Duration::from_secs(1))),
        ShutdownOutcome::Clean
    );
}
