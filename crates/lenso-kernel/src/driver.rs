use std::future::poll_fn;

use super::{
    AbortHandle, CancellationToken, Cell, Context, Duration, Either, Future, FutureExt,
    InvocationContext, LocalBoxFuture, NativeAppRuntime, Pin, Poll, Rc, RefCell,
    RequestAdmissionPlan, RuntimeFailure, SpawnError, VecDeque, begin_module_supervision, oneshot,
    pending, schedule_module_supervision, select,
};

/// A task owned by a Runtime Driver's single-threaded local lane.
pub type LocalTask = Pin<Box<dyn Future<Output = ()> + 'static>>;

/// Result of joining a Runtime Driver task.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskOutcome {
    /// The task completed normally.
    Completed,
    /// The task observed cooperative cancellation.
    Cancelled,
    /// The task or its Runtime Driver wrapper terminated abnormally.
    Failed,
}

/// Driver-owned handle used to cancel and join local work.
#[derive(Debug)]
pub struct DriverTask {
    pub(super) abort: AbortHandle,
    pub(super) completion: oneshot::Receiver<TaskOutcome>,
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

    pub(super) fn abort_handle(&self) -> AbortHandle {
        self.abort.clone()
    }
}

impl Future for DriverTask {
    type Output = TaskOutcome;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.completion)
            .poll(context)
            .map(|outcome| outcome.unwrap_or(TaskOutcome::Failed))
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

    /// Waits for Runner control work or until the supplied monotonic instant.
    ///
    /// Drivers with an event source should override this to park the lane. The
    /// yielding default preserves compatibility with deterministic and embedded
    /// Drivers that advance work cooperatively.
    fn wait_for_runtime_event(&self, _deadline: Duration) -> LocalBoxFuture<'static, ()> {
        self.yield_now()
    }

    /// Supplies deterministic or entropy-backed jitter bounded by the policy value.
    fn jitter(&self, _maximum: Duration) -> Duration {
        Duration::ZERO
    }

    /// Schedules Kernel-owned work on the local task lane.
    fn spawn_local(&self, task: LocalTask) -> Result<DriverTask, SpawnError>;

    /// Reports whether the embedding Runner requested shutdown.
    fn shutdown_requested(&self) -> bool;
}

#[derive(Clone)]
pub(super) struct DriverControl {
    pub(super) now: Rc<dyn Fn() -> Duration>,
    pub(super) sleep_until: Rc<dyn Fn(Duration) -> LocalBoxFuture<'static, ()>>,
    pub(super) yield_now: Rc<dyn Fn() -> LocalBoxFuture<'static, ()>>,
    pub(super) jitter: Rc<dyn Fn(Duration) -> Duration>,
    pub(super) spawn_local: Rc<dyn Fn(LocalTask) -> Result<DriverTask, SpawnError>>,
}

impl std::fmt::Debug for DriverControl {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DriverControl")
            .finish_non_exhaustive()
    }
}

impl DriverControl {
    pub(super) fn new<D: RuntimeDriver>(driver: &D) -> Self {
        let now_driver = driver.clone();
        let sleep_driver = driver.clone();
        let yield_driver = driver.clone();
        let jitter_driver = driver.clone();
        let spawn_driver = driver.clone();
        Self {
            now: Rc::new(move || now_driver.now()),
            sleep_until: Rc::new(move |deadline| sleep_driver.sleep_until(deadline)),
            yield_now: Rc::new(move || yield_driver.yield_now()),
            jitter: Rc::new(move |maximum| jitter_driver.jitter(maximum)),
            spawn_local: Rc::new(move |task| spawn_driver.spawn_local(task)),
        }
    }
}

pub(super) async fn wait_until<F: Future>(
    driver: &DriverControl,
    deadline: Duration,
    future: F,
) -> Option<F::Output> {
    let work = future.fuse();
    let timer = (driver.sleep_until)(deadline).fuse();
    futures::pin_mut!(work, timer);
    match select(work, timer).await {
        Either::Left((output, _)) => Some(output),
        Either::Right(((), _)) => None,
    }
}

/// A bounded, per-binding Operation admission state.
#[derive(Clone, Debug)]
pub(super) struct RequestAdmission {
    pub(super) limits: RequestAdmissionPlan,
    pub(super) state: Rc<RequestAdmissionState>,
}

#[derive(Debug, Default)]
pub(super) struct RequestAdmissionState {
    pub(super) active: Cell<usize>,
    pub(super) queued: Cell<usize>,
    pub(super) waiters: RefCell<VecDeque<Rc<QueueWaiter>>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum QueueWaiterStatus {
    Waiting,
    Woken,
    Acquired,
    Cancelled,
}

#[derive(Debug)]
pub(super) struct QueueWaiter {
    pub(super) status: Cell<QueueWaiterStatus>,
    pub(super) wakeup: RefCell<Option<oneshot::Sender<()>>>,
}

impl RequestAdmission {
    pub(super) fn new(limits: RequestAdmissionPlan) -> Self {
        Self {
            limits,
            state: Rc::new(RequestAdmissionState::default()),
        }
    }

    pub(super) fn queue_depth(&self) -> usize {
        self.state.queued.get()
    }

    pub(crate) fn try_acquire(
        &self,
        capability: &'static str,
        operation: &str,
        context: &InvocationContext,
        driver: &DriverControl,
    ) -> Result<RequestPermit, RuntimeFailure> {
        ensure_context_active(driver, context)?;
        if self.state.active.get() < self.limits.max_concurrency() {
            self.state.active.set(self.state.active.get() + 1);
            return Ok(RequestPermit {
                state: self.state.clone(),
            });
        }
        Err(RuntimeFailure::ResourceExhausted {
            capability,
            operation: operation.to_owned(),
        })
    }

    pub(super) async fn acquire(
        &self,
        capability: &'static str,
        operation: &str,
        context: &InvocationContext,
        driver: &DriverControl,
    ) -> Result<RequestPermit, RuntimeFailure> {
        if let Ok(permit) = self.try_acquire(capability, operation, context, driver) {
            return Ok(permit);
        }
        if let Err(error) = ensure_context_active(driver, context) {
            return Err(error);
        }

        if self.state.queued.get() >= self.limits.queue_capacity() {
            return Err(RuntimeFailure::ResourceExhausted {
                capability,
                operation: operation.to_owned(),
            });
        }

        let (wakeup, waiter) = oneshot::channel();
        let waiter_state = Rc::new(QueueWaiter {
            status: Cell::new(QueueWaiterStatus::Waiting),
            wakeup: RefCell::new(Some(wakeup)),
        });
        self.state.queued.set(self.state.queued.get() + 1);
        self.state
            .waiters
            .borrow_mut()
            .push_back(waiter_state.clone());
        let queued = QueuedAdmission {
            state: self.state.clone(),
            waiter_state,
            waiter,
        };
        queued.wait(driver, context).await
    }
}

#[derive(Debug)]
pub(super) struct QueuedAdmission {
    pub(super) state: Rc<RequestAdmissionState>,
    pub(super) waiter_state: Rc<QueueWaiter>,
    pub(super) waiter: oneshot::Receiver<()>,
}

impl QueuedAdmission {
    pub(super) async fn wait(
        mut self,
        driver: &DriverControl,
        context: &InvocationContext,
    ) -> Result<RequestPermit, RuntimeFailure> {
        let result = await_with_context(driver, context, &mut self.waiter).await;
        match result {
            Ok(Ok(())) => {
                if self.waiter_state.status.get() == QueueWaiterStatus::Woken {
                    self.waiter_state.status.set(QueueWaiterStatus::Acquired);
                    self.state.queued.set(self.state.queued.get() - 1);
                    Ok(RequestPermit {
                        state: self.state.clone(),
                    })
                } else {
                    Err(RuntimeFailure::Cancelled {
                        request_id: context.request_id(),
                    })
                }
            }
            Ok(Err(_)) => Err(RuntimeFailure::Cancelled {
                request_id: context.request_id(),
            }),
            Err(error) => Err(error),
        }
    }
}

impl Drop for QueuedAdmission {
    fn drop(&mut self) {
        let previous = self
            .waiter_state
            .status
            .replace(QueueWaiterStatus::Cancelled);
        match previous {
            QueueWaiterStatus::Waiting => {
                self.state.queued.set(self.state.queued.get() - 1);
            }
            QueueWaiterStatus::Woken => {
                self.state.queued.set(self.state.queued.get() - 1);
                self.state.active.set(self.state.active.get() - 1);
                wake_next(&self.state);
            }
            QueueWaiterStatus::Acquired | QueueWaiterStatus::Cancelled => {}
        }
        self.state
            .waiters
            .borrow_mut()
            .retain(|waiter| !Rc::ptr_eq(waiter, &self.waiter_state));
    }
}

#[derive(Debug)]
pub(super) struct RequestPermit {
    pub(super) state: Rc<RequestAdmissionState>,
}

impl Drop for RequestPermit {
    fn drop(&mut self) {
        self.state.active.set(self.state.active.get() - 1);
        wake_next(&self.state);
    }
}

pub(super) fn wake_next(state: &Rc<RequestAdmissionState>) {
    loop {
        let Some(waiter) = state.waiters.borrow_mut().pop_front() else {
            return;
        };
        if waiter.status.replace(QueueWaiterStatus::Woken) != QueueWaiterStatus::Waiting {
            continue;
        }
        state.active.set(state.active.get() + 1);
        let sent = waiter
            .wakeup
            .borrow_mut()
            .take()
            .is_some_and(|wakeup| wakeup.send(()).is_ok());
        if sent {
            return;
        }
        waiter.status.set(QueueWaiterStatus::Cancelled);
        state.active.set(state.active.get() - 1);
        state.queued.set(state.queued.get() - 1);
    }
}

pub(super) async fn await_with_context<F: Future>(
    driver: &DriverControl,
    context: &InvocationContext,
    future: F,
) -> Result<F::Output, RuntimeFailure> {
    ensure_context_active(driver, context)?;

    let work = future.fuse();
    futures::pin_mut!(work);
    if let Some(output) = poll_fn(|context| match work.as_mut().poll(context) {
        Poll::Ready(output) => Poll::Ready(Some(output)),
        Poll::Pending => Poll::Ready(None),
    })
    .await
    {
        return Ok(output);
    }
    let cancellation = context.cancellation.cancelled().fuse();
    let deadline: LocalBoxFuture<'static, ()> = context.deadline().map_or_else(
        || Box::pin(pending::<()>()) as LocalBoxFuture<'static, ()>,
        |deadline| (driver.sleep_until)(deadline),
    );
    let deadline = deadline.fuse();
    futures::pin_mut!(cancellation, deadline);

    match select(select(work, cancellation), deadline).await {
        Either::Left((Either::Left((output, _)), _)) => Ok(output),
        Either::Left((Either::Right(((), _)), _)) => Err(RuntimeFailure::Cancelled {
            request_id: context.request_id(),
        }),
        Either::Right(((), _)) => Err(RuntimeFailure::DeadlineExceeded {
            request_id: context.request_id(),
        }),
    }
}

pub(super) async fn await_with_generation_context<F: Future>(
    driver: &DriverControl,
    context: &InvocationContext,
    generation_cancellation: CancellationToken,
    capability: &'static str,
    future: F,
) -> Result<F::Output, RuntimeFailure> {
    ensure_context_active(driver, context)?;
    if generation_cancellation.is_cancelled() {
        return Err(RuntimeFailure::Unavailable { capability });
    }

    let work = future.fuse();
    futures::pin_mut!(work);
    if let Some(output) = poll_fn(|context| match work.as_mut().poll(context) {
        Poll::Ready(output) => Poll::Ready(Some(output)),
        Poll::Pending => Poll::Ready(None),
    })
    .await
    {
        return Ok(output);
    }
    let cancellation = context.cancellation.cancelled().fuse();
    let generation_cancellation = generation_cancellation.cancelled().fuse();
    let deadline: LocalBoxFuture<'static, ()> = context.deadline().map_or_else(
        || Box::pin(pending::<()>()) as LocalBoxFuture<'static, ()>,
        |deadline| (driver.sleep_until)(deadline),
    );
    let deadline = deadline.fuse();
    futures::pin_mut!(cancellation, generation_cancellation, deadline);

    match select(
        select(select(work, cancellation), generation_cancellation),
        deadline,
    )
    .await
    {
        Either::Left((Either::Left((Either::Left((output, _)), _)), _)) => Ok(output),
        Either::Left((Either::Left((Either::Right(((), _)), _)), _)) => {
            Err(RuntimeFailure::Cancelled {
                request_id: context.request_id(),
            })
        }
        Either::Left((Either::Right(((), _)), _)) => {
            Err(RuntimeFailure::Unavailable { capability })
        }
        Either::Right(((), _)) => Err(RuntimeFailure::DeadlineExceeded {
            request_id: context.request_id(),
        }),
    }
}

pub(super) fn is_module_failure(error: &RuntimeFailure) -> bool {
    matches!(error, RuntimeFailure::ModuleFailure { .. })
}

pub(super) fn schedule_module_supervision_after_failure(
    runtime: &Rc<NativeAppRuntime>,
    instance_key: &str,
    error: RuntimeFailure,
) -> RuntimeFailure {
    if is_module_failure(&error)
        && begin_module_supervision(runtime, instance_key).unwrap_or(false)
        && let Err(schedule_error) = schedule_module_supervision(runtime, instance_key)
    {
        return handle_supervision_schedule_failure(runtime, instance_key, schedule_error);
    }
    error
}

pub(super) fn handle_supervision_schedule_failure(
    runtime: &Rc<NativeAppRuntime>,
    instance_key: &str,
    error: RuntimeFailure,
) -> RuntimeFailure {
    let must_fail = runtime
        .supervision
        .borrow()
        .get(instance_key)
        .is_some_and(|state| state.criticality.is_critical() || state.required_path);
    if must_fail {
        runtime.terminal_failure.replace(Some(error.clone()));
        runtime.begin_shutdown();
    }
    error
}

pub(super) fn ensure_context_active(
    driver: &DriverControl,
    context: &InvocationContext,
) -> Result<(), RuntimeFailure> {
    if context.is_cancelled() {
        return Err(RuntimeFailure::Cancelled {
            request_id: context.request_id(),
        });
    }
    if context.is_expired((driver.now)()) {
        return Err(RuntimeFailure::DeadlineExceeded {
            request_id: context.request_id(),
        });
    }
    Ok(())
}

/// The result of bounded cleanup after a graceful shutdown request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShutdownOutcome {
    /// Every managed task, resource, and Module generation was cleaned up.
    Clean,
    /// Cleanup completed but a Module or resource reported a Runtime Failure.
    RuntimeFailure { error: RuntimeFailure },
    /// The global shutdown deadline expired; remaining work was terminated.
    Timeout,
}

/// A successful terminal result returned to the embedding Runner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminalOutcome {
    /// The App completed a bounded clean shutdown.
    CleanShutdown,
    /// The App could not start because a Module reported a startup failure.
    StartupFailure { error: RuntimeFailure },
    /// The running App reported a Runtime Failure during terminal cleanup.
    RuntimeFailure { error: RuntimeFailure },
    /// The running App failed and cleanup reported a second Runtime Failure.
    RuntimeFailureDuringShutdown {
        error: RuntimeFailure,
        cleanup_error: RuntimeFailure,
    },
    /// The running App failed and cleanup then exceeded its global deadline.
    RuntimeFailureWithShutdownTimeout { error: RuntimeFailure },
    /// The App exceeded its one global shutdown deadline.
    ShutdownTimeout,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DeterministicDriver;

    #[test]
    fn ready_work_does_not_register_cancellation_waiters() {
        let driver = DeterministicDriver::new();
        let control = DriverControl::new(&driver);
        let caller_cancellation = CancellationToken::new();
        let generation_cancellation = CancellationToken::new();
        let context = InvocationContext::new(
            1,
            Some(Duration::from_millis(10)),
            caller_cancellation.clone(),
        );
        let observed = Rc::new(Cell::new((usize::MAX, usize::MAX)));
        let observed_waiters = observed.clone();
        let work_caller = caller_cancellation.clone();
        let work_generation = generation_cancellation.clone();
        let work = poll_fn(move |_| {
            observed_waiters.set((
                work_caller.state.waiters.borrow().len(),
                work_generation.state.waiters.borrow().len(),
            ));
            Poll::Ready("done")
        });

        let outcome = driver.run(await_with_generation_context(
            &control,
            &context,
            generation_cancellation,
            "test.capability",
            work,
        ));

        assert_eq!(outcome, Ok("done"));
        assert_eq!(observed.get(), (0, 0));
    }
}
