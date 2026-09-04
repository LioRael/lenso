//! Driver-owned startup and late-result cleanup for authoring version 2.

use super::{
    ExecutionAdapterCatalog, InvocationContext, Kernel, NativeApp, RuntimeDiagnostics,
    RuntimeDriver, RuntimeFailure, await_with_context,
};
use futures::channel::oneshot;
use std::time::Duration;

pub(super) const DEFAULT_STARTUP_CLEANUP_TIMEOUT: Duration = Duration::from_secs(30);

struct CancelOnDrop {
    cancellation: super::CancellationToken,
    cleanup: super::cleanup::StartupCleanupBudget,
    startup_deadline: Option<Duration>,
    driver: super::DriverControl,
    armed: bool,
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if self.armed {
            let now = (self.driver.now)();
            let cleanup_started_at = self
                .startup_deadline
                .filter(|deadline| now >= *deadline)
                .unwrap_or(now);
            self.cleanup.establish_at(cleanup_started_at);
            self.cancellation.cancel();
        }
    }
}

pub(super) async fn start<D: RuntimeDriver>(
    plan: lenso_app_plan::ResolvedAppPlan,
    driver: D,
    adapters: ExecutionAdapterCatalog,
    diagnostics: RuntimeDiagnostics,
    context: InvocationContext,
    cleanup_timeout: Duration,
) -> Result<NativeApp, RuntimeFailure> {
    super::ensure_context_active(&super::DriverControl::new(&driver), &context)?;
    let cleanup = super::cleanup::StartupCleanupBudget::new(
        &super::DriverControl::new(&driver),
        cleanup_timeout,
    );
    let mut guard = CancelOnDrop {
        cancellation: context.cancellation(),
        cleanup: cleanup.clone(),
        startup_deadline: context.deadline(),
        driver: super::DriverControl::new(&driver),
        armed: true,
    };
    let (publish, receive) = oneshot::channel();
    let worker_context = context.clone();
    let worker_driver = driver.clone();
    driver
        .spawn_local(Box::pin(async move {
            let result = Kernel::start_owned(
                plan,
                worker_driver.clone(),
                adapters,
                diagnostics,
                Some(worker_context.clone()),
                Some(cleanup.clone()),
            )
            .await;
            let result = match result {
                Ok(app) => super::ensure_context_active(
                    &super::DriverControl::new(&worker_driver),
                    &worker_context,
                )
                .map(|()| app),
                error => error,
            };
            if let Err(result) = publish.send(result)
                && let Ok(app) = result
            {
                // The complete object has one owner even when it returned after the
                // caller left. It can never become ready; cleanup gets one budget.
                let _ = app.shutdown_with_budget(cleanup.establish()).await;
            }
        }))
        .map_err(|error| RuntimeFailure::Internal {
            detail: format!("cannot schedule startup owner: {error}"),
        })?;

    let result = await_with_context(&super::DriverControl::new(&driver), &context, receive)
        .await
        .and_then(|result| {
            result.map_err(|_| RuntimeFailure::Internal {
                detail: "startup owner ended without a result".to_owned(),
            })
        })?;
    guard.armed = false;
    result
}
