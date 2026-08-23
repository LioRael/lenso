use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::VecDeque,
    fmt,
    marker::PhantomData,
    panic::AssertUnwindSafe,
    rc::{Rc, Weak},
};

use futures::{FutureExt, future::LocalBoxFuture};

use super::{
    CancellationToken, DiagnosticAdmission, DiagnosticEvent, DiagnosticSource, EventAdmissionPlan,
    InvocationContext, NativeAppRuntime, RuntimeFailure, await_with_generation_context,
    diagnostics::diagnostic_operation, ensure_context_active,
    schedule_module_supervision_after_failure,
};

/// Static identity and Rust value types generated for one ephemeral Event Capability.
pub trait EventCapability: 'static {
    /// Typed value published to each explicitly bound subscriber.
    type Event: Clone + 'static;
    /// Stable Capability series identity.
    const ID: &'static str;
    /// Exact generated Descriptor version.
    const DESCRIPTOR_VERSION: &'static str;
}

/// The admission result for one publisher-to-subscriber binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventAdmission {
    /// The event entered that subscriber's volatile queue.
    Accepted,
    /// The subscriber binding is currently unavailable.
    Unavailable,
    /// The subscriber's bounded admission is full.
    Exhausted,
}

/// Alias emphasizing the result's publication context.
pub type EventPublishStatus = EventAdmission;

/// One deterministic result for one explicit subscriber binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventPublishResult {
    subscriber_instance: String,
    admission: EventAdmission,
}

impl EventPublishResult {
    pub(crate) fn new(subscriber_instance: String, admission: EventAdmission) -> Self {
        Self {
            subscriber_instance,
            admission,
        }
    }

    /// Returns the App-local subscriber Instance key.
    pub fn subscriber_instance(&self) -> &str {
        &self.subscriber_instance
    }

    /// Returns the provider Instance key used by the explicit binding.
    pub fn provider_instance(&self) -> &str {
        self.subscriber_instance()
    }

    /// Returns the independent admission result for this binding.
    pub const fn admission(&self) -> EventAdmission {
        self.admission
    }

    /// Returns the status using publication-oriented terminology.
    pub const fn status(&self) -> EventPublishStatus {
        self.admission
    }
}

/// Adapter-facing endpoint for one or more ephemeral Event Operations.
pub trait NativeEventEndpoint: fmt::Debug {
    /// Stable Capability series identity.
    fn capability_id(&self) -> &'static str;
    /// Exact Descriptor version implemented by this endpoint.
    fn descriptor_version(&self) -> &'static str;
    /// Exact stable Event Operation names implemented by this endpoint.
    fn operations(&self) -> &'static [&'static str];
    /// Returns whether the Adapter owns the subscriber's admission queue.
    ///
    /// Native endpoints use the Kernel mailbox by default. An out-of-process
    /// Adapter may return `true` when its transport has already admitted the
    /// value into the subscriber's own bounded queue before this call returns.
    fn owns_event_admission(&self) -> bool {
        false
    }
    /// Publishes one value after the endpoint has admitted it.
    ///
    /// Implementations must return once the value is accepted into their own
    /// bounded volatile queue. Subscriber handler failures after that point
    /// are deliberately outside the publisher's response path.
    fn publish(
        &self,
        operation: &str,
        event: Box<dyn Any>,
        context: InvocationContext,
    ) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>>;
}

struct QueuedEvent {
    operation: String,
    event: Box<dyn Any>,
    context: InvocationContext,
    snapshot: NativeEventEndpointSnapshot,
}

impl fmt::Debug for QueuedEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("QueuedEvent")
            .field("operation", &self.operation)
            .field("request_id", &self.context.request_id())
            .field("generation", &self.snapshot.generation)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
struct NativeEventQueueState {
    pending: VecDeque<QueuedEvent>,
    admitted: usize,
    draining: bool,
}

/// One bounded FIFO mailbox owned by one publisher-to-subscriber binding.
#[derive(Debug)]
pub(crate) struct NativeEventQueue {
    capacity: usize,
    state: RefCell<NativeEventQueueState>,
}

impl NativeEventQueue {
    pub(crate) fn new(admission: EventAdmissionPlan) -> Rc<Self> {
        Self {
            capacity: admission.capacity(),
            state: RefCell::new(NativeEventQueueState::default()),
        }
        .into()
    }

    fn try_enqueue(&self, event: QueuedEvent) -> Option<bool> {
        let mut state = self.state.borrow_mut();
        if state.admitted >= self.capacity {
            return None;
        }
        state.admitted += 1;
        state.pending.push_back(event);
        if state.draining {
            Some(false)
        } else {
            state.draining = true;
            Some(true)
        }
    }

    fn pop(&self) -> Option<QueuedEvent> {
        self.state.borrow_mut().pending.pop_front()
    }

    fn complete(&self) {
        let mut state = self.state.borrow_mut();
        state.admitted = state.admitted.saturating_sub(1);
        if state.pending.is_empty() {
            state.draining = false;
        }
    }

    fn abort(&self) {
        let mut state = self.state.borrow_mut();
        state.pending.clear();
        state.admitted = 0;
        state.draining = false;
    }
}

#[derive(Clone, Debug)]
pub(crate) struct NativeEventEndpointSnapshot {
    pub(crate) endpoint: Rc<dyn NativeEventEndpoint>,
    pub(crate) generation: u64,
    pub(crate) cancellation: CancellationToken,
}

#[derive(Debug)]
pub(crate) struct NativeEventEndpointState {
    pub(crate) capability_id: &'static str,
    pub(crate) descriptor_version: &'static str,
    pub(crate) operations: &'static [&'static str],
    endpoint: RefCell<Option<Rc<dyn NativeEventEndpoint>>>,
    generation: Cell<u64>,
    cancellation: RefCell<CancellationToken>,
    queues: RefCell<Vec<Weak<NativeEventQueue>>>,
}

impl NativeEventEndpointState {
    pub(crate) fn new(endpoint: Rc<dyn NativeEventEndpoint>, generation: u64) -> Self {
        Self {
            capability_id: endpoint.capability_id(),
            descriptor_version: endpoint.descriptor_version(),
            operations: endpoint.operations(),
            endpoint: RefCell::new(Some(endpoint)),
            generation: Cell::new(generation),
            cancellation: RefCell::new(CancellationToken::new()),
            queues: RefCell::new(Vec::new()),
        }
    }

    pub(crate) fn snapshot(&self) -> Option<NativeEventEndpointSnapshot> {
        self.endpoint
            .borrow()
            .clone()
            .map(|endpoint| NativeEventEndpointSnapshot {
                endpoint,
                generation: self.generation.get(),
                cancellation: self.cancellation.borrow().clone(),
            })
    }

    pub(crate) fn mark_unavailable(&self) {
        self.cancellation.borrow().cancel();
        self.endpoint.borrow_mut().take();
        self.reset_queues();
    }

    pub(crate) fn install(&self, endpoint: Rc<dyn NativeEventEndpoint>, generation: u64) {
        self.generation.set(generation);
        self.cancellation.replace(CancellationToken::new());
        self.endpoint.replace(Some(endpoint));
    }

    pub(crate) fn is_current(&self, generation: u64) -> bool {
        self.generation.get() == generation && self.endpoint.borrow().is_some()
    }

    pub(crate) fn register_queue(&self, queue: &Rc<NativeEventQueue>) {
        self.queues.borrow_mut().push(Rc::downgrade(queue));
    }

    fn reset_queues(&self) {
        self.queues.borrow_mut().retain(|queue| {
            let Some(queue) = queue.upgrade() else {
                return false;
            };
            queue.abort();
            true
        });
    }
}

#[derive(Clone, Debug)]
pub(crate) struct NativeEventEndpointBinding {
    pub(crate) module_instance: String,
    pub(crate) state: Rc<NativeEventEndpointState>,
    pub(crate) queue: Rc<NativeEventQueue>,
}

/// An opaque Event endpoint passed to Module lifecycle code.
#[derive(Clone, Debug)]
pub struct ModuleEventDependencyHandle {
    pub(crate) binding: NativeEventEndpointBinding,
    pub(crate) caller_instance: String,
    pub(crate) runtime: Rc<RefCell<std::rc::Weak<NativeAppRuntime>>>,
}

impl ModuleEventDependencyHandle {
    /// Returns the Capability implemented by this handle.
    pub fn capability_id(&self) -> &'static str {
        self.binding.state.capability_id
    }

    /// Returns the exact Descriptor version implemented by this handle.
    pub fn descriptor_version(&self) -> &'static str {
        self.binding.state.descriptor_version
    }

    /// Returns the exact Event Operation table implemented by this handle.
    pub fn operations(&self) -> &'static [&'static str] {
        self.binding.state.operations
    }

    /// Converts this resolved dependency into its generated typed Event handle.
    pub fn typed<C: EventCapability>(&self) -> Result<NativeEventHandle<C>, RuntimeFailure> {
        if self.capability_id() != C::ID || self.descriptor_version() != C::DESCRIPTOR_VERSION {
            return Err(RuntimeFailure::ProtocolViolation { capability: C::ID });
        }
        let runtime = self
            .runtime
            .borrow()
            .upgrade()
            .ok_or(RuntimeFailure::AdmissionClosed)?;
        Ok(NativeEventHandle::from_endpoints(
            std::slice::from_ref(&self.binding),
            runtime,
            &self.caller_instance,
            true,
        ))
    }
}

/// Typed, immutable Event endpoints materialized before App boot completes.
#[derive(Debug)]
pub struct NativeEventHandle<C: EventCapability> {
    endpoints: Vec<NativeEventEndpointBinding>,
    runtime: Rc<NativeAppRuntime>,
    caller_instance: String,
    allow_before_ready: bool,
    capability: PhantomData<fn() -> C>,
}

impl<C: EventCapability> NativeEventHandle<C> {
    pub(crate) fn from_endpoints(
        endpoints: &[NativeEventEndpointBinding],
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

    /// Returns the number of explicitly bound subscriber endpoints.
    pub fn binding_count(&self) -> usize {
        self.endpoints.len()
    }

    /// Publishes to every bound subscriber in deterministic order.
    ///
    /// An empty binding set succeeds with an empty result. Each result is
    /// independent: one exhausted or unavailable subscriber does not prevent
    /// the remaining bindings from being attempted.
    pub async fn publish(&self, operation: &str, event: C::Event) -> Vec<EventPublishResult> {
        self.publish_with_context(operation, self.next_context(), event)
            .await
    }

    /// Publishes with one propagated Invocation Context.
    pub async fn publish_with_context(
        &self,
        operation: &str,
        context: InvocationContext,
        event: C::Event,
    ) -> Vec<EventPublishResult> {
        let context = context
            .for_caller(&self.caller_instance)
            .for_target(C::ID, operation);
        futures::future::join_all(self.endpoints.iter().map(|endpoint| {
            self.publish_to_endpoint(endpoint, operation, context.clone(), event.clone())
        }))
        .await
    }

    async fn publish_to_endpoint(
        &self,
        endpoint: &NativeEventEndpointBinding,
        operation: &str,
        context: InvocationContext,
        event: C::Event,
    ) -> EventPublishResult {
        let operation_name = diagnostic_operation(endpoint.state.operations, operation);
        let was_closed = self.runtime.shutdown_started.get()
            || (!self.allow_before_ready && self.runtime.admission.is_closed());
        let result = self
            .publish_to_endpoint_inner(endpoint, operation, context.clone(), event)
            .await;
        let outcome = match result.admission() {
            EventAdmission::Accepted => DiagnosticAdmission::Accepted,
            EventAdmission::Unavailable if was_closed => DiagnosticAdmission::Closed,
            EventAdmission::Unavailable => DiagnosticAdmission::Unavailable,
            EventAdmission::Exhausted => DiagnosticAdmission::Exhausted,
        };
        self.runtime.diagnostics.emit(
            DiagnosticSource::Admission,
            (self.runtime.driver.now)(),
            |_| DiagnosticEvent::EventAdmission {
                request_id: context.request_id(),
                publisher_instance: self.caller_instance.clone(),
                subscriber_instance: endpoint.module_instance.clone(),
                capability: C::ID,
                operation: operation_name,
                outcome,
            },
        );
        result
    }

    async fn publish_to_endpoint_inner(
        &self,
        endpoint: &NativeEventEndpointBinding,
        operation: &str,
        context: InvocationContext,
        event: C::Event,
    ) -> EventPublishResult {
        let subscriber = endpoint.module_instance.clone();
        let unavailable =
            || EventPublishResult::new(subscriber.clone(), EventAdmission::Unavailable);
        if self.runtime.shutdown_started.get()
            || (!self.allow_before_ready && self.runtime.admission.is_closed())
        {
            return unavailable();
        }
        let Some(snapshot) = endpoint.state.snapshot() else {
            return unavailable();
        };
        if !endpoint.state.operations.contains(&operation) {
            return unavailable();
        }
        let queue = &endpoint.queue;
        if !endpoint.state.is_current(snapshot.generation)
            || ensure_context_active(&self.runtime.driver, &context).is_err()
        {
            return unavailable();
        }
        if snapshot.endpoint.owns_event_admission() {
            // Once an out-of-process Adapter starts admission, wait for its
            // commit acknowledgement. Racing that acknowledgement against the
            // caller deadline can report unavailable after the subscriber has
            // already accepted the Event.
            let result = snapshot
                .endpoint
                .publish(operation, Box::new(event), context.clone())
                .await;
            return match result {
                Ok(()) => EventPublishResult::new(subscriber, EventAdmission::Accepted),
                Err(error) => {
                    let error = schedule_module_supervision_after_failure(
                        &self.runtime,
                        &endpoint.module_instance,
                        error,
                    );
                    self.runtime.diagnostics.emit_runtime_failure(
                        (self.runtime.driver.now)(),
                        Some(&endpoint.module_instance),
                        &error,
                    );
                    let admission = if matches!(error, RuntimeFailure::ResourceExhausted { .. }) {
                        EventAdmission::Exhausted
                    } else {
                        EventAdmission::Unavailable
                    };
                    EventPublishResult::new(subscriber, admission)
                }
            };
        }
        let queued = QueuedEvent {
            operation: operation.to_owned(),
            event: Box::new(event),
            context,
            snapshot,
        };
        let Some(should_start) = queue.try_enqueue(queued) else {
            return EventPublishResult::new(subscriber, EventAdmission::Exhausted);
        };
        if should_start {
            let Some(tasks) = self
                .runtime
                .modules
                .get(&endpoint.module_instance)
                .and_then(|module| module.generation_parts().map(|(_, tasks, _)| tasks))
            else {
                queue.abort();
                return unavailable();
            };
            let drain = drain_event_queue(
                queue.clone(),
                self.runtime.clone(),
                endpoint.module_instance.clone(),
                C::ID,
            );
            if tasks.spawn_local(Box::pin(drain)).is_err() {
                queue.abort();
                return unavailable();
            }
        }
        EventPublishResult::new(subscriber, EventAdmission::Accepted)
    }

    fn next_context(&self) -> InvocationContext {
        InvocationContext::new(self.next_request_id(), None, CancellationToken::new())
            .with_caller_instance(self.caller_instance.clone())
    }

    fn next_request_id(&self) -> super::RequestId {
        let request_id = self.runtime.request_ids.get();
        self.runtime.request_ids.set(request_id.saturating_add(1));
        request_id
    }
}

async fn drain_event_queue(
    queue: Rc<NativeEventQueue>,
    runtime: Rc<NativeAppRuntime>,
    module_instance: String,
    capability: &'static str,
) {
    while let Some(queued) = queue.pop() {
        let result = AssertUnwindSafe(await_with_generation_context(
            &runtime.driver,
            &queued.context,
            queued.snapshot.cancellation,
            capability,
            queued.snapshot.endpoint.publish(
                &queued.operation,
                queued.event,
                queued.context.clone(),
            ),
        ))
        .catch_unwind()
        .await;
        match result {
            Ok(Ok(Ok(()))) => {}
            Ok(Ok(Err(error)) | Err(error)) => {
                runtime.diagnostics.emit_runtime_failure(
                    (runtime.driver.now)(),
                    Some(&module_instance),
                    &error,
                );
                let _ =
                    schedule_module_supervision_after_failure(&runtime, &module_instance, error);
            }
            Err(_) => {
                let error = RuntimeFailure::ModuleFailure {
                    detail: format!("native Event subscriber `{module_instance}` panicked"),
                };
                runtime.diagnostics.emit_runtime_failure(
                    (runtime.driver.now)(),
                    Some(&module_instance),
                    &error,
                );
                let _ =
                    schedule_module_supervision_after_failure(&runtime, &module_instance, error);
            }
        }
        queue.complete();
    }
}
