use lenso_native_adapter::{NativePluginDefinition, NativePluginRegistry};

mod first {
    #[lenso_native_adapter::plugin(id = "example.first", root_slot = "tools", consumer)]
    #[derive(Debug)]
    pub struct Plugin {}
}
mod second {
    #[lenso_native_adapter::plugin(id = "example.second", root_slot = "tools", consumer)]
    #[derive(Debug)]
    pub struct Plugin {}
}

#[test]
fn module_plugins_link_independently_without_identity_or_symbol_collisions() {
    first::Plugin::link();
    second::Plugin::link();
    first::Plugin::link();
    assert_eq!(first::Plugin::PACKAGE_ID, "example.first");
    assert_eq!(second::Plugin::PACKAGE_ID, "example.second");
    let registry = NativePluginRegistry::new().with_linked_factories();
    assert_eq!(
        registry
            .factories()
            .filter(|f| f.package_id().starts_with("example."))
            .count(),
        2
    );
    let first: serde_json::Value = serde_json::from_str(first::PLUGIN_DESCRIPTOR_JSON).unwrap();
    let second: serde_json::Value = serde_json::from_str(second::PLUGIN_DESCRIPTOR_JSON).unwrap();
    assert_eq!(first["plugin_id"], "example.first");
    assert_eq!(second["plugin_id"], "example.second");
    assert_eq!(first["authoring_version"], 2);
}

#[test]
fn kernel_instantiates_only_selected_identity_and_rejects_ambiguous_factories() {
    use lenso_app_plan::{PluginInstancePlan, ResolvedAppPlan};
    use lenso_kernel::{DeterministicDriver, Kernel, ShutdownOutcome};
    use lenso_native_adapter::ConfiguredPluginFactory;
    let plan = || {
        ResolvedAppPlan::new(
            vec![
                PluginInstancePlan::new("selected", first::Plugin::PACKAGE_ID)
                    .with_authoring(2, first::Plugin::RUNTIME_PROFILE),
            ],
            vec![],
        )
    };
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(
            plan(),
            driver.clone(),
            NativePluginRegistry::new().with_linked_factories(),
        ))
        .unwrap();
    assert_eq!(
        driver.run(app.shutdown(std::time::Duration::from_secs(1))),
        ShutdownOutcome::Clean
    );
    let duplicate = NativePluginRegistry::new()
        .with_factory(ConfiguredPluginFactory::<first::Plugin, _>::new(|_| Ok(())))
        .with_factory(ConfiguredPluginFactory::<first::Plugin, _>::new(|_| Ok(())));
    let error = driver
        .run(Kernel::start_native(plan(), driver.clone(), duplicate))
        .err()
        .expect("duplicate identity must fail");
    assert!(format!("{error:?}").contains("multiple statically linked factories"));
}
