use super::*;
use lenso_app_plan::RestartPolicy;
use lenso_kernel::ShutdownOutcome;

#[test]
fn normal_retirement_is_clean_but_a_reported_protocol_failure_poisoned_the_witness() {
    for protocol_failure in [false, true] {
        let source = Arc::new(Mutex::new(BTreeMap::new()));
        let destination = Arc::new(Mutex::new(BTreeMap::new()));
        let source_calls = Arc::new(AtomicUsize::new(0));
        let destination_calls = Arc::new(AtomicUsize::new(0));
        let (driver, app, evidence) = start_process_app_with_supervision(
            &source,
            &destination,
            &source_calls,
            &destination_calls,
            ProcessLimits::default(),
            false,
            &json!({}),
            false,
            RestartPolicy::default(),
        );
        let outcome = driver.run(app.handle::<Sync>("consumer").unwrap().invoke(
            "sync",
            json!({"protocol_failure": protocol_failure, "domain_error": !protocol_failure}),
        ));
        if protocol_failure {
            assert!(matches!(
                outcome,
                Err(RuntimeFailure::ProtocolViolation { .. })
            ));
        } else {
            assert_eq!(outcome.unwrap().unwrap_err(), json!({"kind": "rejected"}));
        }
        assert_eq!(
            driver.run(app.shutdown(Duration::from_secs(1))),
            ShutdownOutcome::Clean
        );
        drop(app);
        assert_eq!(evidence.is_clean(), !protocol_failure);
    }
}

#[test]
fn normal_stop_hook_failure_and_nonzero_exit_remain_cleanup_errors() {
    for (configuration, detail) in [
        (json!({"fail_stop": true}), "fixture stop hook failed"),
        (
            json!({"nonzero_after_stopped": true}),
            "shutdown exited with",
        ),
    ] {
        let source = Arc::new(Mutex::new(BTreeMap::new()));
        let destination = Arc::new(Mutex::new(BTreeMap::new()));
        let source_calls = Arc::new(AtomicUsize::new(0));
        let destination_calls = Arc::new(AtomicUsize::new(0));
        let (driver, app, evidence) = start_process_app_with_supervision(
            &source,
            &destination,
            &source_calls,
            &destination_calls,
            ProcessLimits::default(),
            false,
            &configuration,
            false,
            RestartPolicy::default(),
        );
        let outcome = driver.run(app.shutdown(Duration::from_secs(1)));
        assert!(
            matches!(&outcome, ShutdownOutcome::RuntimeFailure {
                error: RuntimeFailure::PluginFailure { detail: actual }
            } if actual.contains(detail)),
            "unexpected shutdown: {outcome:?}"
        );
        drop(app);
        assert!(!evidence.is_clean());
    }
}

#[test]
fn crashed_process_restarts_without_replay_and_keeps_shutdown_evidence_unclean() {
    for generic_engine in [false, true] {
        let source = Arc::new(Mutex::new(BTreeMap::from([(
            "guide".to_owned(),
            "recovered".to_owned(),
        )])));
        let destination = Arc::new(Mutex::new(BTreeMap::new()));
        let source_calls = Arc::new(AtomicUsize::new(0));
        let destination_calls = Arc::new(AtomicUsize::new(0));
        let (driver, app, evidence) = start_process_app_with_supervision(
            &source,
            &destination,
            &source_calls,
            &destination_calls,
            ProcessLimits::default(),
            generic_engine,
            &json!({}),
            false,
            RestartPolicy::on_failure(
                1,
                Duration::from_secs(5),
                Duration::ZERO,
                Duration::ZERO,
                Duration::ZERO,
            ),
        );
        let handle = app.handle::<Sync>("consumer").unwrap();
        let directory = tempfile::tempdir().unwrap();
        let log = directory.path().join("calls");
        let result = driver.run(handle.invoke("sync", json!({"crash_log": log})));
        assert!(matches!(result, Err(RuntimeFailure::PluginFailure { .. })));
        for _ in 0..100 {
            driver.run(driver.yield_now());
            if app
                .plugin_generation("sync")
                .is_some_and(|value| value >= 2)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(app.plugin_generation("sync"), Some(2));
        assert_eq!(
            driver
                .run(handle.invoke("sync", json!({"document": "guide"})))
                .unwrap()
                .unwrap(),
            json!({"document": "guide", "text": "recovered"})
        );
        assert_eq!(source_calls.load(Ordering::Relaxed), 1);
        assert_eq!(destination_calls.load(Ordering::Relaxed), 1);
        assert_eq!(fs::read_to_string(log).unwrap(), "invoked\n");
        assert_eq!(
            driver.run(app.shutdown(Duration::from_secs(1))),
            ShutdownOutcome::Clean
        );
        drop((handle, app));
        assert!(!evidence.is_clean());
    }
}
