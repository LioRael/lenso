use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    rc::Rc,
    time::Duration,
};

use futures::{FutureExt, channel::oneshot, future::join3};
use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityCardinality, CapabilityEndpointPlan,
    CapabilityRequirementPlan, ModuleInstancePlan, RequestAdmissionPlan, ResolvedAppPlan,
};
use lenso_kernel::{
    CancellationToken, DeterministicDriver, InvocationContext, Kernel, NativeExecutionAdapter,
    NativeRequestEndpoint, NoopModuleLifecycle, PreparedBinding, PreparedNativeApp,
    PreparedNativeModule, RequestCapability, RuntimeDriver, RuntimeFailure,
};

const CAPABILITY_ID: &str = "test.echo";
const DESCRIPTOR_VERSION: &str = "1.0.0";
const OPERATION: &str = "echo";

#[derive(Debug)]
struct Echo;

impl RequestCapability for Echo {
    type Request = String;
    type Response = String;
    type DomainError = String;

    const ID: &'static str = CAPABILITY_ID;
    const DESCRIPTOR_VERSION: &'static str = DESCRIPTOR_VERSION;
}

#[derive(Debug, Default)]
struct Probe {
    started: Cell<usize>,
    active: Rc<Cell<usize>>,
    maximum_active: Cell<usize>,
    request_ids: RefCell<Vec<u64>>,
    deadlines: RefCell<Vec<Option<Duration>>>,
    caller_instances: RefCell<Vec<Option<String>>>,
    releases: RefCell<VecDeque<oneshot::Sender<()>>>,
}

impl Probe {
    fn release_one(&self) {
        let sender = self
            .releases
            .borrow_mut()
            .pop_front()
            .expect("a provider call should be waiting for release");
        let _ = sender.send(());
    }
}

#[derive(Debug)]
struct ActiveGuard {
    active: Rc<Cell<usize>>,
}

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        self.active.set(self.active.get() - 1);
    }
}

#[derive(Debug)]
struct EchoEndpoint {
    probe: Rc<Probe>,
}

impl NativeRequestEndpoint for EchoEndpoint {
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
        request: Box<dyn Any>,
        context: InvocationContext,
    ) -> futures::future::LocalBoxFuture<
        'static,
        Result<Result<Box<dyn Any>, Box<dyn Any>>, RuntimeFailure>,
    > {
        if operation != OPERATION {
            return futures::future::ready(Err(RuntimeFailure::UnknownOperation {
                capability: CAPABILITY_ID,
                operation: operation.to_owned(),
            }))
            .boxed_local();
        }
        let request = request
            .downcast::<String>()
            .expect("the generated request type should cross the native seam");
        let (release, released) = oneshot::channel();
        self.probe.releases.borrow_mut().push_back(release);
        self.probe.started.set(self.probe.started.get() + 1);
        self.probe.active.set(self.probe.active.get() + 1);
        self.probe
            .maximum_active
            .set(self.probe.maximum_active.get().max(self.probe.active.get()));
        self.probe
            .request_ids
            .borrow_mut()
            .push(context.request_id());
        self.probe.deadlines.borrow_mut().push(context.deadline());
        self.probe
            .caller_instances
            .borrow_mut()
            .push(context.caller_instance().map(str::to_owned));
        let probe = self.probe.clone();
        let active = self.probe.active.clone();
        Box::pin(async move {
            let _guard = ActiveGuard { active };
            let _ = released.await;
            let _ = probe;
            Ok(Ok(Box::new(*request) as Box<dyn Any>))
        })
    }
}

#[derive(Debug)]
struct EchoAdapter {
    endpoint: Rc<EchoEndpoint>,
}

impl NativeExecutionAdapter for EchoAdapter {
    fn prepare(&self, _plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure> {
        let endpoint: Rc<dyn NativeRequestEndpoint> = self.endpoint.clone();
        Ok(PreparedNativeApp::new(
            vec![PreparedBinding::new(
                "consumer",
                "provider",
                endpoint.clone(),
            )],
            BTreeMap::from([
                (
                    "consumer".to_owned(),
                    PreparedNativeModule::new(Vec::new(), NoopModuleLifecycle),
                ),
                (
                    "provider".to_owned(),
                    PreparedNativeModule::new(vec![endpoint], NoopModuleLifecycle),
                ),
            ]),
        ))
    }
}

fn plan(admission: RequestAdmissionPlan) -> ResolvedAppPlan {
    AppComposition::new(
        vec![
            ModuleInstancePlan::new("consumer", "package.consumer").with_requirement(
                CapabilityRequirementPlan::new(
                    CAPABILITY_ID,
                    DESCRIPTOR_VERSION,
                    CapabilityCardinality::One,
                ),
            ),
            ModuleInstancePlan::new("provider", "package.provider").with_capability(
                CapabilityEndpointPlan::new(CAPABILITY_ID, DESCRIPTOR_VERSION, [OPERATION]),
            ),
        ],
        vec![
            CapabilityBinding::new("consumer", CAPABILITY_ID, DESCRIPTOR_VERSION, "provider")
                .with_admission(admission),
        ],
    )
    .resolve()
    .expect("the request admission plan should resolve")
}

fn app(
    driver: &DeterministicDriver,
    probe: Rc<Probe>,
    admission: RequestAdmissionPlan,
) -> lenso_kernel::NativeApp {
    driver
        .run(Kernel::start_native(
            plan(admission),
            driver.clone(),
            EchoAdapter {
                endpoint: Rc::new(EchoEndpoint { probe }),
            },
        ))
        .expect("the test App should start")
}

#[test]
fn queue_and_concurrency_limits_reject_overload_and_recover() {
    let driver = DeterministicDriver::new();
    let probe = Rc::new(Probe::default());
    let app = app(&driver, probe.clone(), RequestAdmissionPlan::new(1, 1));
    let handle = app
        .handle::<Echo>("consumer")
        .expect("the test binding should resolve");
    let first = handle.invoke(OPERATION, "first".to_owned());
    let second = handle.invoke(OPERATION, "second".to_owned());
    let third = handle.invoke(OPERATION, "third".to_owned());
    let release = probe.clone();
    let release_driver = driver.clone();
    driver
        .spawn_local(Box::pin(async move {
            release_driver.yield_now().await;
            release.release_one();
            release_driver.yield_now().await;
            release.release_one();
        }))
        .expect("the deterministic Driver should accept the release task");

    let (first, second, third) = driver.run(join3(first, second, third));

    assert_eq!(first.unwrap().unwrap(), "first");
    assert_eq!(second.unwrap().unwrap(), "second");
    assert!(matches!(
        third,
        Err(RuntimeFailure::ResourceExhausted {
            capability,
            operation,
        }) if capability == CAPABILITY_ID && operation == OPERATION
    ));
    assert_eq!(probe.maximum_active.get(), 1);

    let release = probe.clone();
    let recovered = handle.invoke(OPERATION, "recovered".to_owned());
    let release_driver = driver.clone();
    driver
        .spawn_local(Box::pin(async move {
            release_driver.yield_now().await;
            release.release_one();
        }))
        .expect("the deterministic Driver should accept the recovery release");
    assert_eq!(driver.run(recovered).unwrap().unwrap(), "recovered");
}

#[test]
fn waking_a_queued_request_reserves_the_slot_before_new_admission() {
    let driver = DeterministicDriver::new();
    let probe = Rc::new(Probe::default());
    let app = app(&driver, probe.clone(), RequestAdmissionPlan::new(1, 1));
    let first_handle = app
        .handle::<Echo>("consumer")
        .expect("the test binding should resolve");
    let second_handle = app
        .handle::<Echo>("consumer")
        .expect("the test binding should resolve");
    let third_handle = app
        .handle::<Echo>("consumer")
        .expect("the test binding should resolve");
    let first = first_handle.invoke(OPERATION, "first".to_owned());
    let second = second_handle.invoke(OPERATION, "second".to_owned());
    let third_outcome = Rc::new(RefCell::new(None));
    let observed = third_outcome.clone();
    let control_driver = driver.clone();
    let release = probe.clone();
    driver
        .spawn_local(Box::pin(async move {
            control_driver.yield_now().await;
            release.release_one();
            observed.replace(Some(
                third_handle.invoke(OPERATION, "third".to_owned()).await,
            ));
            control_driver.yield_now().await;
            release.release_one();
        }))
        .expect("the deterministic Driver should accept the handoff task");

    let (first, second) = driver.run(futures::future::join(first, second));

    assert_eq!(first.unwrap().unwrap(), "first");
    assert_eq!(second.unwrap().unwrap(), "second");
    assert!(matches!(
        third_outcome.borrow().as_ref(),
        Some(Err(RuntimeFailure::ResourceExhausted {
            capability,
            operation,
        })) if *capability == CAPABILITY_ID && operation == OPERATION
    ));
    assert_eq!(probe.maximum_active.get(), 1);
}

#[test]
fn concurrent_calls_respect_the_maximum_concurrency() {
    let driver = DeterministicDriver::new();
    let probe = Rc::new(Probe::default());
    let app = app(&driver, probe.clone(), RequestAdmissionPlan::new(0, 2));
    let handle = app
        .handle::<Echo>("consumer")
        .expect("the test binding should resolve");
    let first = handle.invoke(OPERATION, "first".to_owned());
    let second = handle.invoke(OPERATION, "second".to_owned());
    let release = probe.clone();
    let release_driver = driver.clone();
    driver
        .spawn_local(Box::pin(async move {
            release_driver.yield_now().await;
            release.release_one();
            release.release_one();
        }))
        .expect("the deterministic Driver should accept the release task");

    let (first, second) = driver.run(futures::future::join(first, second));

    assert_eq!(first.unwrap().unwrap(), "first");
    assert_eq!(second.unwrap().unwrap(), "second");
    assert_eq!(probe.maximum_active.get(), 2);
}

#[test]
fn deadline_stops_one_native_call_without_retrying_it() {
    let driver = DeterministicDriver::new();
    let probe = Rc::new(Probe::default());
    let app = app(&driver, probe.clone(), RequestAdmissionPlan::new(0, 1));
    let handle = app
        .handle::<Echo>("consumer")
        .expect("the test binding should resolve");
    let context = app.invocation_context(Some(Duration::from_millis(10)), CancellationToken::new());
    let advance = driver.clone();
    driver
        .spawn_local(Box::pin(async move {
            advance.yield_now().await;
            advance.advance(Duration::from_millis(10));
        }))
        .expect("the deterministic Driver should accept the clock task");

    let outcome = driver.run(handle.invoke_with_context(OPERATION, context, "late".to_owned()));

    assert!(matches!(
        outcome,
        Err(RuntimeFailure::DeadlineExceeded { request_id }) if request_id == 1
    ));
    assert_eq!(probe.started.get(), 1);
    assert_eq!(probe.active.get(), 0);
    assert_eq!(probe.request_ids.borrow().as_slice(), &[1]);
    assert_eq!(
        probe.deadlines.borrow().as_slice(),
        &[Some(Duration::from_millis(10))]
    );
    assert_eq!(
        probe.caller_instances.borrow().as_slice(),
        &[Some("consumer".to_owned())]
    );
}

#[test]
fn caller_cancellation_stops_a_queued_or_running_call() {
    let driver = DeterministicDriver::new();
    let probe = Rc::new(Probe::default());
    let app = app(&driver, probe.clone(), RequestAdmissionPlan::new(0, 1));
    let handle = app
        .handle::<Echo>("consumer")
        .expect("the test binding should resolve");
    let cancellation = CancellationToken::new();
    let context = app.invocation_context(None, cancellation.clone());
    let trigger = cancellation.clone();
    driver
        .spawn_local(Box::pin(async move {
            trigger.cancel();
        }))
        .expect("the deterministic Driver should accept the cancellation trigger");

    let outcome = driver.run(handle.invoke_with_context(OPERATION, context, "cancel".to_owned()));

    assert!(matches!(
        outcome,
        Err(RuntimeFailure::Cancelled { request_id }) if request_id == 1
    ));
    assert_eq!(probe.started.get(), 1);
    assert_eq!(probe.active.get(), 0);
}

#[test]
fn queued_cancellation_does_not_invoke_or_consume_the_next_capacity_slot() {
    let driver = DeterministicDriver::new();
    let probe = Rc::new(Probe::default());
    let app = app(&driver, probe.clone(), RequestAdmissionPlan::new(1, 1));
    let handle = app
        .handle::<Echo>("consumer")
        .expect("the test binding should resolve");
    let first = handle.invoke(OPERATION, "first".to_owned());
    let cancellation = CancellationToken::new();
    let second_context = app.invocation_context(None, cancellation.clone());
    let second = handle.invoke_with_context(OPERATION, second_context, "cancelled".to_owned());
    let control_driver = driver.clone();
    let cancel = cancellation.clone();
    let release = probe.clone();
    driver
        .spawn_local(Box::pin(async move {
            control_driver.yield_now().await;
            cancel.cancel();
            control_driver.yield_now().await;
            release.release_one();
        }))
        .expect("the deterministic Driver should accept the cancellation task");

    let (first, second) = driver.run(futures::future::join(first, second));

    assert_eq!(first.unwrap().unwrap(), "first");
    assert!(matches!(
        second,
        Err(RuntimeFailure::Cancelled { request_id }) if request_id == 1
    ));
    assert_eq!(probe.started.get(), 1);
    assert_eq!(probe.active.get(), 0);

    let release = probe.clone();
    let recovered = handle.invoke(OPERATION, "recovered".to_owned());
    let recovery_driver = driver.clone();
    driver
        .spawn_local(Box::pin(async move {
            recovery_driver.yield_now().await;
            release.release_one();
        }))
        .expect("the deterministic Driver should accept the recovery task");
    assert_eq!(driver.run(recovered).unwrap().unwrap(), "recovered");
}

#[test]
fn provider_completion_wins_when_completion_and_cancellation_are_ready_together() {
    let driver = DeterministicDriver::new();
    let probe = Rc::new(Probe::default());
    let app = app(&driver, probe.clone(), RequestAdmissionPlan::new(0, 1));
    let handle = app
        .handle::<Echo>("consumer")
        .expect("the test binding should resolve");
    let cancellation = CancellationToken::new();
    let context = app.invocation_context(None, cancellation.clone());
    let control_driver = driver.clone();
    let release = probe.clone();
    driver
        .spawn_local(Box::pin(async move {
            control_driver.yield_now().await;
            release.release_one();
            cancellation.cancel();
        }))
        .expect("the deterministic Driver should accept the race task");

    let outcome =
        driver.run(handle.invoke_with_context(OPERATION, context, "completed".to_owned()));

    assert_eq!(outcome.unwrap().unwrap(), "completed");
    assert_eq!(probe.started.get(), 1);
    assert_eq!(probe.active.get(), 0);
}
