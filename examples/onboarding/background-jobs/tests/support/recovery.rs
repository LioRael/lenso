//! Durable-state and generation-boundary scenarios.

use std::time::Duration;

use http::StatusCode;
use lenso_test::TestSimulator;
use lenso_test::{ScenarioBoundary, SimulatorFault};
use serde_json::{Value, json};

use super::{Harness, runtime};

/// A completion write failure releases the accepted task without acknowledging it.
pub fn failed_completion_write() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("jobs.json");
    let simulator = TestSimulator::new();
    let gate = simulator.gate("job-1.before-delivery");
    let blocked = gate.clone();
    let configured = runtime(&simulator).with_before_delivery(move |_job_id, _attempt| {
        let gate = blocked.clone();
        Box::pin(async move {
            gate.wait().await;
            Ok(())
        })
    });
    let app = Harness::start(simulator, configured, Some(path.clone())).unwrap();
    app.submit(20);
    app.advance(20);
    assert_eq!(gate.reached_count(), 1);
    assert_eq!(app.probe.snapshot().active, 1);
    assert_eq!(app.probe.snapshot().delivery_attempts, [(1, 1)]);
    assert_eq!(app.read("/notifications"), json!([]));
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(gate.release());
    app.simulator.pump();
    assert_eq!(
        app.request("GET", "/jobs", &Value::Null).0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        app.request("GET", "/notifications", &Value::Null).0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    app.assert_drained();
    app.advance(5000);
    assert_eq!(app.probe.snapshot().delivery_attempts, [(1, 1)]);
    assert_eq!(app.probe.snapshot().receipts, 0);
    assert_eq!(app.probe.snapshot().receipt_failures, 0);
    assert!(path.is_dir());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    app.shutdown();
}

/// A failed snapshot write cannot admit a job or create generation-owned work.
pub fn failed_snapshot_write() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("jobs.json");
    let simulator = TestSimulator::new();
    let app = Harness::start(simulator.clone(), runtime(&simulator), Some(path.clone())).unwrap();
    assert_eq!(app.read("/jobs"), json!([]));
    assert_eq!(app.read("/notifications"), json!([]));
    if path.is_file() {
        std::fs::remove_file(&path).unwrap();
    }
    std::fs::create_dir(&path).unwrap();
    let (status, _) = app.request(
        "POST",
        "/jobs",
        &json!({"message": "local reminder", "delay_ms": 20}),
    );
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    app.advance(5000);
    assert_eq!(
        app.request("GET", "/jobs", &Value::Null).0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        app.request("GET", "/notifications", &Value::Null).0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(app.probe.snapshot().started, 0);
    assert_eq!(app.probe.snapshot().receipts, 0);
    assert!(app.probe.snapshot().delivery_attempts.is_empty());
    assert!(path.is_dir());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    app.shutdown();
}

/// Reopen an actual snapshot with fresh runtime/Simulator objects after shutdown.
pub fn pending_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("jobs.json");
    let simulator = TestSimulator::new();
    let app = Harness::start(simulator.clone(), runtime(&simulator), Some(path.clone())).unwrap();
    app.submit(20);
    app.advance(7);
    assert_eq!(app.read("/notifications"), json!([]));
    app.shutdown();
    let old_probe = app.probe.clone();
    let old_attempts = old_probe.snapshot().delivery_attempts;
    drop(app);
    let snapshot = std::fs::read(&path).expect("snapshot really exists on disk");
    assert!(serde_json::from_slice::<Value>(&snapshot).is_ok());

    // Restart deliberately schedules a full new delay on a new monotonic origin.
    let restarted_clock = TestSimulator::new();
    let restored = Harness::start(
        restarted_clock.clone(),
        runtime(&restarted_clock),
        Some(path),
    )
    .unwrap();
    restored.simulator.pump();
    restored.advance(19);
    assert_eq!(restored.read("/notifications"), json!([]));
    assert!(restored.probe.snapshot().delivery_attempts.is_empty());
    restored.advance(1);
    restored.assert_job("completed", 1, 1);
    assert_eq!(restored.probe.snapshot().delivery_attempts, [(1, 1)]);
    simulator.advance(Duration::from_secs(10));
    simulator.pump();
    assert_eq!(old_probe.snapshot().delivery_attempts, old_attempts);
    assert_eq!(old_probe.snapshot().active, 0);
    restored.shutdown();
}

/// A persisted user cancellation is terminal across a fresh App generation.
pub fn cancelled_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("jobs.json");
    let simulator = TestSimulator::new();
    let app = Harness::start(simulator.clone(), runtime(&simulator), Some(path.clone())).unwrap();
    app.submit(20);
    assert!(
        app.request("DELETE", "/jobs/1", &Value::Null)
            .0
            .is_success()
    );
    app.simulator.pump();
    app.assert_job("cancelled", 0, 0);
    app.shutdown();
    drop(app);

    let simulator = TestSimulator::new();
    let restored = Harness::start(simulator.clone(), runtime(&simulator), Some(path)).unwrap();
    restored.advance(5000);
    restored.assert_job("cancelled", 0, 0);
    assert_eq!(restored.probe.snapshot().started, 0);
    assert!(restored.probe.snapshot().delivery_attempts.is_empty());
    restored.shutdown();
}

/// Lost acknowledgement after atomic commit cannot trigger duplicate delivery.
pub fn committed_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("jobs.json");
    let simulator = TestSimulator::new();
    let faults = simulator.faults();
    faults
        .inject_at(
            "job-1",
            ScenarioBoundary::AfterDurableCommit,
            SimulatorFault::DroppedConnection,
        )
        .unwrap();
    let configured = runtime(&simulator).with_after_commit(move |_job_id| {
        let result = faults
            .check_at("job-1", ScenarioBoundary::AfterDurableCommit)
            .unwrap();
        Box::pin(async move { result.map_err(|_| "receipt lost after commit".to_owned()) })
    });
    let app = Harness::start(simulator.clone(), configured, Some(path.clone())).unwrap();
    app.submit(20);
    app.advance(20);
    app.assert_job("completed", 1, 1);
    assert_eq!(app.probe.snapshot().receipts, 0);
    assert_eq!(app.probe.snapshot().receipt_failures, 1);
    app.assert_drained();
    app.advance(5000);
    assert_eq!(app.probe.snapshot().delivery_attempts, [(1, 1)]);
    app.assert_job("completed", 1, 1);
    app.shutdown();
    drop(app);

    let simulator = TestSimulator::new();
    let restored = Harness::start(simulator.clone(), runtime(&simulator), Some(path)).unwrap();
    restored.advance(5000);
    restored.assert_job("completed", 1, 1);
    assert_eq!(restored.probe.snapshot().started, 0);
    assert!(restored.probe.snapshot().delivery_attempts.is_empty());
    restored.shutdown();
}

/// Shutdown in the commit/ack gap drops a blocked receipt future, never the commit.
pub fn committed_gate_shutdown() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("jobs.json");
    let simulator = TestSimulator::new();
    let gate = simulator.gate("job-1.after-commit");
    let blocked = gate.clone();
    let configured = runtime(&simulator).with_after_commit(move |_job_id| {
        let gate = blocked.clone();
        Box::pin(async move {
            gate.wait().await;
            Ok(())
        })
    });
    let app = Harness::start(simulator.clone(), configured, Some(path.clone())).unwrap();
    app.submit(20);
    app.advance(20);
    assert_eq!(gate.reached_count(), 1);
    assert!(!gate.is_released());
    app.assert_job("completed", 1, 1);
    assert_eq!(app.probe.snapshot().active, 1);
    assert_eq!(app.probe.snapshot().receipts, 0);
    app.shutdown();
    let old_probe = app.probe.clone();
    let attempts = old_probe.snapshot().delivery_attempts;
    drop(app);
    assert!(gate.release());
    simulator.advance(Duration::from_secs(10));
    simulator.pump();
    assert_eq!(old_probe.snapshot().delivery_attempts, attempts);
    assert_eq!(old_probe.snapshot().receipts, 0);
    assert_eq!(old_probe.snapshot().receipt_failures, 0);
    assert_eq!(old_probe.snapshot().active, 0);
    assert_eq!(old_probe.snapshot().started, old_probe.snapshot().dropped);

    let simulator = TestSimulator::new();
    let restored = Harness::start(simulator.clone(), runtime(&simulator), Some(path)).unwrap();
    restored.advance(5000);
    restored.assert_job("completed", 1, 1);
    assert_eq!(restored.probe.snapshot().started, 0);
    assert!(restored.probe.snapshot().delivery_attempts.is_empty());
    restored.shutdown();
}

/// Restart preserves the total attempt budget and waits the pending retry delay.
pub fn retry_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("jobs.json");
    let simulator = TestSimulator::new();
    let configured = runtime(&simulator).with_before_delivery(|_job_id, _attempt| {
        Box::pin(async { Err("notification resource unavailable".to_owned()) })
    });
    let app = Harness::start(simulator.clone(), configured, Some(path.clone())).unwrap();
    app.submit(20);
    app.advance(20);
    app.assert_job("retrying", 1, 0);
    assert_eq!(app.probe.snapshot().delivery_attempts, [(1, 1)]);
    app.shutdown();
    let old_probe = app.probe.clone();
    drop(app);

    let restarted_clock = TestSimulator::new();
    let configured = runtime(&restarted_clock).with_before_delivery(|_job_id, _attempt| {
        Box::pin(async { Err("notification resource unavailable".to_owned()) })
    });
    let restored = Harness::start(restarted_clock, configured, Some(path)).unwrap();
    restored.simulator.pump();
    restored.advance(99);
    restored.assert_job("retrying", 1, 0);
    assert!(restored.probe.snapshot().delivery_attempts.is_empty());
    restored.advance(1);
    restored.assert_job("retrying", 2, 0);
    assert_eq!(restored.probe.snapshot().delivery_attempts, [(1, 2)]);
    restored.advance(199);
    assert_eq!(restored.probe.snapshot().delivery_attempts, [(1, 2)]);
    restored.advance(1);
    restored.assert_job("failed", 3, 0);
    assert_eq!(
        restored.probe.snapshot().delivery_attempts,
        [(1, 2), (1, 3)]
    );
    assert_eq!(restored.probe.snapshot().receipts, 0);
    restored.assert_drained();
    restored.advance(5000);
    restored.assert_job("failed", 3, 0);
    assert_eq!(
        restored.probe.snapshot().delivery_attempts,
        [(1, 2), (1, 3)]
    );
    simulator.advance(Duration::from_secs(10));
    simulator.pump();
    assert_eq!(old_probe.snapshot().delivery_attempts, [(1, 1)]);
    assert_eq!(old_probe.snapshot().active, 0);
    restored.shutdown();
}

/// Corrupt durable state rejects startup instead of silently resetting jobs.
pub fn invalid_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("jobs.json");
    std::fs::write(&path, b"{invalid-json").unwrap();
    let simulator = TestSimulator::new();
    let configured = runtime(&simulator);
    let probe = configured.probe();
    assert!(Harness::start(simulator.clone(), configured, Some(path.clone())).is_err());
    simulator.advance(Duration::from_secs(10));
    simulator.pump();
    let snapshot = probe.snapshot();
    assert_eq!(snapshot.active, 0);
    assert_eq!(snapshot.started, 0);
    assert_eq!(snapshot.dropped, 0);
    assert!(snapshot.delivery_attempts.is_empty());
    assert_eq!(std::fs::read(path).unwrap(), b"{invalid-json");
}
