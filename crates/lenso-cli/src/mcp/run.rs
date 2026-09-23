//! Explicitly enabled, fixed-distribution lifecycle for one local MCP App run.
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use anyhow::{Context as _, bail, ensure};
use serde::Serialize;

const READY_RECEIPT: &[u8] = b"lenso.local-host-ready.v1\n";

#[derive(Clone, Debug, Serialize)]
pub(super) struct RunStatus {
    schema_version: u32,
    kind: &'static str,
    request_id: String,
    state: &'static str,
    distribution: PathBuf,
    timeout_seconds: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    diagnostic_code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    help: Option<&'static str>,
}

#[derive(Debug)]
struct RunEntry {
    status: RunStatus,
    stop: Arc<AtomicBool>,
}

#[derive(Debug, Default)]
struct RunState {
    entries: BTreeMap<String, RunEntry>,
    active: Option<String>,
}

#[derive(Debug, Default)]
pub(super) struct RunController {
    state: Arc<Mutex<RunState>>,
    workers: Mutex<Vec<JoinHandle<()>>>,
}

impl Drop for RunController {
    fn drop(&mut self) {
        if let Ok(state) = self.state.lock() {
            for entry in state.entries.values() {
                entry.stop.store(true, Ordering::SeqCst);
            }
        }
        if let Ok(workers) = self.workers.get_mut() {
            for worker in workers.drain(..) {
                let _ = worker.join();
            }
        }
    }
}

impl RunController {
    pub(super) fn start(
        &self,
        root: &Path,
        request_id: &str,
        timeout_seconds: u64,
    ) -> anyhow::Result<RunStatus> {
        let executable = std::env::current_exe().context("locate local Lenso CLI")?;
        self.start_with_executable(root, request_id, timeout_seconds, &executable, true)
    }

    fn start_with_executable(
        &self,
        root: &Path,
        request_id: &str,
        timeout_seconds: u64,
        executable: &Path,
        verify_distribution: bool,
    ) -> anyhow::Result<RunStatus> {
        ensure!(
            !request_id.is_empty()
                && request_id.len() <= 128
                && request_id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')),
            "request_id must be 1 to 128 ASCII letters, digits, dots, hyphens or underscores"
        );
        ensure!(
            (1..=3600).contains(&timeout_seconds),
            "App run timeout must be 1 to 3600 seconds"
        );
        let distribution = root.join("dist");
        if verify_distribution {
            lenso_engine_app::app::inspect_app_check(&distribution)
                .context("built App distribution is invalid; run app build first")?;
        }
        let mut state = self.state.lock().expect("MCP run state lock");
        if let Some(entry) = state.entries.get(request_id) {
            ensure!(
                entry.status.timeout_seconds == timeout_seconds,
                "request_id was already used with another timeout"
            );
            return Ok(entry.status.clone());
        }
        ensure!(state.active.is_none(), "another MCP App run is active");
        ensure!(
            state.entries.len() < 32,
            "MCP run history is full; restart the local bridge"
        );
        let receipt_dir = tempfile::tempdir()?;
        let ready_file = receipt_dir.path().join("host-ready");
        let mut command = Command::new(executable);
        command
            .args(["app", "start", "--from"])
            .arg(&distribution)
            .arg("--ready-file")
            .arg(&ready_file)
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt as _;
            command.process_group(0);
        }
        let child = command.spawn().context("start local App distribution")?;
        let stop = Arc::new(AtomicBool::new(false));
        let status = RunStatus {
            schema_version: 1,
            kind: "lenso.mcp-app-run",
            request_id: request_id.to_owned(),
            state: "starting",
            distribution,
            timeout_seconds,
            diagnostic_code: None,
            reason: None,
            help: None,
        };
        state.entries.insert(
            request_id.to_owned(),
            RunEntry {
                status: status.clone(),
                stop: stop.clone(),
            },
        );
        state.active = Some(request_id.to_owned());
        drop(state);
        let shared = self.state.clone();
        let id = request_id.to_owned();
        let worker = thread::spawn(move || {
            supervise(child, &ready_file, &stop, timeout_seconds, &shared, &id);
            drop(receipt_dir);
        });
        self.workers
            .lock()
            .expect("MCP run workers lock")
            .push(worker);
        Ok(status)
    }

    pub(super) fn status(&self, request_id: &str) -> anyhow::Result<RunStatus> {
        self.state
            .lock()
            .expect("MCP run state lock")
            .entries
            .get(request_id)
            .map(|entry| entry.status.clone())
            .context("unknown MCP App run request_id")
    }

    pub(super) fn active_state(&self) -> Option<&'static str> {
        let state = self.state.lock().expect("MCP run state lock");
        state
            .active
            .as_ref()
            .and_then(|id| state.entries.get(id))
            .map(|entry| entry.status.state)
    }

    pub(super) fn stop(&self, request_id: &str) -> anyhow::Result<RunStatus> {
        let mut state = self.state.lock().expect("MCP run state lock");
        let entry = state
            .entries
            .get_mut(request_id)
            .context("unknown MCP App run request_id")?;
        if matches!(entry.status.state, "starting" | "running" | "stopping") {
            entry.stop.store(true, Ordering::SeqCst);
            entry.status.state = "stopping";
        }
        Ok(entry.status.clone())
    }
}

fn supervise(
    mut child: Child,
    ready_file: &Path,
    stop: &AtomicBool,
    timeout_seconds: u64,
    shared: &Mutex<RunState>,
    id: &str,
) {
    let deadline = Instant::now() + Duration::from_secs(timeout_seconds);
    let startup_deadline = Instant::now() + Duration::from_secs(60.min(timeout_seconds));
    let mut ready = false;
    loop {
        if stop.load(Ordering::SeqCst) {
            let terminated = terminate(&mut child).is_ok();
            finish(
                shared,
                id,
                if terminated { "stopped" } else { "failed" },
                if terminated {
                    None
                } else {
                    Some("LENSO_RUN_STOP_FAILED")
                },
            );
            return;
        }
        match child.try_wait() {
            Ok(Some(_)) => {
                finish(
                    shared,
                    id,
                    "failed",
                    Some(if ready {
                        "LENSO_RUN_EXITED"
                    } else {
                        "LENSO_RUN_STARTUP_FAILED"
                    }),
                );
                return;
            }
            Err(_) => {
                let _ = terminate(&mut child);
                finish(shared, id, "failed", Some("LENSO_RUN_STATUS_FAILED"));
                return;
            }
            Ok(None) => {}
        }
        if !ready {
            match fs::symlink_metadata(ready_file) {
                Ok(metadata) => {
                    if !metadata.file_type().is_file()
                        || fs::read(ready_file).ok().as_deref() != Some(READY_RECEIPT)
                    {
                        let _ = terminate(&mut child);
                        finish(shared, id, "failed", Some("LENSO_RUN_READY_INVALID"));
                        return;
                    }
                    ready = true;
                    let mut state = shared.lock().expect("MCP run state lock");
                    if let Some(entry) = state.entries.get_mut(id)
                        && !entry.stop.load(Ordering::SeqCst)
                    {
                        entry.status.state = "running";
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => {
                    let _ = terminate(&mut child);
                    finish(shared, id, "failed", Some("LENSO_RUN_READY_INVALID"));
                    return;
                }
            }
        }
        if Instant::now() >= deadline || (!ready && Instant::now() >= startup_deadline) {
            let _ = terminate(&mut child);
            finish(
                shared,
                id,
                "failed",
                Some(if ready {
                    "LENSO_RUN_TIMEOUT"
                } else {
                    "LENSO_RUN_READY_TIMEOUT"
                }),
            );
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn finish(
    shared: &Mutex<RunState>,
    id: &str,
    status: &'static str,
    diagnostic: Option<&'static str>,
) {
    let mut state = shared.lock().expect("MCP run state lock");
    if let Some(entry) = state.entries.get_mut(id) {
        entry.status.state = status;
        entry.status.diagnostic_code = diagnostic;
        if let Some(code) = diagnostic {
            let (reason, help) = match code {
                "LENSO_RUN_STARTUP_FAILED" => (
                    "The built Host exited before its Ready Gate.",
                    "Run `lenso app start --from dist --check` locally and inspect its bounded startup diagnostics.",
                ),
                "LENSO_RUN_READY_INVALID" => (
                    "The built Host returned an invalid Ready receipt.",
                    "Rebuild the App with the matching Host and verify the distribution before starting it again.",
                ),
                "LENSO_RUN_READY_TIMEOUT" => (
                    "The built Host did not become ready within the startup budget.",
                    "Check required local services and run `lenso app start --from dist --check` for startup diagnostics.",
                ),
                "LENSO_RUN_TIMEOUT" => (
                    "The authorized App run reached its lifetime limit.",
                    "Request a new bounded run if continued local observation is needed.",
                ),
                "LENSO_RUN_EXITED" => (
                    "The built Host exited after it became ready.",
                    "Inspect the App's local runtime logs and restart with a new request_id after fixing the failure.",
                ),
                "LENSO_RUN_STOP_FAILED" => (
                    "The local Host did not stop cleanly.",
                    "Inspect the process group before starting another App run.",
                ),
                _ => (
                    "The local Host process could not be observed safely.",
                    "Inspect the built distribution and local process state before retrying.",
                ),
            };
            entry.status.reason = Some(reason);
            entry.status.help = Some(help);
        }
    }
    state.active = None;
}

fn terminate(child: &mut Child) -> anyhow::Result<()> {
    if child.try_wait()?.is_some() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        use nix::{
            sys::signal::{Signal, killpg},
            unistd::Pid,
        };
        let id = i32::try_from(child.id())?;
        if let Err(error) = killpg(Pid::from_raw(id), Signal::SIGTERM)
            && error != nix::errno::Errno::ESRCH
        {
            return Err(error.into());
        }
    }
    #[cfg(not(unix))]
    child.kill()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait()?.is_none() {
        if Instant::now() >= deadline {
            #[cfg(unix)]
            {
                use nix::{
                    sys::signal::{Signal, killpg},
                    unistd::Pid,
                };
                let _ = killpg(Pid::from_raw(i32::try_from(child.id())?), Signal::SIGKILL);
            }
            #[cfg(not(unix))]
            child.kill()?;
            child.wait()?;
            bail!("local App run did not stop within its shutdown budget");
        }
        thread::sleep(Duration::from_millis(25));
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use std::{
        fs,
        os::unix::fs::PermissionsExt as _,
        thread,
        time::{Duration, Instant},
    };

    use super::RunController;

    #[test]
    fn run_is_idempotent_exclusive_ready_and_stoppable() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join(".lenso")).unwrap();
        let script = temp.path().join("host-script");
        fs::write(&script, "#!/bin/sh\nwhile [ \"$1\" != --ready-file ]; do shift; done\nprintf 'lenso.local-host-ready.v1\\n' > \"$2.tmp\"\nmv \"$2.tmp\" \"$2\"\nexec sleep 30\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        let controller = RunController::default();
        controller
            .start_with_executable(temp.path(), "run-1", 10, &script, false)
            .unwrap();
        assert!(
            controller
                .start_with_executable(temp.path(), "run-2", 10, &script, false)
                .is_err()
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if controller.status("run-1").unwrap().state == "running" {
                break;
            }
            assert!(Instant::now() < deadline, "run did not become ready");
            thread::sleep(Duration::from_millis(25));
        }
        assert_eq!(
            controller
                .start_with_executable(temp.path(), "run-1", 10, &script, false)
                .unwrap()
                .state,
            "running"
        );
        assert_eq!(controller.stop("run-1").unwrap().state, "stopping");
        loop {
            if controller.status("run-1").unwrap().state == "stopped" {
                break;
            }
            assert!(Instant::now() < deadline, "run did not stop");
            thread::sleep(Duration::from_millis(25));
        }
    }

    #[test]
    fn startup_exit_and_invalid_readiness_fail_without_a_live_run() {
        let temp = tempfile::tempdir().unwrap();
        let script = temp.path().join("bad-host");
        fs::write(&script, "#!/bin/sh\nexit 24\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        let controller = RunController::default();
        controller
            .start_with_executable(temp.path(), "exit", 5, &script, false)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = controller.status("exit").unwrap();
            if status.state == "failed" {
                assert_eq!(status.diagnostic_code, Some("LENSO_RUN_STARTUP_FAILED"));
                assert!(status.reason.is_some());
                assert!(status.help.is_some());
                break;
            }
            assert!(Instant::now() < deadline, "startup exit was not observed");
            thread::sleep(Duration::from_millis(25));
        }
        fs::write(&script, "#!/bin/sh\nwhile [ \"$1\" != --ready-file ]; do shift; done\nprintf 'wrong receipt' > \"$2.tmp\"\nmv \"$2.tmp\" \"$2\"\nexec sleep 30\n").unwrap();
        controller
            .start_with_executable(temp.path(), "invalid", 5, &script, false)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = controller.status("invalid").unwrap();
            if status.state == "failed" {
                assert_eq!(status.diagnostic_code, Some("LENSO_RUN_READY_INVALID"));
                break;
            }
            assert!(
                Instant::now() < deadline,
                "invalid readiness was not rejected"
            );
            thread::sleep(Duration::from_millis(25));
        }
    }
}
