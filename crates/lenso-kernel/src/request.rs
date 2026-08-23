use std::time::Duration;

use super::{
    CancellationToken, EventCapability, InvocationContext, LocalBoxFuture,
    ModuleEventDependencyHandle, NativeAppRuntime, NativeEndpointBinding, NativeEventHandle,
    NativeRequestEndpoint, NativeRequestHandle, NativeStreamEndpointBinding, NativeStreamHandle,
    Rc, RefCell, StreamCapability, Weak,
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
        invoke_erased_native_request::<Self>(endpoint, operation, request, context)
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
    pub(super) capability_id: String,
    pub(super) provider_instance: String,
    pub(super) provider_order: usize,
    pub(super) handle: Option<ModuleDependencyHandle>,
    pub(super) stream_handle: Option<ModuleStreamDependencyHandle>,
    pub(super) event_handle: Option<ModuleEventDependencyHandle>,
}

impl ModuleDependency {
    pub(super) fn new(
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
    pub(super) binding: NativeEndpointBinding,
    pub(super) caller_instance: String,
    pub(super) runtime: Rc<RefCell<Weak<NativeAppRuntime>>>,
}

/// An opaque, Adapter-resolved stream Capability endpoint passed to lifecycle code.
#[derive(Clone, Debug)]
pub struct ModuleStreamDependencyHandle {
    pub(super) binding: NativeStreamEndpointBinding,
    pub(super) caller_instance: String,
    pub(super) runtime: Rc<RefCell<Weak<NativeAppRuntime>>>,
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
    pub(super) bindings: Vec<ModuleDependency>,
    pub(super) caller_instance: String,
    pub(super) runtime: Rc<RefCell<Weak<NativeAppRuntime>>>,
}

impl ModuleDependencies {
    pub(super) fn new(
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

    /// Creates a Kernel Invocation Context for work initiated by this Module.
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
            .with_caller_instance(self.caller_instance.clone()))
    }

    /// Creates a Module Invocation Context with a Driver-relative deadline.
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
