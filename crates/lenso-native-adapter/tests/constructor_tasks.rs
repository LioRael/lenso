use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use futures::channel::oneshot;
use lenso_app_plan::{PluginInstancePlan, ResolvedAppPlan};
use lenso_kernel::{
    DeterministicDriver, Kernel, ManagedTaskError, RuntimeDriver, RuntimeFailure, ShutdownOutcome,
};
use lenso_native_adapter::{
    CompleteObjectLifecycle, LifecycleContext, ManagedTasksError, NativePluginFactory,
    NativePluginFactoryContext, NativePluginInstance, NativePluginRegistry, PluginObject,
};

#[derive(Debug, Default)]
struct TaskProbe {
    ready_runs: Cell<u32>,
    cancellation_runs: Cell<u32>,
    task_drops: Cell<u32>,
    stop_runs: Cell<u32>,
}

struct TaskLifetime(Rc<TaskProbe>);

impl Drop for TaskLifetime {
    fn drop(&mut self) {
        self.0.task_drops.set(self.0.task_drops.get() + 1);
    }
}

#[lenso_native_adapter::plugin]
#[derive(Debug)]
struct ConstructingTaskPlugin {
    lifecycle: LifecycleContext,
    probe: Rc<TaskProbe>,
    ready_signal: RefCell<Option<oneshot::Receiver<()>>>,
}

#[lenso_native_adapter::plugin_impl]
impl ConstructingTaskPlugin {
    #[create]
    fn create(#[lifecycle] lifecycle: LifecycleContext) -> Result<Self, &'static str> {
        let ready = lifecycle
            .readiness()
            .map_err(|_| "no construction readiness")?;
        assert!(!ready.is_open());
        let probe = Rc::new(TaskProbe::default());
        let task_probe = probe.clone();
        let lifetime = TaskLifetime(probe.clone());
        let (started, ready_signal) = oneshot::channel();
        lifecycle
            .spawn_local(async move {
                let _lifetime = lifetime;
                ready.wait().await;
                task_probe.ready_runs.set(task_probe.ready_runs.get() + 1);
                let _ = started.send(());
                ready.cancellation().cancelled().await;
                task_probe
                    .cancellation_runs
                    .set(task_probe.cancellation_runs.get() + 1);
            })
            .map_err(|_| "construction task was rejected")?;

        assert_eq!(probe.ready_runs.get(), 0);
        Ok(Self {
            lifecycle,
            probe,
            ready_signal: RefCell::new(Some(ready_signal)),
        })
    }

    #[stop]
    fn stop(&self, #[lifecycle] lifecycle: LifecycleContext) {
        assert!(matches!(
            lifecycle.readiness(),
            Err(RuntimeFailure::AdmissionClosed)
        ));
        assert!(matches!(
            lifecycle.spawn_local(async {}),
            Err(ManagedTasksError::Inactive)
        ));
        self.probe.stop_runs.set(self.probe.stop_runs.get() + 1);
        drop(lifecycle);
    }
}

#[derive(Debug)]
struct TaskFactory {
    object: PluginObject<ConstructingTaskPlugin>,
}

impl NativePluginFactory for TaskFactory {
    fn package_id(&self) -> &'static str {
        "test.constructor-tasks"
    }

    fn runtime_profile(&self) -> &'static str {
        "lenso.native-authoring@2"
    }

    fn instantiate(
        &self,
        context: NativePluginFactoryContext<'_>,
    ) -> Result<NativePluginInstance, RuntimeFailure> {
        let lifecycle =
            CompleteObjectLifecycle::linked(self.object.clone(), context.configuration())?;
        Ok(NativePluginInstance::with_lifecycle(Vec::new(), lifecycle))
    }
}

#[test]
fn constructor_work_waits_for_ready_and_is_cancelled_with_its_generation() {
    let object = PluginObject::empty();
    let plan = ResolvedAppPlan::new(
        vec![
            PluginInstancePlan::new("tasks", "test.constructor-tasks")
                .with_authoring(2, "lenso.native-authoring@2"),
        ],
        vec![],
    );
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(
            plan,
            driver.clone(),
            NativePluginRegistry::new().with_factory(TaskFactory {
                object: object.clone(),
            }),
        ))
        .expect("authoring-2 construction should register generation-owned work");
    let plugin = object.get().expect("constructor should install the object");
    let ready_signal = plugin
        .ready_signal
        .borrow_mut()
        .take()
        .expect("one readiness observation");
    driver
        .run(ready_signal)
        .expect("managed work should observe the same Ready Gate");
    assert_eq!(plugin.probe.ready_runs.get(), 1);
    assert_eq!(plugin.probe.task_drops.get(), 0);
    assert_eq!(plugin.probe.cancellation_runs.get(), 0);

    assert_eq!(
        driver.run(app.shutdown(Duration::from_secs(1))),
        ShutdownOutcome::Clean
    );
    assert_eq!(plugin.probe.stop_runs.get(), 1);
    assert_eq!(plugin.probe.task_drops.get(), 1);
    assert_eq!(plugin.probe.cancellation_runs.get(), 1);
    assert!(matches!(
        plugin.lifecycle.spawn_local(async {}),
        Err(ManagedTasksError::Scope(ManagedTaskError::ScopeClosed))
    ));
    driver.run(driver.yield_now());
    assert_eq!(plugin.probe.ready_runs.get(), 1);
    assert_eq!(plugin.probe.task_drops.get(), 1);
}
