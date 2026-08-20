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
}

impl PreparedNativeApp {
    /// Completes Adapter preparation with one endpoint per consumer binding.
    pub fn new(bindings: BTreeMap<(String, &'static str), Rc<dyn NativeRequestEndpoint>>) -> Self {
        Self {
            bindings: bindings
                .into_iter()
                .map(|(key, endpoint)| (key, vec![endpoint]))
                .collect(),
        }
    }

    /// Completes Adapter preparation with deterministic one, optional, and many bindings.
    pub fn from_many(
        bindings: BTreeMap<(String, &'static str), Vec<Rc<dyn NativeRequestEndpoint>>>,
    ) -> Self {
        Self { bindings }
    }
}

/// Host-specific seam that instantiates native Module generations and prepares endpoints.
pub trait NativeExecutionAdapter: std::fmt::Debug {
    /// Instantiates the exact Plan and confirms its endpoint and binding tables.
    fn prepare(&self, plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure>;
}

/// A started native App whose generated clients can invoke resolved bindings.
#[derive(Debug)]
pub struct NativeApp {
    bindings: BTreeMap<(String, &'static str), Vec<Rc<dyn NativeRequestEndpoint>>>,
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
        let prepared = adapter.prepare(&plan)?;
        driver.yield_now().await;
        Ok(NativeApp {
            bindings: prepared.bindings,
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
