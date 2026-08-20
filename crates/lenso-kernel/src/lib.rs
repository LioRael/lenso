//! Portable Lenso vNext Kernel and Runtime Driver seam.

use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::BTreeMap,
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
    future::{AbortHandle, Abortable, LocalBoxFuture},
    task::{LocalSpawnExt, SpawnError},
};
use lenso_app_plan::{PlanResolutionError, ResolvedAppPlan};

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

/// A Kernel-owned task handle that is cleaned up with its Module generation.
#[derive(Clone, Debug)]
pub struct ManagedTask {
    task: Rc<RefCell<Option<DriverTask>>>,
}

impl ManagedTask {
    /// Requests cancellation of the underlying task.
    pub fn cancel(&self) {
        if let Some(task) = self.task.borrow().as_ref() {
            task.cancel();
        }
    }

    async fn cancel_and_join(self) {
        let task = self.task.borrow_mut().take();
        if let Some(task) = task {
            task.cancel();
            let _ = task.await;
        }
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
    pub fn spawn_local(&self, task: LocalTask) -> Result<ManagedTask, SpawnError> {
        let handle = ManagedTask {
            task: Rc::new(RefCell::new(Some((self.spawn)(task)?))),
        };
        self.state.tasks.borrow_mut().push(handle.clone());
        Ok(handle)
    }

    /// Returns the number of tasks still tracked by this scope.
    pub fn task_count(&self) -> usize {
        self.state.tasks.borrow().len()
    }

    async fn cancel_all(&self) {
        let tasks = std::mem::take(&mut *self.state.tasks.borrow_mut());
        for task in tasks {
            task.cancel_and_join().await;
        }
    }
}

/// Context supplied while a Module reserves reversible resources.
#[derive(Clone, Debug)]
pub struct PrepareContext {
    instance_key: String,
    dependencies: ModuleDependencies,
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
}

/// Context supplied while a Module initializes against prepared dependencies.
#[derive(Clone, Debug)]
pub struct ActivateContext {
    instance_key: String,
    dependencies: ModuleDependencies,
    ready_gate: AppReadyGate,
    tasks: ManagedTaskScope,
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
        }
    }

    /// Returns the generation-owned task scope.
    pub fn tasks(&self) -> &ManagedTaskScope {
        &self.tasks
    }
}

/// Context supplied after the App Ready Gate has opened.
#[derive(Clone, Debug)]
pub struct ReadinessContext {
    instance_key: String,
    dependencies: ModuleDependencies,
    ready_gate: AppReadyGate,
    tasks: ManagedTaskScope,
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

#[derive(Debug)]
struct NativeModuleRuntime {
    lifecycle: Rc<dyn ModuleLifecycle>,
    tasks: ManagedTaskScope,
}

#[derive(Debug)]
struct NativeAppRuntime {
    _modules: BTreeMap<String, NativeModuleRuntime>,
    _dependencies: BTreeMap<String, ModuleDependencies>,
    _activation_order: Vec<String>,
    ready_gate: AppReadyGate,
}

/// A started native App whose generated clients can invoke resolved bindings.
#[derive(Debug)]
pub struct NativeApp {
    bindings: BTreeMap<(String, &'static str), Vec<Rc<dyn NativeRequestEndpoint>>>,
    runtime: NativeAppRuntime,
}

impl NativeApp {
    /// Confirms that a generated client has one resolved binding before use.
    pub fn ensure_binding<C: RequestCapability>(
        &self,
        caller_instance: &str,
    ) -> Result<(), RuntimeFailure> {
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
        let endpoints = self
            .endpoints::<C>(caller_instance)
            .filter(|endpoints| !endpoints.is_empty())
            .ok_or(RuntimeFailure::Unavailable { capability: C::ID })?;
        Ok(NativeRequestHandle::from_endpoints(endpoints))
    }

    /// Materializes an optional typed handle; an absent binding remains `None`.
    pub fn optional_handle<C: RequestCapability>(
        &self,
        caller_instance: &str,
    ) -> Option<NativeRequestHandle<C>> {
        self.endpoints::<C>(caller_instance)
            .filter(|endpoints| !endpoints.is_empty())
            .map(NativeRequestHandle::from_endpoints)
    }

    /// Materializes a typed handle whose endpoints may be empty for a `many` requirement.
    pub fn many_handle<C: RequestCapability>(
        &self,
        caller_instance: &str,
    ) -> Result<NativeRequestHandle<C>, RuntimeFailure> {
        let endpoints = self.endpoints::<C>(caller_instance).unwrap_or(&[]);
        Ok(NativeRequestHandle::from_endpoints(endpoints))
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

    fn endpoints<C: RequestCapability>(
        &self,
        caller_instance: &str,
    ) -> Option<&[Rc<dyn NativeRequestEndpoint>]> {
        self.bindings
            .get(&(caller_instance.to_owned(), C::ID))
            .map(Vec::as_slice)
    }
}

/// Typed, immutable native Capability endpoints materialized before App boot completes.
#[derive(Debug)]
pub struct NativeRequestHandle<C: RequestCapability> {
    endpoints: Vec<Rc<dyn NativeRequestEndpoint>>,
    capability: PhantomData<fn() -> C>,
}

impl<C: RequestCapability> NativeRequestHandle<C> {
    fn from_endpoints(endpoints: &[Rc<dyn NativeRequestEndpoint>]) -> Self {
        Self {
            endpoints: endpoints.to_vec(),
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
        decode_outcome::<C>(endpoint.invoke(operation, Box::new(request)).await?)
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
        if self.endpoints.is_empty() {
            return Ok(Vec::new());
        }
        let mut outcomes = Vec::with_capacity(self.endpoints.len());
        for endpoint in &self.endpoints {
            outcomes.push(decode_outcome::<C>(
                endpoint
                    .invoke(operation, Box::new(request.clone()))
                    .await?,
            )?);
        }
        Ok(outcomes)
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
        let PreparedNativeApp { bindings, modules } = adapter.prepare(&plan)?;
        let dependencies = module_dependencies(&plan, &bindings);
        let module_runtimes = native_module_runtimes(&plan, &driver, modules)?;
        let ready_gate = AppReadyGate::new();
        let prepared_instances =
            prepare_native_modules(&module_runtimes, &dependencies, &activation_order).await?;
        if let Err(error) = activate_native_modules(
            &module_runtimes,
            &dependencies,
            &activation_order,
            &ready_gate,
        )
        .await
        {
            let _ = deactivate_in_reverse(
                &module_runtimes,
                &dependencies,
                &prepared_instances,
                DeactivationReason::StartupRollback,
            )
            .await;
            return Err(error);
        }
        open_native_readiness(&driver, &ready_gate).await;
        Ok(NativeApp {
            bindings,
            runtime: NativeAppRuntime {
                _modules: module_runtimes,
                _dependencies: dependencies,
                _activation_order: activation_order,
                ready_gate,
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
            },
        );
    }
    Ok(runtimes)
}

async fn prepare_native_modules(
    modules: &BTreeMap<String, NativeModuleRuntime>,
    dependencies: &BTreeMap<String, ModuleDependencies>,
    activation_order: &[String],
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
        };
        if let Err(error) = module.lifecycle.prepare(context).await {
            let _ = deactivate_in_reverse(
                modules,
                dependencies,
                &prepared_instances,
                DeactivationReason::StartupRollback,
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
        };
        module.lifecycle.activate(context).await?;
    }
    Ok(())
}

async fn open_native_readiness<D: RuntimeDriver>(driver: &D, ready_gate: &AppReadyGate) {
    ready_gate.open();
    driver.yield_now().await;
}

fn module_dependencies(
    plan: &ResolvedAppPlan,
    endpoints: &BTreeMap<(String, &'static str), Vec<Rc<dyn NativeRequestEndpoint>>>,
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
                    .map(|endpoint| ModuleDependencyHandle { endpoint }),
            ));
    }
    dependencies
}

async fn deactivate_in_reverse(
    modules: &BTreeMap<String, NativeModuleRuntime>,
    dependencies: &BTreeMap<String, ModuleDependencies>,
    activation_order: &[String],
    reason: DeactivationReason,
) -> Option<RuntimeFailure> {
    let mut first_error = None;
    for instance_key in activation_order.iter().rev() {
        let module = modules
            .get(instance_key)
            .expect("deactivation order only contains planned Module Instances");
        if let Err(error) = module
            .lifecycle
            .deactivate(DeactivateContext {
                instance_key: instance_key.clone(),
                dependencies: dependencies.get(instance_key).cloned().unwrap_or_default(),
                reason,
                tasks: module.tasks.clone(),
            })
            .await
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        module.tasks.cancel_all().await;
    }
    first_error
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
