//! Shared executable scenarios. Every request crosses the real event Ingress.

use std::{path::PathBuf, time::Duration};

use bytes::Bytes;
use http::{Request, StatusCode};
use lenso_kernel::{RuntimeFailure, ShutdownOutcome};
use lenso_onboarding_background_jobs::{JobProbe, JobRuntime, Plugin, health};
use lenso_test::{
    ScenarioBoundary, ScenarioReceiptEvent, ScenarioTerminal, ScenarioTransition, SimulatorFault,
    TestApp, TestSimulator,
};
use lenso_web_host::{NativeWebHost, SimulatedWebHost};
use serde_json::{Value, json};

mod smoke_corpus;
pub use smoke_corpus::smoke_corpus;

fn runtime(simulator: &TestSimulator) -> JobRuntime {
    let clock = simulator.clone();
    let timer = simulator.clone();
    JobRuntime::new(
        move || clock.now(),
        move |deadline| timer.sleep_until(deadline),
    )
}

struct Harness {
    app: TestApp,
    simulator: TestSimulator,
    web: SimulatedWebHost,
    probe: JobProbe,
}

impl Harness {
    fn start(
        simulator: TestSimulator,
        runtime: JobRuntime,
        snapshot: Option<PathBuf>,
    ) -> Result<Self, RuntimeFailure> {
        let probe = runtime.probe();
        let prepared = NativeWebHost::new()
            .plugin::<health::Plugin>()
            .configured_plugin::<Plugin, _>(move |plugin| {
                plugin.configure(runtime.clone(), snapshot.clone())
            })
            .prepare_simulated()
            .expect("the real Host derives a valid Plan");
        let (plan, registry, web) = prepared.into_parts();
        let app = TestApp::builder(plan)
            .with_registry(registry)
            .with_simulator(simulator.clone())
            .start()?;
        Ok(Self {
            app,
            simulator,
            web,
            probe,
        })
    }

    fn request(&self, method: &str, path: &str, body: &Value) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json")
            .body(Bytes::from(serde_json::to_vec(body).unwrap()))
            .unwrap();
        let response = self.app.run(self.web.request(request)).unwrap();
        let body = if response.body().is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(response.body()).unwrap()
        };
        (response.status(), body)
    }

    fn submit(&self, delay_ms: u64) {
        let (status, job) = self.request(
            "POST",
            "/jobs",
            &json!({"message": "local reminder", "delay_ms": delay_ms}),
        );
        assert_eq!(status, StatusCode::ACCEPTED);
        assert_eq!(job["id"], 1);
        self.simulator.pump();
    }

    fn read(&self, path: &str) -> Value {
        let (status, body) = self.request("GET", path, &Value::Null);
        assert_eq!(status, StatusCode::OK);
        body
    }

    fn assert_job(&self, status: &str, attempts: u32, notification_count: usize) {
        let jobs = self.read("/jobs");
        assert_eq!(jobs.as_array().unwrap().len(), 1);
        assert_eq!(jobs[0]["status"], status);
        assert_eq!(jobs[0]["attempts"], attempts);
        let notifications = self.read("/notifications");
        assert_eq!(notifications.as_array().unwrap().len(), notification_count);
        if notification_count == 1 {
            assert_eq!(notifications[0]["job_id"], 1);
            assert_eq!(notifications[0]["message"], "local reminder");
        }
    }

    fn advance(&self, millis: u64) {
        self.simulator.advance(Duration::from_millis(millis));
        self.simulator.pump();
    }

    fn assert_drained(&self) {
        let snapshot = self.probe.snapshot();
        assert_eq!(snapshot.active, 0);
        assert_eq!(snapshot.started, snapshot.dropped);
    }

    fn shutdown(&self) {
        assert_eq!(
            self.app.shutdown(Duration::from_secs(1)),
            ShutdownOutcome::Clean
        );
        self.simulator.pump();
        self.assert_drained();
        assert_eq!(self.probe.snapshot().stop_rejected, 1);
    }
}

/// A job's requested delay uses exactly the Simulator clock, with no real sleep.
pub fn virtual_deadline() {
    let simulator = TestSimulator::new();
    let app = Harness::start(simulator.clone(), runtime(&simulator), None).unwrap();
    app.submit(20);
    app.advance(19);
    assert_eq!(app.read("/notifications"), json!([]));
    assert!(app.probe.snapshot().delivery_attempts.is_empty());
    assert_eq!(app.probe.snapshot().active, 1);
    app.advance(1);
    app.assert_job("completed", 1, 1);
    assert_eq!(app.probe.snapshot().delivery_attempts, [(1, 1)]);
    assert_eq!(app.probe.snapshot().receipts, 1);
    app.assert_drained();
    app.shutdown();
}

/// Run one reproducible transient failure and return the complete existing receipt.
pub fn transient_retry() -> Vec<ScenarioReceiptEvent> {
    let directory = tempfile::tempdir().unwrap();
    let simulator = TestSimulator::new();
    let receipt = simulator.receipt();
    let faults = simulator.faults();
    faults
        .inject_at(
            "job-1",
            ScenarioBoundary::ResourceAcquire,
            SimulatorFault::ResourceUnavailable,
        )
        .unwrap();
    let before_receipt = receipt.clone();
    let after_receipt = receipt.clone();
    let runtime = runtime(&simulator)
        .with_before_delivery(move |_job_id, _attempt| {
            let fault = faults
                .check_at("job-1", ScenarioBoundary::ResourceAcquire)
                .unwrap();
            let receipt = before_receipt.clone();
            Box::pin(async move {
                if let Err(fault) = fault {
                    receipt
                        .fault("generation-1", "job-1", ScenarioTransition::Paused, fault)
                        .unwrap();
                    Err("notification resource unavailable".to_owned())
                } else {
                    receipt
                        .transition("generation-1", "job-1", ScenarioTransition::Resumed)
                        .unwrap();
                    Ok(())
                }
            })
        })
        .with_after_commit(move |_job_id| {
            let receipt = after_receipt.clone();
            Box::pin(async move {
                receipt
                    .transition("generation-1", "job-1", ScenarioTransition::DurableCommit)
                    .unwrap();
                Ok(())
            })
        });
    let app = Harness::start(simulator, runtime, Some(directory.path().join("jobs.json"))).unwrap();
    receipt
        .transition("generation-1", "job-1", ScenarioTransition::Started)
        .unwrap();
    app.submit(20);
    app.advance(20);
    assert_eq!(app.probe.snapshot().delivery_attempts, [(1, 1)]);
    assert_eq!(app.read("/notifications"), json!([]));
    app.advance(99);
    assert_eq!(app.probe.snapshot().delivery_attempts, [(1, 1)]);
    assert_eq!(app.read("/notifications"), json!([]));
    app.advance(1);
    app.assert_job("completed", 2, 1);
    assert_eq!(app.probe.snapshot().delivery_attempts, [(1, 1), (1, 2)]);
    assert_eq!(app.probe.snapshot().receipts, 1);
    receipt
        .terminal("generation-1", "job-1", ScenarioTerminal::Succeeded)
        .unwrap();
    app.advance(1000);
    app.assert_job("completed", 2, 1);
    assert_eq!(app.probe.snapshot().delivery_attempts, [(1, 1), (1, 2)]);
    app.shutdown();
    receipt
        .transition("generation-1", "job-1", ScenarioTransition::CleanupStarted)
        .unwrap();
    receipt.events()
}

/// Exhaust the bounded retry policy without ever committing a notification.
pub fn retry_exhaustion() {
    let simulator = TestSimulator::new();
    let faults = simulator.faults();
    for _ in 0..3 {
        faults
            .inject_at(
                "job-1",
                ScenarioBoundary::ResourceAcquire,
                SimulatorFault::ResourceUnavailable,
            )
            .unwrap();
    }
    let runtime = runtime(&simulator).with_before_delivery(move |_job_id, _attempt| {
        let result = faults
            .check_at("job-1", ScenarioBoundary::ResourceAcquire)
            .unwrap();
        Box::pin(async move { result.map_err(|_| "notification resource unavailable".to_owned()) })
    });
    let app = Harness::start(simulator, runtime, None).unwrap();
    app.submit(20);
    app.advance(20);
    app.advance(100);
    app.advance(199);
    assert_eq!(app.probe.snapshot().delivery_attempts, [(1, 1), (1, 2)]);
    assert_eq!(app.read("/notifications"), json!([]));
    app.advance(1);
    app.assert_job("failed", 3, 0);
    assert_eq!(
        app.probe.snapshot().delivery_attempts,
        [(1, 1), (1, 2), (1, 3)]
    );
    assert_eq!(app.probe.snapshot().receipts, 0);
    app.assert_drained();
    app.advance(5000);
    app.assert_job("failed", 3, 0);
    assert_eq!(app.probe.snapshot().delivery_attempts.len(), 3);
    app.shutdown();
}

/// Cancelling while resource acquisition is blocked must drop the whole wait.
pub fn frozen_resource_cancellation() {
    let simulator = TestSimulator::new();
    let resource = simulator.resource("notification-store");
    assert!(resource.freeze());
    let blocked = resource.clone();
    let runtime = runtime(&simulator).with_before_delivery(move |_job_id, _attempt| {
        let resource = blocked.clone();
        Box::pin(async move {
            resource.acquire().await;
            Ok(())
        })
    });
    let app = Harness::start(simulator, runtime, None).unwrap();
    app.submit(20);
    app.advance(20);
    assert_eq!(app.probe.snapshot().active, 1);
    assert_eq!(app.probe.snapshot().delivery_attempts, [(1, 1)]);
    assert_eq!(resource.acquisition_count(), 0);
    assert!(
        app.request("DELETE", "/jobs/1", &Value::Null)
            .0
            .is_success()
    );
    app.simulator.pump();
    app.assert_job("cancelled", 1, 0);
    app.assert_drained();
    let attempts = app.probe.snapshot().delivery_attempts;
    assert!(resource.thaw());
    app.advance(5000);
    app.assert_job("cancelled", 1, 0);
    assert_eq!(resource.acquisition_count(), 0);
    assert_eq!(app.probe.snapshot().delivery_attempts, attempts);
    assert_eq!(app.probe.snapshot().receipts, 0);
    app.shutdown();
}

mod recovery;
pub use recovery::{
    cancelled_restart, committed_gate_shutdown, committed_restart, failed_completion_write,
    failed_snapshot_write, invalid_snapshot, pending_restart, retry_restart,
};

/// Removing the jobs Plugin leaves health available and removes every job route.
pub fn without_jobs() {
    let (plan, registry, web) = NativeWebHost::new()
        .plugin::<health::Plugin>()
        .prepare_simulated()
        .unwrap()
        .into_parts();
    let app = TestApp::builder(plan)
        .with_registry(registry)
        .start()
        .unwrap();
    for (path, expected) in [
        ("/health", StatusCode::OK),
        ("/jobs", StatusCode::NOT_FOUND),
        ("/notifications", StatusCode::NOT_FOUND),
    ] {
        let response = app
            .run(web.request(Request::get(path).body(Bytes::new()).unwrap()))
            .unwrap();
        assert_eq!(response.status(), expected);
    }
    assert_eq!(app.shutdown(Duration::from_secs(1)), ShutdownOutcome::Clean);
}
