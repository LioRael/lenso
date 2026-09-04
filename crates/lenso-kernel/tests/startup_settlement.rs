use futures::{channel::oneshot, future::poll_fn};
use lenso_app_plan::{PluginInstancePlan, ResolvedAppPlan};
use lenso_kernel::{
    ActivateContext, CancellationToken, DeactivateContext, DeterministicDriver,
    ExecutionAdapterCatalog, InvocationContext, Kernel, NativeExecutionAdapter, PluginFuture,
    PluginLifecycle, PreparedNativeApp, PreparedNativePlugin, RuntimeDiagnostics, RuntimeDriver,
    RuntimeFailure,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    rc::Rc,
    time::Duration,
};

#[derive(Debug)]
struct LateLifecycle {
    completion: Rc<RefCell<Option<oneshot::Receiver<()>>>>,
    entered: Rc<Cell<bool>>,
    constructed: Rc<Cell<bool>>,
    activated: Rc<Cell<bool>>,
    stopped: Rc<Cell<usize>>,
    cleanup_remaining: Rc<Cell<Option<Duration>>>,
    cleanup_cancelled: Rc<Cell<bool>>,
}

impl PluginLifecycle for LateLifecycle {
    fn construct(&self, _context: ActivateContext) -> PluginFuture {
        self.entered.set(true);
        let completion = self.completion.borrow_mut().take().unwrap();
        let constructed = self.constructed.clone();
        Box::pin(async move {
            let _ = completion.await;
            constructed.set(true);
            Ok(())
        })
    }

    fn activate(&self, _context: ActivateContext) -> PluginFuture {
        self.activated.set(true);
        Box::pin(futures::future::ready(Ok(())))
    }

    fn deactivate(&self, context: DeactivateContext) -> PluginFuture {
        self.stopped.set(self.stopped.get() + 1);
        self.cleanup_remaining.set(context.remaining_budget());
        self.cleanup_cancelled
            .set(context.cancellation().is_cancelled());
        Box::pin(futures::future::ready(Ok(())))
    }
}

#[derive(Debug)]
struct LateAdapter {
    completion: Rc<RefCell<Option<oneshot::Receiver<()>>>>,
    entered: Rc<Cell<bool>>,
    constructed: Rc<Cell<bool>>,
    activated: Rc<Cell<bool>>,
    stopped: Rc<Cell<usize>>,
    cleanup_remaining: Rc<Cell<Option<Duration>>>,
    cleanup_cancelled: Rc<Cell<bool>>,
}

impl NativeExecutionAdapter for LateAdapter {
    fn supports_runtime_profile(&self, version: u32, profile: &str) -> bool {
        version == 2 && profile == "lenso.native-authoring@2"
    }

    fn prepare(&self, _plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure> {
        let lifecycle = LateLifecycle {
            completion: self.completion.clone(),
            entered: self.entered.clone(),
            constructed: self.constructed.clone(),
            activated: self.activated.clone(),
            stopped: self.stopped.clone(),
            cleanup_remaining: self.cleanup_remaining.clone(),
            cleanup_cancelled: self.cleanup_cancelled.clone(),
        };
        Ok(PreparedNativeApp::new(
            vec![],
            BTreeMap::from([(
                "plugin".to_owned(),
                PreparedNativePlugin::new(vec![], lifecycle),
            )]),
        ))
    }
}

fn plan() -> ResolvedAppPlan {
    ResolvedAppPlan::new(
        vec![
            PluginInstancePlan::new("plugin", "late").with_authoring(2, "lenso.native-authoring@2"),
        ],
        vec![],
    )
}

#[test]
fn cancellation_followed_by_late_construction_cleans_once_without_activation() {
    let driver = DeterministicDriver::new();
    let cancellation = CancellationToken::new();
    let (finish, completion) = oneshot::channel();
    let entered = Rc::new(Cell::new(false));
    let constructed = Rc::new(Cell::new(false));
    let activated = Rc::new(Cell::new(false));
    let stopped = Rc::new(Cell::new(0));
    let cleanup_remaining = Rc::new(Cell::new(None));
    let cleanup_cancelled = Rc::new(Cell::new(true));
    let adapter = LateAdapter {
        completion: Rc::new(RefCell::new(Some(completion))),
        entered: entered.clone(),
        constructed: constructed.clone(),
        activated: activated.clone(),
        stopped: stopped.clone(),
        cleanup_remaining: cleanup_remaining.clone(),
        cleanup_cancelled: cleanup_cancelled.clone(),
    };
    let context = InvocationContext::new(7, Some(Duration::from_secs(1)), cancellation.clone());
    let outcome = driver.run(async {
        let startup = Kernel::start_controlled(
            plan(),
            driver.clone(),
            ExecutionAdapterCatalog::single(adapter),
            RuntimeDiagnostics::new(),
            context,
            Duration::from_secs(1),
        );
        futures::pin_mut!(startup);
        poll_fn(|cx| {
            assert!(startup.as_mut().poll(cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        driver.yield_now().await;
        assert!(entered.get());
        driver.advance(Duration::from_secs(1));
        cancellation.cancel();
        driver.advance(Duration::from_millis(400));
        finish.send(()).unwrap();
        let outcome = startup.await;
        for _ in 0..8 {
            driver.yield_now().await;
        }
        outcome
    });
    assert!(matches!(
        outcome,
        Err(RuntimeFailure::Cancelled { request_id: 7 })
    ));
    assert!(constructed.get());
    assert!(!activated.get());
    assert_eq!(stopped.get(), 1);
    assert_eq!(cleanup_remaining.get(), Some(Duration::from_millis(600)));
    assert!(!cleanup_cancelled.get());
}

#[test]
fn dropping_the_startup_waiter_does_not_drop_the_constructor() {
    let driver = DeterministicDriver::new();
    let (finish, completion) = oneshot::channel();
    let entered = Rc::new(Cell::new(false));
    let constructed = Rc::new(Cell::new(false));
    let activated = Rc::new(Cell::new(false));
    let stopped = Rc::new(Cell::new(0));
    let cleanup_remaining = Rc::new(Cell::new(None));
    let cleanup_cancelled = Rc::new(Cell::new(true));
    let adapter = LateAdapter {
        completion: Rc::new(RefCell::new(Some(completion))),
        entered: entered.clone(),
        constructed: constructed.clone(),
        activated: activated.clone(),
        stopped: stopped.clone(),
        cleanup_remaining: cleanup_remaining.clone(),
        cleanup_cancelled: cleanup_cancelled.clone(),
    };
    driver.run(async {
        let mut startup = Box::pin(Kernel::start_native(plan(), driver.clone(), adapter));
        poll_fn(|cx| {
            assert!(startup.as_mut().poll(cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        driver.yield_now().await;
        assert!(entered.get());
        drop(startup);
        finish.send(()).unwrap();
        for _ in 0..8 {
            driver.yield_now().await;
        }
    });
    assert!(constructed.get());
    assert!(!activated.get());
    assert_eq!(stopped.get(), 1);
    assert!(cleanup_remaining.get().is_some());
    assert!(!cleanup_cancelled.get());
}

#[test]
fn constructor_returning_after_the_shared_cleanup_deadline_is_retained_without_stop() {
    let driver = DeterministicDriver::new();
    let (finish, completion) = oneshot::channel();
    let entered = Rc::new(Cell::new(false));
    let constructed = Rc::new(Cell::new(false));
    let activated = Rc::new(Cell::new(false));
    let stopped = Rc::new(Cell::new(0));
    let cleanup_remaining = Rc::new(Cell::new(None));
    let cleanup_cancelled = Rc::new(Cell::new(true));
    let adapter = LateAdapter {
        completion: Rc::new(RefCell::new(Some(completion))),
        entered: entered.clone(),
        constructed: constructed.clone(),
        activated: activated.clone(),
        stopped: stopped.clone(),
        cleanup_remaining: cleanup_remaining.clone(),
        cleanup_cancelled,
    };
    let context = InvocationContext::new(9, Some(Duration::from_secs(1)), CancellationToken::new());
    let outcome = driver.run(async {
        let startup = Kernel::start_controlled(
            plan(),
            driver.clone(),
            ExecutionAdapterCatalog::single(adapter),
            RuntimeDiagnostics::new(),
            context,
            Duration::from_secs(1),
        );
        futures::pin_mut!(startup);
        poll_fn(|cx| {
            assert!(startup.as_mut().poll(cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        driver.yield_now().await;
        assert!(entered.get());
        driver.advance(Duration::from_secs(1));
        let outcome = startup.await;
        driver.advance(Duration::from_secs(2));
        finish.send(()).unwrap();
        for _ in 0..8 {
            driver.yield_now().await;
        }
        outcome
    });

    assert!(matches!(
        outcome,
        Err(RuntimeFailure::DeadlineExceeded { request_id: 9 })
    ));
    assert!(constructed.get());
    assert!(!activated.get());
    assert_eq!(stopped.get(), 0);
    assert_eq!(cleanup_remaining.get(), None);
}
