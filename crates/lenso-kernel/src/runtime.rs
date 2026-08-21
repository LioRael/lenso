use super::{
    AppAdmission, AppReadyGate, BTreeMap, CancellationToken, Cell, DriverControl, DriverTask,
    Duration, ErasedDomainResult, EventCapability, ExecutionAdapterCatalog, InvocationContext,
    LocalBoxFuture, ManagedResourceScope, ManagedTask, ManagedTaskScope, ModuleCriticality,
    ModuleDependencies, ModuleLifecycle, NativeEventBindingTable, NativeEventEndpointStateTable,
    NativeEventHandle, NativeRequestEndpoint, NativeStreamBindingTable, NativeStreamEndpoint,
    NativeStreamEndpointStateTable, NativeStreamHandle, PhantomData, Rc, RefCell, RequestAdmission,
    RequestCapability, RequestId, ResolvedAppPlan, RestartPolicy, RuntimeFailure, ShutdownOutcome,
    StreamCapability, await_with_generation_context, begin_module_supervision,
    ensure_context_active, event, handle_supervision_schedule_failure, oneshot,
    schedule_module_supervision, schedule_module_supervision_after_failure,
    shutdown_native_modules,
};

#[derive(Clone, Debug)]
pub(super) struct NativeEndpointSnapshot {
    pub(super) endpoint: Rc<dyn NativeRequestEndpoint>,
    pub(super) generation: u64,
    pub(super) cancellation: CancellationToken,
}

#[derive(Debug)]
pub(super) struct NativeEndpointState {
    pub(super) capability_id: &'static str,
    pub(super) descriptor_version: &'static str,
    pub(super) operations: &'static [&'static str],
    pub(super) endpoint: RefCell<Option<Rc<dyn NativeRequestEndpoint>>>,
    pub(super) generation: Cell<u64>,
    pub(super) cancellation: RefCell<CancellationToken>,
}

#[derive(Clone, Debug)]
pub(crate) struct NativeStreamEndpointSnapshot {
    pub(crate) endpoint: Rc<dyn NativeStreamEndpoint>,
    pub(crate) generation: u64,
    pub(crate) cancellation: CancellationToken,
}

#[derive(Debug)]
pub(crate) struct NativeStreamEndpointState {
    pub(super) capability_id: &'static str,
    pub(super) descriptor_version: &'static str,
    pub(super) operations: &'static [&'static str],
    pub(super) endpoint: RefCell<Option<Rc<dyn NativeStreamEndpoint>>>,
    pub(super) generation: Cell<u64>,
    pub(super) cancellation: RefCell<CancellationToken>,
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
    pub(super) fn new(endpoint: Rc<dyn NativeRequestEndpoint>, generation: u64) -> Self {
        Self {
            capability_id: endpoint.capability_id(),
            descriptor_version: endpoint.descriptor_version(),
            operations: endpoint.operations(),
            endpoint: RefCell::new(Some(endpoint)),
            generation: Cell::new(generation),
            cancellation: RefCell::new(CancellationToken::new()),
        }
    }

    pub(super) fn snapshot(&self) -> Option<NativeEndpointSnapshot> {
        self.endpoint
            .borrow()
            .clone()
            .map(|endpoint| NativeEndpointSnapshot {
                endpoint,
                generation: self.generation.get(),
                cancellation: self.cancellation.borrow().clone(),
            })
    }

    pub(super) fn mark_unavailable(&self) {
        self.cancellation.borrow().cancel();
        self.endpoint.borrow_mut().take();
    }

    pub(super) fn install(&self, endpoint: Rc<dyn NativeRequestEndpoint>, generation: u64) {
        self.generation.set(generation);
        self.cancellation.replace(CancellationToken::new());
        self.endpoint.replace(Some(endpoint));
    }

    pub(super) fn is_current(&self, generation: u64) -> bool {
        self.generation.get() == generation && self.endpoint.borrow().is_some()
    }
}

#[derive(Clone, Debug)]
pub(super) struct NativeEndpointBinding {
    pub(super) module_instance: String,
    pub(super) state: Rc<NativeEndpointState>,
    pub(super) admissions: BTreeMap<String, RequestAdmission>,
}

impl NativeEndpointBinding {
    pub(super) fn admission(&self, operation: &str) -> Option<&RequestAdmission> {
        self.admissions.get(operation)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct NativeStreamEndpointBinding {
    pub(crate) module_instance: String,
    pub(crate) state: Rc<NativeStreamEndpointState>,
    pub(super) admissions: BTreeMap<String, RequestAdmission>,
}

impl NativeStreamEndpointBinding {
    pub(crate) fn admission(&self, operation: &str) -> Option<&RequestAdmission> {
        self.admissions.get(operation)
    }
}

#[derive(Debug)]
pub(super) struct NativeModuleGeneration {
    pub(super) lifecycle: Rc<dyn ModuleLifecycle>,
    pub(super) tasks: ManagedTaskScope,
    pub(super) resources: ManagedResourceScope,
}

pub(super) enum GenerationPreparationFailure {
    Lifecycle,
    Cleanup(RuntimeFailure),
}

#[derive(Debug)]
pub(super) struct NativeModuleRuntime {
    pub(super) generation: RefCell<Option<NativeModuleGeneration>>,
}

impl NativeModuleRuntime {
    pub(super) fn take_generation(&self) -> Option<NativeModuleGeneration> {
        self.generation.borrow_mut().take()
    }

    pub(super) fn install_generation(&self, generation: NativeModuleGeneration) {
        debug_assert!(self.generation.borrow().is_none());
        self.generation.replace(Some(generation));
    }

    pub(super) fn generation_parts(
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
pub(super) struct ModuleSupervision {
    pub(super) policy: RestartPolicy,
    pub(super) criticality: ModuleCriticality,
    pub(super) required_path: bool,
    pub(super) generation: u64,
    pub(super) attempts: Vec<Duration>,
    pub(super) stable_since: Option<Duration>,
    pub(super) restarting: bool,
}

#[derive(Debug, Default)]
pub(super) struct ShutdownCoordinator {
    pub(super) started: Cell<bool>,
    pub(super) outcome: RefCell<Option<ShutdownOutcome>>,
    pub(super) waiters: RefCell<Vec<oneshot::Sender<ShutdownOutcome>>>,
}

impl ShutdownCoordinator {
    pub(super) fn start(&self) -> bool {
        !self.started.replace(true)
    }

    pub(super) fn complete(&self, outcome: &ShutdownOutcome) {
        if self.outcome.borrow().is_some() {
            return;
        }
        self.outcome.replace(Some(outcome.clone()));
        for waiter in self.waiters.borrow_mut().drain(..) {
            let _ = waiter.send(outcome.clone());
        }
    }

    pub(super) fn wait(&self) -> LocalBoxFuture<'static, ShutdownOutcome> {
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

pub(super) struct NativeAppRuntime {
    pub(super) plan: ResolvedAppPlan,
    pub(super) adapters: Rc<ExecutionAdapterCatalog>,
    pub(super) modules: BTreeMap<String, NativeModuleRuntime>,
    pub(super) dependencies: BTreeMap<String, ModuleDependencies>,
    pub(super) endpoint_states: BTreeMap<(String, String), Rc<NativeEndpointState>>,
    pub(super) stream_endpoint_states: NativeStreamEndpointStateTable,
    pub(super) event_endpoint_states: NativeEventEndpointStateTable,
    pub(super) supervision: RefCell<BTreeMap<String, ModuleSupervision>>,
    pub(super) supervision_tasks: RefCell<BTreeMap<String, ManagedTask>>,
    pub(super) activation_order: Vec<String>,
    pub(super) ready_gate: AppReadyGate,
    pub(super) admission: AppAdmission,
    pub(super) driver: DriverControl,
    pub(super) request_ids: Rc<Cell<RequestId>>,
    pub(super) supervision_cancellation: CancellationToken,
    pub(super) shutdown_started: Cell<bool>,
    pub(super) shutdown: ShutdownCoordinator,
    pub(super) shutdown_task: RefCell<Option<DriverTask>>,
    pub(super) terminal_failure: RefCell<Option<RuntimeFailure>>,
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
    pub(super) fn begin_shutdown(&self) {
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
    pub(super) bindings: BTreeMap<(String, &'static str), Vec<NativeEndpointBinding>>,
    pub(super) stream_bindings: NativeStreamBindingTable,
    pub(super) event_bindings: NativeEventBindingTable,
    pub(super) runtime: Rc<NativeAppRuntime>,
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

    pub(super) fn next_request_id(&self) -> RequestId {
        let request_id = self.runtime.request_ids.get();
        self.runtime.request_ids.set(request_id.saturating_add(1));
        request_id
    }

    pub(super) fn endpoints<C: RequestCapability>(
        &self,
        caller_instance: &str,
    ) -> Option<&[NativeEndpointBinding]> {
        self.bindings
            .get(&(caller_instance.to_owned(), C::ID))
            .map(Vec::as_slice)
    }

    pub(super) fn stream_endpoints<C: StreamCapability>(
        &self,
        caller_instance: &str,
    ) -> Option<&[NativeStreamEndpointBinding]> {
        self.stream_bindings
            .get(&(caller_instance.to_owned(), C::ID))
            .map(Vec::as_slice)
    }

    pub(super) fn event_endpoints<C: EventCapability>(
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
    pub(super) endpoints: Vec<NativeEndpointBinding>,
    pub(super) runtime: Rc<NativeAppRuntime>,
    pub(super) caller_instance: String,
    pub(super) allow_before_ready: bool,
    pub(super) capability: PhantomData<fn() -> C>,
}

impl<C: RequestCapability> NativeRequestHandle<C> {
    pub(super) fn from_endpoints(
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

    pub(super) fn next_context(&self) -> InvocationContext {
        self.invocation_context(None, CancellationToken::new())
            .with_caller_instance(self.caller_instance.clone())
    }

    pub(super) fn next_request_id(&self) -> RequestId {
        let request_id = self.runtime.request_ids.get();
        self.runtime.request_ids.set(request_id.saturating_add(1));
        request_id
    }
}

pub(super) fn decode_outcome<C: RequestCapability>(
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
