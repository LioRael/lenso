//! Execution ownership outlives caller interest and never guesses termination.

use super::driver::RequestPermit;
use super::{
    CancellationToken, DriverControl, InvocationContext, LocalBoxFuture, RuntimeFailure,
    ensure_context_active,
};
use futures::{FutureExt, channel::oneshot};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::{Future, poll_fn},
    rc::Rc,
    task::Poll,
};

#[derive(Default, Debug)]
pub(super) struct ExecutionLedger {
    next_id: Cell<u64>,
    entries: RefCell<BTreeMap<u64, ExecutionEntry>>,
    provider_admissions: RefCell<BTreeMap<(String, String, String), super::RequestAdmission>>,
}

#[allow(
    clippy::too_many_arguments,
    reason = "request execution carries explicit admission and generation context"
)]
pub(super) async fn request<T: 'static>(
    runtime: &super::NativeAppRuntime,
    provider: &str,
    operation: &str,
    context: &InvocationContext,
    generation: CancellationToken,
    capability: &'static str,
    permit: RequestPermit,
    invoke: impl FnOnce(InvocationContext) -> LocalBoxFuture<'static, T>,
) -> Result<T, RuntimeFailure> {
    let instance = runtime
        .plan
        .plugin_instance(provider)
        .expect("prepared endpoint has a planned provider");
    let named_caller = context
        .caller_instance()
        .and_then(|caller| runtime.plan.plugin_instance(caller))
        .is_some_and(|caller| caller.authoring_version() == 2);
    if instance.authoring_version() == 1 && !named_caller {
        let _permit = permit;
        return super::await_with_generation_context(
            &runtime.driver,
            context,
            generation,
            capability,
            invoke(context.clone()),
        )
        .await;
    }
    let limits = instance
        .provided_capabilities()
        .iter()
        .find(|endpoint| endpoint.capability_id() == capability)
        .and_then(|endpoint| endpoint.operation_admission(operation))
        .unwrap_or_default();
    let aggregate = runtime
        .executions
        .provider_admissions
        .borrow_mut()
        .entry((
            provider.to_owned(),
            capability.to_owned(),
            operation.to_owned(),
        ))
        .or_insert_with(|| super::RequestAdmission::new(limits))
        .clone();
    let provider_permit = aggregate
        .acquire(capability, operation, context, &runtime.driver)
        .await?;
    if instance.authoring_version() == 1 {
        let _permits = (permit, provider_permit);
        return super::await_with_generation_context(
            &runtime.driver,
            context,
            generation,
            capability,
            invoke(context.clone()),
        )
        .await;
    }
    execute(
        runtime.executions.clone(),
        &runtime.driver,
        provider,
        context,
        generation,
        capability,
        vec![permit, provider_permit],
        invoke,
    )
    .await
}

/// Executes one non-request Adapter operation under the same Driver-owned
/// settlement rules as authoring-version-2 requests.
pub(super) async fn operation<T: 'static>(
    runtime: &super::NativeAppRuntime,
    provider: &str,
    context: &InvocationContext,
    generation: CancellationToken,
    capability: &'static str,
    invoke: impl FnOnce(InvocationContext) -> LocalBoxFuture<'static, T>,
) -> Result<T, RuntimeFailure> {
    let instance = runtime
        .plan
        .plugin_instance(provider)
        .expect("prepared endpoint has a planned provider");
    if instance.authoring_version() == 1 {
        return super::await_with_generation_context(
            &runtime.driver,
            context,
            generation,
            capability,
            invoke(context.clone()),
        )
        .await;
    }
    execute(
        runtime.executions.clone(),
        &runtime.driver,
        provider,
        context,
        generation,
        capability,
        vec![],
        invoke,
    )
    .await
}

#[derive(Debug)]
struct ExecutionEntry {
    outstanding: usize,
    provider: String,
    // Only observed execution completion removes the entry and releases these.
    // A Driver dropping a Future leaves the entry uncertain and capacity held.
    _permits: Vec<RequestPermit>,
}

impl ExecutionLedger {
    pub(super) fn is_settled(&self, provider: Option<&str>) -> bool {
        !self
            .entries
            .borrow()
            .values()
            .any(|entry| provider.is_none_or(|provider| entry.provider == provider))
    }

    fn admit(&self, provider: &str, permits: Vec<RequestPermit>) -> Result<u64, RuntimeFailure> {
        let mut entries = self.entries.borrow_mut();
        let mut candidate = self.next_id.get();
        let mut available = None;
        for _ in 0..=entries.len() {
            if !entries.contains_key(&candidate) {
                available = Some(candidate);
                break;
            }
            candidate = candidate.wrapping_add(1);
        }
        let id = available.ok_or(RuntimeFailure::AdmissionClosed)?;
        self.next_id.set(id.wrapping_add(1));
        entries.insert(
            id,
            ExecutionEntry {
                outstanding: 1,
                provider: provider.to_owned(),
                _permits: permits,
            },
        );
        Ok(id)
    }

    fn settle(&self, id: u64) {
        let mut entries = self.entries.borrow_mut();
        if let Some(entry) = entries.get_mut(&id) {
            entry.outstanding -= 1;
            if entry.outstanding == 0 {
                entries.remove(&id);
            }
        }
    }
}

/// Proof that Adapter-managed work has actually ended. Dropping the token,
/// acknowledging cancellation, or disconnecting does not settle execution.
#[derive(Debug)]
#[must_use = "call settle only after retained execution has actually terminated"]
pub struct ExecutionLease {
    scope: ExecutionScope,
}

impl ExecutionLease {
    /// Reports observed termination after retained resources are safe.
    pub fn settle(self) {
        self.scope.ledger.settle(self.scope.id);
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ExecutionScope {
    ledger: Rc<ExecutionLedger>,
    id: u64,
}

impl ExecutionScope {
    pub(crate) fn retain(&self) -> Result<ExecutionLease, RuntimeFailure> {
        let mut entries = self.ledger.entries.borrow_mut();
        let entry = entries
            .get_mut(&self.id)
            .ok_or(RuntimeFailure::AdmissionClosed)?;
        entry.outstanding = entry
            .outstanding
            .checked_add(1)
            .ok_or(RuntimeFailure::AdmissionClosed)?;
        Ok(ExecutionLease {
            scope: self.clone(),
        })
    }
}

/// The fast path polls inline; pending work is transferred to the Driver before
/// this Future can yield. Dropping the caller never drops its execution owner.
#[allow(
    clippy::too_many_arguments,
    reason = "explicit execution ownership transfer"
)]
pub(super) async fn execute<T: 'static>(
    ledger: Rc<ExecutionLedger>,
    driver: &DriverControl,
    provider: &str,
    context: &InvocationContext,
    generation: CancellationToken,
    capability: &'static str,
    permits: Vec<RequestPermit>,
    invoke: impl FnOnce(InvocationContext) -> LocalBoxFuture<'static, T>,
) -> Result<T, RuntimeFailure> {
    ensure_context_active(driver, context)?;
    if generation.is_cancelled() {
        return Err(RuntimeFailure::Unavailable { capability });
    }
    let id = ledger.admit(provider, permits)?;
    let mut execution_context = context.clone();
    execution_context.execution = Some(ExecutionScope {
        ledger: ledger.clone(),
        id,
    });
    execution_context.remaining_budget = context
        .deadline()
        .map(|deadline| deadline.saturating_sub((driver.now)()));
    let mut future = invoke(execution_context.clone());
    let ready = poll_fn(|cx| {
        Poll::Ready(match future.as_mut().poll(cx) {
            Poll::Ready(output) => Some(output),
            Poll::Pending => None,
        })
    })
    .await;
    if let Some(output) = ready {
        ledger.settle(id);
        ensure_context_active(driver, context)?;
        if generation.is_cancelled() {
            return Err(RuntimeFailure::Unavailable { capability });
        }
        return Ok(output);
    }
    let (sender, mut receiver) = oneshot::channel();
    let execution_driver = driver.clone();
    let execution_generation = generation.clone();
    (driver.spawn_local)(Box::pin(async move {
        let output = future.await;
        let result = ensure_context_active(&execution_driver, &execution_context).and_then(|()| {
            if execution_generation.is_cancelled() {
                Err(RuntimeFailure::Unavailable { capability })
            } else {
                Ok(output)
            }
        });
        ledger.settle(id);
        // A result accepted here remains final even if the waiter is polled later.
        let _ = sender.send(result);
    }))
    .map_err(|error| RuntimeFailure::Internal {
        detail: format!("cannot schedule execution owner: {error}"),
    })?;
    let mut cancelled = context.cancellation.cancelled().boxed_local();
    let mut generation_cancelled = generation.cancelled().boxed_local();
    let mut deadline = context.deadline().map_or_else(
        || futures::future::pending().boxed_local(),
        |deadline| (driver.sleep_until)(deadline),
    );
    poll_fn(|cx| {
        // A previously accepted terminal result wins over subsequent cancellation.
        if let Poll::Ready(result) = std::pin::Pin::new(&mut receiver).poll(cx) {
            return Poll::Ready(result.unwrap_or_else(|_| {
                Err(RuntimeFailure::Internal {
                    detail: "execution owner ended without settlement".to_owned(),
                })
            }));
        }
        let _ = cancelled.as_mut().poll(cx);
        let _ = deadline.as_mut().poll(cx);
        if let Err(error) = ensure_context_active(driver, context) {
            return Poll::Ready(Err(error));
        }
        if generation_cancelled.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(RuntimeFailure::Unavailable { capability }));
        }
        Poll::Pending
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DeterministicDriver, RequestAdmission, RequestAdmissionPlan, RuntimeDriver};
    use std::time::Duration;

    #[test]
    fn cancelled_waiter_retains_execution_and_capacity_until_work_really_finishes() {
        let driver = DeterministicDriver::new();
        let control = DriverControl::new(&driver);
        let ledger = Rc::new(ExecutionLedger::default());
        let cancellation = CancellationToken::new();
        let context = InvocationContext::new(1, None, cancellation.clone());
        let admission = RequestAdmission::new(RequestAdmissionPlan::new(0, 1));
        let permit = admission
            .try_acquire("test", "work", &context, &control)
            .unwrap();
        let (finish, work) = oneshot::channel::<()>();
        let execution = execute(
            ledger.clone(),
            &control,
            "provider",
            &context,
            CancellationToken::new(),
            "test",
            vec![permit],
            |_| work.boxed_local(),
        );
        driver.run(async {
            futures::pin_mut!(execution);
            assert!(execution.as_mut().now_or_never().is_none());
            cancellation.cancel();
            assert!(matches!(
                execution.await,
                Err(RuntimeFailure::Cancelled { request_id: 1 })
            ));
        });
        assert!(!ledger.is_settled(Some("provider")));
        let new_context = InvocationContext::new(2, None, CancellationToken::new());
        for _ in 0..32 {
            assert!(matches!(
                admission.try_acquire("test", "work", &new_context, &control),
                Err(RuntimeFailure::ResourceExhausted { .. })
            ));
        }
        assert_eq!(ledger.entries.borrow().len(), 1);
        finish.send(()).unwrap();
        driver.run(driver.yield_now());
        assert!(ledger.is_settled(None));
        assert!(
            admission
                .try_acquire("test", "work", &new_context, &control)
                .is_ok()
        );
    }

    #[test]
    fn dropped_waiter_does_not_drop_the_execution_owner() {
        let driver = DeterministicDriver::new();
        let control = DriverControl::new(&driver);
        let ledger = Rc::new(ExecutionLedger::default());
        let context = InvocationContext::new(1, None, CancellationToken::new());
        let (finish, work) = oneshot::channel::<()>();
        assert!(
            execute(
                ledger.clone(),
                &control,
                "provider",
                &context,
                CancellationToken::new(),
                "test",
                vec![],
                |_| work.boxed_local()
            )
            .now_or_never()
            .is_none()
        );
        assert!(!ledger.is_settled(None));
        finish.send(()).unwrap();
        driver.run(driver.yield_now());
        assert!(ledger.is_settled(None));
    }

    #[test]
    fn cancellation_precedes_inclusive_deadline_and_same_poll_completion() {
        for cancel in [false, true] {
            let driver = DeterministicDriver::new();
            let control = DriverControl::new(&driver);
            let cancellation = CancellationToken::new();
            let context =
                InvocationContext::new(1, Some(Duration::from_secs(1)), cancellation.clone());
            let worker_driver = driver.clone();
            let work = async move {
                worker_driver.advance(Duration::from_secs(1));
                if cancel {
                    cancellation.cancel();
                }
                42
            }
            .boxed_local();
            let result = driver.run(execute(
                Rc::default(),
                &control,
                "provider",
                &context,
                CancellationToken::new(),
                "test",
                vec![],
                |_| work,
            ));
            assert_eq!(
                result,
                Err(if cancel {
                    RuntimeFailure::Cancelled { request_id: 1 }
                } else {
                    RuntimeFailure::DeadlineExceeded { request_id: 1 }
                })
            );
        }
    }

    #[test]
    fn provider_context_carries_a_relative_dispatch_budget() {
        let driver = DeterministicDriver::new();
        let control = DriverControl::new(&driver);
        driver.advance(Duration::from_millis(250));
        let context =
            InvocationContext::new(1, Some(Duration::from_secs(1)), CancellationToken::new());

        let remaining = driver
            .run(execute(
                Rc::default(),
                &control,
                "provider",
                &context,
                CancellationToken::new(),
                "test",
                vec![],
                |context| futures::future::ready(context.remaining_budget()).boxed_local(),
            ))
            .unwrap();

        assert_eq!(remaining, Some(Duration::from_millis(750)));
        assert_eq!(context.remaining_budget(), None);
    }

    #[test]
    fn an_already_accepted_success_survives_later_cancellation() {
        let driver = DeterministicDriver::new();
        let control = DriverControl::new(&driver);
        let cancellation = CancellationToken::new();
        let context = InvocationContext::new(1, None, cancellation.clone());
        let (finish, work) = oneshot::channel::<u32>();
        let execution = execute(
            Rc::default(),
            &control,
            "provider",
            &context,
            CancellationToken::new(),
            "test",
            vec![],
            |_| work.boxed_local(),
        );
        futures::pin_mut!(execution);
        assert!(execution.as_mut().now_or_never().is_none());
        finish.send(42).unwrap();
        driver.run(driver.yield_now());
        cancellation.cancel();
        assert_eq!(driver.run(execution), Ok(Ok(42)));
    }
}
