use std::time::Duration;

use super::{
    CancellationToken, EventCapability, InvocationContext, LocalBoxFuture, NativeAppRuntime,
    NativeEndpointBinding, NativeEventHandle, NativeRequestEndpoint, NativeRequestHandle,
    NativeStreamEndpointBinding, NativeStreamHandle, PluginEventDependencyHandle, Rc, RefCell,
    StreamCapability, Weak,
};

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

    /// Invokes one native endpoint using the most specific binding available.
    ///
    /// Generated bindings override this hook with a typed path. Older bindings retain the
    /// type-erased compatibility path without requiring regeneration.
    #[doc(hidden)]
    fn invoke_native(
        endpoint: &dyn NativeRequestEndpoint,
        operation: &str,
        request: Self::Request,
        context: InvocationContext,
    ) -> NativeRequestFuture<Self>
    where
        Self: Sized,
    {
        invoke_typed_or_erased_native_request::<Self>(endpoint, operation, request, context)
    }
}

/// A typed native request result before Kernel cancellation and supervision are applied.
#[doc(hidden)]
pub type NativeRequestFuture<C> = LocalBoxFuture<
    'static,
    Result<
        Result<<C as RequestCapability>::Response, <C as RequestCapability>::DomainError>,
        RuntimeFailure,
    >,
>;

type TypedNativeRequestFn<C> =
    dyn Fn(&str, <C as RequestCapability>::Request, InvocationContext) -> NativeRequestFuture<C>;

/// Runtime-provided typed endpoint used when a request crosses an execution boundary.
///
/// Generated in-process endpoints may expose a more specific endpoint type. This generic
/// carrier lets execution adapters preserve typed request, response, and domain-error values
/// without routing them through `Box<dyn Any>`.
#[doc(hidden)]
pub struct TypedNativeRequestEndpoint<C: RequestCapability> {
    invoke: Rc<TypedNativeRequestFn<C>>,
}

impl<C: RequestCapability> TypedNativeRequestEndpoint<C> {
    /// Creates a typed endpoint around one runtime-owned dispatcher.
    pub fn new(
        invoke: impl Fn(&str, C::Request, InvocationContext) -> NativeRequestFuture<C> + 'static,
    ) -> Self {
        Self {
            invoke: Rc::new(invoke),
        }
    }

    /// Dispatches one request without type erasure.
    pub fn invoke(
        &self,
        operation: &str,
        request: C::Request,
        context: InvocationContext,
    ) -> NativeRequestFuture<C> {
        (self.invoke)(operation, request, context)
    }
}

impl<C: RequestCapability> std::fmt::Debug for TypedNativeRequestEndpoint<C> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TypedNativeRequestEndpoint")
            .field("capability", &C::ID)
            .finish_non_exhaustive()
    }
}

/// Uses a runtime-provided typed endpoint when present, retaining erased compatibility.
#[doc(hidden)]
pub fn invoke_typed_or_erased_native_request<C: RequestCapability>(
    endpoint: &dyn NativeRequestEndpoint,
    operation: &str,
    request: C::Request,
    context: InvocationContext,
) -> NativeRequestFuture<C> {
    if let Some(endpoint) = endpoint
        .typed_endpoint()
        .and_then(|endpoint| endpoint.downcast_ref::<TypedNativeRequestEndpoint<C>>())
    {
        endpoint.invoke(operation, request, context)
    } else {
        invoke_erased_native_request::<C>(endpoint, operation, request, context)
    }
}

/// Compatibility dispatcher used by generated bindings when a typed endpoint is unavailable.
#[doc(hidden)]
pub fn invoke_erased_native_request<C: RequestCapability>(
    endpoint: &dyn NativeRequestEndpoint,
    operation: &str,
    request: C::Request,
    context: InvocationContext,
) -> NativeRequestFuture<C> {
    let invocation = endpoint.invoke(operation, Box::new(request), context);
    Box::pin(async move {
        match invocation.await? {
            Ok(value) => value
                .downcast::<C::Response>()
                .map(|value| Ok(*value))
                .map_err(|_| RuntimeFailure::ProtocolViolation { capability: C::ID }),
            Err(value) => value
                .downcast::<C::DomainError>()
                .map(|value| Err(*value))
                .map_err(|_| RuntimeFailure::ProtocolViolation { capability: C::ID }),
        }
    })
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
    MissingPluginFactory {
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
    /// A Plugin generation reported a failure that should trigger supervision.
    PluginFailure { detail: String },
    /// A Plugin Instance exhausted its finite restart budget.
    PluginRestartExhausted { instance: String, attempts: usize },
}

/// The lifecycle phase represented by a Plugin context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginLifecyclePhase {
    /// The Plugin may validate configuration and reserve reversible resources.
    Prepare,
    /// The Plugin may initialize against already prepared dependencies.
    Activate,
    /// The App Ready Gate has opened and externally triggered work may begin.
    Ready,
    /// The Plugin must release work and resources owned by this generation.
    Deactivate,
}

#[cfg(test)]
mod typed_endpoint_tests {
    use std::any::Any;

    use super::*;

    #[derive(Debug)]
    struct Echo;

    impl RequestCapability for Echo {
        type Request = u64;
        type Response = u64;
        type DomainError = ();
        const ID: &'static str = "test.echo@1";
        const DESCRIPTOR_VERSION: &'static str = "1.0.0";
    }

    #[derive(Debug)]
    struct Endpoint {
        typed: TypedNativeRequestEndpoint<Echo>,
    }

    impl NativeRequestEndpoint for Endpoint {
        fn capability_id(&self) -> &'static str {
            Echo::ID
        }

        fn descriptor_version(&self) -> &'static str {
            Echo::DESCRIPTOR_VERSION
        }

        fn operations(&self) -> &'static [&'static str] {
            &["echo"]
        }

        fn typed_endpoint(&self) -> Option<&dyn Any> {
            Some(&self.typed)
        }

        fn invoke(
            &self,
            _operation: &str,
            _request: Box<dyn Any>,
            _context: InvocationContext,
        ) -> LocalBoxFuture<'static, Result<crate::ErasedDomainResult, RuntimeFailure>> {
            panic!("typed dispatch must not call the erased endpoint")
        }
    }

    #[test]
    fn default_dispatch_uses_runtime_typed_endpoint() {
        let endpoint = Endpoint {
            typed: TypedNativeRequestEndpoint::new(|_, request, _| {
                Box::pin(futures::future::ready(Ok(Ok(request + 1))))
            }),
        };
        let context = InvocationContext::new(1, None, CancellationToken::new());

        let result =
            futures::executor::block_on(Echo::invoke_native(&endpoint, "echo", 41, context));

        assert_eq!(result, Ok(Ok(42)));
    }
}

/// A deterministic dependency visible to one Plugin Instance.
#[derive(Clone, Debug)]
pub struct PluginDependency {
    pub(super) capability_id: String,
    pub(super) provider_instance: String,
    pub(super) provider_order: usize,
    pub(super) handle: Option<PluginDependencyHandle>,
    pub(super) stream_handle: Option<PluginStreamDependencyHandle>,
    pub(super) event_handle: Option<PluginEventDependencyHandle>,
}

impl PluginDependency {
    pub(super) fn new(
        capability_id: impl Into<String>,
        provider_instance: impl Into<String>,
        provider_order: usize,
        handle: Option<PluginDependencyHandle>,
        stream_handle: Option<PluginStreamDependencyHandle>,
        event_handle: Option<PluginEventDependencyHandle>,
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
    pub fn handle(&self) -> Option<PluginDependencyHandle> {
        self.handle.clone()
    }

    /// Returns the resolved native stream endpoint handle when the Adapter supplied one.
    pub fn stream_handle(&self) -> Option<PluginStreamDependencyHandle> {
        self.stream_handle.clone()
    }

    /// Returns the resolved native Event endpoint handle when the Adapter supplied one.
    pub fn event_handle(&self) -> Option<PluginEventDependencyHandle> {
        self.event_handle.clone()
    }
}

/// An opaque, Adapter-resolved Capability endpoint passed to lifecycle code.
#[derive(Clone, Debug)]
pub struct PluginDependencyHandle {
    pub(super) binding: NativeEndpointBinding,
    pub(super) caller_instance: String,
    pub(super) runtime: Rc<RefCell<Weak<NativeAppRuntime>>>,
}

/// An opaque, Adapter-resolved stream Capability endpoint passed to lifecycle code.
#[derive(Clone, Debug)]
pub struct PluginStreamDependencyHandle {
    pub(super) binding: NativeStreamEndpointBinding,
    pub(super) caller_instance: String,
    pub(super) runtime: Rc<RefCell<Weak<NativeAppRuntime>>>,
}

impl PluginStreamDependencyHandle {
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

impl PluginDependencyHandle {
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

/// The explicit Capability dependencies available during Plugin lifecycle.
#[derive(Clone, Debug, Default)]
pub struct PluginDependencies {
    pub(super) bindings: Vec<PluginDependency>,
    pub(super) caller_instance: Rc<str>,
    pub(super) runtime: Rc<RefCell<Weak<NativeAppRuntime>>>,
}

impl PluginDependencies {
    pub(super) fn new(
        caller_instance: impl Into<String>,
        runtime: Rc<RefCell<Weak<NativeAppRuntime>>>,
    ) -> Self {
        Self {
            bindings: Vec::new(),
            caller_instance: Rc::from(caller_instance.into()),
            runtime,
        }
    }

    /// Returns dependencies in the order materialized by the Resolved App Plan.
    pub fn bindings(&self) -> &[PluginDependency] {
        &self.bindings
    }

    /// Returns the number of explicit dependencies.
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// Returns whether this Plugin has no explicit dependencies.
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// Creates a Kernel Invocation Context for work initiated by this Plugin.
    ///
    /// The request identity and monotonic deadline come from the same Runtime
    /// Driver as the App. The context is still owned by the caller and its
    /// cancellation token remains explicit.
    pub fn invocation_context(
        &self,
        deadline: Option<Duration>,
        cancellation: CancellationToken,
    ) -> Result<InvocationContext, RuntimeFailure> {
        let runtime = self
            .runtime
            .borrow()
            .upgrade()
            .ok_or(RuntimeFailure::AdmissionClosed)?;
        let request_id = runtime.request_ids.get();
        runtime.request_ids.set(request_id.saturating_add(1));
        Ok(InvocationContext::new(request_id, deadline, cancellation)
            .with_shared_caller_instance(self.caller_instance.clone()))
    }

    /// Creates a Plugin Invocation Context with a Driver-relative deadline.
    pub fn invocation_context_after(
        &self,
        timeout: Duration,
        cancellation: CancellationToken,
    ) -> Result<InvocationContext, RuntimeFailure> {
        let runtime = self
            .runtime
            .borrow()
            .upgrade()
            .ok_or(RuntimeFailure::AdmissionClosed)?;
        let deadline = (runtime.driver.now)().saturating_add(timeout);
        drop(runtime);
        self.invocation_context(Some(deadline), cancellation)
    }

    /// Returns the one explicitly bound typed dependency.
    pub fn one<C: RequestCapability>(&self) -> Result<NativeRequestHandle<C>, RuntimeFailure> {
        let handles: Vec<_> = self
            .bindings
            .iter()
            .filter(|binding| binding.capability_id() == C::ID)
            .filter_map(PluginDependency::handle)
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
            .filter_map(PluginDependency::handle)
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
            .filter_map(PluginDependency::handle)
            .map(|handle| handle.typed::<C>())
            .collect()
    }

    /// Returns the one explicitly bound typed stream dependency.
    pub fn one_stream<C: StreamCapability>(&self) -> Result<NativeStreamHandle<C>, RuntimeFailure> {
        let handles: Vec<_> = self
            .bindings
            .iter()
            .filter(|binding| binding.capability_id() == C::ID)
            .filter_map(PluginDependency::stream_handle)
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
            .filter_map(PluginDependency::stream_handle)
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
            .filter_map(PluginDependency::stream_handle)
            .map(|handle| handle.typed::<C>())
            .collect()
    }

    /// Returns one typed Event handle over every explicit binding in Plan order.
    pub fn many_event<C: EventCapability>(&self) -> Result<NativeEventHandle<C>, RuntimeFailure> {
        let handles: Vec<_> = self
            .bindings
            .iter()
            .filter(|binding| binding.capability_id() == C::ID)
            .filter_map(PluginDependency::event_handle)
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
            .filter_map(PluginDependency::event_handle)
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
            .filter_map(PluginDependency::event_handle)
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
