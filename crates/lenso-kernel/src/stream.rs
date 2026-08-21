use std::{any::Any, cell::Cell, fmt, marker::PhantomData, rc::Rc};

use futures::future::LocalBoxFuture;

use super::{
    InvocationContext, NativeAppRuntime, NativeStreamEndpointBinding, RequestPermit,
    RuntimeFailure, await_with_generation_context, schedule_module_supervision_after_failure,
};

/// Static identity and Rust value types generated for one stream Capability.
pub trait StreamCapability: 'static {
    /// Typed request used to open one stream session.
    type OpenRequest: 'static;
    /// Typed message exchanged in both directions after opening.
    type Message: 'static;
    /// Typed Capability-defined terminal or opening error value.
    type DomainError: 'static;
    /// Stable Capability series identity.
    const ID: &'static str;
    /// Exact generated Descriptor version.
    const DESCRIPTOR_VERSION: &'static str;
}

/// One observable item received from a bidirectional stream.
#[derive(Clone, Debug, PartialEq)]
pub enum StreamEvent<M, E> {
    /// One ordered message from the remote side.
    Message(M),
    /// The remote side closed only its sending direction.
    PeerHalfClosed,
    /// The stream's one terminal outcome. Runtime failures use the outer `Result`.
    Terminal(Result<(), E>),
}

/// Type-erased stream item crossing the Kernel/Adapter seam.
pub enum NativeStreamItem {
    /// One generated message value.
    Message(Box<dyn Any>),
    /// The remote side closed its sending direction.
    PeerHalfClosed,
    /// The stream's one terminal success or Domain Error outcome.
    Terminal(Result<(), Box<dyn Any>>),
}

impl fmt::Debug for NativeStreamItem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(_) => formatter.write_str("Message(<erased>)"),
            Self::PeerHalfClosed => formatter.write_str("PeerHalfClosed"),
            Self::Terminal(Ok(())) => formatter.write_str("Terminal(Ok(()))"),
            Self::Terminal(Err(_)) => formatter.write_str("Terminal(Err(<erased>))"),
        }
    }
}

/// Adapter-owned bidirectional stream session.
pub trait NativeStreamSession: fmt::Debug {
    /// Sends one message, applying the Adapter's bounded admission policy.
    fn send(&self, message: Box<dyn Any>) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>>;
    /// Receives one message, half-close marker, or terminal outcome.
    fn receive(&self) -> LocalBoxFuture<'static, Result<NativeStreamItem, RuntimeFailure>>;
    /// Closes this side's sending direction without terminating the peer receive direction.
    fn close_send(&self) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>>;
    /// Cancels the session idempotently and prevents later delivery to application code.
    fn cancel(&self);
}

/// Adapter-facing endpoint for one or more bidirectional stream Operations.
pub trait NativeStreamEndpoint: fmt::Debug {
    /// Stable Capability series identity.
    fn capability_id(&self) -> &'static str;
    /// Exact Descriptor version implemented by this endpoint.
    fn descriptor_version(&self) -> &'static str;
    /// Exact stable stream Operation names implemented by this endpoint.
    fn operations(&self) -> &'static [&'static str];
    /// Opens one stream without serializing its typed Rust payload.
    fn open(
        &self,
        operation: &str,
        request: Box<dyn Any>,
        context: InvocationContext,
    ) -> LocalBoxFuture<
        'static,
        Result<Result<Box<dyn NativeStreamSession>, Box<dyn Any>>, RuntimeFailure>,
    >;
}

/// Typed, immutable stream endpoints materialized before App boot completes.
#[derive(Debug)]
pub struct NativeStreamHandle<C: StreamCapability> {
    endpoints: Vec<NativeStreamEndpointBinding>,
    runtime: Rc<NativeAppRuntime>,
    caller_instance: String,
    allow_before_ready: bool,
    capability: PhantomData<fn() -> C>,
}

impl<C: StreamCapability> NativeStreamHandle<C> {
    pub(crate) fn from_endpoints(
        endpoints: &[NativeStreamEndpointBinding],
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

    /// Opens one stream with a fresh invocation context.
    pub async fn open(
        &self,
        operation: &str,
        request: C::OpenRequest,
    ) -> Result<Result<NativeStream<C>, C::DomainError>, RuntimeFailure> {
        let context = self.next_context();
        self.open_with_context(operation, context, request).await
    }

    /// Opens one stream with an explicit propagated Invocation Context.
    pub async fn open_with_context(
        &self,
        operation: &str,
        context: InvocationContext,
        request: C::OpenRequest,
    ) -> Result<Result<NativeStream<C>, C::DomainError>, RuntimeFailure> {
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
        let admission = endpoint
            .admission(operation)
            .ok_or_else(|| RuntimeFailure::UnknownOperation {
                capability: C::ID,
                operation: operation.to_owned(),
            })?
            .clone();
        let permit = admission
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
        let generation_cancellation = snapshot.cancellation.clone();
        let outcome = await_with_generation_context(
            &self.runtime.driver,
            &context,
            snapshot.cancellation,
            C::ID,
            snapshot
                .endpoint
                .open(operation, Box::new(request), context.clone()),
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
        match outcome {
            Ok(session) => Ok(Ok(NativeStream::new(
                session,
                self.runtime.clone(),
                generation_cancellation,
                endpoint.module_instance.clone(),
                context,
                permit,
            ))),
            Err(error) => Ok(Err(error
                .downcast::<C::DomainError>()
                .map(|error| *error)
                .map_err(|_| RuntimeFailure::ProtocolViolation { capability: C::ID })?)),
        }
    }

    fn next_context(&self) -> InvocationContext {
        InvocationContext::new(
            self.next_request_id(),
            None,
            super::CancellationToken::new(),
        )
        .with_caller_instance(self.caller_instance.clone())
    }

    fn next_request_id(&self) -> super::RequestId {
        let request_id = self.runtime.request_ids.get();
        self.runtime.request_ids.set(request_id.saturating_add(1));
        request_id
    }
}

/// One opened, typed bidirectional stream session.
#[derive(Debug)]
pub struct NativeStream<C: StreamCapability> {
    inner: Rc<dyn NativeStreamSession>,
    runtime: Rc<NativeAppRuntime>,
    generation_cancellation: super::CancellationToken,
    module_instance: String,
    context: InvocationContext,
    _permit: RequestPermit,
    local_half_closed: Cell<bool>,
    peer_half_closed: Cell<bool>,
    terminal_seen: Cell<bool>,
    cancelled: Cell<bool>,
    capability: PhantomData<fn() -> C>,
}

impl<C: StreamCapability> NativeStream<C> {
    fn new(
        session: Box<dyn NativeStreamSession>,
        runtime: Rc<NativeAppRuntime>,
        generation_cancellation: super::CancellationToken,
        module_instance: String,
        context: InvocationContext,
        permit: RequestPermit,
    ) -> Self {
        Self {
            inner: Rc::from(session),
            runtime,
            generation_cancellation,
            module_instance,
            context,
            _permit: permit,
            local_half_closed: Cell::new(false),
            peer_half_closed: Cell::new(false),
            terminal_seen: Cell::new(false),
            cancelled: Cell::new(false),
            capability: PhantomData,
        }
    }

    /// Sends one typed message to the remote side.
    pub async fn send(&self, message: C::Message) -> Result<(), RuntimeFailure> {
        if let Some(error) = self.cancelled_outcome() {
            return Err(error);
        }
        if self.local_half_closed.get() || self.terminal_seen.get() {
            return Err(self.protocol_violation());
        }
        let inner = self.inner.clone();
        await_with_generation_context(
            &self.runtime.driver,
            &self.context,
            self.generation_cancellation.clone(),
            C::ID,
            inner.send(Box::new(message)),
        )
        .await
        .map_err(|error| self.finish_with_error(error))?
        .map_err(|error| self.finish_with_error(error))
    }

    /// Receives the next ordered event from the remote side.
    pub async fn receive(&self) -> Result<StreamEvent<C::Message, C::DomainError>, RuntimeFailure> {
        if let Some(error) = self.cancelled_outcome() {
            return Err(error);
        }
        if self.terminal_seen.get() {
            return Err(self.protocol_violation());
        }
        let inner = self.inner.clone();
        let item = await_with_generation_context(
            &self.runtime.driver,
            &self.context,
            self.generation_cancellation.clone(),
            C::ID,
            inner.receive(),
        )
        .await
        .map_err(|error| self.finish_with_error(error))?
        .map_err(|error| self.finish_with_error(error))?;
        match item {
            super::NativeStreamItem::Message(message) => {
                if self.peer_half_closed.get() {
                    return Err(self.finish_with_error(self.protocol_violation()));
                }
                message
                    .downcast::<C::Message>()
                    .map(|message| StreamEvent::Message(*message))
                    .map_err(|_| self.finish_with_error(self.protocol_violation()))
            }
            super::NativeStreamItem::PeerHalfClosed => {
                if self.peer_half_closed.replace(true) {
                    return Err(self.finish_with_error(self.protocol_violation()));
                }
                Ok(StreamEvent::PeerHalfClosed)
            }
            super::NativeStreamItem::Terminal(outcome) => {
                if self.terminal_seen.replace(true) {
                    return Err(self.finish_with_error(self.protocol_violation()));
                }
                let outcome = match outcome {
                    Ok(()) => Ok(()),
                    Err(error) => Err(error
                        .downcast::<C::DomainError>()
                        .map(|error| *error)
                        .map_err(|_| self.finish_with_error(self.protocol_violation()))?),
                };
                Ok(StreamEvent::Terminal(outcome))
            }
        }
    }

    /// Closes this side's sending direction while keeping receiving available.
    pub async fn close_send(&self) -> Result<(), RuntimeFailure> {
        if let Some(error) = self.cancelled_outcome() {
            return Err(error);
        }
        if self.terminal_seen.get() || self.local_half_closed.replace(true) {
            return Err(self.protocol_violation());
        }
        let inner = self.inner.clone();
        let result = await_with_generation_context(
            &self.runtime.driver,
            &self.context,
            self.generation_cancellation.clone(),
            C::ID,
            inner.close_send(),
        )
        .await
        .map_err(|error| self.finish_with_error(error))?
        .map_err(|error| self.finish_with_error(error));
        let resource_exhausted = result
            .as_ref()
            .err()
            .is_some_and(|error| matches!(error, RuntimeFailure::ResourceExhausted { .. }));
        if resource_exhausted {
            self.local_half_closed.set(false);
        }
        result
    }

    /// Cancels the stream idempotently. No later frame is delivered to the caller.
    pub fn cancel(&self) {
        if !self.terminal_seen.get() && !self.cancelled.replace(true) {
            self.context.cancellation().cancel();
            self.inner.cancel();
        }
    }

    /// Returns the propagated Kernel Request ID for this stream.
    pub const fn request_id(&self) -> super::RequestId {
        self.context.request_id()
    }

    fn protocol_violation(&self) -> RuntimeFailure {
        RuntimeFailure::ProtocolViolation { capability: C::ID }
    }

    fn cancelled_outcome(&self) -> Option<RuntimeFailure> {
        if !self.cancelled.get() {
            return None;
        }
        if self.terminal_seen.replace(true) {
            Some(self.protocol_violation())
        } else {
            Some(RuntimeFailure::Cancelled {
                request_id: self.context.request_id(),
            })
        }
    }

    fn schedule_failure(&self, error: RuntimeFailure) -> RuntimeFailure {
        schedule_module_supervision_after_failure(&self.runtime, &self.module_instance, error)
    }

    fn finish_with_error(&self, error: RuntimeFailure) -> RuntimeFailure {
        let error = self.schedule_failure(error);
        if !matches!(error, RuntimeFailure::ResourceExhausted { .. }) {
            self.terminal_seen.set(true);
            if !self.cancelled.replace(true) {
                self.context.cancellation().cancel();
                self.inner.cancel();
            }
        }
        error
    }
}

impl<C: StreamCapability> Drop for NativeStream<C> {
    fn drop(&mut self) {
        if !self.cancelled.replace(true) && !self.terminal_seen.get() {
            self.inner.cancel();
        }
    }
}

/// Alias using the transport-neutral term used by the Capability model.
pub type StreamSession<C> = NativeStream<C>;
