//! The existing socket smoke's corpus, run through the real Kernel and Plugin.

use super::{Duration, Harness, StatusCode, TestSimulator, Value, json, runtime, without_jobs};

pub fn smoke_corpus() -> Value {
    let corpus: Value = serde_json::from_str(include_str!("../smoke-corpus.json")).unwrap();
    assert_eq!(corpus["schema"], "lenso.background-jobs-smoke.v1");
    let simulator = TestSimulator::new();
    let app = Harness::start(simulator.clone(), runtime(&simulator), None).unwrap();

    assert_eq!(
        app.request("GET", "/health", &Value::Null),
        (StatusCode::OK, json!("ok"))
    );
    assert_eq!(app.read("/jobs"), json!([]));
    assert_eq!(app.read("/notifications"), json!([]));
    let (status, problem) = app.request("POST", "/jobs", &corpus["invalid"]);
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(problem["code"], "invalid_job");

    let before = simulator.now();
    let (status, accepted) = app.request("POST", "/jobs", &corpus["success"]);
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(accepted["status"], "queued");
    assert_eq!(
        simulator.now(),
        before,
        "acceptance must not wait for delivery"
    );
    assert_eq!(app.read("/notifications"), json!([]));
    let active = app.read("/jobs")[0]["status"].clone();
    assert!(
        corpus["active_statuses"]
            .as_array()
            .unwrap()
            .contains(&active)
    );
    app.simulator.pump();
    let delay = corpus["success"]["delay_ms"].as_u64().unwrap();
    app.advance(delay - 1);
    assert_eq!(app.read("/notifications"), json!([]));
    assert_eq!(app.read("/jobs")[0]["status"], "running");
    app.advance(1);
    let completed = app.read("/jobs")[0].clone();
    assert_eq!(completed["status"], corpus["success_status"]);
    assert_eq!(completed["error"], Value::Null);
    let notifications = app.read("/notifications");
    assert_eq!(
        notifications,
        json!([{
            "job_id": accepted["id"], "message": corpus["success"]["message"]
        }])
    );

    let (status, failed) = app.request("POST", "/jobs", &corpus["failure"]);
    assert_eq!(status, StatusCode::ACCEPTED);
    app.simulator.pump();
    app.advance(corpus["failure"]["delay_ms"].as_u64().unwrap());
    let failure = app.read("/jobs")[1].clone();
    assert_eq!(failure["id"], failed["id"]);
    assert_eq!(failure["status"], corpus["failure_status"]);
    assert_eq!(failure["error"], corpus["failure_error"]);
    assert_eq!(app.read("/notifications"), notifications);

    let (status, cancelled) = app.request("POST", "/jobs", &corpus["shutdown"]);
    assert_eq!(status, StatusCode::ACCEPTED);
    app.simulator.pump();
    assert_eq!(app.read("/jobs")[2]["status"], "running");
    let probe = app.probe.clone();
    app.shutdown();
    simulator.advance(Duration::from_secs(10));
    simulator.pump();
    let mut drained = probe.snapshot();
    assert_eq!(drained.started, 3);
    assert_eq!(drained.dropped, 3);
    assert_eq!(drained.receipts, 1);
    assert_eq!(drained.delivery_attempts, vec![(1, 1)]);
    drained.dropped_job_ids.sort_unstable();
    assert_eq!(drained.dropped_job_ids, vec![1, 2, 3]);
    assert_eq!(drained.shutdown_cancelled_job_ids, vec![3]);
    assert_eq!(cancelled["id"], 3);

    let fresh_simulator = TestSimulator::new();
    let fresh = Harness::start(fresh_simulator.clone(), runtime(&fresh_simulator), None).unwrap();
    assert_eq!(fresh.read("/jobs"), json!([]));
    assert_eq!(fresh.read("/notifications"), json!([]));
    fresh.shutdown();
    without_jobs();

    json!({
        "schema": corpus["schema"],
        "success": completed,
        "failure": failure,
        "notifications": notifications,
        "cancelled_job_id": drained.shutdown_cancelled_job_ids[0],
        "task_dropped_ids": drained.dropped_job_ids,
        "shutdown": "clean",
        "new_tasks_rejected": true,
        "fresh_jobs": [], "fresh_notifications": [],
        "removed_route_statuses": [404, 404]
    })
}
