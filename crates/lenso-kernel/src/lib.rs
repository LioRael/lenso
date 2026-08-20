//! Portable Lenso vNext Kernel and Runtime Driver seam.

use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet, VecDeque},
    future::Future,
    marker::PhantomData,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
    time::Duration,
};

use futures::{
    channel::oneshot,
    executor::{LocalPool, LocalSpawner},
    future::{AbortHandle, Abortable, Either, FutureExt, LocalBoxFuture, pending, select},
    task::{LocalSpawnExt, SpawnError},
};
use lenso_app_plan::{
    ExecutionClassId, ModuleCriticality, PlanResolutionError, RequestAdmissionPlan,
    ResolvedAppPlan, RestartMode, RestartPolicy,
};

type ErasedValue = Box<dyn Any>;
type ErasedDomainResult = Result<ErasedValue, ErasedValue>;
type NativeBindingTable = BTreeMap<(String, &'static str), Vec<NativeEndpointBinding>>;
type NativeEndpointStateTable = BTreeMap<(String, String), Rc<NativeEndpointState>>;

/// Static identity and Rust value types generated for one request Capability.
pub trait RequestCapability: 'static {
    /// Typed request value.
    type Request: 'static;
    /// Typed success value.
    type Response: 'static;
    /// Typed Capability-defined error value.
    type DomainError: 'static;
    /// Stable Capability series identity.
    const ID: &'static str;
    /// Exact generated Descriptor version.
    const DESCRIPTOR_VERSION: &'static str;
}

/// Kernel-generated identity for one logical request invocation.
pub type RequestId = u64;

/// Runtime-owned failure, kept separate from Capability-defined Domain Errors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeFailure {
    /// The consumer has no resolved binding for this Capability.
    Unavailable { capability: &'static str },
    /// The bound provider does not declare the requested Operation.
    UnknownOperation {
        capability: &'static str,
        operation: String,
    },
    /// A singular generated client was used for a requirement with many providers.
    AmbiguousBinding {
        capability: &'static str,
        providers: usize,
    },
    /// Generated native types disagreed with the prepared endpoint.
    ProtocolViolation { capability: &'static str },
    /// A package selected by the Plan was not linked into the native App.
    MissingModuleFactory {
        instance: String,
        package_id: String,
    },
    /// No installed Execution Adapter provides the class selected by one Instance.
    UnavailableExecutionClass {
        instance_key: String,
        execution_class: String,
    },
    /// The Resolved Plan or prepared endpoint set is internally inconsistent.
    InvalidResolvedPlan { detail: String },
    /// New request admission was closed because the App is shutting down.
    AdmissionClosed,
    /// The request could not enter a full bounded admission queue.
    ResourceExhausted {
        capability: &'static str,
        operation: String,
    },
    /// The invocation deadline expired before the request completed.
    DeadlineExceeded { request_id: RequestId },
    /// The caller cancelled the invocation before it completed.
    Cancelled { request_id: RequestId },
    /// The Runtime Driver or Adapter reported an internal execution failure.
    Internal { detail: String },
    /// A Module generation reported a failure that should trigger supervision.
    ModuleFailure { detail: String },
    /// A Module Instance exhausted its finite restart budget.
    ModuleRestartExhausted { instance: String, attempts: usize },
}

/// The lifecycle phase represented by a Module context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModuleLifecyclePhase {
    /// The Module may validate configuration and reserve reversible resources.
    Prepare,
    /// The Module may initialize against already prepared dependencies.
    Activate,
    /// The App Ready Gate has opened and externally triggered work may begin.
    Ready,
    /// The Module must release work and resources owned by this generation.
    Deactivate,
}

/// A deterministic dependency visible to one Module Instance.
#[derive(Clone, Debug)]
pub struct ModuleDependency {
    capability_id: String,
    provider_instance: String,
    provider_order: usize,
    handle: Option<ModuleDependencyHandle>,
}

impl ModuleDependency {
    fn new(
        capability_id: impl Into<String>,
        provider_instance: impl Into<String>,
        provider_order: usize,
        handle: Option<ModuleDependencyHandle>,
    ) -> Self {
        Self {
            capability_id: capability_id.into(),
            provider_instance: provider_instance.into(),
            provider_order,
            handle,
        }
    }

    /// Returns the Capability required by this dependency.
    pub fn capability_id(&self) -> &str {
        &self.capability_id
    }

    /// Returns the App-local provider Instance key.
    pub fn provider_instance(&self) -> &str {
        &self.provider_instance
    }

    /// Returns the deterministic provider order for a `many` binding.
    pub const fn provider_order(&self) -> usize {
        self.provider_order
    }

    /// Returns the resolved native endpoint handle when the Adapter supplied one.
    pub fn handle(&self) -> Option<ModuleDependencyHandle> {
        self.handle.clone()
    }
}

/// An opaque, Adapter-resolved Capability endpoint passed to lifecycle code.
#[derive(Clone, Debug)]
pub struct ModuleDependencyHandle {
    endpoint: Rc<NativeEndpointState>,
}

impl ModuleDependencyHandle {
    /// Returns the Capability implemented by this handle.
    pub fn capability_id(&self) -> &'static str {
        self.endpoint.capability_id
    }

    /// Returns the exact Descriptor version implemented by this handle.
    pub fn descriptor_version(&self) -> &'static str {
        self.endpoint.descriptor_version
    }

    /// Returns the exact Operation table implemented by this handle.
    pub fn operations(&self) -> &'static [&'static str] {
        self.endpoint.operations
    }
}

/// The explicit Capability dependencies available during Module lifecycle.
#[derive(Clone, Debug, Default)]
pub struct ModuleDependencies {
    bindings: Vec<ModuleDependency>,
}

impl ModuleDependencies {
    /// Returns dependencies in the order materialized by the Resolved App Plan.
    pub fn bindings(&self) -> &[ModuleDependency] {
        &self.bindings
    }

    /// Returns the number of explicit dependencies.
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// Returns whether this Module has no explicit dependencies.
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }
}

/// A shared App-wide signal that opens exactly once after every Module activates.
#[derive(Clone, Debug)]
pub struct AppReadyGate {
    state: Rc<AppReadyState>,
}

#[derive(Debug)]
struct AppReadyState {
    open: Cell<bool>,
    waiters: RefCell<Vec<oneshot::Sender<()>>>,
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

    fn open(&self) {
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
    state: Rc<AppAdmissionState>,
}

#[derive(Debug)]
struct AppAdmissionState {
    open: Cell<bool>,
}

impl AppAdmission {
    fn new() -> Self {
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

    fn open(&self) {
        self.state.open.set(true);
    }

    fn close(&self) {
        self.state.open.set(false);
    }
}

/// Cooperative cancellation shared by one Module Instance generation.
#[derive(Clone, Debug)]
pub struct CancellationToken {
    state: Rc<CancellationState>,
}

#[derive(Debug)]
struct CancellationState {
    cancelled: Cell<bool>,
    next_waiter_id: Cell<usize>,
    waiters: RefCell<Vec<(usize, oneshot::Sender<()>)>>,
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
struct CancellationWaiter {
    state: Rc<CancellationState>,
    waiter_id: usize,
    receiver: oneshot::Receiver<()>,
    registered: bool,
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

/// Kernel-owned context propagated across one native request invocation.
#[derive(Clone, Debug)]
pub struct InvocationContext {
    caller_instance: Option<String>,
    request_id: RequestId,
    deadline: Option<Duration>,
    cancellation: CancellationToken,
}

impl InvocationContext {
    /// Creates an invocation context with an absolute Driver-monotonic deadline.
    pub fn new(
        request_id: RequestId,
        deadline: Option<Duration>,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            caller_instance: None,
            request_id,
            deadline,
            cancellation,
        }
    }

    /// Attaches the resolved Caller Module Instance to this context.
    #[must_use]
    pub fn with_caller_instance(mut self, caller_instance: impl Into<String>) -> Self {
        self.caller_instance = Some(caller_instance.into());
        self
    }

    /// Returns the Caller Module Instance, when the App attached one.
    pub fn caller_instance(&self) -> Option<&str> {
        self.caller_instance.as_deref()
    }

    /// Returns the Kernel Request ID used for correlation and cancellation.
    pub const fn request_id(&self) -> RequestId {
        self.request_id
    }

    /// Returns the absolute Driver-monotonic deadline, when one was supplied.
    pub const fn deadline(&self) -> Option<Duration> {
        self.deadline
    }

    /// Returns the caller-owned cooperative cancellation signal.
    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    /// Returns whether the caller has already cancelled this invocation.
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    /// Returns whether the context deadline has passed at a Driver instant.
    pub fn is_expired(&self, now: Duration) -> bool {
        self.deadline.is_some_and(|deadline| deadline <= now)
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

#[derive(Debug)]
struct ManagedResourceEntry {
    resource: Rc<dyn ManagedResource>,
    released: Cell<bool>,
}

/// A handle that releases one managed resource at most once.
#[derive(Clone, Debug)]
pub struct ManagedResourceHandle {
    entry: Rc<ManagedResourceEntry>,
}

impl ManagedResourceHandle {
    /// Returns whether this resource has already had its release attempted.
    pub fn is_released(&self) -> bool {
        self.entry.released.get()
    }

    fn begin_release(&self) -> Option<ResourceFuture> {
        (!self.entry.released.replace(true)).then(|| self.entry.resource.release())
    }

    /// Releases this resource once; repeated calls are successful no-ops.
    pub async fn release(&self) -> Result<(), RuntimeFailure> {
        match self.begin_release() {
            Some(release) => release.await,
            None => Ok(()),
        }
    }
}

/// A Module-generation resource scope backed by Driver-polled cleanup futures.
#[derive(Clone)]
pub struct ManagedResourceScope {
    state: Rc<ManagedResourceScopeState>,
}

#[derive(Debug, Default)]
struct ManagedResourceScopeState {
    resources: RefCell<Vec<ManagedResourceHandle>>,
    closed: Cell<bool>,
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
    fn new() -> Self {
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
                released: Cell::new(false),
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

    fn close(&self) {
        self.state.closed.set(true);
    }

    async fn release_all(&self) -> Option<RuntimeFailure> {
        let resources = std::mem::take(&mut *self.state.resources.borrow_mut());
        let mut first_error = None;
        for resource in resources {
            if let Some(release) = resource.begin_release()
                && let Err(error) = release.await
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        first_error
    }

    fn release_unawaited(&self) {
        let resources = std::mem::take(&mut *self.state.resources.borrow_mut());
        for resource in resources {
            let _ = resource.begin_release();
        }
    }

    async fn release_all_until(
        &self,
        driver: &DriverControl,
        deadline: Duration,
    ) -> Result<Option<RuntimeFailure>, ()> {
        let resources = std::mem::take(&mut *self.state.resources.borrow_mut());
        let mut first_error = None;
        for (index, resource) in resources.iter().enumerate() {
            let Some(release) = resource.begin_release() else {
                continue;
            };
            match wait_until(driver, deadline, release).await {
                Some(Ok(())) => {}
                Some(Err(error)) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
                None => {
                    for pending in resources.iter().skip(index + 1) {
                        let _ = pending.begin_release();
                    }
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
    task: Rc<RefCell<Option<DriverTask>>>,
    abort: AbortHandle,
}

impl ManagedTask {
    /// Requests cancellation of the underlying task.
    pub fn cancel(&self) {
        self.abort.abort();
    }

    async fn join(&self) {
        let task = self.task.borrow_mut().take();
        if let Some(task) = task {
            let _ = task.await;
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
    spawn: Rc<dyn Fn(LocalTask) -> Result<DriverTask, SpawnError>>,
    state: Rc<ManagedTaskScopeState>,
}

#[derive(Debug, Default)]
struct ManagedTaskScopeState {
    tasks: RefCell<Vec<ManagedTask>>,
    closed: Cell<bool>,
    cancellation: CancellationToken,
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
    fn new<D: RuntimeDriver>(driver: &D) -> Self {
        let spawner = driver.clone();
        Self {
            spawn: Rc::new(move |task| spawner.spawn_local(task)),
            state: Rc::new(ManagedTaskScopeState::default()),
        }
    }

    fn new_from_driver_control(driver: &DriverControl) -> Self {
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
        let driver_task = (self.spawn)(task)?;
        let handle = ManagedTask {
            abort: driver_task.abort_handle(),
            task: Rc::new(RefCell::new(Some(driver_task))),
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

    fn close(&self) {
        self.state.closed.set(true);
        self.state.cancellation.cancel();
    }

    fn cancel(&self) {
        self.state.cancellation.cancel();
    }

    fn abort_all(&self) {
        for task in self.state.tasks.borrow().iter() {
            task.cancel();
        }
    }

    async fn cancel_all(&self) {
        self.close();
        let tasks = std::mem::take(&mut *self.state.tasks.borrow_mut());
        for task in tasks {
            task.cancel();
            task.join().await;
        }
    }

    async fn drain_until(&self, driver: &DriverControl, deadline: Duration) -> bool {
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

/// Context supplied while a Module reserves reversible resources.
#[derive(Clone, Debug)]
pub struct PrepareContext {
    instance_key: String,
    dependencies: ModuleDependencies,
    resources: ManagedResourceScope,
    cancellation: CancellationToken,
    admission: AppAdmission,
}

impl PrepareContext {
    /// Returns the App-local Module Instance key.
    pub fn instance_key(&self) -> &str {
        &self.instance_key
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
    instance_key: String,
    dependencies: ModuleDependencies,
    ready_gate: AppReadyGate,
    tasks: ManagedTaskScope,
    resources: ManagedResourceScope,
    cancellation: CancellationToken,
    admission: AppAdmission,
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
    instance_key: String,
    dependencies: ModuleDependencies,
    ready_gate: AppReadyGate,
    tasks: ManagedTaskScope,
    resources: ManagedResourceScope,
    cancellation: CancellationToken,
    admission: AppAdmission,
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
    instance_key: String,
    dependencies: ModuleDependencies,
    reason: DeactivationReason,
    tasks: ManagedTaskScope,
    resources: ManagedResourceScope,
    cancellation: CancellationToken,
    admission: AppAdmission,
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

/// Type-erased native endpoint used only while Kernel constructs and dispatches the graph.
pub trait NativeRequestEndpoint: std::fmt::Debug {
    /// Stable Capability series identity.
    fn capability_id(&self) -> &'static str;
    /// Exact Descriptor version implemented by this endpoint.
    fn descriptor_version(&self) -> &'static str;
    /// Exact stable Operation names implemented by this endpoint.
    fn operations(&self) -> &'static [&'static str];
    /// Dispatches one operation without serializing its typed Rust payload.
    fn invoke(
        &self,
        operation: &str,
        request: ErasedValue,
        context: InvocationContext,
    ) -> LocalBoxFuture<'static, Result<ErasedDomainResult, RuntimeFailure>>;
}

/// One freshly prepared Module Instance generation returned by an Execution Adapter.
#[derive(Debug)]
pub struct PreparedNativeModule {
    endpoints: Vec<Rc<dyn NativeRequestEndpoint>>,
    lifecycle: Rc<dyn ModuleLifecycle>,
}

impl PreparedNativeModule {
    /// Creates one generation from its exact endpoint set and lifecycle Interface.
    pub fn new(
        endpoints: Vec<Rc<dyn NativeRequestEndpoint>>,
        lifecycle: impl ModuleLifecycle,
    ) -> Self {
        Self {
            endpoints,
            lifecycle: Rc::new(lifecycle),
        }
    }

    /// Creates one generation from an already shared lifecycle implementation.
    pub fn with_lifecycle(
        endpoints: Vec<Rc<dyn NativeRequestEndpoint>>,
        lifecycle: Rc<dyn ModuleLifecycle>,
    ) -> Self {
        Self {
            endpoints,
            lifecycle,
        }
    }

    /// Returns the exact endpoints prepared for this generation.
    pub fn endpoints(&self) -> &[Rc<dyn NativeRequestEndpoint>] {
        &self.endpoints
    }

    /// Returns the lifecycle Interface prepared for this generation.
    pub fn lifecycle(&self) -> Rc<dyn ModuleLifecycle> {
        self.lifecycle.clone()
    }

    fn into_parts(self) -> (Vec<Rc<dyn NativeRequestEndpoint>>, Rc<dyn ModuleLifecycle>) {
        (self.endpoints, self.lifecycle)
    }
}

/// One provider-specific binding prepared by an Execution Adapter.
#[derive(Clone, Debug)]
pub struct PreparedBinding {
    consumer_instance: String,
    provider_instance: String,
    endpoint: Rc<dyn NativeRequestEndpoint>,
}

impl PreparedBinding {
    /// Binds one consumer to the endpoint prepared for one exact provider Instance.
    pub fn new(
        consumer_instance: impl Into<String>,
        provider_instance: impl Into<String>,
        endpoint: Rc<dyn NativeRequestEndpoint>,
    ) -> Self {
        Self {
            consumer_instance: consumer_instance.into(),
            provider_instance: provider_instance.into(),
            endpoint,
        }
    }

    fn same_identity(&self, other: &Self) -> bool {
        self.consumer_instance == other.consumer_instance
            && self.provider_instance == other.provider_instance
            && self.endpoint.capability_id() == other.endpoint.capability_id()
    }
}

/// Prepared native bindings returned by an Execution Adapter to Kernel.
#[derive(Debug)]
pub struct PreparedNativeApp {
    bindings: Vec<PreparedBinding>,
    modules: BTreeMap<String, Rc<dyn ModuleLifecycle>>,
    generations: BTreeMap<String, PreparedNativeModule>,
}

impl PreparedNativeApp {
    /// Completes Adapter preparation with provider-specific bindings.
    pub fn new(bindings: Vec<PreparedBinding>) -> Self {
        Self {
            bindings,
            modules: BTreeMap::new(),
            generations: BTreeMap::new(),
        }
    }

    /// Completes Adapter preparation with Module lifecycle implementations.
    pub fn with_modules(
        bindings: Vec<PreparedBinding>,
        modules: BTreeMap<String, Rc<dyn ModuleLifecycle>>,
    ) -> Self {
        Self {
            bindings,
            modules,
            generations: BTreeMap::new(),
        }
    }

    /// Completes Adapter preparation with fresh generations for every Module Instance.
    pub fn with_generations(
        bindings: Vec<PreparedBinding>,
        generations: BTreeMap<String, PreparedNativeModule>,
    ) -> Self {
        let modules = generations
            .iter()
            .map(|(instance_key, generation)| (instance_key.clone(), generation.lifecycle()))
            .collect();
        Self {
            bindings,
            modules,
            generations,
        }
    }

    fn merge(&mut self, other: Self) -> Result<(), RuntimeFailure> {
        for binding in other.bindings {
            if self
                .bindings
                .iter()
                .any(|existing| existing.same_identity(&binding))
            {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "multiple Execution Adapters prepared binding `{}:{}:{}`",
                        binding.consumer_instance,
                        binding.endpoint.capability_id(),
                        binding.provider_instance
                    ),
                });
            }
            self.bindings.push(binding);
        }
        for (instance_key, lifecycle) in other.modules {
            if self
                .modules
                .insert(instance_key.clone(), lifecycle)
                .is_some()
            {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "multiple Execution Adapters prepared Module Instance `{instance_key}`"
                    ),
                });
            }
        }
        for (instance_key, generation) in other.generations {
            if self
                .generations
                .insert(instance_key.clone(), generation)
                .is_some()
            {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "multiple Execution Adapters prepared Module Instance generation `{instance_key}`"
                    ),
                });
            }
        }
        Ok(())
    }
}

/// Host-specific seam that instantiates Module generations and prepares endpoints.
pub trait ExecutionAdapter: std::fmt::Debug + 'static {
    /// Returns the open execution class implemented by this Adapter package.
    fn execution_class(&self) -> ExecutionClassId;

    /// Instantiates the exact Plan and confirms its endpoint and binding tables.
    fn prepare(&self, plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure>;

    /// Creates a fresh generation for one selected Module Instance.
    ///
    /// Adapters that cannot truthfully recreate a generation retain the default
    /// failure, which lets Kernel apply the selected finite policy without
    /// pretending that an in-process fault boundary is recoverable.
    fn recreate(
        &self,
        _plan: &ResolvedAppPlan,
        instance_key: &str,
    ) -> Result<PreparedNativeModule, RuntimeFailure> {
        Err(RuntimeFailure::Internal {
            detail: format!("Execution Adapter cannot recreate Module Instance `{instance_key}`"),
        })
    }
}

/// Native Rust Adapter Interface for statically linked Module packages.
///
/// The blanket implementation below contributes every native Adapter to the
/// open catalog under the official native execution-class identity.
pub trait NativeExecutionAdapter: std::fmt::Debug + 'static {
    /// Instantiates the exact Plan and confirms its endpoint and binding tables.
    fn prepare(&self, plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure>;

    /// Creates a fresh generation for one selected native Module Instance.
    fn recreate(
        &self,
        _plan: &ResolvedAppPlan,
        instance_key: &str,
    ) -> Result<PreparedNativeModule, RuntimeFailure> {
        Err(RuntimeFailure::Internal {
            detail: format!("Execution Adapter cannot recreate Module Instance `{instance_key}`"),
        })
    }
}

impl<T: NativeExecutionAdapter> ExecutionAdapter for T {
    fn execution_class(&self) -> ExecutionClassId {
        ExecutionClassId::native_rust()
    }

    fn prepare(&self, plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure> {
        NativeExecutionAdapter::prepare(self, plan)
    }

    fn recreate(
        &self,
        plan: &ResolvedAppPlan,
        instance_key: &str,
    ) -> Result<PreparedNativeModule, RuntimeFailure> {
        NativeExecutionAdapter::recreate(self, plan, instance_key)
    }
}

/// The execution classes contributed by installed Adapter packages.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExecutionClassSet(BTreeSet<ExecutionClassId>);

impl ExecutionClassSet {
    /// Returns whether an installed Adapter provides this execution class.
    pub fn contains(&self, execution_class: &ExecutionClassId) -> bool {
        self.0.contains(execution_class)
    }

    /// Iterates the execution classes in deterministic identity order.
    pub fn iter(&self) -> impl Iterator<Item = &ExecutionClassId> {
        self.0.iter()
    }
}

/// A Runner could not assemble one unambiguous Adapter catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutionAdapterCatalogError {
    /// More than one installed Adapter claimed the same execution class.
    DuplicateExecutionClass { execution_class: String },
}

impl std::fmt::Display for ExecutionAdapterCatalogError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateExecutionClass { execution_class } => write!(
                formatter,
                "multiple Execution Adapters provide class `{execution_class}`"
            ),
        }
    }
}

impl std::error::Error for ExecutionAdapterCatalogError {}

/// Immutable Adapter catalog assembled by a Runner before Kernel boot.
#[derive(Debug, Default)]
pub struct ExecutionAdapterCatalog {
    adapters: BTreeMap<ExecutionClassId, Rc<dyn ExecutionAdapter>>,
}

impl ExecutionAdapterCatalog {
    /// Creates an empty catalog for an App with no Module Instances.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a catalog containing one Adapter package.
    pub fn single(adapter: impl ExecutionAdapter) -> Self {
        Self::new()
            .with_adapter(adapter)
            .expect("a new catalog cannot contain a duplicate execution class")
    }

    /// Installs one Adapter package under its open execution-class identity.
    pub fn with_adapter(
        self,
        adapter: impl ExecutionAdapter,
    ) -> Result<Self, ExecutionAdapterCatalogError> {
        self.with_shared_adapter(Rc::new(adapter))
    }

    /// Installs an Adapter package discovered as a runtime trait object.
    pub fn with_shared_adapter(
        mut self,
        adapter: Rc<dyn ExecutionAdapter>,
    ) -> Result<Self, ExecutionAdapterCatalogError> {
        let execution_class = adapter.execution_class();
        if self.adapters.contains_key(&execution_class) {
            return Err(ExecutionAdapterCatalogError::DuplicateExecutionClass {
                execution_class: execution_class.to_string(),
            });
        }
        self.adapters.insert(execution_class, adapter);
        Ok(self)
    }

    /// Returns the effective execution classes contributed by installed packages.
    pub fn execution_classes(&self) -> ExecutionClassSet {
        ExecutionClassSet(self.adapters.keys().cloned().collect())
    }

    fn adapter(&self, execution_class: &ExecutionClassId) -> Option<Rc<dyn ExecutionAdapter>> {
        self.adapters.get(execution_class).cloned()
    }

    fn prepare(&self, plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure> {
        let mut required_classes = BTreeSet::new();
        for instance in plan.module_instances() {
            if !self.adapters.contains_key(instance.execution_class()) {
                return Err(RuntimeFailure::UnavailableExecutionClass {
                    instance_key: instance.instance_key().to_owned(),
                    execution_class: instance.execution_class().to_string(),
                });
            }
            required_classes.insert(instance.execution_class().clone());
        }

        let mut prepared = PreparedNativeApp::with_modules(Vec::new(), BTreeMap::new());
        for execution_class in required_classes {
            let adapter = self
                .adapters
                .get(&execution_class)
                .expect("required execution classes were validated");
            prepared.merge(adapter.prepare(plan)?)?;
        }
        Ok(prepared)
    }
}

#[derive(Clone, Debug)]
struct NativeEndpointSnapshot {
    endpoint: Rc<dyn NativeRequestEndpoint>,
    generation: u64,
    cancellation: CancellationToken,
}

#[derive(Debug)]
struct NativeEndpointState {
    capability_id: &'static str,
    descriptor_version: &'static str,
    operations: &'static [&'static str],
    endpoint: RefCell<Option<Rc<dyn NativeRequestEndpoint>>>,
    generation: Cell<u64>,
    cancellation: RefCell<CancellationToken>,
}

impl NativeEndpointState {
    fn new(endpoint: Rc<dyn NativeRequestEndpoint>, generation: u64) -> Self {
        Self {
            capability_id: endpoint.capability_id(),
            descriptor_version: endpoint.descriptor_version(),
            operations: endpoint.operations(),
            endpoint: RefCell::new(Some(endpoint)),
            generation: Cell::new(generation),
            cancellation: RefCell::new(CancellationToken::new()),
        }
    }

    fn snapshot(&self) -> Option<NativeEndpointSnapshot> {
        self.endpoint
            .borrow()
            .clone()
            .map(|endpoint| NativeEndpointSnapshot {
                endpoint,
                generation: self.generation.get(),
                cancellation: self.cancellation.borrow().clone(),
            })
    }

    fn mark_unavailable(&self) {
        self.cancellation.borrow().cancel();
        self.endpoint.borrow_mut().take();
    }

    fn install(&self, endpoint: Rc<dyn NativeRequestEndpoint>, generation: u64) {
        self.generation.set(generation);
        self.cancellation.replace(CancellationToken::new());
        self.endpoint.replace(Some(endpoint));
    }

    fn is_current(&self, generation: u64) -> bool {
        self.generation.get() == generation && self.endpoint.borrow().is_some()
    }
}

#[derive(Clone, Debug)]
struct NativeEndpointBinding {
    module_instance: String,
    state: Rc<NativeEndpointState>,
    admissions: BTreeMap<String, RequestAdmission>,
}

impl NativeEndpointBinding {
    fn admission(&self, operation: &str) -> Option<&RequestAdmission> {
        self.admissions.get(operation)
    }
}

#[derive(Debug)]
struct NativeModuleGeneration {
    lifecycle: Rc<dyn ModuleLifecycle>,
    tasks: ManagedTaskScope,
    resources: ManagedResourceScope,
}

enum GenerationPreparationFailure {
    Lifecycle,
    Cleanup(RuntimeFailure),
}

#[derive(Debug)]
struct NativeModuleRuntime {
    generation: RefCell<Option<NativeModuleGeneration>>,
}

impl NativeModuleRuntime {
    fn take_generation(&self) -> Option<NativeModuleGeneration> {
        self.generation.borrow_mut().take()
    }

    fn install_generation(&self, generation: NativeModuleGeneration) {
        debug_assert!(self.generation.borrow().is_none());
        self.generation.replace(Some(generation));
    }

    fn generation_parts(
        &self,
    ) -> Option<(
        Rc<dyn ModuleLifecycle>,
        ManagedTaskScope,
        ManagedResourceScope,
    )> {
        self.generation.borrow().as_ref().map(|generation| {
            (
                generation.lifecycle.clone(),
                generation.tasks.clone(),
                generation.resources.clone(),
            )
        })
    }
}

#[derive(Clone, Debug)]
struct ModuleSupervision {
    policy: RestartPolicy,
    criticality: ModuleCriticality,
    required_path: bool,
    generation: u64,
    attempts: Vec<Duration>,
    stable_since: Option<Duration>,
    restarting: bool,
}

struct NativeAppRuntime {
    plan: ResolvedAppPlan,
    adapters: Rc<ExecutionAdapterCatalog>,
    modules: BTreeMap<String, NativeModuleRuntime>,
    dependencies: BTreeMap<String, ModuleDependencies>,
    endpoint_states: BTreeMap<(String, String), Rc<NativeEndpointState>>,
    supervision: RefCell<BTreeMap<String, ModuleSupervision>>,
    activation_order: Vec<String>,
    ready_gate: AppReadyGate,
    admission: AppAdmission,
    driver: DriverControl,
    request_ids: Rc<Cell<RequestId>>,
    supervision_cancellation: CancellationToken,
    shutdown_started: Cell<bool>,
    shutdown_result: RefCell<Option<ShutdownOutcome>>,
    terminal_failure: RefCell<Option<RuntimeFailure>>,
}

impl std::fmt::Debug for NativeAppRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeAppRuntime")
            .field("module_count", &self.modules.len())
            .field("endpoint_count", &self.endpoint_states.len())
            .field("ready", &self.ready_gate.is_open())
            .field("accepting", &self.admission.is_open())
            .field("next_request_id", &self.request_ids.get())
            .field("shutdown_started", &self.shutdown_started.get())
            .field(
                "terminal_failure",
                &self.terminal_failure.borrow().is_some(),
            )
            .finish_non_exhaustive()
    }
}

impl NativeAppRuntime {
    fn begin_shutdown(&self) {
        if self.shutdown_started.replace(true) {
            return;
        }
        self.admission.close();
        self.supervision_cancellation.cancel();
        for endpoint in self.endpoint_states.values() {
            endpoint.mark_unavailable();
        }
        for module in self.modules.values() {
            if let Some((_, tasks, resources)) = module.generation_parts() {
                tasks.close();
                resources.close();
            }
        }
    }
}

/// A started native App whose generated clients can invoke resolved bindings.
#[derive(Clone, Debug)]
pub struct NativeApp {
    bindings: BTreeMap<(String, &'static str), Vec<NativeEndpointBinding>>,
    runtime: Rc<NativeAppRuntime>,
}

impl NativeApp {
    /// Confirms that a generated client has one resolved binding before use.
    pub fn ensure_binding<C: RequestCapability>(
        &self,
        caller_instance: &str,
    ) -> Result<(), RuntimeFailure> {
        if self.runtime.admission.is_closed() {
            return Err(RuntimeFailure::AdmissionClosed);
        }
        self.endpoints::<C>(caller_instance)
            .is_some_and(|endpoints| !endpoints.is_empty())
            .then_some(())
            .ok_or(RuntimeFailure::Unavailable { capability: C::ID })
    }

    /// Materializes one typed handle from the immutable binding selected by the Plan.
    pub fn handle<C: RequestCapability>(
        &self,
        caller_instance: &str,
    ) -> Result<NativeRequestHandle<C>, RuntimeFailure> {
        if self.runtime.admission.is_closed() {
            return Err(RuntimeFailure::AdmissionClosed);
        }
        let endpoints = self
            .endpoints::<C>(caller_instance)
            .filter(|endpoints| !endpoints.is_empty())
            .ok_or(RuntimeFailure::Unavailable { capability: C::ID })?;
        Ok(NativeRequestHandle::from_endpoints(
            endpoints,
            self.runtime.clone(),
            caller_instance,
        ))
    }

    /// Materializes an optional typed handle; an absent binding remains `None`.
    pub fn optional_handle<C: RequestCapability>(
        &self,
        caller_instance: &str,
    ) -> Option<NativeRequestHandle<C>> {
        let caller_instance = caller_instance.to_owned();
        self.endpoints::<C>(&caller_instance)
            .filter(|endpoints| !endpoints.is_empty())
            .map(|endpoints| {
                NativeRequestHandle::from_endpoints(
                    endpoints,
                    self.runtime.clone(),
                    &caller_instance,
                )
            })
    }

    /// Materializes a typed handle whose endpoints may be empty for a `many` requirement.
    pub fn many_handle<C: RequestCapability>(
        &self,
        caller_instance: &str,
    ) -> Result<NativeRequestHandle<C>, RuntimeFailure> {
        if self.runtime.admission.is_closed() {
            return Err(RuntimeFailure::AdmissionClosed);
        }
        let endpoints = self.endpoints::<C>(caller_instance).unwrap_or(&[]);
        Ok(NativeRequestHandle::from_endpoints(
            endpoints,
            self.runtime.clone(),
            caller_instance,
        ))
    }

    /// Returns the number of immutable provider endpoints bound to one requirement.
    pub fn binding_count<C: RequestCapability>(&self, caller_instance: &str) -> usize {
        self.endpoints::<C>(caller_instance).map_or(0, <[_]>::len)
    }

    /// Returns whether every declared Module has completed activation.
    pub fn is_ready(&self) -> bool {
        self.runtime.ready_gate.is_open()
    }

    /// Returns the App-wide readiness signal observed by Module tasks.
    pub fn ready_gate(&self) -> AppReadyGate {
        self.runtime.ready_gate.clone()
    }

    /// Returns whether new externally triggered work may be admitted.
    pub fn is_accepting(&self) -> bool {
        self.runtime.admission.is_open()
    }

    /// Returns the App-wide admission state.
    pub fn admission(&self) -> AppAdmission {
        self.runtime.admission.clone()
    }

    /// Returns the terminal supervision failure, when a critical App path exhausted its budget.
    pub fn terminal_failure(&self) -> Option<RuntimeFailure> {
        self.runtime.terminal_failure.borrow().clone()
    }

    /// Returns whether supervision has produced a terminal App failure.
    pub fn is_failed(&self) -> bool {
        self.runtime.terminal_failure.borrow().is_some()
    }

    /// Returns the current ready generation for one Module Instance, when it is available.
    pub fn module_generation(&self, instance_key: &str) -> Option<u64> {
        self.runtime
            .supervision
            .borrow()
            .get(instance_key)
            .and_then(|state| {
                self.runtime
                    .endpoint_states
                    .iter()
                    .find(|((module, _), endpoint)| {
                        module == instance_key && endpoint.is_current(state.generation)
                    })
                    .map(|_| state.generation)
            })
    }

    /// Reports a Module Instance failure and schedules its finite supervision policy.
    pub fn report_module_failure(&self, instance_key: &str) -> Result<(), RuntimeFailure> {
        if !begin_module_supervision(&self.runtime, instance_key)? {
            return Ok(());
        }
        schedule_module_supervision(&self.runtime, instance_key).map_err(|error| {
            handle_supervision_schedule_failure(&self.runtime, instance_key, error)
        })
    }

    /// Starts shutdown admission closure and cooperative cancellation.
    pub fn request_shutdown(&self) {
        self.runtime.begin_shutdown();
    }

    /// Performs bounded graceful shutdown using one global deadline.
    pub async fn shutdown(&self, timeout: Duration) -> ShutdownOutcome {
        if let Some(outcome) = self.runtime.shutdown_result.borrow().clone() {
            return outcome;
        }
        self.runtime.begin_shutdown();
        let outcome = shutdown_native_modules(&self.runtime, timeout).await;
        self.runtime.shutdown_result.replace(Some(outcome.clone()));
        outcome
    }

    /// Invokes a generated request Operation through the caller's resolved binding.
    pub async fn invoke<C: RequestCapability>(
        &self,
        caller_instance: &str,
        operation: &str,
        request: C::Request,
    ) -> Result<Result<C::Response, C::DomainError>, RuntimeFailure> {
        self.handle::<C>(caller_instance)?
            .invoke(operation, request)
            .await
    }

    /// Creates a request context with a fresh Kernel Request ID.
    ///
    /// `deadline` is an absolute instant returned by the selected
    /// [`RuntimeDriver`]'s monotonic clock.
    pub fn invocation_context(
        &self,
        deadline: Option<Duration>,
        cancellation: CancellationToken,
    ) -> InvocationContext {
        InvocationContext::new(self.next_request_id(), deadline, cancellation)
    }

    /// Creates a request context whose deadline is relative to the Driver's clock.
    pub fn invocation_context_after(
        &self,
        timeout: Duration,
        cancellation: CancellationToken,
    ) -> InvocationContext {
        self.invocation_context(
            Some((self.runtime.driver.now)().saturating_add(timeout)),
            cancellation,
        )
    }

    /// Invokes a request with an explicit propagated Invocation Context.
    pub async fn invoke_with_context<C: RequestCapability>(
        &self,
        caller_instance: &str,
        operation: &str,
        context: InvocationContext,
        request: C::Request,
    ) -> Result<Result<C::Response, C::DomainError>, RuntimeFailure> {
        self.handle::<C>(caller_instance)?
            .invoke_with_context(operation, context, request)
            .await
    }

    fn next_request_id(&self) -> RequestId {
        let request_id = self.runtime.request_ids.get();
        self.runtime.request_ids.set(request_id.saturating_add(1));
        request_id
    }

    fn endpoints<C: RequestCapability>(
        &self,
        caller_instance: &str,
    ) -> Option<&[NativeEndpointBinding]> {
        self.bindings
            .get(&(caller_instance.to_owned(), C::ID))
            .map(Vec::as_slice)
    }
}

/// Typed, immutable native Capability endpoints materialized before App boot completes.
#[derive(Debug)]
pub struct NativeRequestHandle<C: RequestCapability> {
    endpoints: Vec<NativeEndpointBinding>,
    runtime: Rc<NativeAppRuntime>,
    caller_instance: String,
    capability: PhantomData<fn() -> C>,
}

impl<C: RequestCapability> NativeRequestHandle<C> {
    fn from_endpoints(
        endpoints: &[NativeEndpointBinding],
        runtime: Rc<NativeAppRuntime>,
        caller_instance: &str,
    ) -> Self {
        Self {
            endpoints: endpoints.to_vec(),
            runtime,
            caller_instance: caller_instance.to_owned(),
            capability: PhantomData,
        }
    }

    /// Returns the number of provider endpoints captured by this handle.
    pub fn binding_count(&self) -> usize {
        self.endpoints.len()
    }

    /// Invokes a singular Capability binding without falling back across providers.
    pub async fn invoke(
        &self,
        operation: &str,
        request: C::Request,
    ) -> Result<Result<C::Response, C::DomainError>, RuntimeFailure> {
        let context = self.next_context();
        self.invoke_with_context(operation, context, request).await
    }

    /// Invokes a singular binding with an explicit Invocation Context.
    pub async fn invoke_with_context(
        &self,
        operation: &str,
        context: InvocationContext,
        request: C::Request,
    ) -> Result<Result<C::Response, C::DomainError>, RuntimeFailure> {
        let context = context.with_caller_instance(self.caller_instance.clone());
        if self.runtime.admission.is_closed() {
            return Err(RuntimeFailure::AdmissionClosed);
        }
        let endpoint = match self.endpoints.as_slice() {
            [] => return Err(RuntimeFailure::Unavailable { capability: C::ID }),
            [endpoint] => endpoint,
            endpoints => {
                return Err(RuntimeFailure::AmbiguousBinding {
                    capability: C::ID,
                    providers: endpoints.len(),
                });
            }
        };
        let snapshot = endpoint
            .state
            .snapshot()
            .ok_or(RuntimeFailure::Unavailable { capability: C::ID })?;
        let admission =
            endpoint
                .admission(operation)
                .ok_or_else(|| RuntimeFailure::UnknownOperation {
                    capability: C::ID,
                    operation: operation.to_owned(),
                })?;
        let _permit = admission
            .acquire(
                C::ID,
                operation,
                context.clone(),
                self.runtime.driver.clone(),
            )
            .await?;
        if !endpoint.state.is_current(snapshot.generation) {
            return Err(RuntimeFailure::Unavailable { capability: C::ID });
        }
        ensure_context_active(&self.runtime.driver, &context)?;
        let outcome = await_with_generation_context(
            &self.runtime.driver,
            &context,
            snapshot.cancellation,
            C::ID,
            snapshot
                .endpoint
                .invoke(operation, Box::new(request), context.clone()),
        )
        .await
        .map_err(|error| {
            schedule_module_supervision_after_failure(
                &self.runtime,
                &endpoint.module_instance,
                error,
            )
        })?
        .map_err(|error| {
            schedule_module_supervision_after_failure(
                &self.runtime,
                &endpoint.module_instance,
                error,
            )
        })?;
        decode_outcome::<C>(outcome)
    }

    /// Invokes every provider in the resolved many order with the same typed request.
    pub async fn invoke_many(
        &self,
        operation: &str,
        request: C::Request,
    ) -> Result<Vec<Result<C::Response, C::DomainError>>, RuntimeFailure>
    where
        C::Request: Clone,
    {
        let context = self.next_context();
        self.invoke_many_with_context(operation, context, request)
            .await
    }

    /// Invokes every provider with one shared explicit Invocation Context.
    pub async fn invoke_many_with_context(
        &self,
        operation: &str,
        context: InvocationContext,
        request: C::Request,
    ) -> Result<Vec<Result<C::Response, C::DomainError>>, RuntimeFailure>
    where
        C::Request: Clone,
    {
        let context = context.with_caller_instance(self.caller_instance.clone());
        if self.runtime.admission.is_closed() {
            return Err(RuntimeFailure::AdmissionClosed);
        }
        if self.endpoints.is_empty() {
            return Ok(Vec::new());
        }
        let mut outcomes = Vec::with_capacity(self.endpoints.len());
        for endpoint in &self.endpoints {
            let snapshot = endpoint
                .state
                .snapshot()
                .ok_or(RuntimeFailure::Unavailable { capability: C::ID })?;
            let admission =
                endpoint
                    .admission(operation)
                    .ok_or_else(|| RuntimeFailure::UnknownOperation {
                        capability: C::ID,
                        operation: operation.to_owned(),
                    })?;
            let _permit = admission
                .acquire(
                    C::ID,
                    operation,
                    context.clone(),
                    self.runtime.driver.clone(),
                )
                .await?;
            if !endpoint.state.is_current(snapshot.generation) {
                return Err(RuntimeFailure::Unavailable { capability: C::ID });
            }
            ensure_context_active(&self.runtime.driver, &context)?;
            let outcome = await_with_generation_context(
                &self.runtime.driver,
                &context,
                snapshot.cancellation,
                C::ID,
                snapshot
                    .endpoint
                    .invoke(operation, Box::new(request.clone()), context.clone()),
            )
            .await
            .map_err(|error| {
                schedule_module_supervision_after_failure(
                    &self.runtime,
                    &endpoint.module_instance,
                    error,
                )
            })?
            .map_err(|error| {
                schedule_module_supervision_after_failure(
                    &self.runtime,
                    &endpoint.module_instance,
                    error,
                )
            })?;
            outcomes.push(decode_outcome::<C>(outcome)?);
        }
        Ok(outcomes)
    }

    /// Creates a fresh context for a request started through this handle.
    pub fn invocation_context(
        &self,
        deadline: Option<Duration>,
        cancellation: CancellationToken,
    ) -> InvocationContext {
        InvocationContext::new(self.next_request_id(), deadline, cancellation)
    }

    fn next_context(&self) -> InvocationContext {
        self.invocation_context(None, CancellationToken::new())
            .with_caller_instance(self.caller_instance.clone())
    }

    fn next_request_id(&self) -> RequestId {
        let request_id = self.runtime.request_ids.get();
        self.runtime.request_ids.set(request_id.saturating_add(1));
        request_id
    }
}

fn decode_outcome<C: RequestCapability>(
    outcome: ErasedDomainResult,
) -> Result<Result<C::Response, C::DomainError>, RuntimeFailure> {
    match outcome {
        Ok(value) => value
            .downcast::<C::Response>()
            .map(|value| Ok(*value))
            .map_err(|_| RuntimeFailure::ProtocolViolation { capability: C::ID }),
        Err(value) => value
            .downcast::<C::DomainError>()
            .map(|value| Err(*value))
            .map_err(|_| RuntimeFailure::ProtocolViolation { capability: C::ID }),
    }
}

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

    fn abort_handle(&self) -> AbortHandle {
        self.abort.clone()
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
struct DriverControl {
    now: Rc<dyn Fn() -> Duration>,
    sleep_until: Rc<dyn Fn(Duration) -> LocalBoxFuture<'static, ()>>,
    yield_now: Rc<dyn Fn() -> LocalBoxFuture<'static, ()>>,
    jitter: Rc<dyn Fn(Duration) -> Duration>,
    spawn_local: Rc<dyn Fn(LocalTask) -> Result<DriverTask, SpawnError>>,
}

impl std::fmt::Debug for DriverControl {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DriverControl")
            .finish_non_exhaustive()
    }
}

impl DriverControl {
    fn new<D: RuntimeDriver>(driver: &D) -> Self {
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

async fn wait_until<F: Future>(
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
struct RequestAdmission {
    limits: RequestAdmissionPlan,
    state: Rc<RequestAdmissionState>,
}

#[derive(Debug, Default)]
struct RequestAdmissionState {
    active: Cell<usize>,
    queued: Cell<usize>,
    waiters: RefCell<VecDeque<Rc<QueueWaiter>>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum QueueWaiterStatus {
    Waiting,
    Woken,
    Acquired,
    Cancelled,
}

#[derive(Debug)]
struct QueueWaiter {
    status: Cell<QueueWaiterStatus>,
    wakeup: RefCell<Option<oneshot::Sender<()>>>,
}

impl RequestAdmission {
    fn new(limits: RequestAdmissionPlan) -> Self {
        Self {
            limits,
            state: Rc::new(RequestAdmissionState::default()),
        }
    }

    fn acquire(
        &self,
        capability: &'static str,
        operation: &str,
        context: InvocationContext,
        driver: DriverControl,
    ) -> LocalBoxFuture<'static, Result<RequestPermit, RuntimeFailure>> {
        if let Err(error) = ensure_context_active(&driver, &context) {
            return Box::pin(futures::future::ready(Err(error)));
        }

        if self.state.active.get() < self.limits.max_concurrency() {
            self.state.active.set(self.state.active.get() + 1);
            return Box::pin(futures::future::ready(Ok(RequestPermit {
                state: self.state.clone(),
            })));
        }

        if self.state.queued.get() >= self.limits.queue_capacity() {
            return Box::pin(futures::future::ready(Err(
                RuntimeFailure::ResourceExhausted {
                    capability,
                    operation: operation.to_owned(),
                },
            )));
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
        Box::pin(async move { queued.wait(&driver, &context).await })
    }
}

#[derive(Debug)]
struct QueuedAdmission {
    state: Rc<RequestAdmissionState>,
    waiter_state: Rc<QueueWaiter>,
    waiter: oneshot::Receiver<()>,
}

impl QueuedAdmission {
    async fn wait(
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
struct RequestPermit {
    state: Rc<RequestAdmissionState>,
}

impl Drop for RequestPermit {
    fn drop(&mut self) {
        self.state.active.set(self.state.active.get() - 1);
        wake_next(&self.state);
    }
}

fn wake_next(state: &Rc<RequestAdmissionState>) {
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

async fn await_with_context<F: Future>(
    driver: &DriverControl,
    context: &InvocationContext,
    future: F,
) -> Result<F::Output, RuntimeFailure> {
    ensure_context_active(driver, context)?;

    let work = future.fuse();
    let cancellation = context.cancellation.cancelled().fuse();
    let deadline: LocalBoxFuture<'static, ()> = context.deadline().map_or_else(
        || Box::pin(pending::<()>()) as LocalBoxFuture<'static, ()>,
        |deadline| (driver.sleep_until)(deadline),
    );
    let deadline = deadline.fuse();
    futures::pin_mut!(work, cancellation, deadline);

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

async fn await_with_generation_context<F: Future>(
    driver: &DriverControl,
    context: &InvocationContext,
    generation_cancellation: CancellationToken,
    capability: &'static str,
    future: F,
) -> Result<F::Output, RuntimeFailure> {
    ensure_context_active(driver, context)?;

    let work = future.fuse();
    let cancellation = context.cancellation.cancelled().fuse();
    let generation_cancellation = generation_cancellation.cancelled().fuse();
    let deadline: LocalBoxFuture<'static, ()> = context.deadline().map_or_else(
        || Box::pin(pending::<()>()) as LocalBoxFuture<'static, ()>,
        |deadline| (driver.sleep_until)(deadline),
    );
    let deadline = deadline.fuse();
    futures::pin_mut!(work, cancellation, generation_cancellation, deadline);

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

fn is_module_failure(error: &RuntimeFailure) -> bool {
    matches!(error, RuntimeFailure::ModuleFailure { .. })
}

fn schedule_module_supervision_after_failure(
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

fn handle_supervision_schedule_failure(
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

fn ensure_context_active(
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
    /// The App ran to completion.
    Completed,
    /// The embedding Runner requested cooperative shutdown.
    ShutdownRequested,
    /// The App completed a bounded clean shutdown.
    CleanShutdown,
    /// The App could not start because a Module reported a startup failure.
    StartupFailure { error: RuntimeFailure },
    /// The running App reported a Runtime Failure during terminal cleanup.
    RuntimeFailure { error: RuntimeFailure },
    /// The App exceeded its one global shutdown deadline.
    ShutdownTimeout,
}

/// A reason the Kernel rejected a Resolved App Plan before boot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanValidationError {
    /// The Plan schema cannot be executed by this Kernel version.
    UnsupportedSchemaVersion { expected: u32, actual: u32 },
    /// The Plan graph is structurally invalid and cannot be booted.
    InvalidResolvedPlan { detail: String },
}

/// The portable App execution engine.
#[derive(Debug)]
pub struct Kernel;

impl Kernel {
    /// Starts one App backed by a single statically linked native Adapter package.
    pub async fn start_native<D: RuntimeDriver, A: NativeExecutionAdapter>(
        plan: ResolvedAppPlan,
        driver: D,
        adapter: A,
    ) -> Result<NativeApp, RuntimeFailure> {
        Self::start(plan, driver, ExecutionAdapterCatalog::single(adapter)).await
    }

    /// Starts Module Instances through the Adapter catalog assembled by the Runner.
    pub async fn start<D: RuntimeDriver>(
        plan: ResolvedAppPlan,
        driver: D,
        adapters: ExecutionAdapterCatalog,
    ) -> Result<NativeApp, RuntimeFailure> {
        plan.validate()
            .map_err(|error| runtime_plan_error(&error))?;

        let activation_order = plan
            .activation_order()
            .map_err(|error| runtime_plan_error(&error))?;
        let adapters = Rc::new(adapters);
        let PreparedNativeApp {
            bindings: prepared_bindings,
            modules,
            generations,
        } = adapters.prepare(&plan)?;
        let (bindings, endpoint_states) = native_bindings(&plan, &prepared_bindings);
        let dependencies = module_dependencies(&plan, &bindings);
        let driver_control = DriverControl::new(&driver);
        let admission = AppAdmission::new();
        let module_runtimes = native_module_runtimes(&plan, &driver, modules, generations)?;
        let ready_gate = AppReadyGate::new();
        let prepared_instances = prepare_native_modules(
            &module_runtimes,
            &dependencies,
            &activation_order,
            &admission,
        )
        .await?;
        if let Err(error) = activate_native_modules(
            &module_runtimes,
            &dependencies,
            &activation_order,
            &ready_gate,
            &admission,
        )
        .await
        {
            let _ = deactivate_in_reverse(
                &module_runtimes,
                &dependencies,
                &prepared_instances,
                DeactivationReason::StartupRollback,
                &admission,
            )
            .await;
            return Err(error);
        }
        open_native_readiness(&driver, &ready_gate, &admission).await;
        let runtime = Rc::new(NativeAppRuntime {
            plan: plan.clone(),
            adapters,
            modules: module_runtimes,
            dependencies,
            endpoint_states,
            supervision: RefCell::new(module_supervision(&plan)),
            activation_order,
            ready_gate,
            admission,
            driver: driver_control,
            request_ids: Rc::new(Cell::new(1)),
            supervision_cancellation: CancellationToken::new(),
            shutdown_started: Cell::new(false),
            shutdown_result: RefCell::new(None),
            terminal_failure: RefCell::new(None),
        });
        Ok(NativeApp { bindings, runtime })
    }

    /// Validates and boots one already resolved Plan.
    pub async fn boot<D: RuntimeDriver>(
        plan: ResolvedAppPlan,
        driver: D,
    ) -> Result<TerminalOutcome, PlanValidationError> {
        if let Err(error) = plan.validate() {
            return Err(match error {
                PlanResolutionError::UnsupportedSchemaVersion { expected, actual } => {
                    PlanValidationError::UnsupportedSchemaVersion { expected, actual }
                }
                error => PlanValidationError::InvalidResolvedPlan {
                    detail: error.to_string(),
                },
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

fn runtime_plan_error(error: &PlanResolutionError) -> RuntimeFailure {
    RuntimeFailure::InvalidResolvedPlan {
        detail: error.to_string(),
    }
}

fn native_module_runtimes<D: RuntimeDriver>(
    plan: &ResolvedAppPlan,
    driver: &D,
    mut modules: BTreeMap<String, Rc<dyn ModuleLifecycle>>,
    mut generations: BTreeMap<String, PreparedNativeModule>,
) -> Result<BTreeMap<String, NativeModuleRuntime>, RuntimeFailure> {
    if let Some(instance_key) = modules.keys().find(|instance_key| {
        !plan
            .module_instances()
            .iter()
            .any(|instance| instance.instance_key() == instance_key.as_str())
    }) {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!("Adapter prepared unknown Module Instance `{instance_key}`"),
        });
    }
    if let Some(instance_key) = generations.keys().find(|instance_key| {
        !plan
            .module_instances()
            .iter()
            .any(|instance| instance.instance_key() == instance_key.as_str())
    }) {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!("Adapter prepared unknown Module Instance generation `{instance_key}`"),
        });
    }

    let mut runtimes = BTreeMap::new();
    for instance in plan.module_instances() {
        let lifecycle = generations
            .remove(instance.instance_key())
            .map(|generation| generation.lifecycle())
            .or_else(|| modules.remove(instance.instance_key()))
            .unwrap_or_else(|| Rc::new(NoopModuleLifecycle));
        runtimes.insert(
            instance.instance_key().to_owned(),
            NativeModuleRuntime {
                generation: RefCell::new(Some(NativeModuleGeneration {
                    lifecycle,
                    tasks: ManagedTaskScope::new(driver),
                    resources: ManagedResourceScope::new(),
                })),
            },
        );
    }
    Ok(runtimes)
}

async fn prepare_native_modules(
    modules: &BTreeMap<String, NativeModuleRuntime>,
    dependencies: &BTreeMap<String, ModuleDependencies>,
    activation_order: &[String],
    admission: &AppAdmission,
) -> Result<Vec<String>, RuntimeFailure> {
    let mut prepared_instances = Vec::with_capacity(activation_order.len());
    for instance_key in activation_order {
        let module = modules
            .get(instance_key)
            .expect("activation order only contains planned Module Instances");
        let (lifecycle, tasks, resources) = module
            .generation_parts()
            .expect("every startup Module Instance has a generation");
        let cancellation = tasks.cancellation();
        prepared_instances.push(instance_key.clone());
        let context = PrepareContext {
            instance_key: instance_key.clone(),
            dependencies: dependencies.get(instance_key).cloned().unwrap_or_default(),
            resources,
            cancellation,
            admission: admission.clone(),
        };
        if let Err(error) = lifecycle.prepare(context).await {
            let _ = deactivate_in_reverse(
                modules,
                dependencies,
                &prepared_instances,
                DeactivationReason::StartupRollback,
                admission,
            )
            .await;
            return Err(error);
        }
    }
    Ok(prepared_instances)
}

async fn activate_native_modules(
    modules: &BTreeMap<String, NativeModuleRuntime>,
    dependencies: &BTreeMap<String, ModuleDependencies>,
    activation_order: &[String],
    ready_gate: &AppReadyGate,
    admission: &AppAdmission,
) -> Result<(), RuntimeFailure> {
    for instance_key in activation_order {
        let module = modules
            .get(instance_key)
            .expect("activation order only contains planned Module Instances");
        let (lifecycle, tasks, resources) = module
            .generation_parts()
            .expect("every startup Module Instance has a generation");
        let cancellation = tasks.cancellation();
        let context = ActivateContext {
            instance_key: instance_key.clone(),
            dependencies: dependencies.get(instance_key).cloned().unwrap_or_default(),
            ready_gate: ready_gate.clone(),
            tasks,
            resources,
            cancellation,
            admission: admission.clone(),
        };
        lifecycle.activate(context).await?;
    }
    Ok(())
}

async fn open_native_readiness<D: RuntimeDriver>(
    driver: &D,
    ready_gate: &AppReadyGate,
    admission: &AppAdmission,
) {
    ready_gate.open();
    admission.open();
    driver.yield_now().await;
}

fn native_bindings(
    plan: &ResolvedAppPlan,
    prepared: &[PreparedBinding],
) -> (NativeBindingTable, NativeEndpointStateTable) {
    let mut bindings = BTreeMap::new();
    let mut endpoint_states = BTreeMap::new();
    for binding in plan.capability_bindings() {
        let Some(endpoint) = prepared.iter().find_map(|prepared| {
            (prepared.consumer_instance == binding.consumer_instance()
                && prepared.provider_instance == binding.provider_instance()
                && prepared.endpoint.capability_id() == binding.capability_id())
            .then_some(&prepared.endpoint)
        }) else {
            continue;
        };
        let state = endpoint_states
            .entry((
                binding.provider_instance().to_owned(),
                endpoint.capability_id().to_owned(),
            ))
            .or_insert_with(|| Rc::new(NativeEndpointState::new(endpoint.clone(), 1)))
            .clone();
        let admissions = endpoint
            .operations()
            .iter()
            .map(|operation| {
                (
                    (*operation).to_owned(),
                    RequestAdmission::new(plan.request_admission_for(binding, operation)),
                )
            })
            .collect();
        bindings
            .entry((
                binding.consumer_instance().to_owned(),
                endpoint.capability_id(),
            ))
            .or_insert_with(Vec::new)
            .push(NativeEndpointBinding {
                module_instance: binding.provider_instance().to_owned(),
                state,
                admissions,
            });
    }
    (bindings, endpoint_states)
}

fn module_dependencies(
    plan: &ResolvedAppPlan,
    endpoints: &BTreeMap<(String, &'static str), Vec<NativeEndpointBinding>>,
) -> BTreeMap<String, ModuleDependencies> {
    let mut dependencies: BTreeMap<String, ModuleDependencies> = plan
        .module_instances()
        .iter()
        .map(|instance| {
            (
                instance.instance_key().to_owned(),
                ModuleDependencies::default(),
            )
        })
        .collect();
    for binding in plan.capability_bindings() {
        dependencies
            .entry(binding.consumer_instance().to_owned())
            .or_default()
            .bindings
            .push(ModuleDependency::new(
                binding.capability_id(),
                binding.provider_instance(),
                binding.provider_order(),
                endpoints
                    .iter()
                    .find(|((consumer, capability), _)| {
                        consumer == binding.consumer_instance()
                            && *capability == binding.capability_id()
                    })
                    .and_then(|(_, endpoints)| endpoints.get(binding.provider_order()))
                    .map(|endpoint| ModuleDependencyHandle {
                        endpoint: endpoint.state.clone(),
                    }),
            ));
    }
    dependencies
}

async fn deactivate_in_reverse(
    modules: &BTreeMap<String, NativeModuleRuntime>,
    dependencies: &BTreeMap<String, ModuleDependencies>,
    activation_order: &[String],
    reason: DeactivationReason,
    admission: &AppAdmission,
) -> Option<RuntimeFailure> {
    let mut first_error = None;
    for instance_key in activation_order.iter().rev() {
        let module = modules
            .get(instance_key)
            .expect("deactivation order only contains planned Module Instances");
        let Some(generation) = module.take_generation() else {
            continue;
        };
        generation.tasks.close();
        generation.resources.close();
        let result = generation
            .lifecycle
            .deactivate(DeactivateContext {
                instance_key: instance_key.clone(),
                dependencies: dependencies.get(instance_key).cloned().unwrap_or_default(),
                reason,
                tasks: generation.tasks.clone(),
                resources: generation.resources.clone(),
                cancellation: generation.tasks.cancellation(),
                admission: admission.clone(),
            })
            .await;
        if let Err(error) = result
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        generation.tasks.cancel_all().await;
        if let Some(error) = generation.resources.release_all().await
            && first_error.is_none()
        {
            first_error = Some(error);
        }
    }
    first_error
}

async fn shutdown_native_modules(runtime: &NativeAppRuntime, timeout: Duration) -> ShutdownOutcome {
    let deadline = (runtime.driver.now)().saturating_add(timeout);
    (runtime.driver.yield_now)().await;

    for module in runtime.modules.values() {
        let Some((_, tasks, _)) = module.generation_parts() else {
            continue;
        };
        if !tasks.drain_until(&runtime.driver, deadline).await {
            terminate_remaining_cleanup(runtime);
            return ShutdownOutcome::Timeout;
        }
    }

    let mut first_error = None;
    for instance_key in runtime.activation_order.iter().rev() {
        let module = runtime
            .modules
            .get(instance_key)
            .expect("deactivation order only contains planned Module Instances");
        let Some((lifecycle, tasks, resources)) = module.generation_parts() else {
            continue;
        };
        let cancellation = tasks.cancellation();
        let result = wait_until(
            &runtime.driver,
            deadline,
            lifecycle.deactivate(DeactivateContext {
                instance_key: instance_key.clone(),
                dependencies: runtime
                    .dependencies
                    .get(instance_key)
                    .cloned()
                    .unwrap_or_default(),
                reason: DeactivationReason::Shutdown,
                tasks,
                resources: resources.clone(),
                cancellation,
                admission: runtime.admission.clone(),
            }),
        )
        .await;
        match result {
            Some(Ok(())) => {}
            Some(Err(error)) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
            None => {
                terminate_remaining_cleanup(runtime);
                return ShutdownOutcome::Timeout;
            }
        }

        match resources.release_all_until(&runtime.driver, deadline).await {
            Ok(Some(error)) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
            Ok(None) => {}
            Err(()) => {
                terminate_remaining_cleanup(runtime);
                return ShutdownOutcome::Timeout;
            }
        }
    }

    first_error.map_or(ShutdownOutcome::Clean, |error| {
        ShutdownOutcome::RuntimeFailure { error }
    })
}

fn terminate_remaining_cleanup(runtime: &NativeAppRuntime) {
    for module in runtime.modules.values() {
        if let Some((_, tasks, resources)) = module.generation_parts() {
            tasks.abort_all();
            resources.release_unawaited();
        }
    }
}

fn module_supervision(plan: &ResolvedAppPlan) -> BTreeMap<String, ModuleSupervision> {
    plan.module_instances()
        .iter()
        .map(|instance| {
            (
                instance.instance_key().to_owned(),
                ModuleSupervision {
                    policy: instance.restart_policy(),
                    criticality: instance.criticality(),
                    required_path: plan.module_instance_is_required(instance.instance_key()),
                    generation: 1,
                    attempts: Vec::new(),
                    stable_since: Some(Duration::ZERO),
                    restarting: false,
                },
            )
        })
        .collect()
}

fn begin_module_supervision(
    runtime: &Rc<NativeAppRuntime>,
    instance_key: &str,
) -> Result<bool, RuntimeFailure> {
    if runtime.shutdown_started.get() || runtime.terminal_failure.borrow().is_some() {
        return Err(RuntimeFailure::AdmissionClosed);
    }
    if !runtime.modules.contains_key(instance_key) {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!("unknown Module Instance `{instance_key}`"),
        });
    }
    let mut supervision = runtime.supervision.borrow_mut();
    let state = supervision
        .get_mut(instance_key)
        .expect("every planned Module Instance has supervision state");
    if state.restarting {
        return Ok(false);
    }
    let now = (runtime.driver.now)();
    if state.stable_since.is_some_and(|stable_since| {
        !state.policy.stability().is_zero()
            && now.saturating_sub(stable_since) >= state.policy.stability()
    }) {
        state.attempts.clear();
    } else {
        state
            .attempts
            .retain(|attempted_at| now.saturating_sub(*attempted_at) < state.policy.window());
    }
    state.stable_since = None;
    state.restarting = true;
    drop(supervision);
    for ((provider, _), endpoint) in &runtime.endpoint_states {
        if provider == instance_key {
            endpoint.mark_unavailable();
        }
    }
    Ok(true)
}

fn schedule_module_supervision(
    runtime: &Rc<NativeAppRuntime>,
    instance_key: &str,
) -> Result<(), RuntimeFailure> {
    let task_runtime = runtime.clone();
    let task_instance_key = instance_key.to_owned();
    (runtime.driver.spawn_local)(Box::pin(async move {
        let _ = supervise_module_instance(task_runtime, task_instance_key).await;
    }))
    .map(|_| ())
    .map_err(|error| {
        if let Some(state) = runtime.supervision.borrow_mut().get_mut(instance_key) {
            state.restarting = false;
        }
        RuntimeFailure::Internal {
            detail: format!("failed to schedule Module supervision: {error:?}"),
        }
    })
}

async fn supervise_module_instance(
    runtime: Rc<NativeAppRuntime>,
    instance_key: String,
) -> Result<(), RuntimeFailure> {
    if runtime.shutdown_started.get() {
        return Err(RuntimeFailure::AdmissionClosed);
    }
    let generation = runtime
        .modules
        .get(&instance_key)
        .and_then(NativeModuleRuntime::take_generation);
    if let Some(generation) = generation
        && let Some(error) = cleanup_native_generation(
            &runtime,
            &instance_key,
            generation,
            DeactivationReason::SupervisionRestart,
        )
        .await
    {
        return finish_module_cleanup_failure(&runtime, &instance_key, error);
    }

    loop {
        let Some((_attempt, delay)) = next_restart_attempt(&runtime, &instance_key) else {
            return finish_module_exhaustion(&runtime, &instance_key);
        };
        if !wait_for_supervision_delay(&runtime, delay).await {
            return Err(RuntimeFailure::AdmissionClosed);
        }
        if runtime.shutdown_started.get() {
            return Err(RuntimeFailure::AdmissionClosed);
        }

        let Some(instance) = runtime
            .plan
            .module_instances()
            .iter()
            .find(|instance| instance.instance_key() == instance_key)
        else {
            return Err(RuntimeFailure::InvalidResolvedPlan {
                detail: format!("unknown Module Instance `{instance_key}`"),
            });
        };
        let Some(adapter) = runtime.adapters.adapter(instance.execution_class()) else {
            return Err(RuntimeFailure::UnavailableExecutionClass {
                instance_key: instance_key.clone(),
                execution_class: instance.execution_class().to_string(),
            });
        };
        let Ok(prepared) = adapter.recreate(&runtime.plan, &instance_key) else {
            continue;
        };
        let (endpoints, lifecycle) = prepared.into_parts();
        if validate_native_endpoint_set(&instance_key, instance, &endpoints).is_err() {
            continue;
        }
        let generation_number = runtime
            .supervision
            .borrow()
            .get(&instance_key)
            .map_or(1, |state| state.generation.saturating_add(1));
        let generation =
            match prepare_and_activate_generation(&runtime, &instance_key, lifecycle).await {
                Ok(generation) => generation,
                Err(GenerationPreparationFailure::Lifecycle) => continue,
                Err(GenerationPreparationFailure::Cleanup(error)) => {
                    return finish_module_cleanup_failure(&runtime, &instance_key, error);
                }
            };

        if runtime.shutdown_started.get() {
            let _ = cleanup_native_generation(
                &runtime,
                &instance_key,
                generation,
                DeactivationReason::SupervisionRestart,
            )
            .await;
            return Err(RuntimeFailure::AdmissionClosed);
        }

        if let Some(module) = runtime.modules.get(&instance_key) {
            module.install_generation(generation);
        }
        install_module_endpoints(&runtime, &instance_key, endpoints, generation_number);
        if let Some(state) = runtime.supervision.borrow_mut().get_mut(&instance_key) {
            state.generation = generation_number;
            state.stable_since = Some((runtime.driver.now)());
            state.restarting = false;
        }
        return Ok(());
    }
}

async fn wait_for_supervision_delay(runtime: &NativeAppRuntime, delay: Duration) -> bool {
    if delay.is_zero() {
        return true;
    }
    let deadline = (runtime.driver.now)().saturating_add(delay);
    let timer = (runtime.driver.sleep_until)(deadline).fuse();
    let cancellation = runtime.supervision_cancellation.cancelled().fuse();
    futures::pin_mut!(timer, cancellation);
    matches!(select(timer, cancellation).await, Either::Left(((), _)))
}

fn next_restart_attempt(
    runtime: &NativeAppRuntime,
    instance_key: &str,
) -> Option<(usize, Duration)> {
    let now = (runtime.driver.now)();
    let mut supervision = runtime.supervision.borrow_mut();
    let state = supervision
        .get_mut(instance_key)
        .expect("every planned Module Instance has supervision state");
    if state.policy.mode() != RestartMode::OnFailure {
        return None;
    }
    // Keep all attempts from this supervision episode together. Pruning here
    // would turn a backoff longer than the rolling window into an unbounded
    // restart loop.
    if state.attempts.len() >= state.policy.max_attempts() {
        return None;
    }
    let attempt = state.attempts.len().saturating_add(1);
    state.attempts.push(now);
    let exponent = u32::try_from(attempt.saturating_sub(1).min(31)).unwrap_or(31);
    let backoff = state.policy.backoff().saturating_mul(1_u32 << exponent);
    let jitter = (runtime.driver.jitter)(state.policy.jitter()).min(state.policy.jitter());
    Some((attempt, backoff.saturating_add(jitter)))
}

fn finish_module_exhaustion(
    runtime: &Rc<NativeAppRuntime>,
    instance_key: &str,
) -> Result<(), RuntimeFailure> {
    let (attempts, must_fail) = {
        let mut supervision = runtime.supervision.borrow_mut();
        let state = supervision
            .get_mut(instance_key)
            .expect("every planned Module Instance has supervision state");
        state.restarting = false;
        (
            state.attempts.len(),
            state.criticality.is_critical() || state.required_path,
        )
    };
    if !must_fail {
        return Ok(());
    }
    let error = RuntimeFailure::ModuleRestartExhausted {
        instance: instance_key.to_owned(),
        attempts,
    };
    runtime.terminal_failure.replace(Some(error.clone()));
    runtime.begin_shutdown();
    Err(error)
}

fn finish_module_cleanup_failure(
    runtime: &Rc<NativeAppRuntime>,
    instance_key: &str,
    error: RuntimeFailure,
) -> Result<(), RuntimeFailure> {
    let must_fail = {
        let mut supervision = runtime.supervision.borrow_mut();
        let state = supervision
            .get_mut(instance_key)
            .expect("every planned Module Instance has supervision state");
        state.restarting = false;
        state.criticality.is_critical() || state.required_path
    };
    if must_fail {
        runtime.terminal_failure.replace(Some(error.clone()));
        runtime.begin_shutdown();
    }
    Err(error)
}

async fn prepare_and_activate_generation(
    runtime: &NativeAppRuntime,
    instance_key: &str,
    lifecycle: Rc<dyn ModuleLifecycle>,
) -> Result<NativeModuleGeneration, GenerationPreparationFailure> {
    let tasks = ManagedTaskScope::new_from_driver_control(&runtime.driver);
    let resources = ManagedResourceScope::new();
    let prepared = NativeModuleGeneration {
        lifecycle: lifecycle.clone(),
        tasks: tasks.clone(),
        resources: resources.clone(),
    };
    let dependencies = runtime
        .dependencies
        .get(instance_key)
        .cloned()
        .unwrap_or_default();
    if lifecycle
        .prepare(PrepareContext {
            instance_key: instance_key.to_owned(),
            dependencies: dependencies.clone(),
            resources: resources.clone(),
            cancellation: tasks.cancellation(),
            admission: runtime.admission.clone(),
        })
        .await
        .is_err()
    {
        let failure = if let Some(cleanup_error) = cleanup_native_generation(
            runtime,
            instance_key,
            prepared,
            DeactivationReason::SupervisionRestart,
        )
        .await
        {
            GenerationPreparationFailure::Cleanup(cleanup_error)
        } else {
            GenerationPreparationFailure::Lifecycle
        };
        return Err(failure);
    }
    if lifecycle
        .activate(ActivateContext {
            instance_key: instance_key.to_owned(),
            dependencies,
            ready_gate: runtime.ready_gate.clone(),
            tasks: tasks.clone(),
            resources: resources.clone(),
            cancellation: tasks.cancellation(),
            admission: runtime.admission.clone(),
        })
        .await
        .is_err()
    {
        let failure = if let Some(cleanup_error) = cleanup_native_generation(
            runtime,
            instance_key,
            prepared,
            DeactivationReason::SupervisionRestart,
        )
        .await
        {
            GenerationPreparationFailure::Cleanup(cleanup_error)
        } else {
            GenerationPreparationFailure::Lifecycle
        };
        return Err(failure);
    }
    Ok(prepared)
}

async fn cleanup_native_generation(
    runtime: &NativeAppRuntime,
    instance_key: &str,
    generation: NativeModuleGeneration,
    reason: DeactivationReason,
) -> Option<RuntimeFailure> {
    generation.tasks.close();
    generation.resources.close();
    generation.tasks.cancel_all().await;
    let dependencies = runtime
        .dependencies
        .get(instance_key)
        .cloned()
        .unwrap_or_default();
    let mut first_error = None;
    if let Err(error) = generation
        .lifecycle
        .deactivate(DeactivateContext {
            instance_key: instance_key.to_owned(),
            dependencies,
            reason,
            tasks: generation.tasks.clone(),
            resources: generation.resources.clone(),
            cancellation: generation.tasks.cancellation(),
            admission: runtime.admission.clone(),
        })
        .await
    {
        first_error = Some(error);
    }
    if let Some(error) = generation.resources.release_all().await
        && first_error.is_none()
    {
        first_error = Some(error);
    }
    first_error
}

fn install_module_endpoints(
    runtime: &NativeAppRuntime,
    instance_key: &str,
    endpoints: Vec<Rc<dyn NativeRequestEndpoint>>,
    generation: u64,
) {
    for endpoint in endpoints {
        if let Some(state) = runtime
            .endpoint_states
            .get(&(instance_key.to_owned(), endpoint.capability_id().to_owned()))
        {
            state.install(endpoint, generation);
        }
    }
}

fn validate_native_endpoint_set(
    instance_key: &str,
    expected: &lenso_app_plan::ModuleInstancePlan,
    actual: &[Rc<dyn NativeRequestEndpoint>],
) -> Result<(), RuntimeFailure> {
    if expected.provided_capabilities().len() != actual.len() {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!(
                "Module Instance `{instance_key}` prepared {} endpoints; expected {}",
                actual.len(),
                expected.provided_capabilities().len()
            ),
        });
    }
    for descriptor in expected.provided_capabilities() {
        let matching: Vec<_> = actual
            .iter()
            .filter(|endpoint| endpoint.capability_id() == descriptor.capability_id())
            .collect();
        if matching.len() != 1 {
            return Err(RuntimeFailure::InvalidResolvedPlan {
                detail: format!(
                    "Module Instance `{instance_key}` prepared {} endpoints for Capability `{}`",
                    matching.len(),
                    descriptor.capability_id()
                ),
            });
        }
        let endpoint = matching[0];
        let actual_operations = endpoint.operations();
        let expected_operations: Vec<_> =
            descriptor.operations().iter().map(String::as_str).collect();
        if endpoint.descriptor_version() != descriptor.descriptor_version()
            || actual_operations != expected_operations.as_slice()
        {
            return Err(RuntimeFailure::InvalidResolvedPlan {
                detail: format!(
                    "Module Instance `{instance_key}` endpoint `{}` differs from its resolved Descriptor",
                    descriptor.capability_id()
                ),
            });
        }
        let mut unique = actual_operations.to_vec();
        unique.sort_unstable();
        unique.dedup();
        if unique.len() != actual_operations.len() {
            return Err(RuntimeFailure::InvalidResolvedPlan {
                detail: format!(
                    "Module Instance `{instance_key}` endpoint `{}` has duplicate Operations",
                    descriptor.capability_id()
                ),
            });
        }
    }
    Ok(())
}

#[derive(Debug)]
struct DeterministicState {
    now: Cell<Duration>,
    shutdown_requested: Cell<bool>,
    jitter: Cell<Duration>,
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
                jitter: Cell::new(Duration::ZERO),
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

    /// Sets the deterministic jitter returned to supervision callers, capped by each policy.
    pub fn set_jitter(&self, jitter: Duration) {
        self.state.jitter.set(jitter);
    }

    /// Configures deterministic jitter while retaining builder-style Driver setup.
    #[must_use]
    pub fn with_jitter(self, jitter: Duration) -> Self {
        self.set_jitter(jitter);
        self
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

    fn jitter(&self, maximum: Duration) -> Duration {
        self.state.jitter.get().min(maximum)
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
