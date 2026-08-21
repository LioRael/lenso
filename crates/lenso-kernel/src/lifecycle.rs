use super::{
    AbortHandle, AssertUnwindSafe, Cell, Context, DriverControl, DriverTask, Duration, Future,
    FutureExt, LocalBoxFuture, LocalTask, ModuleDependencies, ModuleLifecyclePhase, Pin, Poll, Rc,
    RefCell, RuntimeDriver, RuntimeFailure, SpawnError, TaskOutcome, oneshot, wait_until,
};

/// A shared App-wide signal that opens exactly once after every Module activates.
#[derive(Clone, Debug)]
pub struct AppReadyGate {
    pub(super) state: Rc<AppReadyState>,
}

#[derive(Debug)]
pub(super) struct AppReadyState {
    pub(super) open: Cell<bool>,
    pub(super) waiters: RefCell<Vec<oneshot::Sender<()>>>,
}

impl AppReadyGate {
    /// Creates a closed App Ready Gate.
    pub fn new() -> Self {
        Self {
            state: Rc::new(AppReadyState {
                open: Cell::new(false),
                waiters: RefCell::new(Vec::new()),
            }),
        }
    }

    /// Returns whether the App Ready Gate has opened.
    pub fn is_open(&self) -> bool {
        self.state.open.get()
    }

    /// Waits until the whole App has completed activation.
    pub fn wait(&self) -> LocalBoxFuture<'static, ()> {
        if self.is_open() {
            return Box::pin(futures::future::ready(()));
        }

        let (wakeup, waiter) = oneshot::channel();
        self.state.waiters.borrow_mut().push(wakeup);
        Box::pin(async move {
            let _ = waiter.await;
        })
    }

    pub(super) fn open(&self) {
        if self.state.open.replace(true) {
            return;
        }
        for waiter in self.state.waiters.borrow_mut().drain(..) {
            let _ = waiter.send(());
        }
    }
}

impl Default for AppReadyGate {
    fn default() -> Self {
        Self::new()
    }
}

/// App-wide admission for externally triggered work.
#[derive(Clone, Debug)]
pub struct AppAdmission {
    pub(super) state: Rc<AppAdmissionState>,
}

#[derive(Debug)]
pub(super) struct AppAdmissionState {
    pub(super) open: Cell<bool>,
}

impl AppAdmission {
    pub(super) fn new() -> Self {
        Self {
            state: Rc::new(AppAdmissionState {
                open: Cell::new(false),
            }),
        }
    }

    /// Returns whether new externally triggered work may be admitted.
    pub fn is_open(&self) -> bool {
        self.state.open.get()
    }

    /// Returns whether new externally triggered work is rejected.
    pub fn is_closed(&self) -> bool {
        !self.is_open()
    }

    pub(super) fn open(&self) {
        self.state.open.set(true);
    }

    pub(super) fn close(&self) {
        self.state.open.set(false);
    }
}

/// Cooperative cancellation shared by one Module Instance generation.
#[derive(Clone, Debug)]
pub struct CancellationToken {
    pub(super) state: Rc<CancellationState>,
}

#[derive(Debug)]
pub(super) struct CancellationState {
    pub(super) cancelled: Cell<bool>,
    pub(super) next_waiter_id: Cell<usize>,
    pub(super) waiters: RefCell<Vec<(usize, oneshot::Sender<()>)>>,
}

impl CancellationToken {
    /// Creates a token that has not been cancelled.
    pub fn new() -> Self {
        Self {
            state: Rc::new(CancellationState {
                cancelled: Cell::new(false),
                next_waiter_id: Cell::new(0),
                waiters: RefCell::new(Vec::new()),
            }),
        }
    }

    /// Returns whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.state.cancelled.get()
    }

    /// Waits until cancellation is requested.
    pub fn cancelled(&self) -> LocalBoxFuture<'static, ()> {
        if self.is_cancelled() {
            return Box::pin(futures::future::ready(()));
        }
        let (wakeup, waiter) = oneshot::channel();
        let waiter_id = self.state.next_waiter_id.get();
        self.state.next_waiter_id.set(waiter_id.saturating_add(1));
        self.state.waiters.borrow_mut().push((waiter_id, wakeup));
        Box::pin(CancellationWaiter {
            state: self.state.clone(),
            waiter_id,
            receiver: waiter,
            registered: true,
        })
    }

    /// Requests cooperative cancellation and wakes every current waiter.
    pub fn cancel(&self) {
        if self.state.cancelled.replace(true) {
            return;
        }
        for (_, waiter) in self.state.waiters.borrow_mut().drain(..) {
            let _ = waiter.send(());
        }
    }
}

#[derive(Debug)]
pub(super) struct CancellationWaiter {
    pub(super) state: Rc<CancellationState>,
    pub(super) waiter_id: usize,
    pub(super) receiver: oneshot::Receiver<()>,
    pub(super) registered: bool,
}

impl Future for CancellationWaiter {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        match Pin::new(&mut self.receiver).poll(context) {
            Poll::Ready(_) => {
                self.registered = false;
                Poll::Ready(())
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Drop for CancellationWaiter {
    fn drop(&mut self) {
        if !self.registered {
            return;
        }
        self.state
            .waiters
            .borrow_mut()
            .retain(|(waiter_id, _)| *waiter_id != self.waiter_id);
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// A future used to release one Driver-backed managed resource.
pub type ResourceFuture = LocalBoxFuture<'static, Result<(), RuntimeFailure>>;

/// A resource whose release is owned by one Module Instance generation.
pub trait ManagedResource: std::fmt::Debug + 'static {
    /// Releases the resource exactly once when its generation is cleaned up.
    fn release(&self) -> ResourceFuture;
}

/// Error returned when a resource cannot be registered in a closed scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceRegistrationError {
    /// The Module generation has begun shutdown or rollback cleanup.
    ScopeClosed,
}

pub(super) struct ManagedResourceEntry {
    pub(super) resource: Rc<dyn ManagedResource>,
    pub(super) release: RefCell<ManagedResourceRelease>,
}

pub(super) enum ManagedResourceRelease {
    Pending,
    Running(ResourceFuture),
    Complete(Result<(), RuntimeFailure>),
}

impl std::fmt::Debug for ManagedResourceEntry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = match &*self.release.borrow() {
            ManagedResourceRelease::Pending => "pending",
            ManagedResourceRelease::Running(_) => "running",
            ManagedResourceRelease::Complete(Ok(())) => "released",
            ManagedResourceRelease::Complete(Err(_)) => "failed",
        };
        formatter
            .debug_struct("ManagedResourceEntry")
            .field("release", &state)
            .finish_non_exhaustive()
    }
}

/// A handle that releases one managed resource at most once.
#[derive(Clone, Debug)]
pub struct ManagedResourceHandle {
    pub(super) entry: Rc<ManagedResourceEntry>,
}

impl ManagedResourceHandle {
    /// Returns whether this resource's release future completed.
    pub fn is_released(&self) -> bool {
        matches!(
            &*self.entry.release.borrow(),
            ManagedResourceRelease::Complete(_)
        )
    }

    /// Releases this resource once; repeated calls are successful no-ops.
    pub async fn release(&self) -> Result<(), RuntimeFailure> {
        ManagedResourceReleaseOperation {
            entry: self.entry.clone(),
        }
        .await
    }
}

pub(super) struct ManagedResourceReleaseOperation {
    pub(super) entry: Rc<ManagedResourceEntry>,
}

impl Future for ManagedResourceReleaseOperation {
    type Output = Result<(), RuntimeFailure>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let mut release = self.entry.release.borrow_mut();
        if matches!(*release, ManagedResourceRelease::Pending) {
            *release = ManagedResourceRelease::Running(self.entry.resource.release());
        }
        match &mut *release {
            ManagedResourceRelease::Running(future) => match future.as_mut().poll(context) {
                Poll::Ready(result) => {
                    *release = ManagedResourceRelease::Complete(result.clone());
                    Poll::Ready(result)
                }
                Poll::Pending => Poll::Pending,
            },
            ManagedResourceRelease::Complete(result) => Poll::Ready(result.clone()),
            ManagedResourceRelease::Pending => unreachable!("pending release was started"),
        }
    }
}

/// A Module-generation resource scope backed by Driver-polled cleanup futures.
#[derive(Clone)]
pub struct ManagedResourceScope {
    pub(super) state: Rc<ManagedResourceScopeState>,
}

#[derive(Debug, Default)]
pub(super) struct ManagedResourceScopeState {
    pub(super) resources: RefCell<Vec<ManagedResourceHandle>>,
    pub(super) closed: Cell<bool>,
}

impl std::fmt::Debug for ManagedResourceScope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ManagedResourceScope")
            .field("resource_count", &self.resource_count())
            .finish()
    }
}

impl ManagedResourceScope {
    pub(super) fn new() -> Self {
        Self {
            state: Rc::new(ManagedResourceScopeState::default()),
        }
    }

    /// Registers a resource owned by this Module Instance generation.
    pub fn register(
        &self,
        resource: impl ManagedResource,
    ) -> Result<ManagedResourceHandle, ResourceRegistrationError> {
        if self.state.closed.get() {
            return Err(ResourceRegistrationError::ScopeClosed);
        }
        let handle = ManagedResourceHandle {
            entry: Rc::new(ManagedResourceEntry {
                resource: Rc::new(resource),
                release: RefCell::new(ManagedResourceRelease::Pending),
            }),
        };
        self.state.resources.borrow_mut().push(handle.clone());
        Ok(handle)
    }

    /// Returns the number of resources that still need cleanup.
    pub fn resource_count(&self) -> usize {
        self.state
            .resources
            .borrow()
            .iter()
            .filter(|resource| !resource.is_released())
            .count()
    }

    pub(super) fn close(&self) {
        self.state.closed.set(true);
    }

    pub(super) async fn release_all(&self) -> Option<RuntimeFailure> {
        let resources = std::mem::take(&mut *self.state.resources.borrow_mut());
        let mut first_error = None;
        for resource in resources {
            if let Err(error) = resource.release().await
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        first_error
    }

    pub(super) async fn release_all_until(
        &self,
        driver: &DriverControl,
        deadline: Duration,
    ) -> Result<Option<RuntimeFailure>, ()> {
        let resources = std::mem::take(&mut *self.state.resources.borrow_mut());
        let mut first_error = None;
        for (index, resource) in resources.iter().enumerate() {
            match wait_until(driver, deadline, resource.release()).await {
                Some(Ok(())) => {}
                Some(Err(error)) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
                None => {
                    self.state
                        .resources
                        .borrow_mut()
                        .extend(resources.into_iter().skip(index));
                    return Err(());
                }
            }
        }
        Ok(first_error)
    }
}

/// A Kernel-owned task handle that is cleaned up with its Module generation.
#[derive(Clone, Debug)]
pub struct ManagedTask {
    pub(super) task: Rc<RefCell<Option<DriverTask>>>,
    pub(super) abort: AbortHandle,
    pub(super) failed: Rc<Cell<bool>>,
}

impl ManagedTask {
    pub(super) fn from_driver_task(task: DriverTask) -> Self {
        Self {
            abort: task.abort_handle(),
            task: Rc::new(RefCell::new(Some(task))),
            failed: Rc::new(Cell::new(false)),
        }
    }

    /// Requests cancellation of the underlying task.
    pub fn cancel(&self) {
        self.abort.abort();
    }

    pub(super) async fn join(&self) -> TaskOutcome {
        let task = self.task.borrow_mut().take();
        if let Some(task) = task {
            let outcome = task.await;
            if self.failed.get() {
                TaskOutcome::Failed
            } else {
                outcome
            }
        } else if self.failed.get() {
            TaskOutcome::Failed
        } else {
            TaskOutcome::Completed
        }
    }
}

/// Error returned when a managed task cannot be admitted to its scope.
#[derive(Debug)]
pub enum ManagedTaskError {
    /// The Module generation has begun shutdown or rollback cleanup.
    ScopeClosed,
    /// The Runtime Driver rejected the local task.
    Driver(SpawnError),
}

impl From<SpawnError> for ManagedTaskError {
    fn from(error: SpawnError) -> Self {
        Self::Driver(error)
    }
}

/// A Module-generation task scope backed by the selected Runtime Driver.
#[derive(Clone)]
pub struct ManagedTaskScope {
    pub(super) spawn: Rc<dyn Fn(LocalTask) -> Result<DriverTask, SpawnError>>,
    pub(super) state: Rc<ManagedTaskScopeState>,
}

pub(super) struct ManagedTaskScopeState {
    pub(super) tasks: RefCell<Vec<ManagedTask>>,
    pub(super) closed: Cell<bool>,
    pub(super) cancellation: CancellationToken,
    pub(super) failure_handler: RefCell<Option<Rc<dyn Fn()>>>,
    pub(super) unreported_failure: Cell<bool>,
}

impl Default for ManagedTaskScopeState {
    fn default() -> Self {
        Self {
            tasks: RefCell::new(Vec::new()),
            closed: Cell::new(false),
            cancellation: CancellationToken::new(),
            failure_handler: RefCell::new(None),
            unreported_failure: Cell::new(false),
        }
    }
}

impl std::fmt::Debug for ManagedTaskScopeState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ManagedTaskScopeState")
            .field("task_count", &self.tasks.borrow().len())
            .field("closed", &self.closed.get())
            .field("unreported_failure", &self.unreported_failure.get())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for ManagedTaskScope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ManagedTaskScope")
            .field("task_count", &self.task_count())
            .finish()
    }
}

impl ManagedTaskScope {
    pub(super) fn new<D: RuntimeDriver>(driver: &D) -> Self {
        let spawner = driver.clone();
        Self {
            spawn: Rc::new(move |task| spawner.spawn_local(task)),
            state: Rc::new(ManagedTaskScopeState::default()),
        }
    }

    pub(super) fn new_from_driver_control(driver: &DriverControl) -> Self {
        let spawn = driver.spawn_local.clone();
        Self {
            spawn,
            state: Rc::new(ManagedTaskScopeState::default()),
        }
    }

    /// Spawns work owned by this Module Instance generation.
    pub fn spawn_local(&self, task: LocalTask) -> Result<ManagedTask, ManagedTaskError> {
        if self.state.closed.get() {
            return Err(ManagedTaskError::ScopeClosed);
        }
        let failed = Rc::new(Cell::new(false));
        let task_failed = failed.clone();
        let state = self.state.clone();
        let monitored = Box::pin(async move {
            if AssertUnwindSafe(task).catch_unwind().await.is_err() {
                task_failed.set(true);
                state.report_failure();
            }
        });
        let driver_task = (self.spawn)(monitored)?;
        let handle = ManagedTask {
            failed,
            ..ManagedTask::from_driver_task(driver_task)
        };
        self.state.tasks.borrow_mut().push(handle.clone());
        Ok(handle)
    }

    /// Returns the number of tasks still tracked by this scope.
    pub fn task_count(&self) -> usize {
        self.state.tasks.borrow().len()
    }

    /// Returns the cooperative cancellation token for this generation.
    pub fn cancellation(&self) -> CancellationToken {
        self.state.cancellation.clone()
    }

    pub(super) fn close(&self) {
        self.state.closed.set(true);
        self.state.cancellation.cancel();
    }

    pub(super) fn set_failure_handler(&self, handler: &Rc<dyn Fn()>) {
        self.state.failure_handler.replace(Some(handler.clone()));
        if self.state.unreported_failure.replace(false) {
            handler();
        }
    }

    pub(super) fn cancel(&self) {
        self.state.cancellation.cancel();
    }

    pub(super) fn abort_all(&self) {
        for task in self.state.tasks.borrow().iter() {
            task.cancel();
        }
    }

    pub(super) async fn cancel_all(&self) {
        self.close();
        let tasks = std::mem::take(&mut *self.state.tasks.borrow_mut());
        for task in tasks {
            task.cancel();
            let _ = task.join().await;
        }
    }

    pub(super) async fn drain_until(&self, driver: &DriverControl, deadline: Duration) -> bool {
        self.cancel();
        let tasks = std::mem::take(&mut *self.state.tasks.borrow_mut());
        for (index, task) in tasks.iter().enumerate() {
            if wait_until(driver, deadline, task.join()).await.is_none() {
                for pending in tasks.iter().skip(index) {
                    pending.cancel();
                }
                return false;
            }
        }
        true
    }
}

impl ManagedTaskScopeState {
    pub(super) fn report_failure(&self) {
        let handler = self.failure_handler.borrow().clone();
        if let Some(handler) = handler {
            handler();
        } else {
            self.unreported_failure.set(true);
        }
    }
}

/// Context supplied while a Module reserves reversible resources.
#[derive(Clone, Debug)]
pub struct PrepareContext {
    pub(super) instance_key: String,
    pub(super) entrypoint: String,
    pub(super) configuration: String,
    pub(super) dependencies: ModuleDependencies,
    pub(super) resources: ManagedResourceScope,
    pub(super) cancellation: CancellationToken,
    pub(super) admission: AppAdmission,
}

impl PrepareContext {
    /// Returns the App-local Module Instance key.
    pub fn instance_key(&self) -> &str {
        &self.instance_key
    }

    /// Returns the exact package entrypoint selected by the immutable Plan.
    pub fn entrypoint(&self) -> &str {
        &self.entrypoint
    }

    /// Returns opaque Module-owned configuration selected by the immutable Plan.
    pub fn configuration(&self) -> &str {
        &self.configuration
    }

    /// Returns the phase represented by this context.
    pub const fn phase(&self) -> ModuleLifecyclePhase {
        ModuleLifecyclePhase::Prepare
    }

    /// Returns the explicit dependencies selected for this Instance.
    pub fn dependencies(&self) -> &ModuleDependencies {
        &self.dependencies
    }

    /// Returns the generation-owned resource scope.
    pub fn resources(&self) -> &ManagedResourceScope {
        &self.resources
    }

    /// Returns the generation-owned cooperative cancellation token.
    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    /// Returns the App admission state, which remains closed until readiness.
    pub fn admission(&self) -> AppAdmission {
        self.admission.clone()
    }
}

/// Context supplied while a Module initializes against prepared dependencies.
#[derive(Clone, Debug)]
pub struct ActivateContext {
    pub(super) instance_key: String,
    pub(super) dependencies: ModuleDependencies,
    pub(super) ready_gate: AppReadyGate,
    pub(super) tasks: ManagedTaskScope,
    pub(super) resources: ManagedResourceScope,
    pub(super) cancellation: CancellationToken,
    pub(super) admission: AppAdmission,
}

impl ActivateContext {
    /// Returns the App-local Module Instance key.
    pub fn instance_key(&self) -> &str {
        &self.instance_key
    }

    /// Returns the phase represented by this context.
    pub const fn phase(&self) -> ModuleLifecyclePhase {
        ModuleLifecyclePhase::Activate
    }

    /// Returns the explicit dependencies selected for this Instance.
    pub fn dependencies(&self) -> &ModuleDependencies {
        &self.dependencies
    }

    /// Returns the closed-until-fully-active App Ready Gate.
    pub fn ready_gate(&self) -> AppReadyGate {
        self.ready_gate.clone()
    }

    /// Returns the readiness context a Module may pass to managed work.
    pub fn readiness(&self) -> ReadinessContext {
        ReadinessContext {
            instance_key: self.instance_key.clone(),
            dependencies: self.dependencies.clone(),
            ready_gate: self.ready_gate.clone(),
            tasks: self.tasks.clone(),
            resources: self.resources.clone(),
            cancellation: self.cancellation.clone(),
            admission: self.admission.clone(),
        }
    }

    /// Returns the generation-owned task scope.
    pub fn tasks(&self) -> &ManagedTaskScope {
        &self.tasks
    }

    /// Returns the generation-owned resource scope.
    pub fn resources(&self) -> &ManagedResourceScope {
        &self.resources
    }

    /// Returns the generation-owned cooperative cancellation token.
    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    /// Returns the App admission state, which remains closed until readiness.
    pub fn admission(&self) -> AppAdmission {
        self.admission.clone()
    }
}

/// Context supplied after the App Ready Gate has opened.
#[derive(Clone, Debug)]
pub struct ReadinessContext {
    pub(super) instance_key: String,
    pub(super) dependencies: ModuleDependencies,
    pub(super) ready_gate: AppReadyGate,
    pub(super) tasks: ManagedTaskScope,
    pub(super) resources: ManagedResourceScope,
    pub(super) cancellation: CancellationToken,
    pub(super) admission: AppAdmission,
}

impl ReadinessContext {
    /// Returns the App-local Module Instance key.
    pub fn instance_key(&self) -> &str {
        &self.instance_key
    }

    /// Returns the phase represented by this context.
    pub const fn phase(&self) -> ModuleLifecyclePhase {
        ModuleLifecyclePhase::Ready
    }

    /// Returns the explicit dependencies selected for this Instance.
    pub fn dependencies(&self) -> &ModuleDependencies {
        &self.dependencies
    }

    /// Returns the opened App Ready Gate.
    pub fn ready_gate(&self) -> AppReadyGate {
        self.ready_gate.clone()
    }

    /// Waits for the App Ready Gate to open.
    pub fn wait(&self) -> LocalBoxFuture<'static, ()> {
        self.ready_gate.wait()
    }

    /// Returns whether the App Ready Gate has opened.
    pub fn is_open(&self) -> bool {
        self.ready_gate.is_open()
    }

    /// Returns the generation-owned task scope.
    pub fn tasks(&self) -> &ManagedTaskScope {
        &self.tasks
    }

    /// Returns the generation-owned resource scope.
    pub fn resources(&self) -> &ManagedResourceScope {
        &self.resources
    }

    /// Returns the generation-owned cooperative cancellation token.
    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    /// Returns whether new externally triggered work may be admitted.
    pub fn is_accepting(&self) -> bool {
        self.admission.is_open()
    }

    /// Returns the App admission state.
    pub fn admission(&self) -> AppAdmission {
        self.admission.clone()
    }
}

/// The reason a Module generation is being deactivated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeactivationReason {
    /// Startup failed and prepared work is being rolled back.
    StartupRollback,
    /// The embedding App requested a graceful stop.
    Shutdown,
    /// Supervision is releasing a failed generation before recreation.
    SupervisionRestart,
}

/// Context supplied while a Module releases one generation.
#[derive(Clone, Debug)]
pub struct DeactivateContext {
    pub(super) instance_key: String,
    pub(super) dependencies: ModuleDependencies,
    pub(super) reason: DeactivationReason,
    pub(super) tasks: ManagedTaskScope,
    pub(super) resources: ManagedResourceScope,
    pub(super) cancellation: CancellationToken,
    pub(super) admission: AppAdmission,
}

impl DeactivateContext {
    /// Returns the App-local Module Instance key.
    pub fn instance_key(&self) -> &str {
        &self.instance_key
    }

    /// Returns the phase represented by this context.
    pub const fn phase(&self) -> ModuleLifecyclePhase {
        ModuleLifecyclePhase::Deactivate
    }

    /// Returns the explicit dependencies selected for this Instance.
    pub fn dependencies(&self) -> &ModuleDependencies {
        &self.dependencies
    }

    /// Returns why this generation is being deactivated.
    pub const fn reason(&self) -> DeactivationReason {
        self.reason
    }

    /// Returns the generation-owned task scope.
    pub fn tasks(&self) -> &ManagedTaskScope {
        &self.tasks
    }

    /// Returns the generation-owned resource scope.
    pub fn resources(&self) -> &ManagedResourceScope {
        &self.resources
    }

    /// Returns the generation-owned cooperative cancellation token.
    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    /// Returns the App admission state, which is closed during deactivation.
    pub fn admission(&self) -> AppAdmission {
        self.admission.clone()
    }
}

/// The result type returned by prepare, activate, and deactivate hooks.
pub type ModuleFuture = LocalBoxFuture<'static, Result<(), RuntimeFailure>>;

/// Adapter-facing lifecycle Interface for one Module Instance generation.
pub trait ModuleLifecycle: std::fmt::Debug + 'static {
    /// Reserves reversible resources without exposing external work.
    fn prepare(&self, _context: PrepareContext) -> ModuleFuture {
        Box::pin(futures::future::ready(Ok(())))
    }

    /// Initializes the generation against already prepared dependencies.
    fn activate(&self, _context: ActivateContext) -> ModuleFuture {
        Box::pin(futures::future::ready(Ok(())))
    }

    /// Releases resources and work owned by this generation.
    fn deactivate(&self, _context: DeactivateContext) -> ModuleFuture {
        Box::pin(futures::future::ready(Ok(())))
    }
}

/// Default no-op lifecycle used by endpoint-only native fixtures.
#[derive(Debug, Default)]
pub struct NoopModuleLifecycle;

impl ModuleLifecycle for NoopModuleLifecycle {}
