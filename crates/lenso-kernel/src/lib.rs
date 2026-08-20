//! Portable Lenso vNext Kernel and Runtime Driver seam.

use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
    time::Duration,
};

use futures::{
    channel::oneshot,
    executor::{LocalPool, LocalSpawner},
    future::{AbortHandle, Abortable, LocalBoxFuture},
    task::{LocalSpawnExt, SpawnError},
};
use lenso_app_plan::{PLAN_SCHEMA_VERSION, ResolvedAppPlan};

/// A task owned by a Runtime Driver's single-threaded local lane.
pub type LocalTask = Pin<Box<dyn Future<Output = ()> + 'static>>;

/// Result of joining a Runtime Driver task.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskOutcome {
    /// The task completed normally.
    Completed,
    /// The task observed cooperative cancellation.
    Cancelled,
}

/// Driver-owned handle used to cancel and join local work.
#[derive(Debug)]
pub struct DriverTask {
    abort: AbortHandle,
    completion: oneshot::Receiver<TaskOutcome>,
}

impl DriverTask {
    /// Creates a task handle from Driver-owned cancellation and completion primitives.
    pub fn new(abort: AbortHandle, completion: oneshot::Receiver<TaskOutcome>) -> Self {
        Self { abort, completion }
    }

    /// Requests cooperative cancellation of this task.
    pub fn cancel(&self) {
        self.abort.abort();
    }
}

impl Future for DriverTask {
    type Output = TaskOutcome;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.completion)
            .poll(context)
            .map(|outcome| outcome.unwrap_or(TaskOutcome::Cancelled))
    }
}

/// Host facilities required to advance the portable Kernel.
pub trait RuntimeDriver: Clone + 'static {
    /// Returns the current monotonic instant relative to Driver startup.
    fn now(&self) -> Duration;

    /// Waits until the supplied monotonic instant.
    fn sleep_until(&self, deadline: Duration) -> LocalBoxFuture<'static, ()>;

    /// Cooperatively yields to other work on the local task lane.
    fn yield_now(&self) -> LocalBoxFuture<'static, ()>;

    /// Schedules Kernel-owned work on the local task lane.
    fn spawn_local(&self, task: LocalTask) -> Result<DriverTask, SpawnError>;

    /// Reports whether the embedding Runner requested shutdown.
    fn shutdown_requested(&self) -> bool;
}

/// A successful terminal result returned to the embedding Runner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalOutcome {
    /// The App ran to completion.
    Completed,
    /// The embedding Runner requested cooperative shutdown.
    ShutdownRequested,
}

/// A reason the Kernel rejected a Resolved App Plan before boot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanValidationError {
    /// The Plan schema cannot be executed by this Kernel version.
    UnsupportedSchemaVersion { expected: u32, actual: u32 },
}

/// The portable App execution engine.
#[derive(Debug)]
pub struct Kernel;

impl Kernel {
    /// Validates and boots one already resolved Plan.
    pub async fn boot<D: RuntimeDriver>(
        plan: ResolvedAppPlan,
        driver: D,
    ) -> Result<TerminalOutcome, PlanValidationError> {
        if plan.schema_version() != PLAN_SCHEMA_VERSION {
            return Err(PlanValidationError::UnsupportedSchemaVersion {
                expected: PLAN_SCHEMA_VERSION,
                actual: plan.schema_version(),
            });
        }

        driver.sleep_until(driver.now()).await;
        driver.yield_now().await;

        Ok(if driver.shutdown_requested() {
            TerminalOutcome::ShutdownRequested
        } else {
            TerminalOutcome::Completed
        })
    }
}

#[derive(Debug)]
struct DeterministicState {
    now: Cell<Duration>,
    shutdown_requested: Cell<bool>,
    pool: RefCell<Option<LocalPool>>,
    spawner: LocalSpawner,
    timers: RefCell<Vec<(Duration, oneshot::Sender<()>)>>,
}

/// A deterministic, OS-independent Runtime Driver for Kernel conformance tests.
#[derive(Clone, Debug)]
pub struct DeterministicDriver {
    state: Rc<DeterministicState>,
}

impl DeterministicDriver {
    /// Creates a Driver at monotonic instant zero.
    pub fn new() -> Self {
        let pool = LocalPool::new();
        let spawner = pool.spawner();
        Self {
            state: Rc::new(DeterministicState {
                now: Cell::new(Duration::ZERO),
                shutdown_requested: Cell::new(false),
                pool: RefCell::new(Some(pool)),
                spawner,
                timers: RefCell::new(Vec::new()),
            }),
        }
    }

    /// Runs a root Kernel future until its deterministic terminal result.
    pub fn run<F: Future>(&self, future: F) -> F::Output {
        let mut pool = self
            .state
            .pool
            .borrow_mut()
            .take()
            .expect("deterministic Driver cannot run recursively");
        let output = pool.run_until(future);
        self.state.pool.replace(Some(pool));
        output
    }

    /// Advances monotonic time without consulting a host clock.
    pub fn advance(&self, duration: Duration) {
        self.state.now.set(self.state.now.get() + duration);
        let now = self.state.now.get();
        let mut timers = self.state.timers.borrow_mut();
        let mut pending = Vec::with_capacity(timers.len());
        for (deadline, wakeup) in timers.drain(..) {
            if deadline <= now {
                let _ = wakeup.send(());
            } else {
                pending.push((deadline, wakeup));
            }
        }
        *timers = pending;
    }

    /// Requests cooperative Kernel shutdown.
    pub fn request_shutdown(&self) {
        self.state.shutdown_requested.set(true);
    }

    /// Returns the current deterministic monotonic instant.
    pub fn now(&self) -> Duration {
        self.state.now.get()
    }
}

impl Default for DeterministicDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeDriver for DeterministicDriver {
    fn now(&self) -> Duration {
        self.now()
    }

    fn sleep_until(&self, deadline: Duration) -> LocalBoxFuture<'static, ()> {
        if deadline <= self.now() {
            return Box::pin(futures::future::ready(()));
        }
        let (wakeup, sleeper) = oneshot::channel();
        self.state.timers.borrow_mut().push((deadline, wakeup));
        Box::pin(async move {
            let _ = sleeper.await;
        })
    }

    fn yield_now(&self) -> LocalBoxFuture<'static, ()> {
        let mut yielded = false;
        Box::pin(futures::future::poll_fn(move |context| {
            if yielded {
                Poll::Ready(())
            } else {
                yielded = true;
                context.waker().wake_by_ref();
                Poll::Pending
            }
        }))
    }

    fn spawn_local(&self, task: LocalTask) -> Result<DriverTask, SpawnError> {
        let (abort, registration) = AbortHandle::new_pair();
        let (completed, completion) = oneshot::channel();
        self.state.spawner.spawn_local(async move {
            let outcome = if Abortable::new(task, registration).await.is_ok() {
                TaskOutcome::Completed
            } else {
                TaskOutcome::Cancelled
            };
            let _ = completed.send(outcome);
        })?;
        Ok(DriverTask::new(abort, completion))
    }

    fn shutdown_requested(&self) -> bool {
        self.state.shutdown_requested.get()
    }
}
