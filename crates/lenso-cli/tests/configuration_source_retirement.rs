#![cfg(unix)]

use std::{
    fs::{self, OpenOptions},
    io::Write as _,
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use lenso_app_plan::authoring::{
    HostCatalog, HostDefaultPlugin, HostPluginRelease, HostSlot, PluginDescriptor,
};
use nix::{
    sys::signal::{Signal, kill, killpg},
    unistd::Pid,
};

const WAIT: Duration = Duration::from_secs(15);

// The shell only forwards the CLI's private readiness path. The Host itself is
// this test executable, so signal handling needs neither Python nor unsafe code.
#[test]
#[ignore = "subprocess fixture, invoked by the fake distribution"]
fn managed_host_helper() {
    let root = PathBuf::from(std::env::var_os("LENSO_RETIREMENT_ROOT").unwrap());
    thread::spawn(|| {
        thread::sleep(Duration::from_secs(30));
        std::process::exit(90);
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut termination =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
        let token = std::env::var("LENSO_MANAGED_SHUTDOWN_TOKEN").unwrap();
        assert_eq!(token.len(), 64);
        assert!(
            token
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );
        let receipt = PathBuf::from(std::env::var_os("LENSO_MANAGED_SHUTDOWN_RECEIPT").unwrap());
        assert!(receipt.is_absolute());
        assert!(!receipt.exists(), "receipt must start absent");
        let mode = fs::read_to_string(root.join("mode")).unwrap();
        let pid = std::process::id();
        for line in fs::read_to_string(root.join("events"))
            .unwrap_or_default()
            .lines()
        {
            if let Some(previous) = line.strip_prefix("ready ") {
                let previous = previous.split_whitespace().next().unwrap().parse().unwrap();
                assert!(
                    kill(Pid::from_raw(previous), None).is_err(),
                    "previous Host is still alive before replacement readiness"
                );
            }
        }
        event(&root, &format!("ready {pid} {token}"));
        let ready = PathBuf::from(std::env::var_os("LENSO_RETIREMENT_READY").unwrap());
        let pending = ready.with_extension("pending");
        fs::write(&pending, b"lenso.local-host-ready.v1\n").unwrap();
        fs::rename(pending, ready).unwrap();
        termination.recv().await.unwrap();
        // This event represents completed managed teardown, not signal receipt.
        event(&root, &format!("stopped {pid}"));
        match mode.as_str() {
            "missing" => {}
            "wrong" => fs::write(&receipt, "0".repeat(64)).unwrap(),
            "old" => fs::write(&receipt, fs::read(root.join("old-token")).unwrap()).unwrap(),
            "newline" => fs::write(&receipt, format!("{token}\n")).unwrap(),
            _ => fs::write(&receipt, token).unwrap(),
        }
        if mode == "nonzero" {
            std::process::exit(7);
        }
        fs::write(root.join(format!("acknowledged-{pid}")), b"").unwrap();
    });
}

fn event(root: &Path, value: &str) {
    writeln!(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("events"))
            .unwrap(),
        "{value}"
    )
    .unwrap();
}

struct Distribution {
    directory: tempfile::TempDir,
}

impl Distribution {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::create_dir(root.join(".lenso")).unwrap();
        fs::create_dir_all(root.join("intent/.lenso")).unwrap();
        fs::write(root.join(".lenso/host-mode"), "native").unwrap();
        let executable = std::env::current_exe().unwrap();
        let quoted = executable.to_str().unwrap().replace('\'', "'\\''");
        let host = root.join(".lenso/host");
        fs::write(
            &host,
            format!(
                "#!/bin/sh\nset -eu\nwhile [ \"$#\" -gt 0 ]; do\n\
                 if [ \"$1\" = --ready-file ]; then\n\
                 export LENSO_RETIREMENT_READY=\"$2\"; shift 2\n\
                 else shift; fi\ndone\n\
                 exec '{quoted}' --exact managed_host_helper --ignored --nocapture\n"
            ),
        )
        .unwrap();
        fs::set_permissions(host, fs::Permissions::from_mode(0o700)).unwrap();
        let descriptor = PluginDescriptor::new("example.agent", "1.0.0", "agent")
            .with_configuration_schema(serde_json::json!({
                "type": "object",
                "properties": {"greeting": {"type": "string"}},
                "additionalProperties": false
            }));
        let catalog = HostCatalog::new(
            [HostSlot::one("agent")],
            [HostPluginRelease::new(descriptor)],
            [HostDefaultPlugin::new("example.agent", "default")],
        );
        fs::write(
            root.join(".lenso/host-catalog.json"),
            serde_json::to_vec(&catalog).unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("policy.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema": "lenso.configuration-source-policy.v1",
                "source_reference": "retirement-test",
                "source": {"type": "file", "path": root.join("snapshot.json")},
                "objects": [{
                    "plugin_id": "example.agent", "instance_key": "default",
                    "fields": ["greeting"]
                }],
                "max_stale_seconds": 20
            }))
            .unwrap(),
        )
        .unwrap();
        let distribution = Self { directory };
        distribution.snapshot(1, "first");
        distribution
    }

    fn root(&self) -> &Path {
        self.directory.path()
    }

    fn snapshot(&self, revision: u64, greeting: &str) {
        let pending = self.root().join("snapshot.pending");
        fs::write(
            &pending,
            serde_json::to_vec(&serde_json::json!({
                "schema": "lenso.plugin-configuration-snapshot.v1",
                "revision": revision,
                "configurations": [{
                    "plugin_id": "example.agent", "instance_key": "default",
                    "toml": format!("greeting = '{greeting}'\n")
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        fs::rename(pending, self.root().join("snapshot.json")).unwrap();
    }

    fn launch(&self, mode: &str, attempt: usize) -> Supervisor {
        fs::write(self.root().join("mode"), mode).unwrap();
        let ready = self.root().join(format!("ready-{attempt}"));
        let log = self.root().join(format!("stderr-{attempt}"));
        let child = Command::new(env!("CARGO_BIN_EXE_lenso"))
            .args(["app", "start", "--from"])
            .arg(self.root())
            .arg("--configuration-policy")
            .arg(self.root().join("policy.json"))
            .arg("--ready-file")
            .arg(&ready)
            .env("LENSO_RETIREMENT_ROOT", self.root())
            .stdout(Stdio::null())
            .stderr(fs::File::create(&log).unwrap())
            .spawn()
            .unwrap();
        Supervisor { child, ready, log }
    }

    fn events(&self) -> Vec<String> {
        fs::read_to_string(self.root().join("events"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn fence_exists(&self) -> bool {
        self.root()
            .join(".lenso/supervised-start.uncertain")
            .exists()
    }

    fn wait_revision(&self, supervisor: &mut Supervisor, field: &str, revision: u64) {
        supervisor.wait_until(|| {
            let state = fs::read(
                self.root()
                    .join("intent/.lenso/configuration-source-state.json"),
            )
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
            state.is_some_and(|state| state[field]["revision"] == revision)
        });
    }

    fn assert_fenced(&self, attempt: usize) {
        let events = self.events();
        let mut second = self.launch("clean", attempt);
        assert!(!second.finish().success(), "uncertain start was accepted");
        assert!(
            second.stderr().contains("unconfirmed"),
            "{}",
            second.stderr()
        );
        assert!(!second.ready.exists());
        assert!(self.fence_exists());
        assert_eq!(self.events(), events, "fenced start launched another Host");
    }
}

impl Drop for Distribution {
    fn drop(&mut self) {
        // Hosts have a watchdog as a backstop even if the supervisor is killed.
        for line in self.events() {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.first() == Some(&"ready")
                && let Some(pid) = fields.get(1).and_then(|pid| pid.parse::<i32>().ok())
            {
                let process = Command::new("ps")
                    .args(["-o", "pgid=,command=", "-p", &pid.to_string()])
                    .output();
                if let Ok(process) = process {
                    let text = String::from_utf8_lossy(&process.stdout);
                    if text.split_whitespace().next() == Some(pid.to_string().as_str())
                        && text.contains("--exact managed_host_helper --ignored")
                    {
                        let _ = killpg(Pid::from_raw(pid), Signal::SIGKILL);
                    }
                }
            }
        }
    }
}

struct Supervisor {
    child: Child,
    ready: PathBuf,
    log: PathBuf,
}

impl Supervisor {
    fn stderr(&self) -> String {
        fs::read_to_string(&self.log).unwrap()
    }

    fn wait_until(&mut self, condition: impl Fn() -> bool) {
        let deadline = Instant::now() + WAIT;
        while !condition() {
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "supervisor exited early: {}",
                self.stderr()
            );
            assert!(Instant::now() < deadline, "timed out: {}", self.stderr());
            thread::sleep(Duration::from_millis(25));
        }
    }

    fn wait_ready(&mut self) {
        let ready = self.ready.clone();
        self.wait_until(|| ready.exists());
    }

    fn signal(&self, signal: Signal) {
        kill(
            Pid::from_raw(i32::try_from(self.child.id()).unwrap()),
            signal,
        )
        .unwrap();
    }

    fn finish(&mut self) -> ExitStatus {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(Instant::now() < deadline, "did not stop: {}", self.stderr());
            thread::sleep(Duration::from_millis(25));
        }
    }

    fn stop_cleanly(&mut self) {
        self.signal(Signal::SIGTERM);
        assert!(self.finish().success(), "{}", self.stderr());
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[test]
fn cooperative_sigterm_allows_repeated_starts_without_operator_cleanup() {
    let distribution = Distribution::new();
    let mut tokens = Vec::new();
    for attempt in 0..3 {
        let mut supervisor = distribution.launch("clean", attempt);
        supervisor.wait_ready();
        assert!(
            distribution.fence_exists(),
            "live supervisor must hold a fence"
        );
        supervisor.stop_cleanly();
        assert!(!distribution.fence_exists(), "clean teardown left a fence");
        let events = distribution.events();
        assert_eq!(events.len(), (attempt + 1) * 2);
        let ready: Vec<_> = events[attempt * 2].split_whitespace().collect();
        assert_eq!(events[attempt * 2 + 1], format!("stopped {}", ready[1]));
        tokens.push(ready[2].to_owned());
    }
    tokens.sort();
    tokens.dedup();
    assert_eq!(tokens.len(), 3, "launches reused shutdown authority");
}

#[test]
fn changed_root_retires_old_host_before_new_readiness() {
    let distribution = Distribution::new();
    let mut supervisor = distribution.launch("clean", 0);
    supervisor.wait_ready();
    distribution.wait_revision(&mut supervisor, "last_activated", 1);
    distribution.snapshot(2, "second");
    distribution.wait_revision(&mut supervisor, "last_activated", 2);
    let events = distribution.events();
    assert_eq!(events.len(), 3, "{events:?}");
    let old: Vec<_> = events[0].split_whitespace().collect();
    let new: Vec<_> = events[2].split_whitespace().collect();
    assert_eq!(events[1], format!("stopped {}", old[1]));
    assert_eq!(new[0], "ready");
    assert_ne!(old[1], new[1]);
    assert_ne!(old[2], new[2]);
    assert!(kill(Pid::from_raw(old[1].parse().unwrap()), None).is_err());
    supervisor.stop_cleanly();
    assert!(!distribution.fence_exists());
}

#[test]
fn accepted_unchanged_root_does_not_restart_host() {
    let distribution = Distribution::new();
    let mut supervisor = distribution.launch("clean", 0);
    supervisor.wait_ready();
    distribution.wait_revision(&mut supervisor, "last_activated", 1);
    let original = distribution.events();
    distribution.snapshot(2, "first");
    distribution.wait_revision(&mut supervisor, "desired", 2);
    // Acceptance is persisted before the supervisor consumes the sync result.
    thread::sleep(Duration::from_secs(2));
    assert!(supervisor.child.try_wait().unwrap().is_none());
    assert_eq!(distribution.events(), original);
    supervisor.stop_cleanly();
    assert!(!distribution.fence_exists());
}

#[test]
fn missing_wrong_or_nonexact_receipt_and_nonzero_exit_keep_fence() {
    for mode in ["missing", "wrong", "newline", "nonzero"] {
        let distribution = Distribution::new();
        let mut supervisor = distribution.launch(mode, 0);
        supervisor.wait_ready();
        supervisor.signal(Signal::SIGTERM);
        assert!(
            !supervisor.finish().success(),
            "{mode}: {}",
            supervisor.stderr()
        );
        assert!(distribution.fence_exists(), "{mode}: lost uncertain fence");
        distribution.assert_fenced(1);
    }
}

#[test]
fn previous_launch_token_cannot_retire_current_host() {
    let distribution = Distribution::new();
    let mut first = distribution.launch("clean", 0);
    first.wait_ready();
    let token = distribution.events()[0]
        .split_whitespace()
        .nth(2)
        .unwrap()
        .to_owned();
    first.stop_cleanly();
    fs::write(distribution.root().join("old-token"), token).unwrap();
    let mut second = distribution.launch("old", 1);
    second.wait_ready();
    second.signal(Signal::SIGTERM);
    assert!(!second.finish().success());
    assert!(distribution.fence_exists());
    distribution.assert_fenced(2);
}

#[test]
fn killed_supervisor_retains_fence_even_after_host_acknowledges() {
    let distribution = Distribution::new();
    let mut supervisor = distribution.launch("clean", 0);
    supervisor.wait_ready();
    let pid = distribution.events()[0]
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    supervisor.signal(Signal::SIGKILL);
    assert!(!supervisor.finish().success());
    distribution.assert_fenced(1);
    kill(Pid::from_raw(pid), Signal::SIGTERM).unwrap();
    let deadline = Instant::now() + WAIT;
    while !distribution
        .root()
        .join(format!("acknowledged-{pid}"))
        .exists()
    {
        assert!(Instant::now() < deadline, "orphan Host did not stop");
        thread::sleep(Duration::from_millis(25));
    }
    distribution.assert_fenced(2);
}
