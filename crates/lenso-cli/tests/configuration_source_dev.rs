#![cfg(unix)]

use std::{
    fs::{self, File},
    io::{Read as _, Write as _},
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

// Both cases compile independent release-mode Apps. Share the runner's small
// CI machine instead of making their cold builds compete for the same budget.
static APP_BUILD_TEST: Mutex<()> = Mutex::new(());

struct Phase<'a> {
    name: String,
    start: Instant,
    log: &'a Path,
    offset: usize,
}

impl<'a> Phase<'a> {
    fn new(name: impl Into<String>, log: &'a Path) -> Self {
        Self {
            name: name.into(),
            start: Instant::now(),
            log,
            offset: fs::read(log).unwrap_or_default().len(),
        }
    }
}

impl Drop for Phase<'_> {
    fn drop(&mut self) {
        let bytes = fs::read(self.log).unwrap_or_default();
        let log = String::from_utf8_lossy(bytes.get(self.offset..).unwrap_or_default());
        let builds = log
            .lines()
            .filter(|line| line.contains("Finished "))
            .filter_map(|line| line.split_once("target(s) in ").map(|(_, time)| time))
            .filter(|time| {
                time.len() <= 32
                    && time
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || b". hms".contains(&byte))
            })
            .rev()
            .take(6)
            .collect::<Vec<_>>();
        let compiled = log
            .lines()
            .filter(|line| line.contains("Compiling "))
            .count();
        let lock_waits = log
            .lines()
            .filter(|line| line.contains("Blocking waiting"))
            .count();
        // Direct stderr survives libtest's successful-test capture. Keep source
        // contents and paths out of CI output; only report build timing/counts.
        let _ = writeln!(
            std::io::stderr().lock(),
            "[configuration_source_dev/{}] {}: {:.2?}; visible Cargo durations (newest first): \
             {builds:?}; compilations: {compiled}; lock waits: {lock_waits}",
            std::thread::current().name().unwrap_or("unknown"),
            self.name,
            self.start.elapsed()
        );
    }
}

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

fn candidate_crate_patches(packages: &[&str]) -> String {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut patches = toml::map::Map::new();
    for package in packages {
        let path = crates.join(package).canonicalize().unwrap();
        patches.insert(
            (*package).to_owned(),
            toml::Value::Table(toml::map::Map::from_iter([(
                "path".to_owned(),
                toml::Value::String(path.to_str().unwrap().to_owned()),
            )])),
        );
    }
    toml::to_string(&toml::Value::Table(toml::map::Map::from_iter([(
        "patch".to_owned(),
        toml::Value::Table(toml::map::Map::from_iter([(
            "crates-io".to_owned(),
            toml::Value::Table(patches),
        )])),
    )])))
    .unwrap()
}

fn use_candidate_crates(root: &Path, packages: &[&str]) {
    fs::create_dir_all(root.join(".cargo")).unwrap();
    fs::write(
        root.join(".cargo/config.toml"),
        candidate_crate_patches(packages),
    )
    .unwrap();
}

fn recent_cargo_diagnostics(log: &str) -> Vec<&str> {
    log.lines()
        .rev()
        .filter(|line| {
            line.contains("Compiling ")
                || line.contains("error:")
                || line.contains("failed")
                || line.contains("candidate versions")
        })
        .take(12)
        .collect()
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
    let mut outputs = fs::read_dir(source.join(".lenso"))
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("dev-"))
        })
        .flat_map(|dev| {
            fs::read_dir(dev)
                .ok()
                .into_iter()
                .flat_map(|entries| entries.filter_map(Result::ok))
        })
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("generation-"))
        })
        .collect::<Vec<_>>();
    outputs.sort();
    outputs
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
    await_source_state(source, dev, revision, revision, log)
}

fn await_source_state(
    source: &Path,
    dev: &mut Child,
    revision: u64,
    activated_revision: u64,
    log: &Path,
) -> PathBuf {
    let _phase = Phase::new(
        format!("source revision {revision}, last activation {activated_revision}"),
        log,
    );
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        for output in generations(source) {
            let state_path = output.join("intent/.lenso/configuration-source-state.json");
            if let Ok(bytes) = fs::read(&state_path)
                && let Ok(state) = serde_json::from_slice::<serde_json::Value>(&bytes)
                && state["last_activated"]["revision"] == activated_revision
                && state["desired"]["revision"] == revision
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
                assert_eq!(status["last_activated_revision"], activated_revision);
                assert_eq!(status["pending_activation"], revision != activated_revision);
                assert_eq!(
                    status["desired_matches_last_activated_root_and_policy"],
                    true
                );
                return output;
            }
        }
        assert!(
            dev.try_wait().unwrap().is_none(),
            "App development Host exited: {}",
            fs::read_to_string(log).unwrap_or_default()
        );
        let output = fs::read_to_string(log).unwrap_or_default();
        assert!(
            !output.contains("App rebuild failed; edit the source to retry.")
                && !output
                    .contains("Configuration candidate failed readiness; candidate not activated")
                && !output.contains("this session cannot safely retry dynamic activation"),
            "App build or readiness failed: {:?}",
            recent_cargo_diagnostics(&output)
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

fn start_dev(cli: &str, source: &Path, policy: &Path, log: &Path, cwd: Option<&Path>) -> DevGuard {
    let mut command = Command::new(cli);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    DevGuard(
        command
            .args(["app", "dev", "--root"])
            .arg(source)
            .arg("--configuration-policy")
            .arg(policy)
            .args(["--configuration-poll-seconds", "1"])
            .stdout(Stdio::null())
            .stderr(Stdio::from(
                File::options().create(true).append(true).open(log).unwrap(),
            ))
            .spawn()
            .unwrap(),
    )
}

fn await_source_outage(dev: &mut Child, log: &Path, from: usize) {
    let _phase = Phase::new("source outage or rejection", log);
    // A pending candidate can already be inside --prepare when its source is
    // removed; that check has a 60-second bound before the next source poll.
    let deadline = Instant::now() + Duration::from_secs(75);
    loop {
        let bytes = fs::read(log).unwrap_or_default();
        if bytes.len() > from {
            let new_log = String::from_utf8_lossy(&bytes[from..]);
            if new_log.contains("Configuration source unavailable or rejected")
                || new_log.contains("Configuration source unavailable during rebuild")
            {
                return;
            }
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
#[expect(
    clippy::too_many_lines,
    reason = "the ordered outage, revision renewal, and clean restart form one Process Host lifecycle"
)]
fn real_process_host_recovers_missing_file_source_and_activates_new_revision() {
    let _serial_build = APP_BUILD_TEST
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
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
    let guest_manifest = source.join("app/local.starter/Cargo.toml");
    let mut manifest = fs::read_to_string(&guest_manifest).unwrap();
    manifest.push_str(&candidate_crate_patches(&["lenso-plugin-sdk"]));
    fs::write(&guest_manifest, manifest).unwrap();
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
    let mut dev = start_dev(cli, &source, &policy, &log, None);

    let build_phase = Phase::new("initial Process distribution build", &log);
    let deadline = Instant::now() + Duration::from_secs(300);
    while !generation(&source).is_some_and(|path| path.join(".lenso/host").is_file()) {
        assert!(dev.0.try_wait().unwrap().is_none());
        let output = fs::read_to_string(&log).unwrap_or_default();
        assert!(
            !output.contains("App rebuild failed; edit the source to retry."),
            "App distribution build failed; recent Cargo diagnostics: {:?}",
            recent_cargo_diagnostics(&output)
        );
        assert!(
            Instant::now() < deadline,
            "App distribution was not built; recent Cargo diagnostics: {:?}",
            recent_cargo_diagnostics(&output)
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    drop(build_phase);
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
    assert_eq!(await_source_state(&source, &mut dev.0, 2, 1, &log), output);

    // Unlike the initial missing source, this outage happens after a real
    // Host has activated. Poll failure must retain that Host and its receipt,
    // with renewed source authority but the original activation receipt.
    let log_offset = usize::try_from(fs::metadata(&log).unwrap().len()).unwrap();
    fs::remove_file(&snapshot).unwrap();
    await_source_outage(&mut dev.0, &log, log_offset);
    let state: serde_json::Value = serde_json::from_slice(
        &fs::read(output.join("intent/.lenso/configuration-source-state.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(state["desired"]["revision"], 2);
    assert_eq!(state["last_activated"]["revision"], 1);
    let status = Command::new(cli)
        .args(["app", "config-status", "--root"])
        .arg(&output)
        .arg("--json")
        .output()
        .unwrap();
    assert!(status.status.success());
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["desired_revision"], 2);
    assert_eq!(status["last_activated_revision"], 1);
    assert_eq!(status["pending_activation"], true);
    assert!(
        !fs::read_to_string(&log)
            .unwrap()
            .contains("Local Host exited")
    );

    write_snapshot(&snapshot, 3, "");
    assert_eq!(await_source_state(&source, &mut dev.0, 3, 1, &log), output);
    let log_offset = usize::try_from(fs::metadata(&log).unwrap().len()).unwrap();
    write_snapshot(&snapshot, 4, "unauthorized = 'no'\n");
    await_source_outage(&mut dev.0, &log, log_offset);
    let state: serde_json::Value = serde_json::from_slice(
        &fs::read(output.join("intent/.lenso/configuration-source-state.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(state["desired"]["revision"], 3);
    assert_eq!(state["last_activated"]["revision"], 1);
    assert!(dev.0.try_wait().unwrap().is_none());
    assert!(
        dev.stop(),
        "App development supervisor did not stop cleanly"
    );
    assert!(!output.exists());

    write_snapshot(&snapshot, 3, "");
    let mut dev = start_dev(cli, &source, &policy, &log, None);
    let resumed = await_revision(&source, &mut dev.0, 3, &log);
    assert_ne!(resumed, output);
    assert!(
        dev.stop(),
        "App development supervisor did not stop cleanly"
    );
}

#[test]
fn external_configuration_changes_openapi_title_after_supervised_restart() {
    let _serial_build = APP_BUILD_TEST
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("source");
    let cli = env!("CARGO_BIN_EXE_lenso");
    use_candidate_crates(
        temporary.path(),
        &[
            "lenso",
            "lenso-openapi-plugin",
            "lenso-app-plan",
            "lenso-runner",
            "lenso-capability-http-endpoint",
            "lenso-web-host",
            "lenso-test",
            "lenso-kernel",
        ],
    );
    let created = Command::new(cli)
        .current_dir(temporary.path())
        .args(["app", "create"])
        .arg(&source)
        .args(["--web", "--no-install"])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let added = Command::new(cli)
        .current_dir(temporary.path())
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
    let mut dev = start_dev(cli, &source, &policy, &log, Some(temporary.path()));

    let first = await_revision(&source, &mut dev.0, 1, &log);
    assert_eq!(openapi_title(&log), "First API");

    write_openapi_snapshot(&snapshot, 2, "title = 'Second API'\n");
    let second = await_revision(&source, &mut dev.0, 2, &log);
    assert_ne!(second, first, "changed Root needs a new built Generation");
    assert_eq!(openapi_title(&log), "Second API");
    assert_eq!(generations(&source), vec![second.clone()]);

    let log_offset = usize::try_from(fs::metadata(&log).unwrap().len()).unwrap();
    write_openapi_snapshot(&snapshot, 3, "title = 'Rejected API'\nversion = '2.0.0'\n");
    await_source_outage(&mut dev.0, &log, log_offset);
    assert_eq!(openapi_title(&log), "Second API");
    let log_offset = usize::try_from(fs::metadata(&log).unwrap().len()).unwrap();
    write_openapi_snapshot(&snapshot, 1, "title = 'Stale API'\n");
    await_source_outage(&mut dev.0, &log, log_offset);
    let active = await_revision(&source, &mut dev.0, 2, &log);
    assert_eq!(active, second);
    assert_eq!(openapi_title(&log), "Second API");
    assert!(
        dev.stop(),
        "App development supervisor did not stop cleanly"
    );
}
