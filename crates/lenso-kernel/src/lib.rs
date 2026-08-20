//! Portable Lenso vNext Kernel and Runtime Driver seam.

use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
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
use lenso_app_plan::{PlanResolutionError, RequestAdmissionPlan, ResolvedAppPlan};

type ErasedValue = Box<dyn Any>;
type ErasedDomainResult = Result<ErasedValue, ErasedValue>;

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
    endpoint: Rc<dyn NativeRequestEndpoint>,
}

impl ModuleDependencyHandle {
    /// Returns the Capability implemented by this handle.
    pub fn capability_id(&self) -> &'static str {
        self.endpoint.capability_id()
    }

    /// Returns the exact Descriptor version implemented by this handle.
    pub fn descriptor_version(&self) -> &'static str {
        self.endpoint.descriptor_version()
    }

    /// Returns the exact Operation table implemented by this handle.
    pub fn operations(&self) -> &'static [&'static str] {
        self.endpoint.operations()
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

/// Prepared native bindings returned by an Execution Adapter to Kernel.
#[derive(Debug)]
pub struct PreparedNativeApp {
    bindings: BTreeMap<(String, &'static str), Vec<Rc<dyn NativeRequestEndpoint>>>,
    modules: BTreeMap<String, Rc<dyn ModuleLifecycle>>,
}

impl PreparedNativeApp {
    /// Completes Adapter preparation with one endpoint per consumer binding.
    pub fn new(bindings: BTreeMap<(String, &'static str), Rc<dyn NativeRequestEndpoint>>) -> Self {
        Self {
            bindings: bindings
                .into_iter()
                .map(|(key, endpoint)| (key, vec![endpoint]))
                .collect(),
            modules: BTreeMap::new(),
        }
    }

    /// Completes Adapter preparation with deterministic one, optional, and many bindings.
    pub fn from_many(
        bindings: BTreeMap<(String, &'static str), Vec<Rc<dyn NativeRequestEndpoint>>>,
    ) -> Self {
        Self {
            bindings,
            modules: BTreeMap::new(),
        }
    }

    /// Completes Adapter preparation with Module lifecycle implementations.
    pub fn with_modules(
        bindings: BTreeMap<(String, &'static str), Vec<Rc<dyn NativeRequestEndpoint>>>,
        modules: BTreeMap<String, Rc<dyn ModuleLifecycle>>,
    ) -> Self {
        Self { bindings, modules }
    }
}

/// Host-specific seam that instantiates native Module generations and prepares endpoints.
pub trait NativeExecutionAdapter: std::fmt::Debug {
    /// Instantiates the exact Plan and confirms its endpoint and binding tables.
    fn prepare(&self, plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure>;
}

#[derive(Clone, Debug)]
struct NativeEndpointBinding {
    endpoint: Rc<dyn NativeRequestEndpoint>,
    admissions: BTreeMap<String, RequestAdmission>,
}

impl NativeEndpointBinding {
    fn admission(&self, operation: &str) -> Option<&RequestAdmission> {
        self.admissions.get(operation)
    }
}

#[derive(Debug)]
struct NativeModuleRuntime {
    lifecycle: Rc<dyn ModuleLifecycle>,
    tasks: ManagedTaskScope,
    resources: ManagedResourceScope,
}

struct NativeAppRuntime {
    modules: BTreeMap<String, NativeModuleRuntime>,
    dependencies: BTreeMap<String, ModuleDependencies>,
    activation_order: Vec<String>,
    ready_gate: AppReadyGate,
    admission: AppAdmission,
    driver: DriverControl,
    request_ids: Rc<Cell<RequestId>>,
    shutdown_started: Cell<bool>,
    shutdown_result: RefCell<Option<ShutdownOutcome>>,
}

impl std::fmt::Debug for NativeAppRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeAppRuntime")
            .field("module_count", &self.modules.len())
            .field("ready", &self.ready_gate.is_open())
            .field("accepting", &self.admission.is_open())
            .field("next_request_id", &self.request_ids.get())
            .field("shutdown_started", &self.shutdown_started.get())
            .finish_non_exhaustive()
    }
}

impl NativeAppRuntime {
    fn begin_shutdown(&self) {
        if self.shutdown_started.replace(true) {
            return;
        }
        self.admission.close();
        for module in self.modules.values() {
            module.tasks.close();
            module.resources.close();
        }
    }
}

/// A started native App whose generated clients can invoke resolved bindings.
#[derive(Debug)]
pub struct NativeApp {
    bindings: BTreeMap<(String, &'static str), Vec<NativeEndpointBinding>>,
    runtime: NativeAppRuntime,
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
            self.runtime.admission.clone(),
            self.runtime.driver.clone(),
            self.runtime.request_ids.clone(),
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
                    self.runtime.admission.clone(),
                    self.runtime.driver.clone(),
                    self.runtime.request_ids.clone(),
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
            self.runtime.admission.clone(),
            self.runtime.driver.clone(),
            self.runtime.request_ids.clone(),
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
    admission: AppAdmission,
    driver: DriverControl,
    request_ids: Rc<Cell<RequestId>>,
    caller_instance: String,
    capability: PhantomData<fn() -> C>,
}

impl<C: RequestCapability> NativeRequestHandle<C> {
    fn from_endpoints(
        endpoints: &[NativeEndpointBinding],
        admission: AppAdmission,
        driver: DriverControl,
        request_ids: Rc<Cell<RequestId>>,
        caller_instance: &str,
    ) -> Self {
        Self {
            endpoints: endpoints.to_vec(),
            admission,
            driver,
            request_ids,
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
        if self.admission.is_closed() {
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
        let admission =
            endpoint
                .admission(operation)
                .ok_or_else(|| RuntimeFailure::UnknownOperation {
                    capability: C::ID,
                    operation: operation.to_owned(),
                })?;
        let _permit = admission
            .acquire(C::ID, operation, context.clone(), self.driver.clone())
            .await?;
        ensure_context_active(&self.driver, &context)?;
        let outcome = await_with_context(
            &self.driver,
            &context,
            endpoint
                .endpoint
                .invoke(operation, Box::new(request), context.clone()),
        )
        .await??;
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
        if self.admission.is_closed() {
            return Err(RuntimeFailure::AdmissionClosed);
        }
        if self.endpoints.is_empty() {
            return Ok(Vec::new());
        }
        let mut outcomes = Vec::with_capacity(self.endpoints.len());
        for endpoint in &self.endpoints {
            let admission =
                endpoint
                    .admission(operation)
                    .ok_or_else(|| RuntimeFailure::UnknownOperation {
                        capability: C::ID,
                        operation: operation.to_owned(),
                    })?;
            let _permit = admission
                .acquire(C::ID, operation, context.clone(), self.driver.clone())
                .await?;
            ensure_context_active(&self.driver, &context)?;
            let outcome = await_with_context(
                &self.driver,
                &context,
                endpoint
                    .endpoint
                    .invoke(operation, Box::new(request.clone()), context.clone()),
            )
            .await??;
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
        let request_id = self.request_ids.get();
        self.request_ids.set(request_id.saturating_add(1));
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
        Self {
            now: Rc::new(move || now_driver.now()),
            sleep_until: Rc::new(move |deadline| sleep_driver.sleep_until(deadline)),
            yield_now: Rc::new(move || yield_driver.yield_now()),
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
    /// Starts statically linked native Modules selected by a resolved Plan.
    pub async fn start_native<D: RuntimeDriver, A: NativeExecutionAdapter>(
        plan: ResolvedAppPlan,
        driver: D,
        adapter: A,
    ) -> Result<NativeApp, RuntimeFailure> {
        plan.validate()
            .map_err(|error| runtime_plan_error(&error))?;

        let activation_order = plan
            .activation_order()
            .map_err(|error| runtime_plan_error(&error))?;
        let PreparedNativeApp {
            bindings: prepared_bindings,
            modules,
        } = adapter.prepare(&plan)?;
        let bindings = native_bindings(&plan, &prepared_bindings);
        let dependencies = module_dependencies(&plan, &bindings);
        let driver_control = DriverControl::new(&driver);
        let admission = AppAdmission::new();
        let module_runtimes = native_module_runtimes(&plan, &driver, modules)?;
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
        Ok(NativeApp {
            bindings,
            runtime: NativeAppRuntime {
                modules: module_runtimes,
                dependencies,
                activation_order,
                ready_gate,
                admission,
                driver: driver_control,
                request_ids: Rc::new(Cell::new(1)),
                shutdown_started: Cell::new(false),
                shutdown_result: RefCell::new(None),
            },
        })
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

    let mut runtimes = BTreeMap::new();
    for instance in plan.module_instances() {
        let lifecycle = modules
            .remove(instance.instance_key())
            .unwrap_or_else(|| Rc::new(NoopModuleLifecycle));
        runtimes.insert(
            instance.instance_key().to_owned(),
            NativeModuleRuntime {
                lifecycle,
                tasks: ManagedTaskScope::new(driver),
                resources: ManagedResourceScope::new(),
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
        prepared_instances.push(instance_key.clone());
        let context = PrepareContext {
            instance_key: instance_key.clone(),
            dependencies: dependencies.get(instance_key).cloned().unwrap_or_default(),
            resources: module.resources.clone(),
            cancellation: module.tasks.cancellation(),
            admission: admission.clone(),
        };
        if let Err(error) = module.lifecycle.prepare(context).await {
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
        let context = ActivateContext {
            instance_key: instance_key.clone(),
            dependencies: dependencies.get(instance_key).cloned().unwrap_or_default(),
            ready_gate: ready_gate.clone(),
            tasks: module.tasks.clone(),
            resources: module.resources.clone(),
            cancellation: module.tasks.cancellation(),
            admission: admission.clone(),
        };
        module.lifecycle.activate(context).await?;
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
    prepared: &BTreeMap<(String, &'static str), Vec<Rc<dyn NativeRequestEndpoint>>>,
) -> BTreeMap<(String, &'static str), Vec<NativeEndpointBinding>> {
    let mut bindings = BTreeMap::new();
    for binding in plan.capability_bindings() {
        let Some(endpoints) = prepared
            .iter()
            .find_map(|((consumer, capability), endpoints)| {
                (consumer == binding.consumer_instance() && *capability == binding.capability_id())
                    .then_some(endpoints)
            })
        else {
            continue;
        };
        let Some(endpoint) = endpoints.get(binding.provider_order()) else {
            continue;
        };
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
                endpoint: endpoint.clone(),
                admissions,
            });
    }
    bindings
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
                    .cloned()
                    .map(|endpoint| ModuleDependencyHandle {
                        endpoint: endpoint.endpoint,
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
        module.tasks.close();
        module.resources.close();
        if let Err(error) = module
            .lifecycle
            .deactivate(DeactivateContext {
                instance_key: instance_key.clone(),
                dependencies: dependencies.get(instance_key).cloned().unwrap_or_default(),
                reason,
                tasks: module.tasks.clone(),
                resources: module.resources.clone(),
                cancellation: module.tasks.cancellation(),
                admission: admission.clone(),
            })
            .await
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        module.tasks.cancel_all().await;
        if let Some(error) = module.resources.release_all().await
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
        if !module.tasks.drain_until(&runtime.driver, deadline).await {
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
        let result = wait_until(
            &runtime.driver,
            deadline,
            module.lifecycle.deactivate(DeactivateContext {
                instance_key: instance_key.clone(),
                dependencies: runtime
                    .dependencies
                    .get(instance_key)
                    .cloned()
                    .unwrap_or_default(),
                reason: DeactivationReason::Shutdown,
                tasks: module.tasks.clone(),
                resources: module.resources.clone(),
                cancellation: module.tasks.cancellation(),
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

        match module
            .resources
            .release_all_until(&runtime.driver, deadline)
            .await
        {
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
        module.tasks.abort_all();
        module.resources.release_unawaited();
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
