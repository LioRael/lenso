mod support;

#[test]
fn smoke_corpus_replays_without_wall_clock_waits() {
    assert_eq!(support::smoke_corpus(), support::smoke_corpus());
}

#[test]
fn socket_smoke_sentinel_matches_simulated_corpus() {
    let started = std::time::Instant::now();
    let result = std::process::Command::new("python3")
        .arg(format!("{}/smoke.py", env!("CARGO_MANIFEST_DIR")))
        .arg(env!("CARGO_BIN_EXE_lenso-onboarding-background-jobs"))
        .output()
        .expect("Python 3 runs the retained real-socket and process sentinel");
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(
        result.status.success(),
        "{output}\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let receipt = output
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find_map(|value| value.get("corpus_receipt").cloned())
        .expect("the real sentinel emits its observable corpus receipt");
    assert_eq!(receipt, support::smoke_corpus());
    eprintln!(
        "real socket/process corpus sentinel: {:?}",
        started.elapsed()
    );
}

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
