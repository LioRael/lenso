use std::time::Duration;

use lenso_app_plan::ResolvedAppPlan;
use lenso_kernel::{
    DeterministicDriver, ExecutionAdapterCatalog, Kernel, RuntimeDriver, RuntimeFailure,
    ShutdownOutcome, TaskOutcome,
};

async fn configured_task_failure() {
    futures::future::ready(()).await;
    panic!("configured task failure");
}

#[test]
fn boots_an_empty_resolved_app_plan() {
    let driver = DeterministicDriver::new();

    let app = driver
        .run(Kernel::start(
            ResolvedAppPlan::empty(),
            driver.clone(),
            ExecutionAdapterCatalog::new(),
        ))
        .expect("the empty App should start through the production Kernel seam");

    assert!(app.is_ready());
    assert_eq!(
        driver.run(app.shutdown(Duration::from_secs(1))),
        ShutdownOutcome::Clean
    );
    assert_eq!(driver.now(), Duration::ZERO);
}

#[test]
fn rejects_an_invalid_plan_before_boot() {
    let driver = DeterministicDriver::new();
    let plan = ResolvedAppPlan::with_schema_version(0);

    let outcome = driver.run(Kernel::start(
        plan,
        driver.clone(),
        ExecutionAdapterCatalog::new(),
    ));

    assert!(matches!(
        outcome,
        Err(RuntimeFailure::InvalidResolvedPlan { detail })
            if detail.contains("unsupported Plan schema version 0")
    ));
}

#[test]
fn reports_a_requested_shutdown() {
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start(
            ResolvedAppPlan::empty(),
            driver.clone(),
            ExecutionAdapterCatalog::new(),
        ))
        .expect("the empty App should start");
    app.request_shutdown();
    assert_eq!(
        driver.run(app.shutdown(Duration::from_secs(1))),
        ShutdownOutcome::Clean
    );
}

#[test]
fn deterministic_driver_wakes_timers_and_joins_or_cancels_local_tasks() {
    let driver = DeterministicDriver::new();
    let timer_driver = driver.clone();
    let timer = driver
        .spawn_local(Box::pin(async move {
            timer_driver.sleep_until(Duration::from_millis(10)).await;
        }))
        .expect("timer task should be scheduled");

    let cancelled = driver
        .spawn_local(Box::pin(futures::future::pending()))
        .expect("cancellable task should be scheduled");
    cancelled.cancel();

    let outcomes = driver.run(async {
        driver.yield_now().await;
        driver.advance(Duration::from_millis(10));
        (timer.await, cancelled.await)
    });

    assert_eq!(outcomes, (TaskOutcome::Completed, TaskOutcome::Cancelled));
    assert_eq!(driver.now(), Duration::from_millis(10));
}

#[test]
fn deterministic_driver_distinguishes_abnormal_task_failure_from_cancellation() {
    let driver = DeterministicDriver::new();
    let failed = driver
        .spawn_local(Box::pin(configured_task_failure()))
        .expect("the failing task should be scheduled");

    assert_eq!(driver.run(failed), TaskOutcome::Failed);
}

#[test]
fn produces_the_same_terminal_outcome_for_the_same_driver_script() {
    let run = || {
        let driver = DeterministicDriver::new();
        driver.advance(Duration::from_millis(25));
        let app = driver
            .run(Kernel::start(
                ResolvedAppPlan::empty(),
                driver.clone(),
                ExecutionAdapterCatalog::new(),
            ))
            .expect("the empty App should start");
        driver.run(app.shutdown(Duration::from_secs(1)))
    };

    assert_eq!(run(), run());
}
