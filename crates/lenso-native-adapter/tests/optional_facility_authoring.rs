use std::{cell::RefCell, rc::Rc, time::Duration};

use lenso_app_plan::{PluginInstancePlan, ResolvedAppPlan};
use lenso_kernel::{DeterministicDriver, Kernel, RuntimeFailure, ShutdownOutcome};
use lenso_native_adapter::{
    ConfiguredPluginFactory, NativeFacilities, NativeInstanceFacilities, NativePluginDefinition,
    NativePluginRegistry,
};

#[derive(Clone, Debug)]
struct StateHandle(u64);

mod legacy {
    use super::StateHandle;

    #[lenso_native_adapter::plugin(consumer, lifecycle)]
    #[derive(Clone, Debug)]
    pub struct Plugin {
        #[facility(id = "state")]
        pub state: Option<StateHandle>,
    }

    impl lenso_native_adapter::Lifecycle for Plugin {}
}

mod complete {
    use super::StateHandle;

    #[lenso_native_adapter::plugin(consumer)]
    #[derive(Debug)]
    pub struct Plugin {
        #[facility(id = "state")]
        pub state: Option<StateHandle>,
    }

    #[lenso_native_adapter::plugin_impl]
    impl Plugin {
        #[create]
        fn create(state: Option<StateHandle>) -> Self {
            Self { state }
        }
    }
}

mod mandatory {
    use super::StateHandle;

    #[lenso_native_adapter::plugin(consumer)]
    #[derive(Debug)]
    pub struct Plugin {
        #[facility(id = "state")]
        pub state: StateHandle,
    }

    #[lenso_native_adapter::plugin_impl]
    impl Plugin {
        #[create]
        fn create(state: StateHandle) -> Self {
            Self { state }
        }
    }
}

fn construct<P: NativePluginDefinition>(
    authoring: u32,
    facilities: NativeFacilities,
    inspect: fn(&P) -> Option<u64>,
) -> Result<Vec<Option<u64>>, RuntimeFailure> {
    let observed = Rc::new(RefCell::new(Vec::new()));
    let values = observed.clone();
    let registry = NativePluginRegistry::new()
        .with_factory(ConfiguredPluginFactory::<P, _>::new(move |plugin| {
            values.borrow_mut().push(inspect(plugin));
            Ok(())
        }))
        .with_facilities(
            NativeInstanceFacilities::new()
                .with("selected", facilities)
                .unwrap(),
        );
    let plan = ResolvedAppPlan::new(
        vec![
            PluginInstancePlan::new("selected", P::PACKAGE_ID)
                .with_authoring(authoring, P::RUNTIME_PROFILE),
        ],
        vec![],
    );
    let driver = DeterministicDriver::new();
    let app = driver.run(Kernel::start_native(plan, driver.clone(), registry))?;
    assert_eq!(
        driver.run(app.shutdown(Duration::from_secs(1))),
        ShutdownOutcome::Clean
    );
    let result = observed.borrow().clone();
    Ok(result)
}

fn check_optional<P: NativePluginDefinition>(authoring: u32, inspect: fn(&P) -> Option<u64>) {
    assert_eq!(
        construct::<P>(authoring, NativeFacilities::new(), inspect).unwrap(),
        vec![None]
    );
    assert_eq!(
        construct::<P>(
            authoring,
            NativeFacilities::new()
                .with("state", StateHandle(7))
                .unwrap(),
            inspect,
        )
        .unwrap(),
        vec![Some(7)]
    );
    assert!(
        construct::<P>(
            authoring,
            NativeFacilities::new()
                .with("state", "wrong type".to_owned())
                .unwrap(),
            inspect,
        )
        .is_err()
    );
    assert!(
        construct::<P>(
            authoring,
            NativeFacilities::new()
                .with_factory::<StateHandle>("state", || {
                    Err(RuntimeFailure::PluginFailure {
                        detail: "owner factory unavailable".into(),
                    })
                })
                .unwrap(),
            inspect,
        )
        .is_err()
    );
}

#[test]
fn legacy_constructor_preserves_optional_absence_and_rejects_invalid_present_inputs() {
    check_optional::<legacy::Plugin>(1, |plugin| plugin.state.as_ref().map(|state| state.0));
}

#[test]
fn complete_constructor_preserves_optional_absence_and_rejects_invalid_present_inputs() {
    check_optional::<complete::Plugin>(2, |plugin| plugin.state.as_ref().map(|state| state.0));
}

#[test]
fn mandatory_complete_constructor_still_requires_the_exact_typed_attachment() {
    assert!(
        construct::<mandatory::Plugin>(2, NativeFacilities::new(), |plugin| Some(plugin.state.0))
            .is_err()
    );
    assert_eq!(
        construct::<mandatory::Plugin>(
            2,
            NativeFacilities::new()
                .with("state", StateHandle(9))
                .unwrap(),
            |plugin| Some(plugin.state.0)
        )
        .unwrap(),
        vec![Some(9)]
    );
}
