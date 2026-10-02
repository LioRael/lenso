//! Run with `cargo run -p lenso-onboarding-background-jobs --example simulate`.
//! No socket, external service, credential, Tokio runtime, or wall-clock sleep.

#[path = "../tests/support/mod.rs"]
mod support;

fn main() {
    support::virtual_deadline();
    let receipt = support::transient_retry();
    assert_eq!(receipt, support::transient_retry());
    println!("transient retry: identical complete receipts across two fresh Apps");
    for event in receipt {
        println!("{event:?}");
    }
    support::retry_exhaustion();
    support::frozen_resource_cancellation();
    support::pending_restart();
    support::cancelled_restart();
    support::committed_restart();
    support::committed_gate_shutdown();
    support::retry_restart();
    support::invalid_snapshot();
    support::failed_snapshot_write();
    support::failed_completion_write();
    support::without_jobs();
    println!(
        "PASS: 13 deterministic background-job scenarios; every started App shut down cleanly"
    );
}
