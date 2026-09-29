//! Real default Process Guest behind the configuration supervisor, not a fake Host.

use std::{
    fs,
    net::SocketAddr,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

use nix::{
    sys::signal::{Signal, kill},
    unistd::Pid,
};
use serde_json::json;

use super::ProcessGuard;

pub fn assert_update_and_restart(distribution: &Path) {
    let private = tempfile::tempdir().unwrap();
    let snapshot = private.path().join("source.json");
    let policy = private.path().join("policy.json");
    let log = private.path().join("supervisor.log");
    write_snapshot(&snapshot, 1, 1024);
    fs::write(
        &policy,
        serde_json::to_vec(&json!({
            "schema": "lenso.configuration-source-policy.v1",
            "source_reference": "process-starter-acceptance",
            "source": {"type": "file", "path": snapshot},
            "objects": [{
                "plugin_id": "lenso.web-ingress",
                "instance_key": "default",
                "fields": ["max_request_body_bytes"]
            }],
            "max_stale_seconds": 12
        }))
        .unwrap(),
    )
    .unwrap();

    let mut child = start(distribution, &policy, &log);
    let first = wait_ready(&mut child, &log, 1);
    super::create_and_read(first, "First");
    let body = json!({"title": "After update", "body": "x".repeat(1200)}).to_string();
    assert_eq!(super::request(first, "POST", "/notes", &body).0, 413);
    write_snapshot(&snapshot, 2, 4096);
    let second = wait_ready(&mut child, &log, 2);
    assert_eq!(super::request(second, "POST", "/notes", &body).0, 201);
    let state: serde_json::Value = serde_json::from_slice(
        &fs::read(distribution.join("intent/.lenso/configuration-source-state.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(state["last_activated"]["revision"], 2);
    stop(&mut child, &log);
    assert!(
        !distribution
            .join(".lenso/supervised-start.uncertain")
            .exists(),
        "confirmed cleanup must permit another ordinary app start"
    );

    // A new supervisor must work without deleting any recovery or state file.
    let mut restarted = start(distribution, &policy, &log);
    let address = wait_ready(&mut restarted, &log, 2);
    assert_eq!(super::request(address, "POST", "/notes", &body).0, 201);
    stop(&mut restarted, &log);
    assert!(
        !distribution
            .join(".lenso/supervised-start.uncertain")
            .exists()
    );
}

fn write_snapshot(path: &Path, revision: u64, limit: usize) {
    let stage = path.with_extension("stage");
    fs::write(
        &stage,
        serde_json::to_vec(&json!({
            "schema": "lenso.plugin-configuration-snapshot.v1",
            "revision": revision,
            "configurations": [{
                "plugin_id": "lenso.web-ingress",
                "instance_key": "default",
                "toml": format!("max_request_body_bytes = {limit}\n")
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    fs::rename(stage, path).unwrap();
}

fn start(distribution: &Path, policy: &Path, log: &Path) -> ProcessGuard {
    ProcessGuard::spawn(
        Command::new(env!("CARGO_BIN_EXE_lenso"))
            .args(["app", "start", "--from"])
            .arg(distribution)
            .arg("--configuration-policy")
            .arg(policy),
        log,
    )
}

fn wait_ready(child: &mut ProcessGuard, log: &Path, revision: u64) -> SocketAddr {
    let deadline = Instant::now() + Duration::from_secs(25);
    loop {
        let output = fs::read_to_string(log).unwrap();
        assert!(
            child.0.try_wait().unwrap().is_none() && Instant::now() < deadline,
            "supervised Process App failed to become ready for revision {revision}:\n{output}"
        );
        if output.contains(&format!(
            "Supervised App ready with external configuration revision {revision}\n"
        )) {
            return output
                .lines()
                .filter_map(|line| line.strip_prefix("Listening on http://"))
                .next_back()
                .unwrap()
                .trim_end_matches('/')
                .parse()
                .unwrap();
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn stop(child: &mut ProcessGuard, log: &Path) {
    kill(
        Pid::from_raw(child.0.id().try_into().unwrap()),
        Signal::SIGTERM,
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success(), "{}", fs::read_to_string(log).unwrap());
            return;
        }
        assert!(
            Instant::now() < deadline,
            "supervised Process App did not stop:\n{}",
            fs::read_to_string(log).unwrap()
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}
