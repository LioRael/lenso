mod support;

#[test]
fn virtual_deadline_is_exact() {
    support::virtual_deadline();
}

#[test]
fn transient_retry_receipt_repeats_exactly() {
    let first = support::transient_retry();
    let second = support::transient_retry();
    assert_eq!(first, second);
    assert_eq!(first.len(), 6);
    assert_eq!(first[1].virtual_time, std::time::Duration::from_millis(20));
    assert_eq!(first[3].virtual_time, std::time::Duration::from_millis(120));
}

#[test]
fn retry_exhaustion_never_notifies() {
    support::retry_exhaustion();
}

#[test]
fn frozen_resource_cancellation_has_no_late_effect() {
    support::frozen_resource_cancellation();
}

#[test]
fn pending_restart_reopens_snapshot() {
    support::pending_restart();
}

#[test]
fn cancelled_restart_does_not_resume() {
    support::cancelled_restart();
}

#[test]
fn committed_restart_does_not_duplicate_lost_receipt() {
    support::committed_restart();
}

#[test]
fn shutdown_in_commit_ack_gap_drops_blocked_receipt() {
    support::committed_gate_shutdown();
}

#[test]
fn retry_restart_preserves_attempt_budget() {
    support::retry_restart();
}

#[test]
fn invalid_snapshot_fails_startup_without_work() {
    support::invalid_snapshot();
}

#[test]
fn snapshot_write_failure_cannot_admit_job() {
    support::failed_snapshot_write();
}

#[test]
fn completion_write_failure_cannot_acknowledge_job() {
    support::failed_completion_write();
}

#[test]
fn jobs_removal_preserves_health() {
    support::without_jobs();
}
