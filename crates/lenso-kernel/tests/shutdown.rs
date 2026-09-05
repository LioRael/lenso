use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    time::Duration,
};

use futures::FutureExt;
use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityEndpointPlan, CapabilityRequirementPlan,
    PluginInstancePlan, ResolvedAppPlan,
};
use lenso_kernel::{
    ActivateContext, DeactivateContext, DeterministicDriver, InvocationContext, Kernel,
    ManagedResource, NativeExecutionAdapter, NativeRequestEndpoint, NoopPluginLifecycle,
    PluginLifecycle, PrepareContext, PreparedBinding, PreparedNativeApp, PreparedNativePlugin,
    RequestCapability, ResourceFuture, RuntimeDriver, RuntimeFailure, ShutdownOutcome,
    invoke_erased_native_request,
};

#[derive(Debug)]
struct ShutdownCall;

impl RequestCapability for ShutdownCall {
    type Request = ();
    type Response = ();
    type DomainError = String;

    const ID: &'static str = "capability.shutdown";
    const DESCRIPTOR_VERSION: &'static str = "1.0.0";

    fn invoke_native(
        endpoint: &dyn NativeRequestEndpoint,
        operation: &str,
        request: Self::Request,
        context: InvocationContext,
    ) -> futures::future::LocalBoxFuture<
        'static,
        Result<Result<Self::Response, Self::DomainError>, RuntimeFailure>,
    > {
        invoke_erased_native_request::<Self>(endpoint, operation, request, context)
    }
}

#[derive(Debug)]
struct ShutdownEndpoint;

impl NativeRequestEndpoint for ShutdownEndpoint {
    fn capability_id(&self) -> &'static str {
        "capability.shutdown"
    }

    fn descriptor_version(&self) -> &'static str {
        "1.0.0"
    }

    fn operations(&self) -> &'static [&'static str] {
        &["shutdown.call"]
    }

    fn invoke(
        &self,
        _operation: &str,
        _request: Box<dyn Any>,
        _context: InvocationContext,
    ) -> futures::future::LocalBoxFuture<
        'static,
        Result<Result<Box<dyn Any>, Box<dyn Any>>, RuntimeFailure>,
    > {
        futures::future::ready(Ok(Ok(Box::new(()) as Box<dyn Any>))).boxed_local()
    }
}

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

impl PluginLifecycle for RecordingLifecycle {
    fn prepare(&self, _context: PrepareContext) -> lenso_kernel::PluginFuture {
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

    fn activate(&self, context: ActivateContext) -> lenso_kernel::PluginFuture {
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

    fn deactivate(&self, context: DeactivateContext) -> lenso_kernel::PluginFuture {
        assert!(matches!(
            context.reason(),
            lenso_kernel::DeactivationReason::Shutdown
                | lenso_kernel::DeactivationReason::StartupRollback
        ));
        if context.reason() == lenso_kernel::DeactivationReason::Shutdown {
            assert!(!context.cancellation().is_cancelled());
            assert!(context.remaining_budget().is_some());
        }
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
    plugins: BTreeMap<String, Rc<dyn PluginLifecycle>>,
}

#[derive(Debug)]
struct CleanupDependencyLifecycle {
    calls: Rc<Cell<usize>>,
}

impl PluginLifecycle for CleanupDependencyLifecycle {
    fn deactivate(&self, context: DeactivateContext) -> lenso_kernel::PluginFuture {
        let ordinary_invocation = context
            .dependencies()
            .invocation_context(None, context.cancellation());
        let invocation = context.dependency_invocation_context();
        let dependency = context
            .dependencies()
            .bindings()
            .first()
            .and_then(lenso_kernel::PluginDependency::handle);
        let calls = self.calls.clone();
        Box::pin(async move {
            let ordinary_invocation = ordinary_invocation?;
            let invocation = invocation?;
            let dependency = dependency.ok_or(RuntimeFailure::Unavailable {
                capability: ShutdownCall::ID,
            })?;
            if !matches!(
                dependency
                    .invoke_erased("shutdown.call", Box::new(()), ordinary_invocation)
                    .await,
                Err(RuntimeFailure::AdmissionClosed)
            ) {
                return Err(RuntimeFailure::Internal {
                    detail: "ordinary dependency context bypassed shutdown admission".to_owned(),
                });
            }
            let outcome = dependency
                .invoke_erased("shutdown.call", Box::new(()), invocation)
                .await?;
            outcome.map_err(|_| RuntimeFailure::ProtocolViolation {
                capability: ShutdownCall::ID,
            })?;
            calls.set(calls.get() + 1);
            Ok(())
        })
    }
}

impl NativeExecutionAdapter for RecordingAdapter {
    fn prepare(&self, _plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure> {
        let endpoint: Rc<dyn NativeRequestEndpoint> = Rc::new(ShutdownEndpoint);
        let generations = self
            .plugins
            .iter()
            .map(|(instance_key, lifecycle)| {
                let endpoints = if instance_key == "provider" {
                    vec![endpoint.clone()]
                } else {
                    Vec::new()
                };
                (
                    instance_key.clone(),
                    PreparedNativePlugin::with_lifecycle(endpoints, lifecycle.clone()),
                )
            })
            .collect();
        Ok(PreparedNativeApp::new(
            vec![PreparedBinding::new("consumer", "provider", endpoint)],
            generations,
        ))
    }
}

#[derive(Debug)]
struct BlockedTaskLifecycle;

impl PluginLifecycle for BlockedTaskLifecycle {
    fn activate(&self, context: ActivateContext) -> lenso_kernel::PluginFuture {
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
            PluginInstancePlan::new("provider", "package.provider").with_capability(
                CapabilityEndpointPlan::new("capability.shutdown", "1.0.0", ["shutdown.call"]),
            ),
            PluginInstancePlan::new("consumer", "package.consumer").with_requirement(
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
    let plugins = ["provider", "consumer"]
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
                }) as Rc<dyn PluginLifecycle>,
            )
        })
        .collect();
    RecordingAdapter { plugins }
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
fn shutdown_allows_scoped_cleanup_dependency_calls_while_external_admission_stays_closed() {
    let calls = Rc::new(Cell::new(0));
    let adapter = RecordingAdapter {
        plugins: BTreeMap::from([
            (
                "consumer".to_owned(),
                Rc::new(CleanupDependencyLifecycle {
                    calls: calls.clone(),
                }) as Rc<dyn PluginLifecycle>,
            ),
            (
                "provider".to_owned(),
                Rc::new(NoopPluginLifecycle) as Rc<dyn PluginLifecycle>,
            ),
        ]),
    };
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(plan(), driver.clone(), adapter))
        .expect("the App should start");
    let external = app
        .handle::<ShutdownCall>("consumer")
        .expect("the external handle should materialize before shutdown");

    app.request_shutdown();
    assert_eq!(
        driver.run(external.invoke("shutdown.call", ())),
        Err(RuntimeFailure::AdmissionClosed)
    );
    assert_eq!(
        driver.run(app.shutdown(Duration::from_secs(1))),
        ShutdownOutcome::Clean
    );
    assert_eq!(calls.get(), 1);
}

#[test]
fn concurrent_and_dropped_shutdown_callers_share_one_cleanup_run() {
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

    let mut abandoned = Box::pin(app.shutdown(Duration::from_secs(1)));
    assert!(abandoned.as_mut().now_or_never().is_none());
    drop(abandoned);

    let first = app.clone();
    let (first, second) = driver.run(futures::future::join(
        first.shutdown(Duration::from_secs(1)),
        app.shutdown(Duration::from_secs(1)),
    ));

    assert_eq!(first, ShutdownOutcome::Clean);
    assert_eq!(second, ShutdownOutcome::Clean);
    assert_eq!(resources_released.get(), 2);
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|event| matches!(event, Event::Deactivate(_)))
            .count(),
        2
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
    assert!(events.borrow().iter().all(|event| !matches!(
        event,
        Event::ResourceReleaseStarted(_) | Event::ResourceReleased(_)
    )));
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
    .plugins;
    let driver = DeterministicDriver::new();

    let outcome = driver.run(Kernel::start_native(
        plan(),
        driver.clone(),
        RecordingAdapter {
            plugins: failing_modules,
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
    let mut plugins = adapter(
        &events,
        &resources_released,
        DeactivationMode::Clean,
        false,
        false,
    )
    .plugins;
    plugins.insert("provider".to_owned(), Rc::new(BlockedTaskLifecycle));
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(
            plan(),
            driver.clone(),
            RecordingAdapter { plugins },
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
    assert!(
        events
            .borrow()
            .iter()
            .all(|event| !matches!(event, Event::Deactivate(_)))
    );
    assert_eq!(
        driver.run(app.shutdown(Duration::from_secs(5))),
        ShutdownOutcome::Timeout
    );
}
