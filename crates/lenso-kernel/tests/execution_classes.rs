use std::{cell::Cell, collections::BTreeMap, rc::Rc};

use futures::future::LocalBoxFuture;
use lenso_app_plan::{AppComposition, ExecutionClass, ModuleInstancePlan, ResolvedAppPlan};
use lenso_kernel::{
    DeterministicDriver, DriverTask, Kernel, LocalTask, NativeExecutionAdapter, PreparedNativeApp,
    RuntimeDriver, RuntimeFailure,
};

#[derive(Debug)]
struct RecordingAdapter {
    prepared: Rc<Cell<bool>>,
}

impl NativeExecutionAdapter for RecordingAdapter {
    fn prepare(&self, _plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure> {
        self.prepared.set(true);
        Ok(PreparedNativeApp::with_modules(
            BTreeMap::new(),
            BTreeMap::new(),
        ))
    }
}

#[derive(Clone, Debug)]
struct BunCapableDriver {
    inner: DeterministicDriver,
}

impl BunCapableDriver {
    fn new() -> Self {
        Self {
            inner: DeterministicDriver::new(),
        }
    }

    fn run<F: std::future::Future>(&self, future: F) -> F::Output {
        self.inner.run(future)
    }
}

impl RuntimeDriver for BunCapableDriver {
    fn now(&self) -> std::time::Duration {
        self.inner.now()
    }

    fn sleep_until(&self, deadline: std::time::Duration) -> LocalBoxFuture<'static, ()> {
        self.inner.sleep_until(deadline)
    }

    fn yield_now(&self) -> LocalBoxFuture<'static, ()> {
        self.inner.yield_now()
    }

    fn spawn_local(&self, task: LocalTask) -> Result<DriverTask, futures::task::SpawnError> {
        self.inner.spawn_local(task)
    }

    fn shutdown_requested(&self) -> bool {
        self.inner.shutdown_requested()
    }

    fn supported_execution_classes(&self) -> lenso_app_plan::ExecutionClassSet {
        lenso_app_plan::ExecutionClassSet::native_rust().with(ExecutionClass::BunChildProcess)
    }
}

#[derive(Debug)]
struct BunCapableAdapter {
    prepared: Rc<Cell<bool>>,
}

impl NativeExecutionAdapter for BunCapableAdapter {
    fn prepare(&self, _plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure> {
        self.prepared.set(true);
        Ok(PreparedNativeApp::with_modules(
            BTreeMap::new(),
            BTreeMap::new(),
        ))
    }

    fn supported_execution_classes(&self) -> lenso_app_plan::ExecutionClassSet {
        lenso_app_plan::ExecutionClassSet::native_rust().with(ExecutionClass::BunChildProcess)
    }
}

#[test]
fn kernel_rejects_an_execution_class_before_the_adapter_prepares_any_module() {
    let plan = AppComposition::new(
        vec![
            ModuleInstancePlan::new("bun", "package.bun")
                .with_execution_class(ExecutionClass::BunChildProcess),
        ],
        vec![],
    )
    .resolve()
    .expect("the execution class is valid authoring data");
    let prepared = Rc::new(Cell::new(false));
    let driver = DeterministicDriver::new();

    let result = driver.run(Kernel::start_native(
        plan,
        driver.clone(),
        RecordingAdapter {
            prepared: prepared.clone(),
        },
    ));

    assert!(matches!(
        result,
        Err(RuntimeFailure::InvalidResolvedPlan { detail })
            if detail.contains("unsupported execution class `bun-child-process`")
    ));
    assert!(!prepared.get());
}

#[test]
fn kernel_requires_both_the_driver_and_adapter_to_provide_an_execution_class() {
    let plan = AppComposition::new(
        vec![
            ModuleInstancePlan::new("bun", "package.bun")
                .with_execution_class(ExecutionClass::BunChildProcess),
        ],
        vec![],
    )
    .resolve()
    .expect("the execution class is valid authoring data");

    let adapter_prepared = Rc::new(Cell::new(false));
    let driver = DeterministicDriver::new();
    let result = driver.run(Kernel::start_native(
        plan.clone(),
        driver.clone(),
        BunCapableAdapter {
            prepared: adapter_prepared.clone(),
        },
    ));
    assert!(result.is_err());
    assert!(!adapter_prepared.get());

    let driver = BunCapableDriver::new();
    let adapter_prepared = Rc::new(Cell::new(false));
    let result = driver.run(Kernel::start_native(
        plan.clone(),
        driver.clone(),
        RecordingAdapter {
            prepared: adapter_prepared.clone(),
        },
    ));
    assert!(result.is_err());
    assert!(!adapter_prepared.get());

    let driver = BunCapableDriver::new();
    let adapter_prepared = Rc::new(Cell::new(false));
    let result = driver.run(Kernel::start_native(
        plan,
        driver.clone(),
        BunCapableAdapter {
            prepared: adapter_prepared.clone(),
        },
    ));
    assert!(result.is_ok());
    assert!(adapter_prepared.get());
}
