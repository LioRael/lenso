use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use lenso_app_plan::{PluginInstancePlan, ResolvedAppPlan};
use lenso_kernel::NativeExecutionAdapter;
use lenso_native_adapter::{
    ConfiguredPluginFactory, Lifecycle, NativeFacilities, NativeInstanceFacilities,
    NativePluginRegistry, plugin,
};

#[derive(Clone, Debug)]
struct StateHandle(u64);

#[plugin(consumer, lifecycle)]
#[derive(Clone, Debug)]
struct FacilityConsumer {
    #[facility(id = "state")]
    state: StateHandle,
}

impl Lifecycle for FacilityConsumer {}

#[test]
fn typed_facility_is_private_and_recreated_from_its_owner_factory() {
    let descriptor: serde_json::Value = serde_json::from_str(PLUGIN_DESCRIPTOR_JSON).unwrap();
    assert_eq!(descriptor["required_capabilities"], serde_json::json!([]));
    let created = Rc::new(Cell::new(0));
    let count = created.clone();
    let facilities = NativeInstanceFacilities::new()
        .with(
            "primary",
            NativeFacilities::new()
                .with_factory("state", move || {
                    count.set(count.get() + 1);
                    Ok(StateHandle(count.get()))
                })
                .unwrap(),
        )
        .unwrap();
    let values = Rc::new(RefCell::new(Vec::new()));
    let observed = values.clone();
    let registry = NativePluginRegistry::new()
        .with_factory_override(ConfiguredPluginFactory::<FacilityConsumer, _>::new(
            move |plugin| {
                observed.borrow_mut().push(plugin.state.0);
                Ok(())
            },
        ))
        .unwrap()
        .with_linked_factories()
        .with_facilities(facilities);
    let plan = ResolvedAppPlan::new(vec![PluginInstancePlan::new("primary", PACKAGE_ID)], vec![]);
    registry.prepare(&plan).unwrap();
    registry.recreate(&plan, "primary").unwrap();
    assert_eq!(*values.borrow(), vec![1, 2]);
}

#[test]
fn missing_or_cross_instance_facility_fails_before_host_initialization() {
    let initialized = Rc::new(Cell::new(false));
    let observed = initialized.clone();
    let registry = NativePluginRegistry::new()
        .with_factory_override(ConfiguredPluginFactory::<FacilityConsumer, _>::new(
            move |_| {
                observed.set(true);
                Ok(())
            },
        ))
        .unwrap()
        .with_linked_factories()
        .with_facilities(
            NativeInstanceFacilities::new()
                .with(
                    "secondary",
                    NativeFacilities::new()
                        .with("state", StateHandle(7))
                        .unwrap(),
                )
                .unwrap(),
        );
    let plan = ResolvedAppPlan::new(
        vec![
            PluginInstancePlan::new("primary", PACKAGE_ID),
            PluginInstancePlan::new("secondary", PACKAGE_ID),
        ],
        vec![],
    );
    assert!(registry.prepare(&plan).is_err());
    assert!(!initialized.get());
}
