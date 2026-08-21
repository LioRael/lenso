//! Portable Lenso vNext Kernel and Runtime Driver seam.

use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet, VecDeque},
    future::Future,
    marker::PhantomData,
    panic::AssertUnwindSafe,
    pin::Pin,
    rc::{Rc, Weak},
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
    EventAdmissionPlan, ExecutionClassId, ModuleCriticality, PlanResolutionError,
    RequestAdmissionPlan, ResolvedAppPlan, RestartMode, RestartPolicy,
};

mod event;
mod stream;

pub use event::{
    EventAdmission, EventCapability, EventPublishResult, EventPublishStatus,
    ModuleEventDependencyHandle, NativeEventEndpoint, NativeEventHandle,
};
pub use stream::{
    NativeStream, NativeStreamEndpoint, NativeStreamHandle, NativeStreamItem, NativeStreamSession,
    StreamCapability, StreamEvent, StreamSession,
};

type ErasedValue = Box<dyn Any>;
type ErasedDomainResult = Result<ErasedValue, ErasedValue>;
type NativeBindingTable = BTreeMap<(String, &'static str), Vec<NativeEndpointBinding>>;
type NativeEndpointStateTable = BTreeMap<(String, String), Rc<NativeEndpointState>>;
type NativeStreamBindingTable = BTreeMap<(String, &'static str), Vec<NativeStreamEndpointBinding>>;
type NativeStreamEndpointStateTable = BTreeMap<(String, String), Rc<NativeStreamEndpointState>>;
type NativeEventBindingTable =
    BTreeMap<(String, &'static str), Vec<event::NativeEventEndpointBinding>>;
type NativeEventEndpointStateTable =
    BTreeMap<(String, String), Rc<event::NativeEventEndpointState>>;

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
    stream_handle: Option<ModuleStreamDependencyHandle>,
    event_handle: Option<ModuleEventDependencyHandle>,
}

impl ModuleDependency {
    fn new(
        capability_id: impl Into<String>,
        provider_instance: impl Into<String>,
        provider_order: usize,
        handle: Option<ModuleDependencyHandle>,
        stream_handle: Option<ModuleStreamDependencyHandle>,
        event_handle: Option<ModuleEventDependencyHandle>,
    ) -> Self {
        Self {
            capability_id: capability_id.into(),
            provider_instance: provider_instance.into(),
            provider_order,
            handle,
            stream_handle,
            event_handle,
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

    /// Returns the resolved native stream endpoint handle when the Adapter supplied one.
    pub fn stream_handle(&self) -> Option<ModuleStreamDependencyHandle> {
        self.stream_handle.clone()
    }

    /// Returns the resolved native Event endpoint handle when the Adapter supplied one.
    pub fn event_handle(&self) -> Option<ModuleEventDependencyHandle> {
        self.event_handle.clone()
    }
}

/// An opaque, Adapter-resolved Capability endpoint passed to lifecycle code.
#[derive(Clone, Debug)]
pub struct ModuleDependencyHandle {
    binding: NativeEndpointBinding,
    caller_instance: String,
    runtime: Rc<RefCell<Weak<NativeAppRuntime>>>,
}

/// An opaque, Adapter-resolved stream Capability endpoint passed to lifecycle code.
#[derive(Clone, Debug)]
pub struct ModuleStreamDependencyHandle {
    binding: NativeStreamEndpointBinding,
    caller_instance: String,
    runtime: Rc<RefCell<Weak<NativeAppRuntime>>>,
}

impl ModuleStreamDependencyHandle {
    /// Returns the Capability implemented by this stream handle.
    pub fn capability_id(&self) -> &'static str {
        self.binding.state.capability_id
    }

    /// Returns the exact Descriptor version implemented by this stream handle.
    pub fn descriptor_version(&self) -> &'static str {
        self.binding.state.descriptor_version
    }

    /// Returns the exact stream Operation table implemented by this handle.
    pub fn operations(&self) -> &'static [&'static str] {
        self.binding.state.operations
    }

    /// Converts this resolved dependency into its generated typed stream handle.
    pub fn typed<C: StreamCapability>(&self) -> Result<NativeStreamHandle<C>, RuntimeFailure> {
        if self.capability_id() != C::ID || self.descriptor_version() != C::DESCRIPTOR_VERSION {
            return Err(RuntimeFailure::ProtocolViolation { capability: C::ID });
        }
        let runtime = self
            .runtime
            .borrow()
            .upgrade()
            .ok_or(RuntimeFailure::AdmissionClosed)?;
        Ok(NativeStreamHandle::from_endpoints(
            std::slice::from_ref(&self.binding),
            runtime,
            &self.caller_instance,
            true,
        ))
    }
}

impl ModuleDependencyHandle {
    /// Returns the Capability implemented by this handle.
    pub fn capability_id(&self) -> &'static str {
        self.binding.state.capability_id
    }

    /// Returns the exact Descriptor version implemented by this handle.
    pub fn descriptor_version(&self) -> &'static str {
        self.binding.state.descriptor_version
    }

    /// Returns the exact Operation table implemented by this handle.
    pub fn operations(&self) -> &'static [&'static str] {
        self.binding.state.operations
    }

    /// Converts this resolved dependency into its generated typed request handle.
    pub fn typed<C: RequestCapability>(&self) -> Result<NativeRequestHandle<C>, RuntimeFailure> {
        if self.capability_id() != C::ID || self.descriptor_version() != C::DESCRIPTOR_VERSION {
            return Err(RuntimeFailure::ProtocolViolation { capability: C::ID });
        }
        let runtime = self
            .runtime
            .borrow()
            .upgrade()
            .ok_or(RuntimeFailure::AdmissionClosed)?;
        Ok(NativeRequestHandle::from_endpoints(
            std::slice::from_ref(&self.binding),
            runtime,
            &self.caller_instance,
            true,
        ))
    }
}

/// The explicit Capability dependencies available during Module lifecycle.
#[derive(Clone, Debug, Default)]
pub struct ModuleDependencies {
    bindings: Vec<ModuleDependency>,
    caller_instance: String,
    runtime: Rc<RefCell<Weak<NativeAppRuntime>>>,
}

impl ModuleDependencies {
    fn new(
        caller_instance: impl Into<String>,
        runtime: Rc<RefCell<Weak<NativeAppRuntime>>>,
    ) -> Self {
        Self {
            bindings: Vec::new(),
            caller_instance: caller_instance.into(),
            runtime,
        }
    }

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

    /// Returns the one explicitly bound typed dependency.
    pub fn one<C: RequestCapability>(&self) -> Result<NativeRequestHandle<C>, RuntimeFailure> {
        let handles: Vec<_> = self
            .bindings
            .iter()
            .filter(|binding| binding.capability_id() == C::ID)
            .filter_map(ModuleDependency::handle)
            .collect();
        match handles.as_slice() {
            [handle] => handle.typed::<C>(),
            [] => Err(RuntimeFailure::Unavailable { capability: C::ID }),
            handles => Err(RuntimeFailure::AmbiguousBinding {
                capability: C::ID,
                providers: handles.len(),
            }),
        }
    }

    /// Returns an optional explicitly bound typed dependency.
    pub fn optional<C: RequestCapability>(
        &self,
    ) -> Result<Option<NativeRequestHandle<C>>, RuntimeFailure> {
        match self
            .bindings
            .iter()
            .filter(|binding| binding.capability_id() == C::ID)
            .filter_map(ModuleDependency::handle)
            .collect::<Vec<_>>()
            .as_slice()
        {
            [] => Ok(None),
            [handle] => handle.typed::<C>().map(Some),
            handles => Err(RuntimeFailure::AmbiguousBinding {
                capability: C::ID,
                providers: handles.len(),
            }),
        }
    }

    /// Returns all explicitly bound typed dependencies in resolved provider order.
    pub fn many<C: RequestCapability>(
        &self,
    ) -> Result<Vec<NativeRequestHandle<C>>, RuntimeFailure> {
        self.bindings
            .iter()
            .filter(|binding| binding.capability_id() == C::ID)
            .filter_map(ModuleDependency::handle)
            .map(|handle| handle.typed::<C>())
            .collect()
    }

    /// Returns the one explicitly bound typed stream dependency.
    pub fn one_stream<C: StreamCapability>(&self) -> Result<NativeStreamHandle<C>, RuntimeFailure> {
        let handles: Vec<_> = self
            .bindings
            .iter()
            .filter(|binding| binding.capability_id() == C::ID)
            .filter_map(ModuleDependency::stream_handle)
            .collect();
        match handles.as_slice() {
            [handle] => handle.typed::<C>(),
            [] => Err(RuntimeFailure::Unavailable { capability: C::ID }),
            handles => Err(RuntimeFailure::AmbiguousBinding {
                capability: C::ID,
                providers: handles.len(),
            }),
        }
    }

    /// Returns an optional explicitly bound typed stream dependency.
    pub fn optional_stream<C: StreamCapability>(
        &self,
    ) -> Result<Option<NativeStreamHandle<C>>, RuntimeFailure> {
        match self
            .bindings
            .iter()
            .filter(|binding| binding.capability_id() == C::ID)
            .filter_map(ModuleDependency::stream_handle)
            .collect::<Vec<_>>()
            .as_slice()
        {
            [] => Ok(None),
            [handle] => handle.typed::<C>().map(Some),
            handles => Err(RuntimeFailure::AmbiguousBinding {
                capability: C::ID,
                providers: handles.len(),
            }),
        }
    }

    /// Returns all explicitly bound typed stream dependencies in Plan order.
    pub fn many_stream<C: StreamCapability>(
        &self,
    ) -> Result<Vec<NativeStreamHandle<C>>, RuntimeFailure> {
        self.bindings
            .iter()
            .filter(|binding| binding.capability_id() == C::ID)
            .filter_map(ModuleDependency::stream_handle)
            .map(|handle| handle.typed::<C>())
            .collect()
    }

    /// Returns one typed Event handle over every explicit binding in Plan order.
    pub fn many_event<C: EventCapability>(&self) -> Result<NativeEventHandle<C>, RuntimeFailure> {
        let handles: Vec<_> = self
            .bindings
            .iter()
            .filter(|binding| binding.capability_id() == C::ID)
            .filter_map(ModuleDependency::event_handle)
            .collect();
        if handles.iter().any(|handle| {
            handle.capability_id() != C::ID || handle.descriptor_version() != C::DESCRIPTOR_VERSION
        }) {
            return Err(RuntimeFailure::ProtocolViolation { capability: C::ID });
        }
        let runtime = self
            .runtime
            .borrow()
            .upgrade()
            .ok_or(RuntimeFailure::AdmissionClosed)?;
        let endpoints = handles
            .iter()
            .map(|handle| handle.binding.clone())
            .collect::<Vec<_>>();
        Ok(NativeEventHandle::from_endpoints(
            &endpoints,
            runtime,
            &self.caller_instance,
            true,
        ))
    }

    /// Returns the one explicitly bound typed Event dependency.
    pub fn one_event<C: EventCapability>(&self) -> Result<NativeEventHandle<C>, RuntimeFailure> {
        match self
            .bindings
            .iter()
            .filter(|binding| binding.capability_id() == C::ID)
            .filter_map(ModuleDependency::event_handle)
            .collect::<Vec<_>>()
            .as_slice()
        {
            [handle] => handle.typed::<C>(),
            [] => Err(RuntimeFailure::Unavailable { capability: C::ID }),
            handles => Err(RuntimeFailure::AmbiguousBinding {
                capability: C::ID,
                providers: handles.len(),
            }),
        }
    }

    /// Returns an optional explicitly bound typed Event dependency.
    pub fn optional_event<C: EventCapability>(
        &self,
    ) -> Result<Option<NativeEventHandle<C>>, RuntimeFailure> {
        match self
            .bindings
            .iter()
            .filter(|binding| binding.capability_id() == C::ID)
            .filter_map(ModuleDependency::event_handle)
            .collect::<Vec<_>>()
            .as_slice()
        {
            [] => Ok(None),
            [handle] => handle.typed::<C>().map(Some),
            handles => Err(RuntimeFailure::AmbiguousBinding {
                capability: C::ID,
                providers: handles.len(),
            }),
        }
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

struct ManagedResourceEntry {
    resource: Rc<dyn ManagedResource>,
    release: RefCell<ManagedResourceRelease>,
}

enum ManagedResourceRelease {
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
    entry: Rc<ManagedResourceEntry>,
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

struct ManagedResourceReleaseOperation {
    entry: Rc<ManagedResourceEntry>,
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

    fn close(&self) {
        self.state.closed.set(true);
    }

    async fn release_all(&self) -> Option<RuntimeFailure> {
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

    async fn release_all_until(
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
    task: Rc<RefCell<Option<DriverTask>>>,
    abort: AbortHandle,
    failed: Rc<Cell<bool>>,
}

impl ManagedTask {
    fn from_driver_task(task: DriverTask) -> Self {
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

    async fn join(&self) -> TaskOutcome {
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
    spawn: Rc<dyn Fn(LocalTask) -> Result<DriverTask, SpawnError>>,
    state: Rc<ManagedTaskScopeState>,
}

struct ManagedTaskScopeState {
    tasks: RefCell<Vec<ManagedTask>>,
    closed: Cell<bool>,
    cancellation: CancellationToken,
    failure_handler: RefCell<Option<Rc<dyn Fn()>>>,
    unreported_failure: Cell<bool>,
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

    fn close(&self) {
        self.state.closed.set(true);
        self.state.cancellation.cancel();
    }

    fn set_failure_handler(&self, handler: &Rc<dyn Fn()>) {
        self.state.failure_handler.replace(Some(handler.clone()));
        if self.state.unreported_failure.replace(false) {
            handler();
        }
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
            let _ = task.join().await;
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

impl ManagedTaskScopeState {
    fn report_failure(&self) {
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
    instance_key: String,
    entrypoint: String,
    configuration: String,
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
    stream_endpoints: Vec<Rc<dyn NativeStreamEndpoint>>,
    event_endpoints: Vec<Rc<dyn NativeEventEndpoint>>,
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
            stream_endpoints: Vec::new(),
            event_endpoints: Vec::new(),
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
            stream_endpoints: Vec::new(),
            event_endpoints: Vec::new(),
            lifecycle,
        }
    }

    /// Creates one generation with request and bidirectional stream endpoints.
    pub fn with_endpoints(
        endpoints: Vec<Rc<dyn NativeRequestEndpoint>>,
        stream_endpoints: Vec<Rc<dyn NativeStreamEndpoint>>,
        lifecycle: impl ModuleLifecycle,
    ) -> Self {
        Self {
            endpoints,
            stream_endpoints,
            event_endpoints: Vec::new(),
            lifecycle: Rc::new(lifecycle),
        }
    }

    /// Creates one generation with shared lifecycle and request/stream endpoints.
    pub fn with_endpoints_lifecycle(
        endpoints: Vec<Rc<dyn NativeRequestEndpoint>>,
        stream_endpoints: Vec<Rc<dyn NativeStreamEndpoint>>,
        lifecycle: Rc<dyn ModuleLifecycle>,
    ) -> Self {
        Self {
            endpoints,
            stream_endpoints,
            event_endpoints: Vec::new(),
            lifecycle,
        }
    }

    /// Creates one generation containing only bidirectional stream endpoints.
    pub fn with_stream_endpoints(
        stream_endpoints: Vec<Rc<dyn NativeStreamEndpoint>>,
        lifecycle: impl ModuleLifecycle,
    ) -> Self {
        Self::with_endpoints(Vec::new(), stream_endpoints, lifecycle)
    }

    /// Creates one generation containing only ephemeral Event endpoints.
    pub fn with_event_endpoints(
        event_endpoints: Vec<Rc<dyn NativeEventEndpoint>>,
        lifecycle: impl ModuleLifecycle,
    ) -> Self {
        Self::with_all_endpoints(Vec::new(), Vec::new(), event_endpoints, lifecycle)
    }

    /// Creates one generation with request, stream, and ephemeral Event endpoints.
    pub fn with_all_endpoints(
        endpoints: Vec<Rc<dyn NativeRequestEndpoint>>,
        stream_endpoints: Vec<Rc<dyn NativeStreamEndpoint>>,
        event_endpoints: Vec<Rc<dyn NativeEventEndpoint>>,
        lifecycle: impl ModuleLifecycle,
    ) -> Self {
        Self {
            endpoints,
            stream_endpoints,
            event_endpoints,
            lifecycle: Rc::new(lifecycle),
        }
    }

    /// Returns the exact endpoints prepared for this generation.
    pub fn endpoints(&self) -> &[Rc<dyn NativeRequestEndpoint>] {
        &self.endpoints
    }

    /// Returns the exact stream endpoints prepared for this generation.
    pub fn stream_endpoints(&self) -> &[Rc<dyn NativeStreamEndpoint>] {
        &self.stream_endpoints
    }

    /// Returns the exact Event endpoints prepared for this generation.
    pub fn event_endpoints(&self) -> &[Rc<dyn NativeEventEndpoint>] {
        &self.event_endpoints
    }

    /// Returns the lifecycle Interface prepared for this generation.
    pub fn lifecycle(&self) -> Rc<dyn ModuleLifecycle> {
        self.lifecycle.clone()
    }

    fn into_parts(
        self,
    ) -> (
        Vec<Rc<dyn NativeRequestEndpoint>>,
        Vec<Rc<dyn NativeStreamEndpoint>>,
        Vec<Rc<dyn NativeEventEndpoint>>,
        Rc<dyn ModuleLifecycle>,
    ) {
        (
            self.endpoints,
            self.stream_endpoints,
            self.event_endpoints,
            self.lifecycle,
        )
    }
}

/// One provider-specific binding prepared by an Execution Adapter.
#[derive(Clone, Debug)]
pub struct PreparedBinding {
    consumer_instance: String,
    provider_instance: String,
    endpoint: Rc<dyn NativeRequestEndpoint>,
}

/// One provider-specific bidirectional stream binding prepared by an Adapter.
#[derive(Clone, Debug)]
pub struct PreparedStreamBinding {
    consumer_instance: String,
    provider_instance: String,
    endpoint: Rc<dyn NativeStreamEndpoint>,
}

/// One provider-specific ephemeral Event binding prepared by an Adapter.
#[derive(Clone, Debug)]
pub struct PreparedEventBinding {
    consumer_instance: String,
    provider_instance: String,
    endpoint: Rc<dyn NativeEventEndpoint>,
}

impl PreparedEventBinding {
    /// Binds one consumer to one exact Event endpoint and provider Instance.
    pub fn new(
        consumer_instance: impl Into<String>,
        provider_instance: impl Into<String>,
        endpoint: Rc<dyn NativeEventEndpoint>,
    ) -> Self {
        Self {
            consumer_instance: consumer_instance.into(),
            provider_instance: provider_instance.into(),
            endpoint,
        }
    }

    /// Returns the App-local consumer Instance selected by the Plan.
    pub fn consumer_instance(&self) -> &str {
        &self.consumer_instance
    }

    /// Returns the App-local provider Instance selected by the Plan.
    pub fn provider_instance(&self) -> &str {
        &self.provider_instance
    }

    /// Returns the exact prepared Event endpoint referenced by this binding.
    pub fn endpoint(&self) -> Rc<dyn NativeEventEndpoint> {
        self.endpoint.clone()
    }

    fn same_identity(&self, other: &Self) -> bool {
        self.consumer_instance == other.consumer_instance
            && self.provider_instance == other.provider_instance
            && self.endpoint.capability_id() == other.endpoint.capability_id()
    }
}

impl PreparedStreamBinding {
    /// Binds one consumer to one exact stream endpoint and provider Instance.
    pub fn new(
        consumer_instance: impl Into<String>,
        provider_instance: impl Into<String>,
        endpoint: Rc<dyn NativeStreamEndpoint>,
    ) -> Self {
        Self {
            consumer_instance: consumer_instance.into(),
            provider_instance: provider_instance.into(),
            endpoint,
        }
    }

    /// Returns the App-local consumer Instance selected by the Plan.
    pub fn consumer_instance(&self) -> &str {
        &self.consumer_instance
    }

    /// Returns the App-local provider Instance selected by the Plan.
    pub fn provider_instance(&self) -> &str {
        &self.provider_instance
    }

    /// Returns the exact prepared stream endpoint referenced by this binding.
    pub fn endpoint(&self) -> Rc<dyn NativeStreamEndpoint> {
        self.endpoint.clone()
    }

    fn same_identity(&self, other: &Self) -> bool {
        self.consumer_instance == other.consumer_instance
            && self.provider_instance == other.provider_instance
            && self.endpoint.capability_id() == other.endpoint.capability_id()
    }
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

    /// Returns the App-local consumer Instance selected by the Plan.
    pub fn consumer_instance(&self) -> &str {
        &self.consumer_instance
    }

    /// Returns the App-local provider Instance selected by the Plan.
    pub fn provider_instance(&self) -> &str {
        &self.provider_instance
    }

    /// Returns the exact prepared endpoint referenced by this binding.
    pub fn endpoint(&self) -> Rc<dyn NativeRequestEndpoint> {
        self.endpoint.clone()
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
    stream_bindings: Vec<PreparedStreamBinding>,
    event_bindings: Vec<PreparedEventBinding>,
    generations: BTreeMap<String, PreparedNativeModule>,
}

impl PreparedNativeApp {
    /// Completes Adapter preparation with the full generation and binding tables.
    pub fn new(
        bindings: Vec<PreparedBinding>,
        generations: BTreeMap<String, PreparedNativeModule>,
    ) -> Self {
        Self {
            bindings,
            stream_bindings: Vec::new(),
            event_bindings: Vec::new(),
            generations,
        }
    }

    /// Creates the complete Adapter result for an empty Plan.
    pub fn empty() -> Self {
        Self::new(Vec::new(), BTreeMap::new())
    }

    /// Adds the exact bidirectional stream bindings prepared by an Adapter.
    #[must_use]
    pub fn with_stream_bindings(mut self, stream_bindings: Vec<PreparedStreamBinding>) -> Self {
        self.stream_bindings = stream_bindings;
        self
    }

    /// Adds the exact ephemeral Event bindings prepared by an Adapter.
    #[must_use]
    pub fn with_event_bindings(mut self, event_bindings: Vec<PreparedEventBinding>) -> Self {
        self.event_bindings = event_bindings;
        self
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
        for binding in other.stream_bindings {
            if self
                .stream_bindings
                .iter()
                .any(|existing| existing.same_identity(&binding))
            {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "multiple Execution Adapters prepared stream binding `{}:{}:{}`",
                        binding.consumer_instance,
                        binding.endpoint.capability_id(),
                        binding.provider_instance
                    ),
                });
            }
            self.stream_bindings.push(binding);
        }
        for binding in other.event_bindings {
            if self
                .event_bindings
                .iter()
                .any(|existing| existing.same_identity(&binding))
            {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "multiple Execution Adapters prepared Event binding `{}:{}:{}`",
                        binding.consumer_instance,
                        binding.endpoint.capability_id(),
                        binding.provider_instance
                    ),
                });
            }
            self.event_bindings.push(binding);
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

        let mut prepared = PreparedNativeApp::empty();
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

#[derive(Clone, Debug)]
pub(crate) struct NativeStreamEndpointSnapshot {
    pub(crate) endpoint: Rc<dyn NativeStreamEndpoint>,
    pub(crate) generation: u64,
    pub(crate) cancellation: CancellationToken,
}

#[derive(Debug)]
pub(crate) struct NativeStreamEndpointState {
    capability_id: &'static str,
    descriptor_version: &'static str,
    operations: &'static [&'static str],
    endpoint: RefCell<Option<Rc<dyn NativeStreamEndpoint>>>,
    generation: Cell<u64>,
    cancellation: RefCell<CancellationToken>,
}

impl NativeStreamEndpointState {
    pub(crate) fn new(endpoint: Rc<dyn NativeStreamEndpoint>, generation: u64) -> Self {
        Self {
            capability_id: endpoint.capability_id(),
            descriptor_version: endpoint.descriptor_version(),
            operations: endpoint.operations(),
            endpoint: RefCell::new(Some(endpoint)),
            generation: Cell::new(generation),
            cancellation: RefCell::new(CancellationToken::new()),
        }
    }

    pub(crate) fn snapshot(&self) -> Option<NativeStreamEndpointSnapshot> {
        self.endpoint
            .borrow()
            .clone()
            .map(|endpoint| NativeStreamEndpointSnapshot {
                endpoint,
                generation: self.generation.get(),
                cancellation: self.cancellation.borrow().clone(),
            })
    }

    pub(crate) fn mark_unavailable(&self) {
        self.cancellation.borrow().cancel();
        self.endpoint.borrow_mut().take();
    }

    pub(crate) fn install(&self, endpoint: Rc<dyn NativeStreamEndpoint>, generation: u64) {
        self.generation.set(generation);
        self.cancellation.replace(CancellationToken::new());
        self.endpoint.replace(Some(endpoint));
    }

    pub(crate) fn is_current(&self, generation: u64) -> bool {
        self.generation.get() == generation && self.endpoint.borrow().is_some()
    }
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

#[derive(Clone, Debug)]
pub(crate) struct NativeStreamEndpointBinding {
    pub(crate) module_instance: String,
    pub(crate) state: Rc<NativeStreamEndpointState>,
    admissions: BTreeMap<String, RequestAdmission>,
}

impl NativeStreamEndpointBinding {
    pub(crate) fn admission(&self, operation: &str) -> Option<&RequestAdmission> {
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

#[derive(Debug, Default)]
struct ShutdownCoordinator {
    started: Cell<bool>,
    outcome: RefCell<Option<ShutdownOutcome>>,
    waiters: RefCell<Vec<oneshot::Sender<ShutdownOutcome>>>,
}

impl ShutdownCoordinator {
    fn start(&self) -> bool {
        !self.started.replace(true)
    }

    fn complete(&self, outcome: &ShutdownOutcome) {
        if self.outcome.borrow().is_some() {
            return;
        }
        self.outcome.replace(Some(outcome.clone()));
        for waiter in self.waiters.borrow_mut().drain(..) {
            let _ = waiter.send(outcome.clone());
        }
    }

    fn wait(&self) -> LocalBoxFuture<'static, ShutdownOutcome> {
        if let Some(outcome) = self.outcome.borrow().clone() {
            return Box::pin(futures::future::ready(outcome));
        }
        let (complete, waiter) = oneshot::channel();
        self.waiters.borrow_mut().push(complete);
        Box::pin(async move {
            waiter.await.unwrap_or(ShutdownOutcome::RuntimeFailure {
                error: RuntimeFailure::Internal {
                    detail: "shutdown coordinator terminated before publishing an outcome"
                        .to_owned(),
                },
            })
        })
    }
}

struct NativeAppRuntime {
    plan: ResolvedAppPlan,
    adapters: Rc<ExecutionAdapterCatalog>,
    modules: BTreeMap<String, NativeModuleRuntime>,
    dependencies: BTreeMap<String, ModuleDependencies>,
    endpoint_states: BTreeMap<(String, String), Rc<NativeEndpointState>>,
    stream_endpoint_states: NativeStreamEndpointStateTable,
    event_endpoint_states: NativeEventEndpointStateTable,
    supervision: RefCell<BTreeMap<String, ModuleSupervision>>,
    supervision_tasks: RefCell<BTreeMap<String, ManagedTask>>,
    activation_order: Vec<String>,
    ready_gate: AppReadyGate,
    admission: AppAdmission,
    driver: DriverControl,
    request_ids: Rc<Cell<RequestId>>,
    supervision_cancellation: CancellationToken,
    shutdown_started: Cell<bool>,
    shutdown: ShutdownCoordinator,
    shutdown_task: RefCell<Option<DriverTask>>,
    terminal_failure: RefCell<Option<RuntimeFailure>>,
}

impl std::fmt::Debug for NativeAppRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeAppRuntime")
            .field("module_count", &self.modules.len())
            .field("endpoint_count", &self.endpoint_states.len())
            .field("stream_endpoint_count", &self.stream_endpoint_states.len())
            .field("event_endpoint_count", &self.event_endpoint_states.len())
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
        for endpoint in self.stream_endpoint_states.values() {
            endpoint.mark_unavailable();
        }
        for endpoint in self.event_endpoint_states.values() {
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
    stream_bindings: NativeStreamBindingTable,
    event_bindings: NativeEventBindingTable,
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
            false,
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
                    false,
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
            false,
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
                let request_current =
                    self.runtime
                        .endpoint_states
                        .iter()
                        .any(|((module, _), endpoint)| {
                            module == instance_key && endpoint.is_current(state.generation)
                        });
                let stream_current =
                    self.runtime
                        .stream_endpoint_states
                        .iter()
                        .any(|((module, _), endpoint)| {
                            module == instance_key && endpoint.is_current(state.generation)
                        });
                let event_current =
                    self.runtime
                        .event_endpoint_states
                        .iter()
                        .any(|((module, _), endpoint)| {
                            module == instance_key && endpoint.is_current(state.generation)
                        });
                (request_current || stream_current || event_current).then_some(state.generation)
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
        self.runtime.begin_shutdown();
        if self.runtime.shutdown.start() {
            let runtime = self.runtime.clone();
            let worker_runtime = runtime.clone();
            match (runtime.driver.spawn_local)(Box::pin(async move {
                let outcome = shutdown_native_modules(&worker_runtime, timeout).await;
                worker_runtime.shutdown.complete(&outcome);
            })) {
                Ok(task) => {
                    runtime.shutdown_task.replace(Some(task));
                }
                Err(error) => runtime.shutdown.complete(&ShutdownOutcome::RuntimeFailure {
                    error: RuntimeFailure::Internal {
                        detail: format!("failed to schedule App shutdown: {error:?}"),
                    },
                }),
            }
        }
        self.runtime.shutdown.wait().await
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

    /// Materializes one typed bidirectional stream handle from the resolved Plan.
    pub fn stream_handle<C: StreamCapability>(
        &self,
        caller_instance: &str,
    ) -> Result<NativeStreamHandle<C>, RuntimeFailure> {
        if self.runtime.admission.is_closed() {
            return Err(RuntimeFailure::AdmissionClosed);
        }
        let endpoints = self
            .stream_endpoints::<C>(caller_instance)
            .filter(|endpoints| !endpoints.is_empty())
            .ok_or(RuntimeFailure::Unavailable { capability: C::ID })?;
        Ok(NativeStreamHandle::from_endpoints(
            endpoints,
            self.runtime.clone(),
            caller_instance,
            false,
        ))
    }

    /// Materializes an optional typed bidirectional stream handle.
    pub fn optional_stream_handle<C: StreamCapability>(
        &self,
        caller_instance: &str,
    ) -> Option<NativeStreamHandle<C>> {
        let caller_instance = caller_instance.to_owned();
        self.stream_endpoints::<C>(&caller_instance)
            .filter(|endpoints| !endpoints.is_empty())
            .map(|endpoints| {
                NativeStreamHandle::from_endpoints(
                    endpoints,
                    self.runtime.clone(),
                    &caller_instance,
                    false,
                )
            })
    }

    /// Returns the number of immutable stream endpoints bound to one requirement.
    pub fn stream_binding_count<C: StreamCapability>(&self, caller_instance: &str) -> usize {
        self.stream_endpoints::<C>(caller_instance)
            .map_or(0, <[_]>::len)
    }

    /// Materializes a typed Event handle and requires at least one subscriber.
    pub fn event_handle<C: EventCapability>(
        &self,
        caller_instance: &str,
    ) -> Result<NativeEventHandle<C>, RuntimeFailure> {
        if self.runtime.admission.is_closed() {
            return Err(RuntimeFailure::AdmissionClosed);
        }
        let endpoints = self
            .event_endpoints::<C>(caller_instance)
            .filter(|endpoints| !endpoints.is_empty())
            .ok_or(RuntimeFailure::Unavailable { capability: C::ID })?;
        Ok(NativeEventHandle::from_endpoints(
            endpoints,
            self.runtime.clone(),
            caller_instance,
            false,
        ))
    }

    /// Materializes an optional typed Event handle.
    pub fn optional_event_handle<C: EventCapability>(
        &self,
        caller_instance: &str,
    ) -> Option<NativeEventHandle<C>> {
        let caller_instance = caller_instance.to_owned();
        self.event_endpoints::<C>(&caller_instance)
            .filter(|endpoints| !endpoints.is_empty())
            .map(|endpoints| {
                NativeEventHandle::from_endpoints(
                    endpoints,
                    self.runtime.clone(),
                    &caller_instance,
                    false,
                )
            })
    }

    /// Materializes a typed Event handle whose endpoint set may be empty.
    pub fn many_event_handle<C: EventCapability>(
        &self,
        caller_instance: &str,
    ) -> Result<NativeEventHandle<C>, RuntimeFailure> {
        if self.runtime.admission.is_closed() {
            return Err(RuntimeFailure::AdmissionClosed);
        }
        let endpoints = self.event_endpoints::<C>(caller_instance).unwrap_or(&[]);
        Ok(NativeEventHandle::from_endpoints(
            endpoints,
            self.runtime.clone(),
            caller_instance,
            false,
        ))
    }

    /// Returns the number of immutable Event subscriber endpoints bound to a requirement.
    pub fn event_binding_count<C: EventCapability>(&self, caller_instance: &str) -> usize {
        self.event_endpoints::<C>(caller_instance)
            .map_or(0, <[_]>::len)
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

    fn stream_endpoints<C: StreamCapability>(
        &self,
        caller_instance: &str,
    ) -> Option<&[NativeStreamEndpointBinding]> {
        self.stream_bindings
            .get(&(caller_instance.to_owned(), C::ID))
            .map(Vec::as_slice)
    }

    fn event_endpoints<C: EventCapability>(
        &self,
        caller_instance: &str,
    ) -> Option<&[event::NativeEventEndpointBinding]> {
        self.event_bindings
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
    allow_before_ready: bool,
    capability: PhantomData<fn() -> C>,
}

impl<C: RequestCapability> NativeRequestHandle<C> {
    fn from_endpoints(
        endpoints: &[NativeEndpointBinding],
        runtime: Rc<NativeAppRuntime>,
        caller_instance: &str,
        allow_before_ready: bool,
    ) -> Self {
        Self {
            endpoints: endpoints.to_vec(),
            runtime,
            caller_instance: caller_instance.to_owned(),
            allow_before_ready,
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
        if self.runtime.shutdown_started.get()
            || (!self.allow_before_ready && self.runtime.admission.is_closed())
        {
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
        if self.runtime.shutdown_started.get()
            || (!self.allow_before_ready && self.runtime.admission.is_closed())
        {
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
    /// The task or its Runtime Driver wrapper terminated abnormally.
    Failed,
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

    fn acquire(
        &self,
        capability: &'static str,
        operation: &str,
        context: InvocationContext,
        driver: DriverControl,
    ) -> LocalBoxFuture<'static, Result<RequestPermit, RuntimeFailure>> {
        if let Ok(permit) = self.try_acquire(capability, operation, &context, &driver) {
            return Box::pin(futures::future::ready(Ok(permit)));
        }
        if let Err(error) = ensure_context_active(&driver, &context) {
            return Box::pin(futures::future::ready(Err(error)));
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
    if generation_cancellation.is_cancelled() {
        return Err(RuntimeFailure::Unavailable { capability });
    }

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
            stream_bindings: prepared_stream_bindings,
            event_bindings: prepared_event_bindings,
            generations,
        } = adapters.prepare(&plan)?;
        validate_prepared_native_app(
            &plan,
            &prepared_bindings,
            &prepared_stream_bindings,
            &prepared_event_bindings,
            &generations,
        )?;
        let (bindings, endpoint_states) = native_bindings(&plan, &prepared_bindings);
        let (stream_bindings, stream_endpoint_states) =
            native_stream_bindings(&plan, &prepared_stream_bindings);
        let (event_bindings, event_endpoint_states) =
            native_event_bindings(&plan, &prepared_event_bindings);
        let runtime_link = Rc::new(RefCell::new(Weak::new()));
        let dependencies = module_dependencies(
            &plan,
            &bindings,
            &stream_bindings,
            &event_bindings,
            &runtime_link,
        );
        let driver_control = DriverControl::new(&driver);
        let admission = AppAdmission::new();
        let module_runtimes = native_module_runtimes(&plan, &driver, generations);
        let ready_gate = AppReadyGate::new();
        let supervision = module_supervision(&plan);
        let runtime = Rc::new(NativeAppRuntime {
            plan,
            adapters,
            modules: module_runtimes,
            dependencies,
            endpoint_states,
            stream_endpoint_states,
            event_endpoint_states,
            supervision: RefCell::new(supervision),
            supervision_tasks: RefCell::new(BTreeMap::new()),
            activation_order,
            ready_gate,
            admission,
            driver: driver_control,
            request_ids: Rc::new(Cell::new(1)),
            supervision_cancellation: CancellationToken::new(),
            shutdown_started: Cell::new(false),
            shutdown: ShutdownCoordinator::default(),
            shutdown_task: RefCell::new(None),
            terminal_failure: RefCell::new(None),
        });
        runtime_link.replace(Rc::downgrade(&runtime));
        attach_managed_task_failure_handlers(&runtime);
        let prepared_instances = prepare_native_modules(
            &runtime.plan,
            &runtime.modules,
            &runtime.dependencies,
            &runtime.activation_order,
            &runtime.admission,
        )
        .await?;
        if let Err(error) = activate_native_modules(
            &runtime.modules,
            &runtime.dependencies,
            &runtime.activation_order,
            &runtime.ready_gate,
            &runtime.admission,
        )
        .await
        {
            let _ = deactivate_in_reverse(
                &runtime.modules,
                &runtime.dependencies,
                &prepared_instances,
                DeactivationReason::StartupRollback,
                &runtime.admission,
            )
            .await;
            return Err(error);
        }
        open_native_readiness(&driver, &runtime.ready_gate, &runtime.admission).await;
        Ok(NativeApp {
            bindings,
            stream_bindings,
            event_bindings,
            runtime,
        })
    }
}

fn attach_managed_task_failure_handlers(runtime: &Rc<NativeAppRuntime>) {
    for (instance_key, module) in &runtime.modules {
        let Some((_, tasks, _)) = module.generation_parts() else {
            continue;
        };
        attach_managed_task_failure_handler(runtime, instance_key, &tasks);
    }
}

fn attach_managed_task_failure_handler(
    runtime: &Rc<NativeAppRuntime>,
    instance_key: &str,
    tasks: &ManagedTaskScope,
) {
    let task_runtime = Rc::downgrade(runtime);
    let task_instance_key = instance_key.to_owned();
    let handler: Rc<dyn Fn()> = Rc::new(move || {
        let Some(runtime) = task_runtime.upgrade() else {
            return;
        };
        if begin_module_supervision(&runtime, &task_instance_key).unwrap_or(false)
            && let Err(error) = schedule_module_supervision(&runtime, &task_instance_key)
        {
            let _ = handle_supervision_schedule_failure(&runtime, &task_instance_key, error);
        }
    });
    tasks.set_failure_handler(&handler);
}

fn runtime_plan_error(error: &PlanResolutionError) -> RuntimeFailure {
    RuntimeFailure::InvalidResolvedPlan {
        detail: error.to_string(),
    }
}

fn validate_prepared_native_app(
    plan: &ResolvedAppPlan,
    bindings: &[PreparedBinding],
    stream_bindings: &[PreparedStreamBinding],
    event_bindings: &[PreparedEventBinding],
    generations: &BTreeMap<String, PreparedNativeModule>,
) -> Result<(), RuntimeFailure> {
    if generations.len() != plan.module_instances().len() {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!(
                "Execution Adapters prepared {} Module generations; expected {}",
                generations.len(),
                plan.module_instances().len()
            ),
        });
    }
    for instance in plan.module_instances() {
        let generation = generations.get(instance.instance_key()).ok_or_else(|| {
            RuntimeFailure::InvalidResolvedPlan {
                detail: format!(
                    "Execution Adapters did not prepare Module Instance `{}`",
                    instance.instance_key()
                ),
            }
        })?;
        validate_native_endpoint_set(
            instance.instance_key(),
            instance,
            generation.endpoints(),
            generation.stream_endpoints(),
            generation.event_endpoints(),
        )?;
    }
    if let Some(instance_key) = generations.keys().find(|instance_key| {
        !plan
            .module_instances()
            .iter()
            .any(|instance| instance.instance_key() == instance_key.as_str())
    }) {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!("Execution Adapter prepared unknown Module Instance `{instance_key}`"),
        });
    }

    let expected_request_bindings = plan
        .capability_bindings()
        .iter()
        .filter(|binding| {
            plan.module_instance(binding.provider_instance())
                .and_then(|provider| {
                    provider
                        .provided_capabilities()
                        .iter()
                        .find(|endpoint| endpoint.capability_id() == binding.capability_id())
                })
                .is_some_and(|endpoint| !endpoint.request_operations().is_empty())
        })
        .count();
    let expected_stream_bindings = plan
        .capability_bindings()
        .iter()
        .filter(|binding| {
            plan.module_instance(binding.provider_instance())
                .and_then(|provider| {
                    provider
                        .provided_capabilities()
                        .iter()
                        .find(|endpoint| endpoint.capability_id() == binding.capability_id())
                })
                .is_some_and(|endpoint| !endpoint.stream_operations().is_empty())
        })
        .count();
    let expected_event_bindings = plan
        .capability_bindings()
        .iter()
        .filter(|binding| {
            plan.module_instance(binding.provider_instance())
                .and_then(|provider| {
                    provider
                        .provided_capabilities()
                        .iter()
                        .find(|endpoint| endpoint.capability_id() == binding.capability_id())
                })
                .is_some_and(|endpoint| !endpoint.event_operations().is_empty())
        })
        .count();
    if bindings.len() != expected_request_bindings {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: if expected_stream_bindings == 0 && stream_bindings.is_empty() {
                format!(
                    "Execution Adapters prepared {} bindings; expected {}",
                    bindings.len(),
                    expected_request_bindings
                )
            } else {
                format!(
                    "Execution Adapters prepared {} request bindings; expected {}",
                    bindings.len(),
                    expected_request_bindings
                )
            },
        });
    }
    if stream_bindings.len() != expected_stream_bindings {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!(
                "Execution Adapters prepared {} stream bindings; expected {}",
                stream_bindings.len(),
                expected_stream_bindings
            ),
        });
    }
    if event_bindings.len() != expected_event_bindings {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!(
                "Execution Adapters prepared {} Event bindings; expected {}",
                event_bindings.len(),
                expected_event_bindings
            ),
        });
    }
    for planned in plan.capability_bindings() {
        let provider = generations
            .get(planned.provider_instance())
            .expect("the resolved Plan references one validated provider generation");
        let descriptor = plan
            .module_instance(planned.provider_instance())
            .and_then(|provider| {
                provider
                    .provided_capabilities()
                    .iter()
                    .find(|endpoint| endpoint.capability_id() == planned.capability_id())
            })
            .expect("the resolved Plan references one validated provider endpoint");
        if !descriptor.request_operations().is_empty() {
            let matching: Vec<_> = bindings
                .iter()
                .filter(|prepared| {
                    prepared.consumer_instance == planned.consumer_instance()
                        && prepared.provider_instance == planned.provider_instance()
                        && prepared.endpoint.capability_id() == planned.capability_id()
                        && prepared.endpoint.descriptor_version() == planned.descriptor_version()
                })
                .collect();
            if matching.len() != 1 {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "Execution Adapters prepared {} request bindings for `{}:{}:{}`; expected 1",
                        matching.len(),
                        planned.consumer_instance(),
                        planned.capability_id(),
                        planned.provider_instance()
                    ),
                });
            }
            if !provider
                .endpoints()
                .iter()
                .any(|endpoint| Rc::ptr_eq(endpoint, &matching[0].endpoint))
            {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "request binding `{}:{}:{}` does not reference its provider generation endpoint",
                        planned.consumer_instance(),
                        planned.capability_id(),
                        planned.provider_instance()
                    ),
                });
            }
        }
        if !descriptor.stream_operations().is_empty() {
            let matching: Vec<_> = stream_bindings
                .iter()
                .filter(|prepared| {
                    prepared.consumer_instance == planned.consumer_instance()
                        && prepared.provider_instance == planned.provider_instance()
                        && prepared.endpoint.capability_id() == planned.capability_id()
                        && prepared.endpoint.descriptor_version() == planned.descriptor_version()
                })
                .collect();
            if matching.len() != 1 {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "Execution Adapters prepared {} stream bindings for `{}:{}:{}`; expected 1",
                        matching.len(),
                        planned.consumer_instance(),
                        planned.capability_id(),
                        planned.provider_instance()
                    ),
                });
            }
            if !provider
                .stream_endpoints()
                .iter()
                .any(|endpoint| Rc::ptr_eq(endpoint, &matching[0].endpoint))
            {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "stream binding `{}:{}:{}` does not reference its provider generation endpoint",
                        planned.consumer_instance(),
                        planned.capability_id(),
                        planned.provider_instance()
                    ),
                });
            }
        }
        if !descriptor.event_operations().is_empty() {
            let matching: Vec<_> = event_bindings
                .iter()
                .filter(|prepared| {
                    prepared.consumer_instance == planned.consumer_instance()
                        && prepared.provider_instance == planned.provider_instance()
                        && prepared.endpoint.capability_id() == planned.capability_id()
                        && prepared.endpoint.descriptor_version() == planned.descriptor_version()
                })
                .collect();
            if matching.len() != 1 {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "Execution Adapters prepared {} Event bindings for `{}:{}:{}`; expected 1",
                        matching.len(),
                        planned.consumer_instance(),
                        planned.capability_id(),
                        planned.provider_instance()
                    ),
                });
            }
            if !provider
                .event_endpoints()
                .iter()
                .any(|endpoint| Rc::ptr_eq(endpoint, &matching[0].endpoint))
            {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "Event binding `{}:{}:{}` does not reference its provider generation endpoint",
                        planned.consumer_instance(),
                        planned.capability_id(),
                        planned.provider_instance()
                    ),
                });
            }
        }
    }
    Ok(())
}

fn native_module_runtimes<D: RuntimeDriver>(
    plan: &ResolvedAppPlan,
    driver: &D,
    mut generations: BTreeMap<String, PreparedNativeModule>,
) -> BTreeMap<String, NativeModuleRuntime> {
    let mut runtimes = BTreeMap::new();
    for instance in plan.module_instances() {
        let lifecycle = generations
            .remove(instance.instance_key())
            .map(|generation| generation.lifecycle())
            .expect("prepared App validation requires one generation per planned Instance");
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
    runtimes
}

async fn prepare_native_modules(
    plan: &ResolvedAppPlan,
    modules: &BTreeMap<String, NativeModuleRuntime>,
    dependencies: &BTreeMap<String, ModuleDependencies>,
    activation_order: &[String],
    admission: &AppAdmission,
) -> Result<Vec<String>, RuntimeFailure> {
    let mut prepared_instances = Vec::with_capacity(activation_order.len());
    for instance_key in activation_order {
        let instance = plan
            .module_instances()
            .iter()
            .find(|instance| instance.instance_key() == instance_key)
            .expect("activation order only contains planned Module Instances");
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
            entrypoint: instance.entrypoint().to_owned(),
            configuration: instance.configuration().to_owned(),
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
        let Some(descriptor) =
            plan.module_instance(binding.provider_instance())
                .and_then(|provider| {
                    provider
                        .provided_capabilities()
                        .iter()
                        .find(|endpoint| endpoint.capability_id() == binding.capability_id())
                })
        else {
            continue;
        };
        if descriptor.request_operations().is_empty() {
            continue;
        }
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

fn native_stream_bindings(
    plan: &ResolvedAppPlan,
    prepared: &[PreparedStreamBinding],
) -> (NativeStreamBindingTable, NativeStreamEndpointStateTable) {
    let mut bindings = BTreeMap::new();
    let mut endpoint_states = BTreeMap::new();
    for binding in plan.capability_bindings() {
        let Some(descriptor) =
            plan.module_instance(binding.provider_instance())
                .and_then(|provider| {
                    provider
                        .provided_capabilities()
                        .iter()
                        .find(|endpoint| endpoint.capability_id() == binding.capability_id())
                })
        else {
            continue;
        };
        if descriptor.stream_operations().is_empty() {
            continue;
        }
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
            .or_insert_with(|| Rc::new(NativeStreamEndpointState::new(endpoint.clone(), 1)))
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
            .push(NativeStreamEndpointBinding {
                module_instance: binding.provider_instance().to_owned(),
                state,
                admissions,
            });
    }
    (bindings, endpoint_states)
}

fn native_event_bindings(
    plan: &ResolvedAppPlan,
    prepared: &[PreparedEventBinding],
) -> (NativeEventBindingTable, NativeEventEndpointStateTable) {
    let mut bindings = BTreeMap::new();
    let mut endpoint_states = BTreeMap::new();
    for binding in plan.capability_bindings() {
        let Some(descriptor) =
            plan.module_instance(binding.provider_instance())
                .and_then(|provider| {
                    provider
                        .provided_capabilities()
                        .iter()
                        .find(|endpoint| endpoint.capability_id() == binding.capability_id())
                })
        else {
            continue;
        };
        if descriptor.event_operations().is_empty() {
            continue;
        }
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
            .or_insert_with(|| Rc::new(event::NativeEventEndpointState::new(endpoint.clone(), 1)))
            .clone();
        let queue = event::NativeEventQueue::new(plan.event_admission_for(binding));
        state.register_queue(&queue);
        bindings
            .entry((
                binding.consumer_instance().to_owned(),
                endpoint.capability_id(),
            ))
            .or_insert_with(Vec::new)
            .push(event::NativeEventEndpointBinding {
                module_instance: binding.provider_instance().to_owned(),
                state,
                queue,
            });
    }
    (bindings, endpoint_states)
}

fn module_dependencies(
    plan: &ResolvedAppPlan,
    endpoints: &BTreeMap<(String, &'static str), Vec<NativeEndpointBinding>>,
    stream_endpoints: &NativeStreamBindingTable,
    event_endpoints: &NativeEventBindingTable,
    runtime: &Rc<RefCell<Weak<NativeAppRuntime>>>,
) -> BTreeMap<String, ModuleDependencies> {
    let mut dependencies: BTreeMap<String, ModuleDependencies> = plan
        .module_instances()
        .iter()
        .map(|instance| {
            (
                instance.instance_key().to_owned(),
                ModuleDependencies::new(instance.instance_key(), runtime.clone()),
            )
        })
        .collect();
    for binding in plan.capability_bindings() {
        dependencies
            .get_mut(binding.consumer_instance())
            .expect("every resolved binding consumer has Module dependencies")
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
                        binding: endpoint.clone(),
                        caller_instance: binding.consumer_instance().to_owned(),
                        runtime: runtime.clone(),
                    }),
                stream_endpoints
                    .iter()
                    .find(|((consumer, capability), _)| {
                        consumer == binding.consumer_instance()
                            && *capability == binding.capability_id()
                    })
                    .and_then(|(_, endpoints)| endpoints.get(binding.provider_order()))
                    .map(|endpoint| ModuleStreamDependencyHandle {
                        binding: endpoint.clone(),
                        caller_instance: binding.consumer_instance().to_owned(),
                        runtime: runtime.clone(),
                    }),
                event_endpoints
                    .iter()
                    .find(|((consumer, capability), _)| {
                        consumer == binding.consumer_instance()
                            && *capability == binding.capability_id()
                    })
                    .and_then(|(_, endpoints)| endpoints.get(binding.provider_order()))
                    .map(|endpoint| ModuleEventDependencyHandle {
                        binding: endpoint.clone(),
                        caller_instance: binding.consumer_instance().to_owned(),
                        runtime: runtime.clone(),
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
        if let Some(error) = cleanup_generation(
            instance_key,
            generation,
            dependencies.get(instance_key).cloned().unwrap_or_default(),
            reason,
            admission.clone(),
        )
        .await
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

    if !drain_supervision_until(runtime, deadline).await {
        terminate_remaining_cleanup(runtime);
        return ShutdownOutcome::Timeout;
    }

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
        if let Some((_, tasks, _)) = module.generation_parts() {
            tasks.abort_all();
        }
    }
    for task in runtime.supervision_tasks.borrow().values() {
        task.cancel();
    }
}

async fn drain_supervision_until(runtime: &NativeAppRuntime, deadline: Duration) -> bool {
    let tasks = std::mem::take(&mut *runtime.supervision_tasks.borrow_mut());
    for (index, task) in tasks.values().enumerate() {
        if wait_until(&runtime.driver, deadline, task.join())
            .await
            .is_none()
        {
            for pending in tasks.values().skip(index) {
                pending.cancel();
            }
            return false;
        }
    }
    true
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
    for ((provider, _), endpoint) in &runtime.stream_endpoint_states {
        if provider == instance_key {
            endpoint.mark_unavailable();
        }
    }
    for ((provider, _), endpoint) in &runtime.event_endpoint_states {
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
    let task = (runtime.driver.spawn_local)(Box::pin(async move {
        let _ = supervise_module_instance(task_runtime, task_instance_key).await;
    }))
    .map_err(|error| {
        if let Some(state) = runtime.supervision.borrow_mut().get_mut(instance_key) {
            state.restarting = false;
        }
        RuntimeFailure::Internal {
            detail: format!("failed to schedule Module supervision: {error:?}"),
        }
    })?;
    runtime
        .supervision_tasks
        .borrow_mut()
        .insert(instance_key.to_owned(), ManagedTask::from_driver_task(task));
    Ok(())
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
        let (endpoints, stream_endpoints, event_endpoints, lifecycle) = prepared.into_parts();
        if validate_native_endpoint_set(
            &instance_key,
            instance,
            &endpoints,
            &stream_endpoints,
            &event_endpoints,
        )
        .is_err()
        {
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
        install_module_endpoints(
            &runtime,
            &instance_key,
            endpoints,
            stream_endpoints,
            event_endpoints,
            generation_number,
        );
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
    runtime: &Rc<NativeAppRuntime>,
    instance_key: &str,
    lifecycle: Rc<dyn ModuleLifecycle>,
) -> Result<NativeModuleGeneration, GenerationPreparationFailure> {
    let tasks = ManagedTaskScope::new_from_driver_control(&runtime.driver);
    attach_managed_task_failure_handler(runtime, instance_key, &tasks);
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
    let instance = runtime
        .plan
        .module_instances()
        .iter()
        .find(|instance| instance.instance_key() == instance_key)
        .expect("supervision only recreates planned Module Instances");
    if lifecycle
        .prepare(PrepareContext {
            instance_key: instance_key.to_owned(),
            entrypoint: instance.entrypoint().to_owned(),
            configuration: instance.configuration().to_owned(),
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
    let dependencies = runtime
        .dependencies
        .get(instance_key)
        .cloned()
        .unwrap_or_default();
    cleanup_generation(
        instance_key,
        generation,
        dependencies,
        reason,
        runtime.admission.clone(),
    )
    .await
}

async fn cleanup_generation(
    instance_key: &str,
    generation: NativeModuleGeneration,
    dependencies: ModuleDependencies,
    reason: DeactivationReason,
    admission: AppAdmission,
) -> Option<RuntimeFailure> {
    generation.tasks.close();
    generation.resources.close();
    generation.tasks.cancel_all().await;
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
            admission,
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
    stream_endpoints: Vec<Rc<dyn NativeStreamEndpoint>>,
    event_endpoints: Vec<Rc<dyn NativeEventEndpoint>>,
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
    for endpoint in stream_endpoints {
        if let Some(state) = runtime
            .stream_endpoint_states
            .get(&(instance_key.to_owned(), endpoint.capability_id().to_owned()))
        {
            state.install(endpoint, generation);
        }
    }
    for endpoint in event_endpoints {
        if let Some(state) = runtime
            .event_endpoint_states
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
    actual_streams: &[Rc<dyn NativeStreamEndpoint>],
    actual_events: &[Rc<dyn NativeEventEndpoint>],
) -> Result<(), RuntimeFailure> {
    let expected_requests = expected
        .provided_capabilities()
        .iter()
        .filter(|descriptor| !descriptor.request_operations().is_empty())
        .count();
    let expected_streams = expected
        .provided_capabilities()
        .iter()
        .filter(|descriptor| !descriptor.stream_operations().is_empty())
        .count();
    let expected_events = expected
        .provided_capabilities()
        .iter()
        .filter(|descriptor| !descriptor.event_operations().is_empty())
        .count();
    if expected_requests != actual.len()
        || expected_streams != actual_streams.len()
        || expected_events != actual_events.len()
    {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!(
                "Module Instance `{instance_key}` prepared {} request, {} stream, and {} Event endpoints; expected {} request, {} stream, and {} Event endpoints",
                actual.len(),
                actual_streams.len(),
                actual_events.len(),
                expected_requests,
                expected_streams,
                expected_events
            ),
        });
    }
    for descriptor in expected.provided_capabilities() {
        let request_operations = descriptor.request_operations();
        if !request_operations.is_empty() {
            let matching: Vec<_> = actual
                .iter()
                .filter(|endpoint| endpoint.capability_id() == descriptor.capability_id())
                .collect();
            if matching.len() != 1 {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "Module Instance `{instance_key}` prepared {} request endpoints for Capability `{}`",
                        matching.len(),
                        descriptor.capability_id()
                    ),
                });
            }
            validate_endpoint_operations(
                instance_key,
                descriptor.capability_id(),
                descriptor.descriptor_version(),
                &request_operations,
                matching[0].descriptor_version(),
                matching[0].operations(),
            )?;
        }
        let stream_operations = descriptor.stream_operations();
        if !stream_operations.is_empty() {
            let matching: Vec<_> = actual_streams
                .iter()
                .filter(|endpoint| endpoint.capability_id() == descriptor.capability_id())
                .collect();
            if matching.len() != 1 {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "Module Instance `{instance_key}` prepared {} stream endpoints for Capability `{}`",
                        matching.len(),
                        descriptor.capability_id()
                    ),
                });
            }
            validate_endpoint_operations(
                instance_key,
                descriptor.capability_id(),
                descriptor.descriptor_version(),
                &stream_operations,
                matching[0].descriptor_version(),
                matching[0].operations(),
            )?;
        }
        let event_operations = descriptor.event_operations();
        if !event_operations.is_empty() {
            let matching: Vec<_> = actual_events
                .iter()
                .filter(|endpoint| endpoint.capability_id() == descriptor.capability_id())
                .collect();
            if matching.len() != 1 {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "Module Instance `{instance_key}` prepared {} Event endpoints for Capability `{}`",
                        matching.len(),
                        descriptor.capability_id()
                    ),
                });
            }
            validate_endpoint_operations(
                instance_key,
                descriptor.capability_id(),
                descriptor.descriptor_version(),
                &event_operations,
                matching[0].descriptor_version(),
                matching[0].operations(),
            )?;
        }
    }
    Ok(())
}

fn validate_endpoint_operations(
    instance_key: &str,
    capability_id: &str,
    expected_version: &str,
    expected_operations: &[&str],
    actual_version: &str,
    actual_operations: &[&str],
) -> Result<(), RuntimeFailure> {
    if actual_version != expected_version || actual_operations != expected_operations {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!(
                "Module Instance `{instance_key}` endpoint `{capability_id}` differs from its resolved Descriptor"
            ),
        });
    }
    let mut unique = actual_operations.to_vec();
    unique.sort_unstable();
    unique.dedup();
    if unique.len() != actual_operations.len() {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!(
                "Module Instance `{instance_key}` endpoint `{capability_id}` has duplicate Operations"
            ),
        });
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
            let outcome = match AssertUnwindSafe(Abortable::new(task, registration))
                .catch_unwind()
                .await
            {
                Ok(Ok(())) => TaskOutcome::Completed,
                Ok(Err(_)) => TaskOutcome::Cancelled,
                Err(_) => TaskOutcome::Failed,
            };
            let _ = completed.send(outcome);
        })?;
        Ok(DriverTask::new(abort, completion))
    }

    fn shutdown_requested(&self) -> bool {
        self.state.shutdown_requested.get()
    }
}
