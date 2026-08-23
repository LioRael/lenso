use std::{marker::PhantomData, rc::Rc, time::Duration};

use super::{
    CancellationToken, DiagnosticAdmission, DiagnosticEvent, DiagnosticOutcome, DiagnosticSource,
    InvocationContext, NativeAppRuntime, NativeEndpointBinding, RequestCapability, RequestId,
    RuntimeFailure, await_with_generation_context, diagnostics::diagnostic_operation,
    ensure_context_active, schedule_module_supervision_after_failure,
};

/// Typed, immutable native Capability endpoints materialized before App boot completes.
#[derive(Debug)]
pub struct NativeRequestHandle<C: RequestCapability> {
    pub(super) endpoints: Vec<NativeEndpointBinding>,
    pub(super) runtime: Rc<NativeAppRuntime>,
    pub(super) caller_instance: String,
    pub(super) caller_is_planned: bool,
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
        let caller_is_planned = runtime.plan.module_instance(caller_instance).is_some();
        Self {
            endpoints: endpoints.to_vec(),
            runtime,
            caller_instance: caller_instance.to_owned(),
            caller_is_planned,
            allow_before_ready,
            capability: PhantomData,
        }
    }

    /// Returns the number of provider endpoints captured by this handle.
    pub fn binding_count(&self) -> usize {
        self.endpoints.len()
    }

    fn diagnostic_caller_instance(&self) -> Option<String> {
        self.caller_is_planned.then(|| self.caller_instance.clone())
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
        let context = context
            .for_caller(&self.caller_instance)
            .for_target(C::ID, operation);
        let invocation_diagnostics = self
            .runtime
            .diagnostics
            .has_interested_observer(DiagnosticSource::Invocation);
        let started_at = invocation_diagnostics.then(|| (self.runtime.driver.now)());
        let operation_name = invocation_diagnostics
            .then(|| {
                self.endpoints
                    .first()
                    .and_then(|endpoint| diagnostic_operation(endpoint.state.operations, operation))
            })
            .flatten();
        if let Some(started_at) = started_at {
            self.runtime
                .diagnostics
                .emit(DiagnosticSource::Invocation, started_at, |_| {
                    DiagnosticEvent::InvocationStarted {
                        request_id: context.request_id(),
                        caller_instance: self.diagnostic_caller_instance(),
                        provider_instance: self
                            .endpoints
                            .first()
                            .map(|endpoint| endpoint.module_instance.clone()),
                        capability: C::ID,
                        operation: operation_name,
                    }
                });
        }
        let request_id = context.request_id();
        let result = self
            .invoke_with_context_inner(operation, context, request)
            .await;
        let outcome = request_diagnostic_outcome(&result);
        if let Some(started_at) = started_at {
            let completed_at = (self.runtime.driver.now)();
            self.runtime
                .diagnostics
                .emit(DiagnosticSource::Invocation, completed_at, |_| {
                    DiagnosticEvent::InvocationCompleted {
                        request_id,
                        caller_instance: self.diagnostic_caller_instance(),
                        provider_instance: self
                            .endpoints
                            .first()
                            .map(|endpoint| endpoint.module_instance.clone()),
                        capability: C::ID,
                        operation: operation_name,
                        outcome,
                        elapsed: completed_at.saturating_sub(started_at),
                    }
                });
        }
        if let Err(error) = &result {
            self.runtime.diagnostics.emit_runtime_failure(
                (self.runtime.driver.now)(),
                self.endpoints
                    .first()
                    .map(|endpoint| endpoint.module_instance.as_str()),
                error,
            );
            if let Some(admission) = diagnostic_admission(error) {
                self.runtime.diagnostics.emit(
                    DiagnosticSource::Admission,
                    (self.runtime.driver.now)(),
                    |_| DiagnosticEvent::AdmissionRejected {
                        request_id,
                        caller_instance: self.diagnostic_caller_instance(),
                        provider_instance: self
                            .endpoints
                            .first()
                            .map(|endpoint| endpoint.module_instance.clone()),
                        capability: C::ID,
                        operation: operation_name,
                        outcome: admission,
                    },
                );
            }
        }
        result
    }

    async fn invoke_with_context_inner(
        &self,
        operation: &str,
        context: InvocationContext,
        request: C::Request,
    ) -> Result<Result<C::Response, C::DomainError>, RuntimeFailure> {
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
            .acquire(C::ID, operation, &context, &self.runtime.driver)
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
            C::invoke_native(
                snapshot.endpoint.as_ref(),
                operation,
                request,
                context.clone(),
            ),
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
        Ok(outcome)
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
        let context = context.for_caller(&self.caller_instance);
        let invocation_diagnostics = self
            .runtime
            .diagnostics
            .has_interested_observer(DiagnosticSource::Invocation);
        let started_at = invocation_diagnostics.then(|| (self.runtime.driver.now)());
        let operation_name = invocation_diagnostics
            .then(|| {
                self.endpoints
                    .first()
                    .and_then(|endpoint| diagnostic_operation(endpoint.state.operations, operation))
            })
            .flatten();
        let request_id = context.request_id();
        if let Some(started_at) = started_at {
            self.runtime
                .diagnostics
                .emit(DiagnosticSource::Invocation, started_at, |_| {
                    DiagnosticEvent::InvocationStarted {
                        request_id,
                        caller_instance: self.diagnostic_caller_instance(),
                        provider_instance: None,
                        capability: C::ID,
                        operation: operation_name,
                    }
                });
        }
        let result = self
            .invoke_many_with_context_inner(operation, context, request)
            .await;
        let outcome = many_request_diagnostic_outcome(&result);
        if let Some(started_at) = started_at {
            let completed_at = (self.runtime.driver.now)();
            self.runtime
                .diagnostics
                .emit(DiagnosticSource::Invocation, completed_at, |_| {
                    DiagnosticEvent::InvocationCompleted {
                        request_id,
                        caller_instance: self.diagnostic_caller_instance(),
                        provider_instance: None,
                        capability: C::ID,
                        operation: operation_name,
                        outcome,
                        elapsed: completed_at.saturating_sub(started_at),
                    }
                });
        }
        if let Err(error) = &result {
            self.runtime
                .diagnostics
                .emit_runtime_failure((self.runtime.driver.now)(), None, error);
            if let Some(admission) = diagnostic_admission(error) {
                self.runtime.diagnostics.emit(
                    DiagnosticSource::Admission,
                    (self.runtime.driver.now)(),
                    |_| DiagnosticEvent::AdmissionRejected {
                        request_id,
                        caller_instance: self.diagnostic_caller_instance(),
                        provider_instance: None,
                        capability: C::ID,
                        operation: operation_name,
                        outcome: admission,
                    },
                );
            }
        }
        result
    }

    async fn invoke_many_with_context_inner(
        &self,
        operation: &str,
        context: InvocationContext,
        request: C::Request,
    ) -> Result<Vec<Result<C::Response, C::DomainError>>, RuntimeFailure>
    where
        C::Request: Clone,
    {
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
                .acquire(C::ID, operation, &context, &self.runtime.driver)
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
                C::invoke_native(
                    snapshot.endpoint.as_ref(),
                    operation,
                    request.clone(),
                    context.clone(),
                ),
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
            outcomes.push(outcome);
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

fn request_diagnostic_outcome<Response, DomainError>(
    result: &Result<Result<Response, DomainError>, RuntimeFailure>,
) -> DiagnosticOutcome {
    match result {
        Ok(Ok(_)) => DiagnosticOutcome::Succeeded,
        Ok(Err(_)) => DiagnosticOutcome::DomainError,
        Err(error) => DiagnosticOutcome::RuntimeFailure(error.into()),
    }
}

fn many_request_diagnostic_outcome<Response, DomainError>(
    result: &Result<Vec<Result<Response, DomainError>>, RuntimeFailure>,
) -> DiagnosticOutcome {
    match result {
        Ok(outcomes) if outcomes.iter().any(Result::is_err) => DiagnosticOutcome::DomainError,
        Ok(_) => DiagnosticOutcome::Succeeded,
        Err(error) => DiagnosticOutcome::RuntimeFailure(error.into()),
    }
}

fn diagnostic_admission(error: &RuntimeFailure) -> Option<DiagnosticAdmission> {
    match error {
        RuntimeFailure::AdmissionClosed => Some(DiagnosticAdmission::Closed),
        RuntimeFailure::ResourceExhausted { .. } => Some(DiagnosticAdmission::Exhausted),
        RuntimeFailure::Unavailable { .. } => Some(DiagnosticAdmission::Unavailable),
        _ => None,
    }
}
