//! Supervision for a built App with a Host-authorized external source.
use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context as _, bail, ensure};
use tokio::{
    process::{Child, Command},
    task::JoinHandle,
    time::Instant,
};

use super::configuration_source::{self, AcceptedSourceProof};

const READY_TIMEOUT: Duration = Duration::from_secs(60);
const POLICY_CHECK_INTERVAL: Duration = Duration::from_secs(1);
const KILL_CONFIRM_TIMEOUT: Duration = Duration::from_secs(2);
const READY_RECEIPT: &[u8] = b"lenso.local-host-ready.v1\n";

struct Active {
    child: Child,
    group_id: u32,
    proof: AcceptedSourceProof,
    observed_at: Instant,
}

impl Active {
    fn deadline(&self) -> Instant {
        self.observed_at + Duration::from_secs(self.proof.max_stale_seconds)
    }
}

pub(super) async fn run(
    from: PathBuf,
    policy: PathBuf,
    ready_file: Option<PathBuf>,
) -> anyhow::Result<()> {
    let from = fs::canonicalize(from).context("locate built App distribution")?;
    let _start_lock = super::local_lock::acquire(
        &from,
        "supervised-start.lock",
        "App distribution is already supervised by another lenso app start session",
    )?;
    let policy = std::path::absolute(policy)?;
    let executable = fs::canonicalize(from.join(".lenso/host"))
        .context("locate built local Host; run lenso app build first")?;
    let (proof, observed_at) = sync_source(&from, &policy).await?;
    let mut active = launch(&from, &policy, &executable, proof, observed_at).await?;
    if let Some(path) = &ready_file {
        if let Err(error) = publish_ready(path) {
            kill_group(&mut active.child, active.group_id).await?;
            return Err(error).context("publish supervised App readiness");
        }
    }
    if Instant::now() >= active.deadline() {
        kill_group(&mut active.child, active.group_id).await?;
        bail!("external configuration proof expired before supervised App readiness receipt");
    }

    let mut next_poll = Instant::now() + poll_interval(active.proof.max_stale_seconds);
    let mut next_policy_check = Instant::now() + POLICY_CHECK_INTERVAL;
    let mut polling: Option<(JoinHandle<anyhow::Result<AcceptedSourceProof>>, Instant)> = None;
    loop {
        let deadline = active.deadline();
        tokio::select! {
            biased;
            _ = tokio::time::sleep_until(deadline) => {
                kill_group(&mut active.child, active.group_id).await?;
                bail!("external configuration freshness expired; App was stopped");
            }
            signal = shutdown_signal() => {
                if let Err(error) = signal {
                    kill_group(&mut active.child, active.group_id).await?;
                    return Err(error).context("watch supervised App shutdown signal");
                }
                stop_group(&mut active.child, active.group_id).await?;
                return Ok(());
            }
            status = wait_for_exit_unreaped(&mut active.child, active.group_id) => {
                kill_group(&mut active.child, active.group_id).await?;
                status?;
                let status = active.child.wait().await?;
                bail!("supervised App exited: {status}");
            }
            _ = tokio::time::sleep_until(next_policy_check) => {
                next_policy_check = Instant::now() + POLICY_CHECK_INTERVAL;
                // This check does not wait for the source lock, which can be
                // held during a bounded HTTPS fetch on another thread.
                if configuration_source::policy_changed_since_active(&from, &policy).unwrap_or(true) {
                    kill_group(&mut active.child, active.group_id).await?;
                    bail!("Host configuration policy changed or cannot be verified; App was stopped");
                }
            }
            _ = tokio::time::sleep_until(next_poll), if polling.is_none() => {
                let started = Instant::now();
                polling = Some((spawn_sync(&from, &policy), started));
                next_poll = Instant::now() + poll_interval(active.proof.max_stale_seconds);
            }
            result = async { (&mut polling.as_mut().expect("poll branch is armed").0).await }, if polling.is_some() => {
                let (_, observed_at) = polling.take().expect("completed poll exists");
                let proof = match result {
                    Ok(Ok(proof)) => proof,
                    Ok(Err(error)) => {
                        eprintln!("External configuration source unavailable or rejected: {error:#}");
                        let current = proof_matches(&from, &policy, &active.proof, active.deadline()).await;
                        if !matches!(&current, Ok(true)) {
                            kill_group(&mut active.child, active.group_id).await?;
                            current.context("active external configuration cannot be verified after source failure")?;
                            bail!("active external configuration changed after source failure; App was stopped");
                        }
                        continue;
                    }
                    Err(error) => {
                        eprintln!("External configuration poll failed: {error}");
                        let current = proof_matches(&from, &policy, &active.proof, active.deadline()).await;
                        if !matches!(&current, Ok(true)) {
                            kill_group(&mut active.child, active.group_id).await?;
                            current.context("active external configuration cannot be verified after poll failure")?;
                            bail!("active external configuration changed after poll failure; App was stopped");
                        }
                        continue;
                    }
                };
                if proof == active.proof {
                    if Instant::now() >= observed_at + Duration::from_secs(proof.max_stale_seconds) {
                        kill_group(&mut active.child, active.group_id).await?;
                        bail!("external configuration freshness expired during revalidation; App was stopped");
                    }
                    let still_current = proof_matches(&from, &policy, &proof, active.deadline()).await;
                    if !matches!(&still_current, Ok(true)) {
                        kill_group(&mut active.child, active.group_id).await?;
                        still_current.context("active external configuration cannot be verified")?;
                        bail!("active external configuration changed; App was stopped");
                    }
                    active.observed_at = observed_at;
                    continue;
                }
                // A newer accepted source revision may revoke a value or scope.
                // The generated Host has no traffic gate before its Ready Gate,
                // so stop the old process before starting the replacement.
                kill_group(&mut active.child, active.group_id).await?;
                eprintln!("External configuration changed; App is stopped while its replacement starts");
                active = launch(&from, &policy, &executable, proof, observed_at).await?;
                next_poll = Instant::now() + poll_interval(active.proof.max_stale_seconds);
                next_policy_check = Instant::now() + POLICY_CHECK_INTERVAL;
            }
        }
    }
}

fn poll_interval(max_stale_seconds: u64) -> Duration {
    Duration::from_secs(10)
        .min(Duration::from_secs(max_stale_seconds) / 2)
        .max(Duration::from_millis(100))
}

fn spawn_sync(from: &Path, policy: &Path) -> JoinHandle<anyhow::Result<AcceptedSourceProof>> {
    let from = from.to_path_buf();
    let policy = policy.to_path_buf();
    tokio::task::spawn_blocking(move || configuration_source::sync_with_proof(&from, &policy))
}

async fn sync_source(from: &Path, policy: &Path) -> anyhow::Result<(AcceptedSourceProof, Instant)> {
    // Age starts before the source request, so a slow fetch never grants a
    // fresh full TTL after it returns.
    let observed_at = Instant::now();
    let proof = spawn_sync(from, policy).await??;
    ensure!(
        Instant::now() < observed_at + Duration::from_secs(proof.max_stale_seconds),
        "external configuration proof expired during source fetch"
    );
    Ok((proof, observed_at))
}

async fn proof_matches(
    from: &Path,
    policy: &Path,
    proof: &AcceptedSourceProof,
    deadline: Instant,
) -> anyhow::Result<bool> {
    let from = from.to_path_buf();
    let policy = policy.to_path_buf();
    let proof = proof.clone();
    let check = tokio::task::spawn_blocking(move || {
        configuration_source::proof_matches_current(&from, &policy, &proof)
    });
    tokio::time::timeout_at(deadline, check)
        .await
        .context("external configuration proof expired during revision fencing")??
}

async fn launch(
    from: &Path,
    policy: &Path,
    executable: &Path,
    proof: AcceptedSourceProof,
    observed_at: Instant,
) -> anyhow::Result<Active> {
    let deadline = observed_at + Duration::from_secs(proof.max_stale_seconds);
    ensure!(
        Instant::now() < deadline,
        "external configuration proof is stale"
    );
    ensure!(
        proof_matches(from, policy, &proof, deadline).await?,
        "external configuration changed before Host startup"
    );

    let marker = tempfile::NamedTempFile::new_in(from.join(".lenso"))?.into_temp_path();
    fs::remove_file(&marker)?;
    let mut command = Command::new(executable);
    command
        .args(super::local_host::host_arguments(from)?)
        .arg("--ready-file")
        .arg(&marker)
        .arg("--defer-activation")
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command.spawn().context("start supervised local Host")?;
    let group_id = child.id().context("supervised Host process ID")?;
    let ready_deadline = (Instant::now() + READY_TIMEOUT).min(deadline);
    loop {
        tokio::select! {
            biased;
            _ = tokio::time::sleep_until(ready_deadline) => {
                kill_group(&mut child, group_id).await?;
                bail!("supervised App did not become ready before its startup or configuration deadline");
            }
            signal = shutdown_signal() => {
                kill_group(&mut child, group_id).await?;
                signal?;
                bail!("supervised App startup was interrupted");
            }
            status = wait_for_exit_unreaped(&mut child, group_id) => {
                kill_group(&mut child, group_id).await?;
                status?;
                bail!("supervised App exited before readiness: {}", child.wait().await?);
            }
            _ = tokio::time::sleep(Duration::from_millis(25)) => {
                match fs::symlink_metadata(&marker) {
                    Ok(metadata) => {
                        if !metadata.file_type().is_file() || fs::read(&marker).ok().as_deref() != Some(READY_RECEIPT) {
                            kill_group(&mut child, group_id).await?;
                            bail!("supervised App returned an invalid readiness receipt");
                        }
                        break;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        kill_group(&mut child, group_id).await?;
                        return Err(error).context("inspect supervised App readiness");
                    }
                }
            }
        }
    }

    let fenced = proof_matches(from, policy, &proof, deadline).await;
    if !matches!(&fenced, Ok(true)) {
        kill_group(&mut child, group_id).await?;
        ensure!(
            fenced?,
            "external configuration changed during Host startup"
        );
    }
    let from_owned = from.to_path_buf();
    let root_revision = proof.plugin_root_revision.clone();
    let receipt = tokio::task::spawn_blocking(move || {
        configuration_source::record_distribution_activation(&from_owned, &root_revision)
    });
    let recorded = tokio::time::timeout_at(deadline, receipt).await;
    if !matches!(&recorded, Ok(Ok(Ok(())))) {
        kill_group(&mut child, group_id).await?;
        recorded.context("configuration proof expired before activation receipt")???;
    }
    let fenced = proof_matches(from, policy, &proof, deadline).await;
    if !matches!(&fenced, Ok(true)) {
        kill_group(&mut child, group_id).await?;
        ensure!(
            fenced?,
            "external configuration changed after Host readiness"
        );
    }
    if Instant::now() >= deadline {
        kill_group(&mut child, group_id).await?;
        bail!("external configuration proof expired before activation");
    }
    eprintln!(
        "Supervised App ready with external configuration revision {}",
        proof.revision
    );
    Ok(Active {
        child,
        group_id,
        proof,
        observed_at,
    })
}

fn publish_ready(path: &Path) -> anyhow::Result<()> {
    let parent = path.parent().context("ready file needs a parent")?;
    let mut stage = tempfile::NamedTempFile::new_in(parent)?;
    stage.write_all(READY_RECEIPT)?;
    stage.persist(path)?;
    Ok(())
}

async fn shutdown_signal() -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            signal = tokio::signal::ctrl_c() => signal?,
            _ = terminate.recv() => {},
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await?;
    Ok(())
}

async fn stop_group(child: &mut Child, group_id: u32) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        if let Err(error) = signal_group(group_id, nix::sys::signal::Signal::SIGTERM) {
            kill_group(child, group_id).await?;
            return Err(error).context("gracefully stop supervised App process group");
        }
        let observed = tokio::time::timeout(
            Duration::from_secs(10),
            wait_for_exit_unreaped(child, group_id),
        )
        .await;
        // Even if the leader exited, descendants in its group can still run.
        // Signal them while the unreaped leader reserves the group ID.
        kill_group(child, group_id).await?;
        if let Ok(result) = observed {
            result?;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = group_id;
        child.start_kill()?;
        child.wait().await?;
    }
    Ok(())
}

async fn kill_group(child: &mut Child, group_id: u32) -> anyhow::Result<()> {
    #[cfg(unix)]
    if let Err(error) = signal_group(group_id, nix::sys::signal::Signal::SIGKILL) {
        let _ = child.start_kill();
        let _ = tokio::time::timeout(KILL_CONFIRM_TIMEOUT, child.wait()).await;
        return Err(error).context("force-stop supervised App process group");
    }
    #[cfg(not(unix))]
    {
        let _ = group_id;
        child.start_kill()?;
    }
    tokio::time::timeout(KILL_CONFIRM_TIMEOUT, child.wait())
        .await
        .context("timed out confirming supervised Host termination")??;
    Ok(())
}

#[cfg(unix)]
async fn wait_for_exit_unreaped(_child: &mut Child, group_id: u32) -> anyhow::Result<()> {
    loop {
        if exited_unreaped(group_id)? {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[cfg(unix)]
fn exited_unreaped(group_id: u32) -> anyhow::Result<bool> {
    use nix::libc;
    let id = i32::try_from(group_id)?;
    // WNOWAIT retains the group leader's PID until the entire process group
    // has been signalled, so an unrelated group cannot reuse it.
    let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
    let result = unsafe {
        libc::waitid(
            libc::P_PID,
            id as libc::id_t,
            info.as_mut_ptr(),
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    if result == -1 {
        let error = std::io::Error::last_os_error();
        if error.kind() == std::io::ErrorKind::Interrupted {
            return Ok(false);
        }
        return Err(error).context("observe supervised Host exit without reaping");
    }
    Ok(unsafe { info.assume_init().si_pid() } == id)
}

#[cfg(not(unix))]
async fn wait_for_exit_unreaped(child: &mut Child, _group_id: u32) -> anyhow::Result<()> {
    child.wait().await?;
    Ok(())
}

#[cfg(unix)]
fn signal_group(group_id: u32, signal: nix::sys::signal::Signal) -> anyhow::Result<()> {
    use nix::{errno::Errno, sys::signal::killpg, unistd::Pid};
    let id = i32::try_from(group_id)?;
    if let Err(error) = killpg(Pid::from_raw(id), signal)
        && error != Errno::ESRCH
    {
        return Err(error.into());
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt as _, path::Path, process::Command as StdCommand};

    use lenso_app_plan::authoring::{
        HostCatalog, HostDefaultPlugin, HostPluginRelease, HostSlot, PluginDescriptor,
    };

    use super::*;

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        fs::create_dir(root.join(".lenso")).unwrap();
        fs::create_dir_all(root.join("intent/.lenso")).unwrap();
        fs::write(root.join(".lenso/host-mode"), b"native").unwrap();
        let host = root.join(".lenso/host");
        fs::write(
            &host,
            b"#!/bin/sh\nset -eu\nroot=$(CDPATH= cd -- \"$(dirname \"$0\")/..\" && pwd)\nready=\nwhile [ \"$#\" -gt 0 ]; do\n  case \"$1\" in\n    --ready-file) ready=$2; shift 2 ;;\n    --defer-activation) shift ;;\n    --) break ;;\n    *) exit 2 ;;\n  esac\ndone\nif [ -f \"$root/previous-pid\" ] && kill -0 \"$(cat \"$root/previous-pid\")\" 2>/dev/null; then\n  : > \"$root/overlap\"\nfi\nprintf '%s' \"$$\" > \"$root/previous-pid\"\nprintf 'lenso.local-host-ready.v1\\n' > \"$ready\"\nexec sleep 60\n",
        )
        .unwrap();
        fs::set_permissions(&host, fs::Permissions::from_mode(0o700)).unwrap();
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
        let snapshot = root.join("snapshot.json");
        write_snapshot(&snapshot, 1, "first");
        let policy = root.join("policy.json");
        write_policy(&policy, &snapshot, 3);
        (temporary, snapshot, policy)
    }

    fn write_snapshot(path: &Path, revision: u64, greeting: &str) {
        fs::write(
            path,
            serde_json::to_vec(&serde_json::json!({
                "schema": "lenso.plugin-configuration-snapshot.v1",
                "revision": revision,
                "configurations": [{
                    "plugin_id": "example.agent",
                    "instance_key": "default",
                    "toml": format!("greeting = {greeting:?}\n")
                }]
            }))
            .unwrap(),
        )
        .unwrap();
    }

    fn write_policy(path: &Path, snapshot: &Path, max_stale_seconds: u64) {
        fs::write(
            path,
            serde_json::to_vec(&serde_json::json!({
                "schema": "lenso.configuration-source-policy.v1",
                "source_reference": "supervised-test",
                "source": {"type": "file", "path": snapshot},
                "objects": [{
                    "plugin_id": "example.agent",
                    "instance_key": "default",
                    "fields": ["greeting"]
                }],
                "max_stale_seconds": max_stale_seconds
            }))
            .unwrap(),
        )
        .unwrap();
    }

    async fn wait_for_activation(root: &Path, revision: u64) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if configuration_source::inspect_status(root)
                    .ok()
                    .and_then(|status| status.last_activated_revision)
                    == Some(revision)
                {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn accepted_update_stops_old_host_and_source_outage_expires_new_host() {
        let (temporary, snapshot, policy) = fixture();
        let root = temporary.path().to_path_buf();
        let supervised = tokio::spawn(run(root.clone(), policy, None));
        wait_for_activation(&root, 1).await;
        write_snapshot(&snapshot, 2, "second");
        wait_for_activation(&root, 2).await;
        assert!(!root.join("overlap").exists());
        fs::remove_file(snapshot).unwrap();
        let result = tokio::time::timeout(Duration::from_secs(6), supervised)
            .await
            .unwrap()
            .unwrap();
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("freshness expired")
        );
        assert!(!root.join("overlap").exists());
    }

    #[tokio::test]
    async fn policy_change_stops_active_host_without_waiting_for_source() {
        let (temporary, snapshot, policy) = fixture();
        let root = temporary.path().to_path_buf();
        write_policy(&policy, &snapshot, 10);
        let ready = root.join("supervisor-ready");
        let supervised = tokio::spawn(run(root.clone(), policy.clone(), Some(ready.clone())));
        wait_for_activation(&root, 1).await;
        tokio::time::timeout(Duration::from_secs(5), async {
            while !ready.is_file() {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap();
        fs::remove_file(&snapshot).unwrap();
        write_policy(&policy, &snapshot, 11);
        let result = tokio::time::timeout(Duration::from_secs(2), supervised)
            .await
            .unwrap()
            .unwrap();
        let error = result.unwrap_err();
        assert!(error.to_string().contains("policy changed"), "{error:#}");
    }

    #[tokio::test]
    async fn second_supervised_start_of_same_distribution_is_rejected_before_host_launch() {
        let (temporary, snapshot, policy) = fixture();
        let root = temporary.path().to_path_buf();
        let ready = root.join("first-supervisor-ready");
        let first = tokio::spawn(run(root.clone(), policy.clone(), Some(ready.clone())));
        tokio::time::timeout(Duration::from_secs(5), async {
            while !ready.is_file() {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap();

        let second =
            tokio::time::timeout(Duration::from_secs(1), run(root.clone(), policy, None)).await;
        fs::remove_file(snapshot).unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(6), first)
            .await
            .expect("first supervisor should retire at its freshness deadline");
        let error = second
            .expect("second supervisor should be rejected promptly")
            .expect_err("second supervisor should not start another Host");
        assert!(
            error.to_string().contains("already supervised"),
            "{error:#}"
        );
        assert!(!root.join("overlap").exists(), "a second Host was launched");
    }

    #[tokio::test]
    async fn early_host_exit_kills_its_unreaped_process_group() {
        let (temporary, _snapshot, policy) = fixture();
        let root = temporary.path();
        let host = root.join(".lenso/host");
        fs::write(
            &host,
            b"#!/bin/sh\nset -eu\nroot=$(CDPATH= cd -- \"$(dirname \"$0\")/..\" && pwd)\nsleep 60 &\nprintf '%s' \"$!\" > \"$root/grandchild-pid\"\nexit 7\n",
        )
        .unwrap();
        fs::set_permissions(&host, fs::Permissions::from_mode(0o700)).unwrap();

        let error = tokio::time::timeout(
            Duration::from_secs(5),
            run(root.to_path_buf(), policy, None),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert!(error.to_string().contains("before readiness"));
        let grandchild: u32 = fs::read_to_string(root.join("grandchild-pid"))
            .unwrap()
            .parse()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let output = StdCommand::new("ps")
                    .args(["-o", "state=", "-p", &grandchild.to_string()])
                    .output()
                    .unwrap();
                let state = String::from_utf8_lossy(&output.stdout);
                if state.trim().is_empty() || state.trim().starts_with('Z') {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("Host grandchild remained live after early leader exit");
    }

    #[tokio::test]
    async fn proof_expiring_during_readiness_never_activates() {
        let (temporary, snapshot, policy) = fixture();
        let root = temporary.path();
        write_policy(&policy, &snapshot, 1);
        let host = root.join(".lenso/host");
        fs::write(
            &host,
            b"#!/bin/sh\nset -eu\nsleep 2\nwhile [ \"$#\" -gt 0 ]; do\n  case \"$1\" in\n    --ready-file) ready=$2; shift 2 ;;\n    *) shift ;;\n  esac\ndone\nprintf 'lenso.local-host-ready.v1\\n' > \"$ready\"\nexec sleep 60\n",
        )
        .unwrap();
        fs::set_permissions(&host, fs::Permissions::from_mode(0o700)).unwrap();

        let error = tokio::time::timeout(
            Duration::from_secs(3),
            run(root.to_path_buf(), policy, None),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert!(error.to_string().contains("configuration deadline"));
        assert_eq!(
            configuration_source::inspect_status(root)
                .unwrap()
                .last_activated_revision,
            None
        );
    }
}
