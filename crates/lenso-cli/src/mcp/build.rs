//! One explicitly enabled, fixed-root App build lane for the local MCP server.
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicI32, Ordering},
    },
    time::Duration,
};

use anyhow::{Context as _, bail, ensure};
use lenso_engine::process::{ProcessBudget, execute_cancellable_command_with_budget};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub(super) struct BuildStatus {
    schema_version: u32,
    kind: &'static str,
    request_id: String,
    state: &'static str,
    output: PathBuf,
    timeout_seconds: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    diagnostic_code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    check: Option<serde_json::Value>,
}

#[derive(Debug)]
struct BuildEntry {
    status: BuildStatus,
    cancelled: Arc<AtomicBool>,
}

#[derive(Debug, Default)]
struct BuildState {
    entries: BTreeMap<String, BuildEntry>,
    active: Option<String>,
}

#[derive(Debug, Default)]
pub(super) struct BuildController {
    state: Arc<Mutex<BuildState>>,
}

impl Drop for BuildController {
    fn drop(&mut self) {
        if let Ok(state) = self.state.lock() {
            for entry in state.entries.values() {
                entry.cancelled.store(true, Ordering::SeqCst);
            }
        }
    }
}

impl BuildController {
    pub(super) fn start(
        &self,
        root: &Path,
        request_id: &str,
        timeout_seconds: u64,
    ) -> anyhow::Result<BuildStatus> {
        let executable = std::env::current_exe().context("locate local Lenso CLI")?;
        self.start_with_executable(root, request_id, timeout_seconds, &executable)
    }

    fn start_with_executable(
        &self,
        root: &Path,
        request_id: &str,
        timeout_seconds: u64,
        executable: &Path,
    ) -> anyhow::Result<BuildStatus> {
        ensure!(
            !request_id.is_empty()
                && request_id.len() <= 128
                && request_id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')),
            "request_id must be 1 to 128 ASCII letters, digits, dots, hyphens or underscores"
        );
        let budget = ProcessBudget::new(Duration::from_secs(timeout_seconds), 8 * 1024 * 1024)?;
        let output = root.join("dist");
        let mut state = self.state.lock().expect("MCP build state lock");
        if let Some(entry) = state.entries.get(request_id) {
            ensure!(
                entry.status.timeout_seconds == timeout_seconds,
                "request_id was already used with another timeout"
            );
            return Ok(entry.status.clone());
        }
        ensure!(state.active.is_none(), "another App build is active");
        ensure!(
            state.entries.len() < 32,
            "MCP build history is full; restart the local bridge"
        );
        match std::fs::symlink_metadata(&output) {
            Ok(_) => bail!("App output already exists; preserve or move it before another build"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let executable = executable.to_path_buf();
        let cancelled = Arc::new(AtomicBool::new(false));
        let status = BuildStatus {
            schema_version: 1,
            kind: "lenso.mcp-app-build",
            request_id: request_id.to_owned(),
            state: "running",
            output: output.clone(),
            timeout_seconds,
            diagnostic_code: None,
            check: None,
        };
        state.entries.insert(
            request_id.to_owned(),
            BuildEntry {
                status: status.clone(),
                cancelled: cancelled.clone(),
            },
        );
        state.active = Some(request_id.to_owned());
        drop(state);
        let state = self.state.clone();
        let root = root.to_path_buf();
        let id = request_id.to_owned();
        std::thread::spawn(move || {
            let result = run_build(&executable, &root, &output, &cancelled, budget);
            let mut state = state.lock().expect("MCP build state lock");
            if let Some(entry) = state.entries.get_mut(&id) {
                match result {
                    Ok(check) => {
                        entry.status.state = "succeeded";
                        entry.status.check = Some(check);
                    }
                    Err(error) => {
                        let message = format!("{error:#}");
                        let outcome_uncertain =
                            message.contains("build output needs reconciliation");
                        entry.status.state =
                            if cancelled.load(Ordering::SeqCst) && !outcome_uncertain {
                                "cancelled"
                            } else {
                                "failed"
                            };
                        entry.status.diagnostic_code = Some(if outcome_uncertain {
                            "LENSO_BUILD_OUTCOME_UNCERTAIN"
                        } else if cancelled.load(Ordering::SeqCst) {
                            "LENSO_BUILD_CANCELLED"
                        } else if message.contains("execution budget") {
                            "LENSO_BUILD_TIMEOUT"
                        } else if message.contains("output exceeds") {
                            "LENSO_BUILD_OUTPUT_LIMIT"
                        } else {
                            "LENSO_BUILD_FAILED"
                        });
                    }
                }
            }
            state.active = None;
        });
        Ok(status)
    }

    pub(super) fn status(&self, request_id: &str) -> anyhow::Result<BuildStatus> {
        self.state
            .lock()
            .expect("MCP build state lock")
            .entries
            .get(request_id)
            .map(|entry| entry.status.clone())
            .context("unknown MCP build request_id")
    }

    pub(super) fn cancel(&self, request_id: &str) -> anyhow::Result<BuildStatus> {
        let mut state = self.state.lock().expect("MCP build state lock");
        let entry = state
            .entries
            .get_mut(request_id)
            .context("unknown MCP build request_id")?;
        if !matches!(entry.status.state, "running" | "cancelling") {
            bail!("MCP build is already terminal");
        }
        entry.cancelled.store(true, Ordering::SeqCst);
        entry.status.state = "cancelling";
        Ok(entry.status.clone())
    }
}

fn run_build(
    executable: &Path,
    root: &Path,
    output: &Path,
    cancelled: &AtomicBool,
    budget: ProcessBudget,
) -> anyhow::Result<serde_json::Value> {
    let mut command = lenso_engine_app::app::build_command(executable);
    command
        .args(["app", "build", "--root"])
        .arg(root)
        .arg("--out")
        .arg(output)
        .current_dir(root);
    let process_result = execute_cancellable_command_with_budget(
        command,
        &serde_json::json!({}),
        Arc::new(AtomicI32::new(0)),
        cancelled,
        budget,
    );
    if let Err(error) = process_result {
        if std::fs::symlink_metadata(output).is_ok() {
            bail!("build output needs reconciliation after unsuccessful process: {error:#}");
        }
        return Err(error);
    }
    let check = lenso_engine_app::app::inspect_app_check(output)
        .context("build output needs reconciliation after successful process")?;
    Ok(serde_json::to_value(check)?)
}

#[cfg(all(test, unix))]
mod tests {
    use std::{
        fs,
        os::unix::fs::PermissionsExt as _,
        thread,
        time::{Duration, Instant},
    };

    use super::BuildController;

    #[test]
    fn build_request_is_idempotent_exclusive_and_cancellable() {
        let temp = tempfile::tempdir().unwrap();
        let script = temp.path().join("slow-build");
        fs::write(&script, "#!/bin/sh\nsleep 30\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        let controller = BuildController::default();
        let first = controller
            .start_with_executable(temp.path(), "build-1", 10, &script)
            .unwrap();
        assert_eq!(first.state, "running");
        assert_eq!(
            controller
                .start_with_executable(temp.path(), "build-1", 10, &script)
                .unwrap()
                .state,
            "running"
        );
        assert!(
            controller
                .start_with_executable(temp.path(), "build-2", 10, &script)
                .is_err()
        );
        assert_eq!(controller.cancel("build-1").unwrap().state, "cancelling");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = controller.status("build-1").unwrap();
            if status.state == "cancelled" {
                assert_eq!(status.diagnostic_code, Some("LENSO_BUILD_CANCELLED"));
                break;
            }
            assert!(
                Instant::now() < deadline,
                "build cancellation did not finish"
            );
            thread::sleep(Duration::from_millis(25));
        }
        assert!(!temp.path().join("dist").exists());

        controller
            .start_with_executable(temp.path(), "timeout", 1, &script)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = controller.status("timeout").unwrap();
            if status.state == "failed" {
                assert_eq!(status.diagnostic_code, Some("LENSO_BUILD_TIMEOUT"));
                break;
            }
            assert!(Instant::now() < deadline, "build timeout did not finish");
            thread::sleep(Duration::from_millis(25));
        }
    }
}
