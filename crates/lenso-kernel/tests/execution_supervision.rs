//! Execution owners report provider failures after caller interest ends.

#[path = "support/execution_supervision.rs"]
mod support;

use futures::{FutureExt, channel::oneshot};
use lenso_app_plan::RestartPolicy;
use lenso_kernel::{CancellationToken, EventAdmission, RuntimeFailure, ShutdownOutcome};
use std::time::Duration;
use support::{Action, Contract, Harness, Kind, OP};

fn late_failure_restarts(cancel: bool) {
    let (finish, gate) = oneshot::channel();
    let h = Harness::new(Kind::Request, Some(gate));
    let handle = h.app.handle::<Contract>("consumer").unwrap();
    let cancellation = CancellationToken::new();
    let context = h.app.invocation_context(None, cancellation.clone());
    let mut waiter = Box::pin(handle.invoke_with_context(OP, context, Action::Fail));
    assert!(waiter.as_mut().now_or_never().is_none());
    if cancel {
        cancellation.cancel();
        assert!(matches!(
            h.driver.run(waiter),
            Err(RuntimeFailure::Cancelled { .. })
        ));
    } else {
        drop(waiter);
    }
    assert_eq!(h.probe.finished.get(), 0);
    finish.send(()).unwrap();
    h.pump();
    h.assert_restarted_once();
    assert_eq!(h.probe.calls.get(), 1, "failed work must never be replayed");
    assert_eq!(h.probe.finished.get(), 1);
    assert_eq!(h.driver.run(handle.invoke(OP, Action::Success)), Ok(Ok(())));
    assert_eq!(
        h.driver.run(h.app.shutdown(Duration::from_secs(1))),
        ShutdownOutcome::Clean
    );
}

#[test]
fn cancelled_waiter_does_not_hide_late_plugin_failure() {
    late_failure_restarts(true);
}

#[test]
fn dropped_waiter_does_not_hide_late_plugin_failure() {
    late_failure_restarts(false);
}

#[test]
fn immediate_live_failure_is_not_replaced_by_its_own_generation_cancellation() {
    let h = Harness::new(Kind::Request, None);
    let handle = h.app.handle::<Contract>("consumer").unwrap();
    assert!(matches!(
        h.driver.run(handle.invoke(OP, Action::Fail)),
        Err(RuntimeFailure::PluginFailure { .. })
    ));
    h.pump();
    h.assert_restarted_once();
}

#[test]
fn accepted_failure_survives_cancel_and_late_poll_without_failing_the_replacement() {
    let (finish, gate) = oneshot::channel();
    let h = Harness::new(Kind::Request, Some(gate));
    let handle = h.app.handle::<Contract>("consumer").unwrap();
    let cancellation = CancellationToken::new();
    let context = h.app.invocation_context(None, cancellation.clone());
    let mut waiter = Box::pin(handle.invoke_with_context(OP, context, Action::Fail));
    assert!(waiter.as_mut().now_or_never().is_none());
    finish.send(()).unwrap();
    h.pump();
    h.assert_restarted_once();
    cancellation.cancel();
    assert!(matches!(
        h.driver.run(waiter),
        Err(RuntimeFailure::PluginFailure { .. })
    ));
    h.pump();
    h.assert_restarted_once();
    assert!(!h.app.is_failed());
    assert_eq!(h.driver.run(handle.invoke(OP, Action::Success)), Ok(Ok(())));
}

#[test]
fn same_poll_cancellation_wins_delivery_but_does_not_hide_provider_failure() {
    let h = Harness::new(Kind::Request, None);
    let handle = h.app.handle::<Contract>("consumer").unwrap();
    assert!(matches!(
        h.driver.run(handle.invoke(OP, Action::CancelAndFail)),
        Err(RuntimeFailure::Cancelled { .. })
    ));
    h.pump();
    h.assert_restarted_once();
}

#[test]
fn successful_domain_and_admission_outcomes_do_not_supervise() {
    let h = Harness::new(Kind::Request, None);
    let handle = h.app.handle::<Contract>("consumer").unwrap();
    assert_eq!(h.driver.run(handle.invoke(OP, Action::Success)), Ok(Ok(())));
    assert_eq!(h.driver.run(handle.invoke(OP, Action::Domain)), Ok(Err(())));
    assert!(matches!(
        h.driver.run(handle.invoke(OP, Action::Exhausted)),
        Err(RuntimeFailure::ResourceExhausted { .. })
    ));
    h.pump();
    assert_eq!(h.recreates.get(), 0);
    assert_eq!(h.app.plugin_generation("provider"), Some(1));
}

#[test]
fn retained_execution_blocks_recreation_until_resources_are_released() {
    let (finish, gate) = oneshot::channel();
    let h = Harness::new(Kind::Request, Some(gate));
    let handle = h.app.handle::<Contract>("consumer").unwrap();
    assert!(
        handle
            .invoke(OP, Action::RetainAndFail)
            .now_or_never()
            .is_none()
    );
    finish.send(()).unwrap();
    h.pump();
    assert_eq!(h.app.plugin_generation("provider"), None);
    assert_eq!(h.recreates.get(), 0, "retained work is still live");
    assert_eq!(h.probe.finished.get(), 1);
    assert!(matches!(
        h.driver.run(handle.invoke(OP, Action::Success)),
        Err(RuntimeFailure::Unavailable { .. })
    ));
    h.probe.retained.borrow_mut().take().unwrap().settle();
    h.pump();
    h.assert_restarted_once();
    assert_eq!(h.driver.run(handle.invoke(OP, Action::Success)), Ok(Ok(())));
    assert_eq!(
        h.driver.run(h.app.shutdown(Duration::from_secs(1))),
        ShutdownOutcome::Clean
    );
}

#[test]
fn duplicate_generation_failure_waits_for_old_execution_and_recreates_once() {
    let (finish, gate) = oneshot::channel();
    let h = Harness::new(Kind::Request, Some(gate));
    let handle = h.app.handle::<Contract>("consumer").unwrap();
    assert!(handle.invoke(OP, Action::Fail).now_or_never().is_none());
    h.app.report_plugin_failure("provider").unwrap();
    h.app.report_plugin_failure("provider").unwrap();
    h.pump();
    assert_eq!(h.recreates.get(), 0);
    finish.send(()).unwrap();
    h.pump();
    h.assert_restarted_once();
    assert!(!h.app.is_failed());
}

#[test]
fn late_failure_respects_restart_never() {
    let (finish, gate) = oneshot::channel();
    let h = Harness::with_policy(Kind::Request, Some(gate), RestartPolicy::never());
    let handle = h.app.handle::<Contract>("consumer").unwrap();
    assert!(handle.invoke(OP, Action::Fail).now_or_never().is_none());
    finish.send(()).unwrap();
    h.pump();
    assert_eq!(h.app.plugin_generation("provider"), None);
    assert_eq!(h.recreates.get(), 0);
}

#[test]
fn cancelled_stream_open_still_reports_late_failure() {
    let (finish, gate) = oneshot::channel();
    let h = Harness::new(Kind::Stream, Some(gate));
    let handle = h.app.stream_handle::<Contract>("consumer").unwrap();
    let cancellation = CancellationToken::new();
    let context = h.app.invocation_context(None, cancellation.clone());
    let mut waiter = Box::pin(handle.open_with_context(OP, context, Action::Fail));
    assert!(waiter.as_mut().now_or_never().is_none());
    cancellation.cancel();
    assert!(matches!(
        h.driver.run(waiter),
        Err(RuntimeFailure::Cancelled { .. })
    ));
    finish.send(()).unwrap();
    h.pump();
    h.assert_restarted_once();
}

#[test]
fn cancelled_stream_operations_still_report_late_failure_and_cancel_once() {
    // One reader and completion per session; no assumption about overlapping receives.
    for operation in ["send", "receive", "close"] {
        let (finish, gate) = oneshot::channel();
        let h = Harness::new(Kind::Stream, Some(gate));
        let handle = h.app.stream_handle::<Contract>("consumer").unwrap();
        let stream = h
            .driver
            .run(handle.open(OP, Action::Success))
            .unwrap()
            .unwrap();
        let mut waiter = Box::pin(async {
            match operation {
                "send" => stream.send(Action::Fail).await,
                "receive" => stream.receive().await.map(|_| ()),
                _ => stream.close_send().await,
            }
        });
        assert!(waiter.as_mut().now_or_never().is_none());
        stream.cancel();
        assert!(matches!(
            h.driver.run(waiter),
            Err(RuntimeFailure::Cancelled { .. })
        ));
        finish.send(()).unwrap();
        h.pump();
        h.assert_restarted_once();
        stream.cancel();
        drop(stream);
        assert_eq!(h.probe.cancels.get(), 1);
    }
}

#[test]
fn cancelled_event_admission_still_reports_late_failure() {
    let (finish, gate) = oneshot::channel();
    let h = Harness::new(
        Kind::Event {
            adapter_admission: true,
        },
        Some(gate),
    );
    let handle = h.app.event_handle::<Contract>("consumer").unwrap();
    let cancellation = CancellationToken::new();
    let context = h.app.invocation_context(None, cancellation.clone());
    let mut waiter = Box::pin(handle.publish_with_context(OP, context, Action::Fail));
    assert!(waiter.as_mut().now_or_never().is_none());
    cancellation.cancel();
    assert_eq!(
        h.driver.run(waiter)[0].admission(),
        EventAdmission::Unavailable
    );
    finish.send(()).unwrap();
    h.pump();
    h.assert_restarted_once();
    assert_eq!(h.probe.calls.get(), 1);
}

#[test]
fn cancelled_kernel_event_delivery_still_reports_late_failure() {
    let (finish, gate) = oneshot::channel();
    let h = Harness::new(
        Kind::Event {
            adapter_admission: false,
        },
        Some(gate),
    );
    let handle = h.app.event_handle::<Contract>("consumer").unwrap();
    let cancellation = CancellationToken::new();
    let context = h.app.invocation_context(None, cancellation.clone());
    assert_eq!(
        h.driver
            .run(handle.publish_with_context(OP, context, Action::Fail))[0]
            .admission(),
        EventAdmission::Accepted
    );
    h.pump();
    assert_eq!(h.probe.calls.get(), 1);
    cancellation.cancel();
    h.pump();
    finish.send(()).unwrap();
    h.pump();
    h.assert_restarted_once();
    assert_eq!(h.probe.calls.get(), 1);
}
