#![cfg(unix)]

use std::{
    fs::{self, File},
    io::{Read as _, Write as _},
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct DevGuard(Child);

impl DevGuard {
    fn stop(&mut self) -> bool {
        use nix::{
            sys::signal::{Signal, kill},
            unistd::Pid,
        };

        if let Some(status) = self.0.try_wait().ok().flatten() {
            return status.success();
        }
        if let Ok(id) = self.0.id().try_into() {
            let pid = Pid::from_raw(id);
            for _ in 0..40 {
                let _ = kill(pid, Signal::SIGINT);
                if let Some(status) = self.0.try_wait().ok().flatten() {
                    return status.success();
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            let _ = kill(pid, Signal::SIGTERM);
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
        false
    }
}

impl Drop for DevGuard {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn write_snapshot(path: &Path, revision: u64, toml: &str) {
    fs::write(
        path,
        serde_json::to_vec(&serde_json::json!({
            "schema": "lenso.plugin-configuration-snapshot.v1",
            "revision": revision,
            "configurations": [{
                "plugin_id": "local.starter", "instance_key": "default", "toml": toml
            }]
        }))
        .unwrap(),
    )
    .unwrap();
}

fn write_openapi_snapshot(path: &Path, revision: u64, toml: &str) {
    fs::write(
        path,
        serde_json::to_vec(&serde_json::json!({
            "schema": "lenso.plugin-configuration-snapshot.v1",
            "revision": revision,
            "configurations": [{
                "plugin_id": "lenso.openapi", "instance_key": "default", "toml": toml
            }]
        }))
        .unwrap(),
    )
    .unwrap();
}

fn openapi_title(log: &Path) -> String {
    let output = fs::read_to_string(log).unwrap();
    let address: SocketAddr = output
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix("Listening on http://"))
        .and_then(|address| address.strip_suffix('/'))
        .unwrap_or_else(|| panic!("App has no Web listener: {output}"))
        .parse()
        .unwrap();
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write!(
        stream,
        "GET /openapi.json HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    assert!(head.starts_with("HTTP/1.1 200"), "{response}");
    serde_json::from_str::<serde_json::Value>(body).unwrap()["info"]["title"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn generations(source: &Path) -> Vec<PathBuf> {
    let Some(dev) = fs::read_dir(source.join(".lenso"))
        .ok()
        .and_then(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .find(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("dev-"))
                })
        })
    else {
        return Vec::new();
    };
    fs::read_dir(dev)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("generation-"))
        })
        .collect()
}

fn generation(source: &Path) -> Option<PathBuf> {
    generations(source)
        .into_iter()
        .filter_map(|path| {
            let number = path
                .file_name()?
                .to_str()?
                .strip_prefix("generation-")?
                .parse::<u64>()
                .ok()?;
            Some((number, path))
        })
        .max_by_key(|(number, _)| *number)
        .map(|(_, path)| path)
}

fn await_revision(source: &Path, dev: &mut Child, revision: u64, log: &Path) -> PathBuf {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        for output in generations(source) {
            let state_path = output.join("intent/.lenso/configuration-source-state.json");
            if let Ok(bytes) = fs::read(&state_path)
                && let Ok(state) = serde_json::from_slice::<serde_json::Value>(&bytes)
                && state["last_activated"]["revision"] == revision
            {
                assert!(dev.try_wait().unwrap().is_none());
                let status = Command::new(env!("CARGO_BIN_EXE_lenso"))
                    .args(["app", "config-status", "--root"])
                    .arg(&output)
                    .arg("--json")
                    .output()
                    .unwrap();
                assert!(status.status.success());
                let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
                assert_eq!(status["desired_revision"], revision);
                assert_eq!(status["last_activated_revision"], revision);
                assert_eq!(status["pending_activation"], false);
                return output;
            }
        }
        assert!(
            dev.try_wait().unwrap().is_none(),
            "App development Host exited: {}",
            fs::read_to_string(log).unwrap_or_default()
        );
        assert!(
            Instant::now() < deadline,
            "configuration revision {revision} did not activate: {} | states: {:?}",
            fs::read_to_string(log).unwrap_or_default(),
            generations(source)
                .into_iter()
                .map(|output| (
                    output.display().to_string(),
                    fs::read_to_string(
                        output.join("intent/.lenso/configuration-source-state.json")
                    )
                    .unwrap_or_default()
                ))
                .collect::<Vec<_>>()
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn await_source_outage(dev: &mut Child, log: &Path, from: usize) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let bytes = fs::read(log).unwrap_or_default();
        if bytes.len() > from
            && String::from_utf8_lossy(&bytes[from..])
                .contains("Configuration source unavailable or rejected")
        {
            return;
        }
        assert!(
            dev.try_wait().unwrap().is_none(),
            "App development supervisor exited during source outage"
        );
        assert!(
            Instant::now() < deadline,
            "source outage was not observed: {}",
            String::from_utf8_lossy(&bytes)
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn real_process_host_recovers_missing_file_source_and_activates_new_revision() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("source");
    let cli = env!("CARGO_BIN_EXE_lenso");
    let created = Command::new(cli)
        .args(["app", "create"])
        .arg(&source)
        .args(["--runtime", "process", "--no-install"])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let snapshot = temporary.path().join("snapshot.json");
    let policy = temporary.path().join("policy.json");
    let log = temporary.path().join("dev.log");
    fs::write(
        &policy,
        serde_json::to_vec(&serde_json::json!({
            "schema": "lenso.configuration-source-policy.v1",
            "source_reference": "development",
            "source": {"type": "file", "path": snapshot},
            "objects": [{"plugin_id": "local.starter", "instance_key": "default", "fields": ["unused"]}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut dev = DevGuard(
        Command::new(cli)
            .args(["app", "dev", "--root"])
            .arg(&source)
            .arg("--configuration-policy")
            .arg(&policy)
            .args(["--configuration-poll-seconds", "1"])
            .stdout(Stdio::null())
            .stderr(Stdio::from(File::create(&log).unwrap()))
            .spawn()
            .unwrap(),
    );

    let deadline = Instant::now() + Duration::from_secs(120);
    while !generation(&source).is_some_and(|path| path.join(".lenso/host").is_file()) {
        assert!(dev.0.try_wait().unwrap().is_none());
        assert!(Instant::now() < deadline, "Host distribution was not built");
        std::thread::sleep(Duration::from_millis(200));
    }
    // The first source read failed. The poll loop must recover without an App
    // source edit or a second build, and must not invent an activation receipt.
    let output = generation(&source).unwrap();
    std::thread::sleep(Duration::from_secs(2));
    assert!(
        !output
            .join("intent/.lenso/configuration-source-state.json")
            .exists()
    );
    write_snapshot(&snapshot, 1, "");
    let output = await_revision(&source, &mut dev.0, 1, &log);
    assert_eq!(generation(&source).unwrap(), output);

    write_snapshot(&snapshot, 2, "");
    await_revision(&source, &mut dev.0, 2, &log);

    // Unlike the initial missing source, this outage happens after a real
    // Host has activated. Poll failure must retain that Host and its receipt;
    // restoring a later revision must use the same built distribution.
    let log_offset = usize::try_from(fs::metadata(&log).unwrap().len()).unwrap();
    fs::remove_file(&snapshot).unwrap();
    await_source_outage(&mut dev.0, &log, log_offset);
    assert_eq!(generation(&source).unwrap(), output);
    let state: serde_json::Value = serde_json::from_slice(
        &fs::read(output.join("intent/.lenso/configuration-source-state.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(state["desired"]["revision"], 2);
    assert_eq!(state["last_activated"]["revision"], 2);
    let status = Command::new(cli)
        .args(["app", "config-status", "--root"])
        .arg(&output)
        .arg("--json")
        .output()
        .unwrap();
    assert!(status.status.success());
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["state"], "last_activated");
    assert_eq!(status["desired_revision"], 2);
    assert_eq!(status["last_activated_revision"], 2);
    assert_eq!(status["pending_activation"], false);
    assert!(
        !fs::read_to_string(&log)
            .unwrap()
            .contains("Local Host exited")
    );

    write_snapshot(&snapshot, 3, "");
    assert_eq!(await_revision(&source, &mut dev.0, 3, &log), output);
    write_snapshot(&snapshot, 4, "unauthorized = 'no'\n");
    std::thread::sleep(Duration::from_secs(2));
    let state: serde_json::Value = serde_json::from_slice(
        &fs::read(output.join("intent/.lenso/configuration-source-state.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(state["desired"]["revision"], 3);
    assert_eq!(state["last_activated"]["revision"], 3);
    assert!(dev.0.try_wait().unwrap().is_none());
    assert!(
        dev.stop(),
        "App development supervisor did not stop cleanly"
    );
}

#[test]
fn external_configuration_changes_the_running_apps_openapi_title_without_a_rebuild() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("source");
    let cli = env!("CARGO_BIN_EXE_lenso");
    let created = Command::new(cli)
        .args(["app", "create"])
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let added = Command::new(cli)
        .args(["app", "add", "@lenso/openapi", "--root"])
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );

    let snapshot = temporary.path().join("snapshot.json");
    let policy = temporary.path().join("policy.json");
    let log = temporary.path().join("dev.log");
    write_openapi_snapshot(&snapshot, 1, "title = 'First API'\n");
    fs::write(
        &policy,
        serde_json::to_vec(&serde_json::json!({
            "schema": "lenso.configuration-source-policy.v1",
            "source_reference": "openapi-development",
            "source": {"type": "file", "path": snapshot},
            "objects": [{
                "plugin_id": "lenso.openapi", "instance_key": "default", "fields": ["title"]
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut dev = DevGuard(
        Command::new(cli)
            .args(["app", "dev", "--root"])
            .arg(&source)
            .arg("--configuration-policy")
            .arg(&policy)
            .args(["--configuration-poll-seconds", "1"])
            .stdout(Stdio::null())
            .stderr(Stdio::from(File::create(&log).unwrap()))
            .spawn()
            .unwrap(),
    );

    let first = await_revision(&source, &mut dev.0, 1, &log);
    assert_eq!(openapi_title(&log), "First API");

    write_openapi_snapshot(&snapshot, 2, "title = 'Second API'\n");
    assert_eq!(await_revision(&source, &mut dev.0, 2, &log), first);
    assert_eq!(openapi_title(&log), "Second API");
    assert_eq!(generations(&source), vec![first.clone()]);

    let log_offset = usize::try_from(fs::metadata(&log).unwrap().len()).unwrap();
    write_openapi_snapshot(&snapshot, 3, "title = 'Rejected API'\nversion = '2.0.0'\n");
    await_source_outage(&mut dev.0, &log, log_offset);
    assert_eq!(openapi_title(&log), "Second API");
    let log_offset = usize::try_from(fs::metadata(&log).unwrap().len()).unwrap();
    write_openapi_snapshot(&snapshot, 1, "title = 'Stale API'\n");
    await_source_outage(&mut dev.0, &log, log_offset);
    let active = await_revision(&source, &mut dev.0, 2, &log);
    assert_eq!(active, first);
    assert_eq!(openapi_title(&log), "Second API");
    assert!(
        dev.stop(),
        "App development supervisor did not stop cleanly"
    );
}
