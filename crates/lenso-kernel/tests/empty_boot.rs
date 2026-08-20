use std::time::Duration;

use lenso_app_plan::ResolvedAppPlan;
use lenso_kernel::{
    DeterministicDriver, Kernel, PlanValidationError, RuntimeDriver, TaskOutcome, TerminalOutcome,
};

#[test]
fn boots_an_empty_resolved_app_plan() {
    let driver = DeterministicDriver::new();

    let outcome = driver.run(Kernel::boot(ResolvedAppPlan::empty(), driver.clone()));

    assert_eq!(outcome, Ok(TerminalOutcome::Completed));
    assert_eq!(driver.now(), Duration::ZERO);
}

#[test]
fn rejects_an_invalid_plan_before_boot() {
    let driver = DeterministicDriver::new();
    let plan = ResolvedAppPlan::with_schema_version(0);

    let outcome = driver.run(Kernel::boot(plan, driver.clone()));

    assert_eq!(
        outcome,
        Err(PlanValidationError::UnsupportedSchemaVersion {
            expected: 1,
            actual: 0,
        })
    );
}

#[test]
fn reports_a_requested_shutdown() {
    let driver = DeterministicDriver::new();
    let shutdown_driver = driver.clone();
    driver
        .spawn_local(Box::pin(async move {
            shutdown_driver.request_shutdown();
        }))
        .expect("shutdown task should be scheduled");

    let outcome = driver.run(Kernel::boot(ResolvedAppPlan::empty(), driver.clone()));

    assert_eq!(outcome, Ok(TerminalOutcome::ShutdownRequested));
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
fn produces_the_same_terminal_outcome_for_the_same_driver_script() {
    let run = || {
        let driver = DeterministicDriver::new();
        driver.advance(Duration::from_millis(25));
        driver.run(Kernel::boot(ResolvedAppPlan::empty(), driver.clone()))
    };

    assert_eq!(run(), run());
}
