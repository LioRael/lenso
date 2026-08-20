use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    time::Duration,
};

use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityEndpointPlan, CapabilityRequirementPlan,
    ModuleInstancePlan, ResolvedAppPlan,
};
use lenso_kernel::{
    ActivateContext, DeactivateContext, DeterministicDriver, Kernel, ManagedResource,
    ModuleLifecycle, NativeExecutionAdapter, PrepareContext, PreparedNativeApp, ResourceFuture,
    RuntimeDriver, RuntimeFailure, ShutdownOutcome,
};

#[derive(Clone, Debug, Eq, PartialEq)]
enum Event {
    Cancelled(String, bool),
    Deactivate(String),
    ResourceReleaseStarted(String),
    ResourceReleased(String),
}

#[derive(Clone, Copy, Debug)]
enum DeactivationMode {
    Clean,
    Failure,
    Blocked,
}

#[derive(Debug)]
struct RecordingResource {
    name: String,
    events: Rc<RefCell<Vec<Event>>>,
    fail: bool,
}

impl ManagedResource for RecordingResource {
    fn release(&self) -> ResourceFuture {
        self.events
            .borrow_mut()
            .push(Event::ResourceReleaseStarted(self.name.clone()));
        let name = self.name.clone();
        let events = self.events.clone();
        let fail = self.fail;
        Box::pin(async move {
            events.borrow_mut().push(Event::ResourceReleased(name));
            if fail {
                Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: "resource release failure".to_owned(),
                })
            } else {
                Ok(())
            }
        })
    }
}

#[derive(Debug)]
struct RecordingLifecycle {
    instance_key: String,
    events: Rc<RefCell<Vec<Event>>>,
    resources_released: Rc<Cell<usize>>,
    fail_prepare: bool,
    deactivation: DeactivationMode,
    resource_failure: bool,
}

impl ModuleLifecycle for RecordingLifecycle {
    fn prepare(&self, _context: PrepareContext) -> lenso_kernel::ModuleFuture {
        let fail = self.fail_prepare;
        Box::pin(async move {
            if fail {
                Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: "startup failure".to_owned(),
                })
            } else {
                Ok(())
            }
        })
    }

    fn activate(&self, context: ActivateContext) -> lenso_kernel::ModuleFuture {
        let cancellation = context.cancellation();
        let admission = context.admission();
        let events = self.events.clone();
        let instance_key = self.instance_key.clone();
        let resource_name = instance_key.clone();
        let resource_events = self.events.clone();
        context
            .resources()
            .register(RecordingResource {
                name: resource_name,
                events: resource_events,
                fail: self.resource_failure,
            })
            .expect("the generation should accept resources before shutdown");
        context
            .tasks()
            .spawn_local(Box::pin(async move {
                cancellation.cancelled().await;
                events
                    .borrow_mut()
                    .push(Event::Cancelled(instance_key, admission.is_closed()));
            }))
            .expect("the generation should accept tasks before shutdown");
        Box::pin(async { Ok(()) })
    }

    fn deactivate(&self, context: DeactivateContext) -> lenso_kernel::ModuleFuture {
        assert!(matches!(
            context.reason(),
            lenso_kernel::DeactivationReason::Shutdown
                | lenso_kernel::DeactivationReason::StartupRollback
        ));
        let events = self.events.clone();
        let instance_key = self.instance_key.clone();
        let resources_released = self.resources_released.clone();
        match self.deactivation {
            DeactivationMode::Clean => Box::pin(async move {
                events.borrow_mut().push(Event::Deactivate(instance_key));
                resources_released.set(resources_released.get() + 1);
                Ok(())
            }),
            DeactivationMode::Failure => Box::pin(async move {
                events.borrow_mut().push(Event::Deactivate(instance_key));
                Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: "deactivation failure".to_owned(),
                })
            }),
            DeactivationMode::Blocked => Box::pin(async move {
                let _ = (events, instance_key, resources_released);
                futures::future::pending::<()>().await;
                Ok(())
            }),
        }
    }
}

#[derive(Debug)]
struct RecordingAdapter {
    modules: BTreeMap<String, Rc<dyn ModuleLifecycle>>,
}

impl NativeExecutionAdapter for RecordingAdapter {
    fn prepare(&self, _plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure> {
        Ok(PreparedNativeApp::with_modules(
            Vec::new(),
            self.modules.clone(),
        ))
    }
}

#[derive(Debug)]
struct BlockedTaskLifecycle;

impl ModuleLifecycle for BlockedTaskLifecycle {
    fn activate(&self, context: ActivateContext) -> lenso_kernel::ModuleFuture {
        context
            .tasks()
            .spawn_local(Box::pin(futures::future::pending()))
            .expect("the generation should accept the blocked task before shutdown");
        Box::pin(async { Ok(()) })
    }
}

fn plan() -> ResolvedAppPlan {
    AppComposition::new(
        vec![
            ModuleInstancePlan::new("provider", "package.provider").with_capability(
                CapabilityEndpointPlan::new("capability.shutdown", "1.0.0", ["shutdown.call"]),
            ),
            ModuleInstancePlan::new("consumer", "package.consumer").with_requirement(
                CapabilityRequirementPlan::one("capability.shutdown", "1.0.0"),
            ),
        ],
        vec![CapabilityBinding::new(
            "consumer",
            "capability.shutdown",
            "1.0.0",
            "provider",
        )],
    )
    .resolve()
    .expect("the shutdown plan should resolve")
}

fn adapter(
    events: &Rc<RefCell<Vec<Event>>>,
    resources_released: &Rc<Cell<usize>>,
    deactivation: DeactivationMode,
    resource_failure: bool,
    fail_prepare: bool,
) -> RecordingAdapter {
    let modules = ["provider", "consumer"]
        .into_iter()
        .map(|instance_key| {
            (
                instance_key.to_owned(),
                Rc::new(RecordingLifecycle {
                    instance_key: instance_key.to_owned(),
                    events: events.clone(),
                    resources_released: resources_released.clone(),
                    fail_prepare: fail_prepare && instance_key == "provider",
                    deactivation,
                    resource_failure,
                }) as Rc<dyn ModuleLifecycle>,
            )
        })
        .collect();
    RecordingAdapter { modules }
}

#[test]
fn shutdown_cancels_managed_work_releases_resources_once_and_deactivates_in_reverse_order() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let resources_released = Rc::new(Cell::new(0));
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(
            plan(),
            driver.clone(),
            adapter(
                &events,
                &resources_released,
                DeactivationMode::Clean,
                false,
                false,
            ),
        ))
        .expect("the App should start");

    assert!(app.is_accepting());
    app.request_shutdown();
    assert!(!app.is_accepting());
    let outcome = driver.run(app.shutdown(Duration::from_secs(1)));

    assert_eq!(outcome, ShutdownOutcome::Clean);
    assert_eq!(
        driver.run(app.shutdown(Duration::from_secs(1))),
        ShutdownOutcome::Clean
    );
    assert_eq!(resources_released.get(), 2);
    assert_eq!(
        *events.borrow(),
        vec![
            Event::Cancelled("consumer".to_owned(), true),
            Event::Cancelled("provider".to_owned(), true),
            Event::Deactivate("consumer".to_owned()),
            Event::ResourceReleaseStarted("consumer".to_owned()),
            Event::ResourceReleased("consumer".to_owned()),
            Event::Deactivate("provider".to_owned()),
            Event::ResourceReleaseStarted("provider".to_owned()),
            Event::ResourceReleased("provider".to_owned()),
        ]
    );
}

#[test]
fn shutdown_reports_cleanup_failure_after_deactivating_and_releasing_every_instance() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let resources_released = Rc::new(Cell::new(0));
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(
            plan(),
            driver.clone(),
            adapter(
                &events,
                &resources_released,
                DeactivationMode::Failure,
                true,
                false,
            ),
        ))
        .expect("the App should start");

    let outcome = driver.run(app.shutdown(Duration::from_secs(1)));

    assert!(matches!(
        outcome,
        ShutdownOutcome::RuntimeFailure {
            error: RuntimeFailure::InvalidResolvedPlan { detail }
        } if detail == "deactivation failure"
    ));
    assert_eq!(resources_released.get(), 0);
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|event| matches!(event, Event::ResourceReleased(_)))
            .count(),
        2
    );
}

#[test]
fn shutdown_reports_a_managed_resource_release_failure() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let resources_released = Rc::new(Cell::new(0));
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(
            plan(),
            driver.clone(),
            adapter(
                &events,
                &resources_released,
                DeactivationMode::Clean,
                true,
                false,
            ),
        ))
        .expect("the App should start");

    let outcome = driver.run(app.shutdown(Duration::from_secs(1)));

    assert!(matches!(
        outcome,
        ShutdownOutcome::RuntimeFailure {
            error: RuntimeFailure::InvalidResolvedPlan { detail }
        } if detail == "resource release failure"
    ));
    assert_eq!(resources_released.get(), 2);
}

#[test]
fn shutdown_timeout_terminates_blocked_deactivation_at_the_global_deadline() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let resources_released = Rc::new(Cell::new(0));
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(
            plan(),
            driver.clone(),
            adapter(
                &events,
                &resources_released,
                DeactivationMode::Blocked,
                false,
                false,
            ),
        ))
        .expect("the App should start");
    let advance_driver = driver.clone();
    driver
        .spawn_local(Box::pin(async move {
            advance_driver.yield_now().await;
            advance_driver.advance(Duration::from_millis(10));
        }))
        .expect("the deterministic Driver should accept the clock task");

    let outcome = driver.run(app.shutdown(Duration::from_millis(10)));

    assert_eq!(outcome, ShutdownOutcome::Timeout);
    assert_eq!(driver.now(), Duration::from_millis(10));
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|event| matches!(event, Event::ResourceReleaseStarted(_)))
            .count(),
        2
    );
}

#[test]
fn start_native_preserves_the_startup_failure_classification() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let resources_released = Rc::new(Cell::new(0));
    let failing_modules = adapter(
        &events,
        &resources_released,
        DeactivationMode::Clean,
        false,
        true,
    )
    .modules;
    let driver = DeterministicDriver::new();

    let outcome = driver.run(Kernel::start_native(
        plan(),
        driver.clone(),
        RecordingAdapter {
            modules: failing_modules,
        },
    ));

    assert!(matches!(
        outcome,
        Err(RuntimeFailure::InvalidResolvedPlan { detail }) if detail == "startup failure"
    ));
}

#[test]
fn shutdown_timeout_terminates_a_blocked_managed_task() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let resources_released = Rc::new(Cell::new(0));
    let mut modules = adapter(
        &events,
        &resources_released,
        DeactivationMode::Clean,
        false,
        false,
    )
    .modules;
    modules.insert("provider".to_owned(), Rc::new(BlockedTaskLifecycle));
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(
            plan(),
            driver.clone(),
            RecordingAdapter { modules },
        ))
        .expect("the App should start");
    let advance_driver = driver.clone();
    driver
        .spawn_local(Box::pin(async move {
            advance_driver.yield_now().await;
            advance_driver.advance(Duration::from_millis(10));
        }))
        .expect("the deterministic Driver should accept the clock task");

    assert_eq!(
        driver.run(app.shutdown(Duration::from_millis(10))),
        ShutdownOutcome::Timeout
    );
    assert_eq!(driver.now(), Duration::from_millis(10));
}
