//! Public Adapter validation and generation isolation with reused Plan topology.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use lenso_app_plan::{
    CapabilityRequirementPlan, PlanResolutionError, PluginInstancePlan, ResolvedAppPlan,
    TerminalPolicy,
};
use lenso_kernel::{
    DeactivateContext, DeterministicDriver, Kernel, NativeExecutionAdapter, PluginFuture,
    PluginLifecycle, PrepareContext, RuntimeFailure, ShutdownOutcome,
};
use lenso_runtime_conformance::{
    ConformanceExecutionAdapter, ConformancePlugin, ConformancePluginFactory,
};

#[derive(Debug)]
struct Generation(Rc<Cell<usize>>);

impl PluginLifecycle for Generation {
    fn prepare(&self, _context: PrepareContext) -> PluginFuture {
        assert_eq!(
            self.0.replace(1),
            0,
            "each startup needs fresh Plugin state"
        );
        Box::pin(async { Ok(()) })
    }

    fn deactivate(&self, _context: DeactivateContext) -> PluginFuture {
        self.0.set(2);
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone, Debug, Default)]
struct Factory(Rc<RefCell<Vec<Rc<Cell<usize>>>>>);

impl ConformancePluginFactory for Factory {
    fn package_id(&self) -> &'static str {
        "test.plugin"
    }

    fn instantiate(
        &self,
        _instance: &PluginInstancePlan,
    ) -> Result<ConformancePlugin, RuntimeFailure> {
        let state = Rc::new(Cell::new(0));
        self.0.borrow_mut().push(state.clone());
        Ok(ConformancePlugin::with_lifecycle(vec![], Generation(state)))
    }
}

#[test]
fn direct_adapter_and_kernel_reject_invalid_snapshots_before_factory_execution() {
    let factory = Factory::default();
    let valid = ResolvedAppPlan::new(
        vec![PluginInstancePlan::new("plugin", "test.plugin")],
        vec![],
    );
    valid.validate().unwrap();
    let cases = [
        (
            ResolvedAppPlan::with_schema_version(0),
            PlanResolutionError::UnsupportedSchemaVersion {
                expected: lenso_app_plan::PLAN_SCHEMA_VERSION,
                actual: 0,
            },
        ),
        (
            ResolvedAppPlan::new(
                vec![
                    PluginInstancePlan::new("plugin", "test.plugin").with_requirement(
                        CapabilityRequirementPlan::one("test.missing@1", "1.0.0"),
                    ),
                ],
                vec![],
            ),
            PlanResolutionError::MissingOneBinding {
                consumer_instance: "plugin".into(),
                capability_id: "test.missing@1".into(),
            },
        ),
        (
            valid.with_terminal_policy(TerminalPolicy::HostEssential {
                roots: vec!["absent".into()],
                closure: vec![],
            }),
            PlanResolutionError::InvalidTerminalPolicy {
                detail: "unknown Plugin Instance `absent`".into(),
            },
        ),
    ];
    for (plan, expected) in cases {
        let adapter = ConformanceExecutionAdapter::new().with_factory(factory.clone());
        // Call the public Adapter first with untrusted input, then its cached error.
        for _ in 0..2 {
            assert_eq!(
                adapter.prepare(&plan).unwrap_err(),
                RuntimeFailure::InvalidResolvedPlan {
                    detail: expected.to_string()
                }
            );
        }
        let driver = DeterministicDriver::new();
        assert_eq!(
            driver
                .run(Kernel::start_native(plan, driver.clone(), adapter))
                .unwrap_err(),
            RuntimeFailure::InvalidResolvedPlan {
                detail: expected.to_string()
            }
        );
    }
    assert!(factory.0.borrow().is_empty());
}

#[test]
fn checked_plan_reuse_still_prepares_fresh_generations_and_lifecycle_state() {
    let plan = ResolvedAppPlan::new(
        vec![PluginInstancePlan::new("plugin", "test.plugin")],
        vec![],
    );
    plan.validate().unwrap();
    let factory = Factory::default();
    let adapter = || ConformanceExecutionAdapter::new().with_factory(factory.clone());
    let first_prepared = adapter().prepare(&plan).unwrap();
    let second_prepared = adapter().prepare(&plan).unwrap();
    assert_eq!(factory.0.borrow().len(), 2);
    assert!(!Rc::ptr_eq(&factory.0.borrow()[0], &factory.0.borrow()[1]));
    drop((first_prepared, second_prepared));

    let driver = DeterministicDriver::new();
    let first = driver
        .run(Kernel::start_native(
            plan.clone(),
            driver.clone(),
            adapter(),
        ))
        .unwrap();
    let second = driver
        .run(Kernel::start_native(plan, driver.clone(), adapter()))
        .unwrap();
    assert!(first.is_ready() && second.is_ready());
    assert_eq!(factory.0.borrow().len(), 4);
    assert_eq!(
        driver.run(first.shutdown(Duration::from_secs(1))),
        ShutdownOutcome::Clean
    );
    assert_eq!(factory.0.borrow()[2].get(), 2);
    assert_eq!(factory.0.borrow()[3].get(), 1);
    assert_eq!(
        driver.run(second.shutdown(Duration::from_secs(1))),
        ShutdownOutcome::Clean
    );
    assert_eq!(factory.0.borrow()[3].get(), 2);
}
