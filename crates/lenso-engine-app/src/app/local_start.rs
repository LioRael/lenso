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

mod retirement;

const READY_TIMEOUT: Duration = Duration::from_secs(60);
const POLICY_CHECK_INTERVAL: Duration = Duration::from_secs(1);
const KILL_CONFIRM_TIMEOUT: Duration = Duration::from_secs(2);
const READY_RECEIPT: &[u8] = b"lenso.local-host-ready.v1\n";
use super::local_host_retirement::CrashFence;

struct Active {
    child: Child,
    group_id: u32,
    proof: AcceptedSourceProof,
    observed_at: Instant,
    retirement: Option<retirement::Receipt>,
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
    let mut crash_fence = CrashFence::new(&from)?;
    let mut shutdown = ShutdownSignal::new()?;
    let policy = std::path::absolute(policy)?;
    let executable = fs::canonicalize(from.join(".lenso/host"))
        .context("locate built local Host; run lenso app build first")?;
    let (proof, observed_at) = sync_source(&from, &policy).await?;
    let mut active = launch(
        &from,
        &policy,
        &executable,
        proof,
        observed_at,
        &mut crash_fence,
        &mut shutdown,
    )
    .await?;
    if let Some(path) = &ready_file
        && let Err(error) = publish_ready(path)
    {
        kill_fenced_group(&mut active.child, active.group_id, &mut crash_fence).await?;
        return Err(error).context("publish supervised App readiness");
    }
    if Instant::now() >= active.deadline() {
        kill_fenced_group(&mut active.child, active.group_id, &mut crash_fence).await?;
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
                kill_fenced_group(&mut active.child, active.group_id, &mut crash_fence).await?;
                bail!("external configuration freshness expired; App was stopped");
            }
            signal = shutdown.wait() => {
                if let Err(error) = signal {
                    kill_fenced_group(&mut active.child, active.group_id, &mut crash_fence).await?;
                    return Err(error).context("watch supervised App shutdown signal");
                }
                return retirement::stop(&mut active, &mut crash_fence, true).await;
            }
            status = wait_for_exit_unreaped(&mut active.child, active.group_id) => {
                if let Err(error) = status {
                    kill_fenced_group(&mut active.child, active.group_id, &mut crash_fence).await?;
                    return Err(error).context("observe supervised Host exit");
                }
                return retirement::stop(&mut active, &mut crash_fence, false).await;
            }
            _ = tokio::time::sleep_until(next_policy_check) => {
                next_policy_check = Instant::now() + POLICY_CHECK_INTERVAL;
                // This check does not wait for the source lock, which can be
                // held during a bounded HTTPS fetch on another thread.
                if configuration_source::policy_changed_since_active(&from, &policy).unwrap_or(true) {
                    kill_fenced_group(&mut active.child, active.group_id, &mut crash_fence).await?;
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
                            kill_fenced_group(&mut active.child, active.group_id, &mut crash_fence).await?;
                            current.context("active external configuration cannot be verified after source failure")?;
                            bail!("active external configuration changed after source failure; App was stopped");
                        }
                        continue;
                    }
                    Err(error) => {
                        eprintln!("External configuration poll failed: {error}");
                        let current = proof_matches(&from, &policy, &active.proof, active.deadline()).await;
                        if !matches!(&current, Ok(true)) {
                            kill_fenced_group(&mut active.child, active.group_id, &mut crash_fence).await?;
                            current.context("active external configuration cannot be verified after poll failure")?;
                            bail!("active external configuration changed after poll failure; App was stopped");
                        }
                        continue;
                    }
                };
                if proof == active.proof {
                    if Instant::now() >= observed_at + Duration::from_secs(proof.max_stale_seconds) {
                        kill_fenced_group(&mut active.child, active.group_id, &mut crash_fence).await?;
                        bail!("external configuration freshness expired during revalidation; App was stopped");
                    }
                    let still_current = proof_matches(&from, &policy, &proof, active.deadline()).await;
                    if !matches!(&still_current, Ok(true)) {
                        kill_fenced_group(&mut active.child, active.group_id, &mut crash_fence).await?;
                        still_current.context("active external configuration cannot be verified")?;
                        bail!("active external configuration changed; App was stopped");
                    }
                    active.observed_at = observed_at;
                    continue;
                }
                if proof.source == active.proof.source
                    && proof.policy_digest == active.proof.policy_digest
                    && proof.plugin_root_revision == active.proof.plugin_root_revision
                {
                    // A newer source revision can resolve to the exact Root
                    // already served by this Host. It needs fresh source and
                    // authority proof, but no new Host activation receipt.
                    let new_deadline = observed_at + Duration::from_secs(proof.max_stale_seconds);
                    let still_current = proof_matches(&from, &policy, &proof, new_deadline).await;
                    if !matches!(&still_current, Ok(true)) {
                        kill_fenced_group(&mut active.child, active.group_id, &mut crash_fence).await?;
                        still_current.context("unchanged active Root cannot be verified")?;
                        bail!("external configuration Root changed; App was stopped");
                    }
                    active.proof = proof;
                    active.observed_at = observed_at;
                    continue;
                }
                // Never overlap Generations. Clean retirement covers managed
                // resources only; unknown or forced cleanup keeps the fence.
                retirement::stop(&mut active, &mut crash_fence, true).await?;
                if let Some(path) = &ready_file {
                    fs::remove_file(path).context("withdraw previous Generation readiness")?;
                }
                active = launch(
                    &from, &policy, &executable, proof, observed_at, &mut crash_fence, &mut shutdown,
                ).await?;
                if let Some(path) = &ready_file
                    && let Err(error) = publish_ready(path)
                {
                    kill_fenced_group(&mut active.child, active.group_id, &mut crash_fence).await?;
                    return Err(error).context("publish replacement App readiness");
                }
                next_poll = Instant::now() + poll_interval(active.proof.max_stale_seconds);
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
    crash_fence: &mut CrashFence,
    shutdown: &mut ShutdownSignal,
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
    // Bootstrap Plugins execute outside this Host and have a separate cleanup
    // owner. Their sources retain conservative recovery until that is covered.
    let retirement = if cfg!(unix) && matches!(proof.source.kind(), "file_snapshot" | "https_poll")
    {
        let receipt = retirement::Receipt::new(from)?;
        receipt.configure(&mut command);
        Some(receipt)
    } else {
        command.env_remove("LENSO_MANAGED_SHUTDOWN_TOKEN");
        command.env_remove("LENSO_MANAGED_SHUTDOWN_RECEIPT");
        None
    };
    #[cfg(unix)]
    command.process_group(0);
    crash_fence.mark()?;
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            crash_fence.clear()?;
            return Err(error).context("start supervised local Host");
        }
    };
    let group_id = child.id().context("supervised Host process ID")?;
    let ready_deadline = (Instant::now() + READY_TIMEOUT).min(deadline);
    loop {
        tokio::select! {
            biased;
            _ = tokio::time::sleep_until(ready_deadline) => {
                kill_fenced_group(&mut child, group_id, crash_fence).await?;
                bail!("supervised App did not become ready before its startup or configuration deadline");
            }
            signal = shutdown.wait() => {
                kill_fenced_group(&mut child, group_id, crash_fence).await?;
                signal?;
                bail!("supervised App startup was interrupted");
            }
            status = wait_for_exit_unreaped(&mut child, group_id) => {
                kill_fenced_group(&mut child, group_id, crash_fence).await?;
                status?;
                bail!("supervised App exited before readiness: {}", child.wait().await?);
            }
            _ = tokio::time::sleep(Duration::from_millis(25)) => {
                match fs::symlink_metadata(&marker) {
                    Ok(metadata) => {
                        if !metadata.file_type().is_file() || fs::read(&marker).ok().as_deref() != Some(READY_RECEIPT) {
                            kill_fenced_group(&mut child, group_id, crash_fence).await?;
                            bail!("supervised App returned an invalid readiness receipt");
                        }
                        break;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        kill_fenced_group(&mut child, group_id, crash_fence).await?;
                        return Err(error).context("inspect supervised App readiness");
                    }
                }
            }
        }
    }

    let fenced = proof_matches(from, policy, &proof, deadline).await;
    if !matches!(&fenced, Ok(true)) {
        kill_fenced_group(&mut child, group_id, crash_fence).await?;
        ensure!(
            fenced?,
            "external configuration changed during Host startup"
        );
    }
    let from_owned = from.to_path_buf();
    let policy_owned = policy.to_path_buf();
    let activation_proof = proof.clone();
    let receipt = tokio::task::spawn_blocking(move || {
        configuration_source::record_distribution_activation(
            &from_owned,
            &policy_owned,
            &activation_proof,
        )
    });
    let recorded = tokio::time::timeout_at(deadline, receipt).await;
    if !matches!(&recorded, Ok(Ok(Ok(())))) {
        kill_fenced_group(&mut child, group_id, crash_fence).await?;
        recorded.context("configuration proof expired before activation receipt")???;
    }
    let fenced = proof_matches(from, policy, &proof, deadline).await;
    if !matches!(&fenced, Ok(true)) {
        kill_fenced_group(&mut child, group_id, crash_fence).await?;
        ensure!(
            fenced?,
            "external configuration changed after Host readiness"
        );
    }
    if Instant::now() >= deadline {
        kill_fenced_group(&mut child, group_id, crash_fence).await?;
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
        retirement,
    })
}

fn publish_ready(path: &Path) -> anyhow::Result<()> {
    let parent = path.parent().context("ready file needs a parent")?;
    let mut stage = tempfile::NamedTempFile::new_in(parent)?;
    stage.write_all(READY_RECEIPT)?;
    stage.persist(path)?;
    Ok(())
}

struct ShutdownSignal {
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
    #[cfg(unix)]
    interrupt: tokio::signal::unix::Signal,
}

impl ShutdownSignal {
    fn new() -> anyhow::Result<Self> {
        Ok(Self {
            #[cfg(unix)]
            terminate: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?,
            #[cfg(unix)]
            interrupt: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?,
        })
    }

    async fn wait(&mut self) -> anyhow::Result<()> {
        #[cfg(unix)]
        tokio::select! {
            _ = self.interrupt.recv() => {},
            _ = self.terminate.recv() => {},
        }
        #[cfg(not(unix))]
        tokio::signal::ctrl_c().await?;
        Ok(())
    }
}

async fn kill_fenced_group(
    child: &mut Child,
    group_id: u32,
    crash_fence: &mut CrashFence,
) -> anyhow::Result<()> {
    // A hard kill does not run the Host's adapter shutdown. Bun and other
    // descendants may own independent process groups, so keep the marker.
    kill_group(child, group_id).await?;
    ensure!(crash_fence.marked, "hard-stopped Host lost its crash fence");
    Ok(())
}

async fn kill_group(child: &mut Child, group_id: u32) -> anyhow::Result<std::process::ExitStatus> {
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
    #[cfg(unix)]
    let group_stopped = tokio::time::timeout(KILL_CONFIRM_TIMEOUT, async {
        wait_for_exit_unreaped(child, group_id).await?;
        loop {
            if group_only_zombies(group_id)? {
                return Ok::<(), anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await;
    let reaped = tokio::time::timeout(KILL_CONFIRM_TIMEOUT, child.wait()).await;
    #[cfg(unix)]
    group_stopped.context("timed out confirming supervised App process group stopped")??;
    Ok(reaped.context("timed out reaping supervised Host")??)
}

use super::local_host_retirement::wait_for_exit_unreaped;
#[cfg(unix)]
use super::local_host_retirement::{exited_unreaped, group_only_zombies};

#[cfg(unix)]
fn signal_group(group_id: u32, signal: nix::sys::signal::Signal) -> anyhow::Result<()> {
    use nix::{errno::Errno, sys::signal::killpg, unistd::Pid};
    let id = i32::try_from(group_id)?;
    match killpg(Pid::from_raw(id), signal) {
        Ok(()) | Err(Errno::ESRCH) => Ok(()),
        #[cfg(target_os = "macos")]
        Err(Errno::EPERM)
            if exited_unreaped(group_id)?
                && super::local_dev::darwin_group_only_zombies(group_id)? =>
        {
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt as _, path::Path, process::Command as StdCommand};

    use lenso_app_plan::authoring::{
        HostCatalog, HostDefaultPlugin, HostPluginRelease, HostSlot, PluginDescriptor,
    };

    use super::*;

    struct DetachedTestChild(PathBuf);

    impl Drop for DetachedTestChild {
        fn drop(&mut self) {
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            let pid = loop {
                if let Ok(contents) = fs::read_to_string(&self.0)
                    && let Ok(pid) = contents.parse::<i32>()
                    && pid > 0
                {
                    break pid;
                }
                if std::time::Instant::now() >= deadline {
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            };
            let Ok(output) = StdCommand::new("ps")
                .args(["-o", "pgid=,command=", "-p", &pid.to_string()])
                .output()
            else {
                return;
            };
            let process = String::from_utf8_lossy(&output.stdout);
            let mut fields = process.split_whitespace();
            let group = pid.to_string();
            if fields.next() == Some(group.as_str())
                && process.contains("detached_descendant_helper")
            {
                let _ = nix::sys::signal::kill(
                    nix::unistd::Pid::from_raw(pid),
                    nix::sys::signal::Signal::SIGKILL,
                );
            }
        }
    }

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
    async fn accepted_update_hard_stops_and_requires_recovery_before_replacement() {
        let (temporary, snapshot, policy) = fixture();
        let root = temporary.path().to_path_buf();
        let supervised = tokio::spawn(run(root.clone(), policy.clone(), None));
        wait_for_activation(&root, 1).await;
        let first_pid = fs::read_to_string(root.join("previous-pid")).unwrap();
        write_snapshot(&snapshot, 2, "second");
        let result = tokio::time::timeout(Duration::from_secs(6), supervised)
            .await
            .unwrap()
            .unwrap();
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("managed cleanup is unconfirmed")
        );
        assert!(!root.join("overlap").exists());
        assert_eq!(
            fs::read_to_string(root.join("previous-pid")).unwrap(),
            first_pid
        );
        assert!(root.join(".lenso/supervised-start.uncertain").is_file());
        let status = configuration_source::inspect_status(&root).unwrap();
        assert_eq!(status.last_activated_revision, Some(1));
        assert!(!status.desired_matches_last_activated_root_and_policy);
        let retry = run(root, policy, None).await.unwrap_err();
        assert!(retry.to_string().contains("unconfirmed"), "{retry:#}");
    }

    #[tokio::test]
    async fn accepted_revision_with_unchanged_root_keeps_current_host() {
        let (temporary, snapshot, policy) = fixture();
        let root = temporary.path().to_path_buf();
        let supervised = tokio::spawn(run(root.clone(), policy, None));
        wait_for_activation(&root, 1).await;
        let first_pid = fs::read_to_string(root.join("previous-pid")).unwrap();

        write_snapshot(&snapshot, 2, "first");
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if configuration_source::inspect_status(&root)
                    .ok()
                    .and_then(|status| status.desired_revision)
                    == Some(2)
                {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap();

        // Let the supervisor consume the accepted poll result and complete
        // another revalidation cycle before asserting that it kept serving.
        tokio::time::sleep(Duration::from_secs(2)).await;

        assert!(!supervised.is_finished(), "unchanged Root stopped the Host");
        assert_eq!(
            fs::read_to_string(root.join("previous-pid")).unwrap(),
            first_pid
        );
        assert!(!root.join("overlap").exists());
        let status = configuration_source::inspect_status(&root).unwrap();
        assert_eq!(status.desired_revision, Some(2));
        assert_eq!(status.last_activated_revision, Some(1));
        assert!(status.desired_matches_last_activated_root_and_policy);
        assert!(status.pending_activation);

        write_snapshot(&snapshot, 3, "second");
        let result = tokio::time::timeout(Duration::from_secs(6), supervised)
            .await
            .unwrap()
            .unwrap();
        assert!(result.unwrap_err().to_string().contains("hard-stopped"));
        assert!(root.join(".lenso/supervised-start.uncertain").is_file());
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
    async fn cancelled_supervisor_keeps_an_uncertain_start_fence() {
        let (temporary, snapshot, policy) = fixture();
        let root = temporary.path().to_path_buf();
        write_policy(&policy, &snapshot, 10);
        let ready = root.join("first-supervisor-ready");
        let first = tokio::spawn(run(root.clone(), policy.clone(), Some(ready.clone())));
        tokio::time::timeout(Duration::from_secs(5), async {
            while !ready.is_file() {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap();
        let host_pid: u32 = fs::read_to_string(root.join("previous-pid"))
            .unwrap()
            .parse()
            .unwrap();
        first.abort();
        let _ = first.await;
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let output = StdCommand::new("ps")
                    .args(["-o", "state=", "-p", &host_pid.to_string()])
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
        .expect("cancelled supervisor left its direct Host alive");

        let second = tokio::time::timeout(Duration::from_secs(1), run(root, policy, None)).await;
        let error = second
            .expect("uncertain start should be rejected promptly")
            .expect_err("uncertain start must not launch a new Host");
        assert!(error.to_string().contains("unconfirmed"), "{error:#}");
    }

    #[tokio::test]
    async fn clean_host_exit_still_requires_manual_recovery_before_restart() {
        use nix::{sys::signal::Signal, unistd::Pid};

        let (temporary, snapshot, policy) = fixture();
        let root = temporary.path().to_path_buf();
        write_policy(&policy, &snapshot, 10);
        let host = root.join(".lenso/host");
        fs::write(
            &host,
            b"#!/bin/sh\nset -eu\nroot=$(CDPATH= cd -- \"$(dirname \"$0\")/..\" && pwd)\ntrap 'exit 0' TERM\nwhile [ \"$1\" != --ready-file ]; do shift; done\nprintf '%s' \"$$\" > \"$root/previous-pid\"\nprintf 'lenso.local-host-ready.v1\\n' > \"$2\"\nwhile :; do sleep 0.1; done\n",
        )
        .unwrap();
        fs::set_permissions(&host, fs::Permissions::from_mode(0o700)).unwrap();
        let first_ready = root.join("first-supervisor-ready");
        let first = tokio::spawn(run(root.clone(), policy.clone(), Some(first_ready.clone())));
        tokio::time::timeout(Duration::from_secs(5), async {
            while !first_ready.is_file() {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap();
        let first_pid: i32 = fs::read_to_string(root.join("previous-pid"))
            .unwrap()
            .parse()
            .unwrap();
        nix::sys::signal::kill(Pid::from_raw(first_pid), Signal::SIGTERM).unwrap();
        let first_result = tokio::time::timeout(Duration::from_secs(4), first)
            .await
            .expect("first supervisor should observe clean Host exit")
            .unwrap()
            .unwrap_err();
        assert!(
            first_result
                .to_string()
                .contains("managed cleanup is unconfirmed")
        );
        assert!(root.join(".lenso/supervised-start.uncertain").is_file());
        let retry = run(root, policy, None).await.unwrap_err();
        assert!(retry.to_string().contains("unconfirmed"), "{retry:#}");
    }

    #[tokio::test]
    async fn failed_spawn_without_child_clears_uncertain_fence() {
        let (temporary, _snapshot, policy) = fixture();
        let root = temporary.path();
        let host = root.join(".lenso/host");
        fs::set_permissions(&host, fs::Permissions::from_mode(0o600)).unwrap();
        let error = run(root.to_path_buf(), policy, None).await.unwrap_err();
        assert!(error.to_string().contains("start supervised local Host"));
        assert!(!root.join(".lenso/supervised-start.uncertain").exists());
    }

    #[test]
    fn detached_descendant_helper() {
        let Some(pid_file) = std::env::var_os("LENSO_TEST_DETACHED_PID_FILE") else {
            return;
        };
        assert!(unsafe { nix::libc::setsid() } > 0);
        fs::write(pid_file, std::process::id().to_string()).unwrap();
        std::thread::sleep(Duration::from_secs(20));
    }

    #[tokio::test]
    async fn clean_host_exit_cannot_clear_fence_for_detached_descendant() {
        use nix::{sys::signal::Signal, unistd::Pid};

        let (temporary, snapshot, policy) = fixture();
        let root = temporary.path().to_path_buf();
        write_policy(&policy, &snapshot, 10);
        let test_binary = std::env::current_exe().unwrap();
        let quoted_binary = test_binary.to_string_lossy().replace('\'', "'\\''");
        let host = root.join(".lenso/host");
        let script = format!(
            "#!/bin/sh\nset -eu\nroot=$(CDPATH= cd -- \"$(dirname \"$0\")/..\" && pwd)\ntrap 'exit 0' TERM\nif [ ! -f \"$root/previous-pid\" ]; then\n  LENSO_TEST_DETACHED_PID_FILE=\"$root/detached-pid\" '{quoted_binary}' --exact app::local_start::tests::detached_descendant_helper >/dev/null 2>&1 &\n  tries=0\n  while [ ! -s \"$root/detached-pid\" ]; do\n    tries=$((tries + 1))\n    [ \"$tries\" -lt 200 ] || exit 1\n    sleep 0.01\n  done\nfi\nwhile [ \"$1\" != --ready-file ]; do shift; done\nprintf '%s' \"$$\" > \"$root/previous-pid\"\nprintf 'lenso.local-host-ready.v1\\n' > \"$2\"\nwhile :; do sleep 0.1; done\n"
        );
        fs::write(&host, script).unwrap();
        fs::set_permissions(&host, fs::Permissions::from_mode(0o700)).unwrap();
        let detached_guard = DetachedTestChild(root.join("detached-pid"));
        let ready = root.join("supervisor-ready");
        let supervised = tokio::spawn(run(root.clone(), policy.clone(), Some(ready.clone())));
        tokio::time::timeout(Duration::from_secs(5), async {
            while !ready.is_file() {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap();
        let host_pid: i32 = fs::read_to_string(root.join("previous-pid"))
            .unwrap()
            .parse()
            .unwrap();
        let detached_pid: i32 = fs::read_to_string(root.join("detached-pid"))
            .unwrap()
            .parse()
            .unwrap();
        nix::sys::signal::kill(Pid::from_raw(host_pid), Signal::SIGTERM).unwrap();
        let first = tokio::time::timeout(Duration::from_secs(4), supervised)
            .await
            .expect("supervisor should observe clean Host exit")
            .unwrap()
            .unwrap_err();
        assert!(first.to_string().contains("managed cleanup is unconfirmed"));

        let observed = StdCommand::new("ps")
            .args([
                "-o",
                "state=,pgid=,command=",
                "-p",
                &detached_pid.to_string(),
            ])
            .output()
            .unwrap();
        let process = String::from_utf8_lossy(&observed.stdout);
        let mut fields = process.split_whitespace();
        let state = fields.next().unwrap_or_default();
        let group = fields.next().unwrap_or_default();
        let owned = !state.starts_with('Z')
            && group == detached_pid.to_string()
            && process.contains("detached_descendant_helper");
        let retry =
            tokio::time::timeout(Duration::from_secs(1), run(root.clone(), policy, None)).await;
        assert!(owned, "expected detached test child still live: {process}");
        let retry = retry
            .expect("second start should be rejected promptly")
            .expect_err("second start must not launch with detached child live");
        assert!(retry.to_string().contains("unconfirmed"), "{retry:#}");
        assert!(root.join(".lenso/supervised-start.uncertain").is_file());
        drop(detached_guard);
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let output = StdCommand::new("ps")
                    .args(["-o", "state=", "-p", &detached_pid.to_string()])
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
        .expect("detached test child remained live after cleanup");
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
        assert!(error.to_string().contains("before readiness"), "{error:#}");
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
