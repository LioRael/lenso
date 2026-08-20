use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    time::Duration,
};

use futures::future::LocalBoxFuture;
use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityCardinality, CapabilityEndpointPlan,
    CapabilityRequirementPlan, ModuleCriticality, ModuleInstancePlan, ResolvedAppPlan,
    RestartPolicy,
};
use lenso_kernel::{
    DeactivateContext, DeactivationReason, DeterministicDriver, Kernel, ManagedResource,
    ModuleFuture, ModuleLifecycle, NativeExecutionAdapter, NativeRequestEndpoint,
    NoopModuleLifecycle, PrepareContext, PreparedBinding, PreparedNativeApp, PreparedNativeModule,
    RequestCapability, ResourceFuture, RuntimeDriver, RuntimeFailure,
};

const CAPABILITY_ID: &str = "capability.supervision";
const DESCRIPTOR_VERSION: &str = "1.0.0";
const OPERATION: &str = "echo";

type SupervisionTestSetup = (
    lenso_kernel::NativeApp,
    DeterministicDriver,
    Rc<RefCell<Vec<Event>>>,
    Rc<Cell<usize>>,
);

#[derive(Debug)]
struct SupervisedCapability;

impl RequestCapability for SupervisedCapability {
    type Request = String;
    type Response = String;
    type DomainError = String;

    const ID: &'static str = CAPABILITY_ID;
    const DESCRIPTOR_VERSION: &'static str = DESCRIPTOR_VERSION;
}

#[derive(Debug)]
struct SupervisedEndpoint {
    generation: u64,
    invocations: Rc<Cell<usize>>,
    fail: bool,
}

impl NativeRequestEndpoint for SupervisedEndpoint {
    fn capability_id(&self) -> &'static str {
        CAPABILITY_ID
    }

    fn descriptor_version(&self) -> &'static str {
        DESCRIPTOR_VERSION
    }

    fn operations(&self) -> &'static [&'static str] {
        &[OPERATION]
    }

    fn invoke(
        &self,
        operation: &str,
        request: Box<dyn std::any::Any>,
        _context: lenso_kernel::InvocationContext,
    ) -> LocalBoxFuture<
        'static,
        Result<Result<Box<dyn std::any::Any>, Box<dyn std::any::Any>>, RuntimeFailure>,
    > {
        let generation = self.generation;
        let invocations = self.invocations.clone();
        let fail = self.fail;
        if operation != OPERATION {
            return Box::pin(futures::future::ready(Err(
                RuntimeFailure::UnknownOperation {
                    capability: CAPABILITY_ID,
                    operation: operation.to_owned(),
                },
            )));
        }
        let Ok(request) = request.downcast::<String>() else {
            return Box::pin(futures::future::ready(Err(
                RuntimeFailure::ProtocolViolation {
                    capability: CAPABILITY_ID,
                },
            )));
        };
        Box::pin(async move {
            invocations.set(invocations.get() + 1);
            if fail {
                return Err(RuntimeFailure::ModuleFailure {
                    detail: "provider generation failed".to_owned(),
                });
            }
            let response: Box<dyn std::any::Any> =
                Box::new(format!("generation-{generation}:{request}"));
            Ok(Ok(response))
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Event {
    Prepare(u64),
    Activate(u64),
    Deactivate {
        generation: u64,
        reason: DeactivationReason,
        tasks: usize,
        resources: usize,
    },
    Release(u64),
}

#[derive(Debug)]
struct RecordingResource {
    generation: u64,
    events: Rc<RefCell<Vec<Event>>>,
    fail_release: bool,
}

impl ManagedResource for RecordingResource {
    fn release(&self) -> ResourceFuture {
        let generation = self.generation;
        let events = self.events.clone();
        let fail_release = self.fail_release;
        Box::pin(async move {
            if fail_release {
                return Err(RuntimeFailure::Internal {
                    detail: "generation resource release failed".to_owned(),
                });
            }
            events.borrow_mut().push(Event::Release(generation));
            Ok(())
        })
    }
}

#[derive(Debug)]
struct RecordingLifecycle {
    generation: u64,
    events: Rc<RefCell<Vec<Event>>>,
    fail_release: bool,
}

impl ModuleLifecycle for RecordingLifecycle {
    fn prepare(&self, context: PrepareContext) -> ModuleFuture {
        let generation = self.generation;
        let events = self.events.clone();
        let fail_release = self.fail_release;
        Box::pin(async move {
            events.borrow_mut().push(Event::Prepare(generation));
            context
                .resources()
                .register(RecordingResource {
                    generation,
                    events: events.clone(),
                    fail_release,
                })
                .map_err(|error| RuntimeFailure::Internal {
                    detail: format!("resource registration failed: {error:?}"),
                })?;
            Ok(())
        })
    }

    fn activate(&self, context: lenso_kernel::ActivateContext) -> ModuleFuture {
        let generation = self.generation;
        let events = self.events.clone();
        let cancellation = context.cancellation();
        context
            .tasks()
            .spawn_local(Box::pin(async move {
                cancellation.cancelled().await;
            }))
            .expect("the generation should accept its managed task");
        Box::pin(async move {
            events.borrow_mut().push(Event::Activate(generation));
            Ok(())
        })
    }

    fn deactivate(&self, context: DeactivateContext) -> ModuleFuture {
        let generation = self.generation;
        let events = self.events.clone();
        let reason = context.reason();
        let tasks = context.tasks().task_count();
        let resources = context.resources().resource_count();
        Box::pin(async move {
            events.borrow_mut().push(Event::Deactivate {
                generation,
                reason,
                tasks,
                resources,
            });
            Ok(())
        })
    }
}

#[derive(Debug)]
struct SupervisionAdapter {
    events: Rc<RefCell<Vec<Event>>>,
    invocations: Rc<Cell<usize>>,
    next_generation: Cell<u64>,
    recreate_failures: Cell<usize>,
    fail_initial_generation: bool,
    fail_initial_release: bool,
}

impl SupervisionAdapter {
    fn new(
        events: Rc<RefCell<Vec<Event>>>,
        invocations: Rc<Cell<usize>>,
        recreate_failures: usize,
        fail_initial_generation: bool,
    ) -> Self {
        Self {
            events,
            invocations,
            next_generation: Cell::new(1),
            recreate_failures: Cell::new(recreate_failures),
            fail_initial_generation,
            fail_initial_release: false,
        }
    }

    fn with_initial_release_failure(mut self) -> Self {
        self.fail_initial_release = true;
        self
    }

    fn endpoint(&self, generation: u64, fail: bool) -> Rc<dyn NativeRequestEndpoint> {
        Rc::new(SupervisedEndpoint {
            generation,
            invocations: self.invocations.clone(),
            fail,
        })
    }

    fn lifecycle(&self, generation: u64, fail_release: bool) -> Rc<dyn ModuleLifecycle> {
        Rc::new(RecordingLifecycle {
            generation,
            events: self.events.clone(),
            fail_release,
        })
    }
}

impl NativeExecutionAdapter for SupervisionAdapter {
    fn prepare(&self, _plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure> {
        let endpoint = self.endpoint(1, self.fail_initial_generation);
        let bindings = vec![PreparedBinding::new("consumer", "provider", endpoint)];
        let modules = BTreeMap::from([
            (
                "consumer".to_owned(),
                Rc::new(NoopModuleLifecycle) as Rc<dyn ModuleLifecycle>,
            ),
            (
                "provider".to_owned(),
                self.lifecycle(1, self.fail_initial_release),
            ),
        ]);
        Ok(PreparedNativeApp::with_modules(bindings, modules))
    }

    fn recreate(
        &self,
        _plan: &ResolvedAppPlan,
        _instance_key: &str,
    ) -> Result<PreparedNativeModule, RuntimeFailure> {
        let generation = self.next_generation.get().saturating_add(1);
        self.next_generation.set(generation);
        if self.recreate_failures.get() > 0 {
            self.recreate_failures
                .set(self.recreate_failures.get().saturating_sub(1));
            return Err(RuntimeFailure::Internal {
                detail: "configured recreation failure".to_owned(),
            });
        }
        Ok(PreparedNativeModule::with_lifecycle(
            vec![self.endpoint(generation, false)],
            self.lifecycle(generation, false),
        ))
    }
}

fn plan(
    policy: RestartPolicy,
    cardinality: Option<CapabilityCardinality>,
    critical: bool,
) -> ResolvedAppPlan {
    let provider = ModuleInstancePlan::new("provider", "package.provider")
        .with_capability(CapabilityEndpointPlan::new(
            CAPABILITY_ID,
            DESCRIPTOR_VERSION,
            [OPERATION],
        ))
        .with_restart_policy(policy);
    let provider = if critical {
        provider.with_criticality(ModuleCriticality::Critical)
    } else {
        provider
    };
    let mut consumer = ModuleInstancePlan::new("consumer", "package.consumer");
    let mut bindings = Vec::new();
    if let Some(cardinality) = cardinality {
        consumer = consumer.with_requirement(CapabilityRequirementPlan::new(
            CAPABILITY_ID,
            DESCRIPTOR_VERSION,
            cardinality,
        ));
        bindings.push(CapabilityBinding::new(
            "consumer",
            CAPABILITY_ID,
            DESCRIPTOR_VERSION,
            "provider",
        ));
    }
    AppComposition::new(vec![provider, consumer], bindings)
        .resolve()
        .expect("the supervision plan should resolve")
}

fn start_app(
    policy: RestartPolicy,
    cardinality: Option<CapabilityCardinality>,
    critical: bool,
    recreate_failures: usize,
    fail_initial_generation: bool,
) -> SupervisionTestSetup {
    let events = Rc::new(RefCell::new(Vec::new()));
    let invocations = Rc::new(Cell::new(0));
    let adapter = SupervisionAdapter::new(
        events.clone(),
        invocations.clone(),
        recreate_failures,
        fail_initial_generation,
    );
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(
            plan(policy, cardinality, critical),
            driver.clone(),
            adapter,
        ))
        .expect("the native App should start");
    (app, driver, events, invocations)
}

fn start_app_with_initial_release_failure(
    policy: RestartPolicy,
    cardinality: Option<CapabilityCardinality>,
    critical: bool,
    recreate_failures: usize,
    fail_initial_generation: bool,
) -> SupervisionTestSetup {
    let events = Rc::new(RefCell::new(Vec::new()));
    let invocations = Rc::new(Cell::new(0));
    let adapter = SupervisionAdapter::new(
        events.clone(),
        invocations.clone(),
        recreate_failures,
        fail_initial_generation,
    )
    .with_initial_release_failure();
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(
            plan(policy, cardinality, critical),
            driver.clone(),
            adapter,
        ))
        .expect("the native App should start");
    (app, driver, events, invocations)
}

fn drive_turn(driver: &DeterministicDriver) {
    let driver_for_task = driver.clone();
    driver.run(async move {
        driver_for_task.yield_now().await;
        driver_for_task.yield_now().await;
    });
}

#[test]
fn stable_handle_is_unavailable_during_deterministic_restart_and_reuses_ready_generation() {
    let policy = RestartPolicy::on_failure(
        2,
        Duration::from_secs(60),
        Duration::from_millis(10),
        Duration::from_millis(5),
        Duration::from_secs(60),
    );
    let (app, driver, events, _) =
        start_app(policy, Some(CapabilityCardinality::One), false, 0, false);
    driver.set_jitter(Duration::from_millis(5));
    let handle = app
        .handle::<SupervisedCapability>("consumer")
        .expect("the stable binding should materialize");
    assert_eq!(app.module_generation("provider"), Some(1));
    assert_eq!(
        driver.run(handle.invoke(OPERATION, "before".to_owned())),
        Ok(Ok("generation-1:before".to_owned()))
    );

    app.report_module_failure("provider")
        .expect("the deterministic Driver should schedule supervision");
    drive_turn(&driver);

    assert_eq!(
        driver.run(handle.invoke(OPERATION, "during".to_owned())),
        Err(RuntimeFailure::Unavailable {
            capability: CAPABILITY_ID,
        })
    );
    assert_eq!(driver.now(), Duration::ZERO);

    driver.advance(Duration::from_millis(14));
    drive_turn(&driver);
    assert_eq!(app.module_generation("provider"), None);
    driver.advance(Duration::from_millis(1));
    for _ in 0..4 {
        drive_turn(&driver);
    }

    assert_eq!(app.module_generation("provider"), Some(2));
    assert_eq!(
        driver.run(handle.invoke(OPERATION, "after".to_owned())),
        Ok(Ok("generation-2:after".to_owned()))
    );

    let recorded = events.borrow();
    let deactivate = recorded
        .iter()
        .position(|event| {
            matches!(
                event,
                Event::Deactivate {
                    generation: 1,
                    reason: DeactivationReason::SupervisionRestart,
                    tasks: 0,
                    resources: 1,
                }
            )
        })
        .expect("the failed generation should be deactivated after its task scope closes");
    let release = recorded
        .iter()
        .position(|event| *event == Event::Release(1))
        .expect("the failed generation resource should be released");
    let replacement_prepare = recorded
        .iter()
        .position(|event| *event == Event::Prepare(2))
        .expect("the replacement should prepare");
    assert!(deactivate < release && release < replacement_prepare);
}

#[test]
fn provider_failure_does_not_replay_the_in_flight_request() {
    let (app, driver, _, invocations) = start_app(
        RestartPolicy::on_failure(
            1,
            Duration::from_secs(60),
            Duration::ZERO,
            Duration::ZERO,
            Duration::from_secs(60),
        ),
        Some(CapabilityCardinality::One),
        false,
        0,
        true,
    );
    let handle = app
        .handle::<SupervisedCapability>("consumer")
        .expect("the stable binding should materialize");

    assert_eq!(
        driver.run(handle.invoke(OPERATION, "once".to_owned())),
        Err(RuntimeFailure::ModuleFailure {
            detail: "provider generation failed".to_owned(),
        })
    );
    assert_eq!(invocations.get(), 1);
    assert_eq!(
        driver.run(handle.invoke(OPERATION, "not-replayed".to_owned())),
        Err(RuntimeFailure::Unavailable {
            capability: CAPABILITY_ID,
        })
    );
    drive_turn(&driver);
    assert_eq!(
        driver.run(handle.invoke(OPERATION, "new-call".to_owned())),
        Ok(Ok("generation-2:new-call".to_owned()))
    );
    assert_eq!(invocations.get(), 2);
}

#[test]
fn a_ready_stability_period_resets_the_finite_attempt_budget() {
    let stability = Duration::from_millis(5);
    let (app, driver, _, _) = start_app(
        RestartPolicy::on_failure(
            1,
            Duration::from_secs(60),
            Duration::ZERO,
            Duration::ZERO,
            stability,
        ),
        Some(CapabilityCardinality::One),
        false,
        0,
        false,
    );

    app.report_module_failure("provider")
        .expect("the deterministic Driver should schedule supervision");
    drive_turn(&driver);
    assert_eq!(app.module_generation("provider"), Some(2));
    driver.advance(stability);
    app.report_module_failure("provider")
        .expect("the deterministic Driver should schedule supervision");
    drive_turn(&driver);
    assert_eq!(app.module_generation("provider"), Some(3));
}

#[test]
fn a_backoff_longer_than_the_attempt_window_still_exhausts_finitely() {
    let (app, driver, _, _) = start_app(
        RestartPolicy::on_failure(
            2,
            Duration::from_millis(1),
            Duration::from_millis(10),
            Duration::ZERO,
            Duration::from_secs(60),
        ),
        Some(CapabilityCardinality::Optional),
        false,
        2,
        false,
    );
    app.report_module_failure("provider")
        .expect("the deterministic Driver should schedule supervision");
    drive_turn(&driver);
    driver.advance(Duration::from_millis(10));
    drive_turn(&driver);
    driver.advance(Duration::from_millis(20));
    drive_turn(&driver);

    assert!(app.is_accepting());
    assert!(app.terminal_failure().is_none());
    assert_eq!(app.module_generation("provider"), None);
}

#[test]
fn cleanup_failure_keeps_the_old_generation_unavailable_and_unpublished() {
    let (app, driver, events, _) = start_app_with_initial_release_failure(
        RestartPolicy::on_failure(
            1,
            Duration::from_secs(60),
            Duration::ZERO,
            Duration::ZERO,
            Duration::from_secs(60),
        ),
        Some(CapabilityCardinality::Optional),
        false,
        0,
        false,
    );

    app.report_module_failure("provider")
        .expect("the deterministic Driver should schedule supervision");
    drive_turn(&driver);
    assert!(app.is_accepting());
    assert_eq!(app.module_generation("provider"), None);
    assert!(!events.borrow().contains(&Event::Prepare(2)));
}

#[test]
fn noncritical_optional_provider_may_remain_unavailable_after_budget_exhaustion() {
    let (app, driver, _, _) = start_app(
        RestartPolicy::on_failure(
            2,
            Duration::from_secs(60),
            Duration::ZERO,
            Duration::ZERO,
            Duration::from_secs(60),
        ),
        Some(CapabilityCardinality::Optional),
        false,
        2,
        false,
    );
    let handle = app
        .handle::<SupervisedCapability>("consumer")
        .expect("the optional binding should materialize while it is ready");

    app.report_module_failure("provider")
        .expect("the deterministic Driver should schedule supervision");
    drive_turn(&driver);
    assert!(app.is_accepting());
    assert!(app.terminal_failure().is_none());
    assert_eq!(
        driver.run(handle.invoke(OPERATION, "unavailable".to_owned())),
        Err(RuntimeFailure::Unavailable {
            capability: CAPABILITY_ID,
        })
    );
}

#[test]
fn required_path_or_explicit_criticality_turns_exhaustion_into_a_terminal_app_failure() {
    let (required_app, required_driver, _, _) = start_app(
        RestartPolicy::on_failure(
            1,
            Duration::from_secs(60),
            Duration::ZERO,
            Duration::ZERO,
            Duration::ZERO,
        ),
        Some(CapabilityCardinality::One),
        false,
        1,
        false,
    );
    required_app
        .report_module_failure("provider")
        .expect("the deterministic Driver should schedule supervision");
    drive_turn(&required_driver);
    assert!(required_app.is_failed());
    assert!(!required_app.is_accepting());
    assert_eq!(
        required_app.terminal_failure(),
        Some(RuntimeFailure::ModuleRestartExhausted {
            instance: "provider".to_owned(),
            attempts: 1,
        })
    );

    let (critical_app, critical_driver, _, _) = start_app(
        RestartPolicy::on_failure(
            1,
            Duration::from_secs(60),
            Duration::ZERO,
            Duration::ZERO,
            Duration::ZERO,
        ),
        Some(CapabilityCardinality::Optional),
        true,
        1,
        false,
    );
    critical_app
        .report_module_failure("provider")
        .expect("the deterministic Driver should schedule supervision");
    drive_turn(&critical_driver);
    assert!(critical_app.is_failed());
    assert_eq!(
        critical_app.terminal_failure(),
        Some(RuntimeFailure::ModuleRestartExhausted {
            instance: "provider".to_owned(),
            attempts: 1,
        })
    );
}
