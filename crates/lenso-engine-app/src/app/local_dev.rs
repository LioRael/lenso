//! Local development rebuilds complete App generations. Without external
//! configuration, a failed candidate leaves the current preview running.
//! Policy-supervised replacement may prepare a locked candidate while the old
//! preview runs. Dynamic readiness can activate Kernel side effects, so one
//! replacement starts only after the previous managed Host positively retires.
use anyhow::{Context, bail};
use clap::Args;
use notify::{RecursiveMode, Watcher};
use std::{
    fs,
    future::Future,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{
    process::{Child, Command},
    sync::mpsc,
    time::Instant,
};

use super::configuration_source::AcceptedSourceProof;

mod changes;
mod frontend;
mod managed_host;
use managed_host::{Host, Retirement};

#[derive(Clone, Debug, Args)]
pub struct DevArgs {
    /// App source root. Defaults to the current directory.
    #[arg(long)]
    root: Option<PathBuf>,
    /// Host-operator policy for an external versioned configuration source.
    #[arg(long)]
    configuration_policy: Option<PathBuf>,
    /// Poll interval for the configured source, including HTTPS revalidation.
    #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u64).range(1..=3600))]
    configuration_poll_seconds: u64,
    /// Trust exact adopted linked Cargo or npm build-time code for each unsandboxed rebuild.
    #[arg(
        long,
        visible_alias = "trust-adopted-build",
        value_name = "PLUGIN_ID@VERSION=sha256:DIGEST"
    )]
    trust_linked_build: Vec<String>,
    /// Arguments for installed terminal support, rerun after each successful rebuild.
    #[arg(last = true)]
    args: Vec<String>,
}

#[derive(Clone, Debug)]
struct TimedProof {
    accepted: AcceptedSourceProof,
    received_at: Instant,
}

impl TimedProof {
    fn deadline(&self) -> Instant {
        self.received_at + Duration::from_secs(self.accepted.max_stale_seconds)
    }

    fn is_fresh(&self) -> bool {
        Instant::now() < self.deadline()
    }
}

pub async fn dev(args: DevArgs) -> anyhow::Result<()> {
    anyhow::ensure!(
        (1..=3600).contains(&args.configuration_poll_seconds),
        "configuration poll interval must be between 1 and 3600 seconds"
    );
    let root = crate::plugins::project_root(args.root)?;
    let policy = args
        .configuration_policy
        .map(absolute_configuration_policy)
        .transpose()?;
    fs::create_dir_all(root.join(".lenso"))?;
    let _dev_lock = lock_dev(&root)?;
    match fs::remove_file(root.join(".lenso/dev-feedback.json")) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("clear previous development feedback"),
    }
    Retirement::check_session(&root)?;
    let generations = tempfile::Builder::new()
        .prefix("dev-")
        .tempdir_in(root.join(".lenso"))?;
    let frontend_config = frontend::FrontendConfig::load(&root)?;
    let frontend_enabled = frontend_config.is_some();
    let (mut watcher, mut events, mut watch_overflow) = watch(&root)?;
    let mut host: Option<Host> = None;
    let mut frontend_process: Option<frontend::FrontendProcess> = None;
    let mut active_backend_url: Option<String> = None;
    let mut revision = 0;
    let mut current_output: Option<PathBuf> = None;
    let mut active: Option<TimedProof> = None;
    let mut active_inputs: Option<changes::Inputs> = None;
    let mut pending_change = Some(changes::Batch::new(
        notify::Event::new(notify::EventKind::Any).add_path(root.clone()),
    ));
    let mut supervised_dynamic_start_available = true;
    let configured_poll = Duration::from_secs(args.configuration_poll_seconds);
    let mut effective_poll = configured_poll;
    let mut poll = tokio::time::interval(configured_poll);
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    poll.tick().await;
    // Register once, then keep the subscription alive while source sync or
    // Host preparation runs outside the main select loops.
    let interrupt = tokio::signal::ctrl_c();
    tokio::pin!(interrupt);
    if let std::task::Poll::Ready(signal) =
        std::future::poll_fn(|cx| std::task::Poll::Ready(interrupt.as_mut().poll(cx))).await
    {
        signal?;
        return Ok(());
    }
    let result: anyhow::Result<()> = async {
        loop {
        revision += 1;
        let output = generations.path().join(format!("generation-{revision}"));
        let mut compiled = true;
        let mut reused = false;
        let mut reuse_failed = false;
        let inputs_before_build = if policy.is_none() {
            changes::Inputs::capture(&root, frontend_enabled).ok().flatten()
        } else { None };
        if policy.is_none()
            && let (Some(batch), Some(current), Some(inputs)) =
                (&pending_change, &current_output, &active_inputs)
            && batch.work(&root, frontend_enabled) == changes::Work::Configuration
        {
            match changes::configuration_candidate(&root, current, &output, batch, inputs, frontend_enabled) {
                Ok(value) => { reused = value; compiled = !value; }
                Err(error) => {
                    eprintln!("Configuration candidate rejected; previous preview retained: {error:#}");
                    reuse_failed = true;
                    compiled = false;
                }
            }
        }
        let built = if reused { true } else if reuse_failed { false } else {
        if let Some(batch) = &pending_change {
            batch.report(&root, batch.work(&root, frontend_enabled), "building", revision, true, "preparing source candidate; previous preview remains active")?;
        }
        let mut build = command(std::env::current_exe()?);
        build
            .args(["app", "build", "--root"])
            .arg(&root)
            .arg("--out")
            .arg(&output);
        for declaration in &args.trust_linked_build {
            build.arg("--trust-linked-build").arg(declaration);
        }
        let mut child = build.spawn().context("start local App build")?;
        let status = loop {
            let deadline = active.as_ref().map(TimedProof::deadline);
            tokio::select! {
                biased;
                () = sleep_until_optional(deadline) => {
                    expire_active_if_needed(
                        &root,
                        &mut active, &mut host, &mut frontend_process,
                        &mut active_backend_url,
                    ).await?;
                }
                status = child.wait() => break status?,
                signal = &mut interrupt => {
                    signal?;
                    stop(&mut child, true).await?;
                    stop_active(&mut host, &mut frontend_process).await?;
                    return Ok(());
                }
                _ = poll.tick(), if policy.is_some() => {
                    let policy = policy.as_deref().expect("poll branch requires policy");
                    if retire_active_on_policy_change(
                        &root, current_output.as_deref(), policy, &mut host,
                        &mut frontend_process, &mut active_backend_url,
                    ).await? {
                        active = None;
                    }
                    if let Some(current) = current_output.as_deref() {
                        match sync_source_with_deadline(
                            current,
                            LivePreviewGuard {
                                root: &root,
                                active_output: current_output.as_deref(),
                                policy,
                                host: &mut host,
                                frontend: &mut frontend_process,
                                active_backend_url: &mut active_backend_url,
                                active: &mut active,
                            },
                        ).await {
                            Ok(proof) => {
                                update_poll_interval(
                                    &mut poll, &mut effective_poll, configured_poll,
                                    proof.accepted.max_stale_seconds,
                                ).await;
                                refresh_active_if_matching(&mut active, &mut host, &proof);
                            }
                            Err(error) => eprintln!(
                                "Configuration source unavailable during rebuild: {error:#}"
                            ),
                        }
                    }
                }
            }
        };
        status.success()
        };
        watch_dependencies(&root, &mut watcher)?;
        expire_active_if_needed(
            &root,
            &mut active, &mut host, &mut frontend_process, &mut active_backend_url,
        ).await?;
        if let Some(policy) = &policy
            && retire_active_on_policy_change(
                &root,
                current_output.as_deref(),
                policy,
                &mut host,
                &mut frontend_process,
                &mut active_backend_url,
            )
            .await?
        {
            active = None;
        }
        if built {
            let activation = if let Some(policy) = &policy {
                match sync_source_with_deadline(
                    &output,
                    LivePreviewGuard {
                        root: &root,
                        active_output: current_output.as_deref(),
                        policy,
                        host: &mut host,
                        frontend: &mut frontend_process,
                        active_backend_url: &mut active_backend_url,
                        active: &mut active,
                    },
                ).await {
                    Ok(proof) => {
                        update_poll_interval(
                            &mut poll, &mut effective_poll, configured_poll,
                            proof.accepted.max_stale_seconds,
                        ).await;
                        activate_supervised_candidate(
                            &output, &args.args, frontend_config.as_ref(), proof,
                            LivePreviewGuard {
                                root: &root,
                                active_output: current_output.as_deref(),
                                policy,
                                host: &mut host,
                                frontend: &mut frontend_process,
                                active_backend_url: &mut active_backend_url,
                                active: &mut active,
                            },
                            &mut supervised_dynamic_start_available,
                        ).await?
                    }
                    Err(error) => {
                        eprintln!("App configuration source rejected; candidate not activated: {error:#}");
                        Some(false)
                    }
                }
            } else if let Err(error) = preflight(&output).await {
                eprintln!("App candidate failed readiness; candidate not activated: {error:#}");
                Some(false)
            } else if let Some(config) = &frontend_config {
                activate_candidate_with_frontend(
                    &output,
                    &args.args,
                    FrontendPreview {
                        root: &root,
                        host: &mut host,
                        frontend_process: &mut frontend_process,
                        active_backend_url: &mut active_backend_url,
                        config,
                    },
                    false,
                ).await?
            } else {
                activate_candidate(&output, &args.args, &mut host, false, false).await?
            };
            match activation {
                Some(true) => {
                    select_output(&mut current_output, &output);
                    active_inputs = if policy.is_some() { None } else { match changes::Inputs::capture(&root, frontend_enabled) {
                        Ok(inputs) => inputs.filter(|after| reused || inputs_before_build.as_ref().is_some_and(|before| after.agrees_with_before_build(before))),
                        Err(error) => { eprintln!("Incremental configuration reuse unavailable: {error:#}"); None }
                    } };
                    if let Some(batch) = &pending_change {
                        batch.report(&root, batch.work(&root, frontend_enabled), "ready", revision, compiled,
                            if reused { "configuration re-resolved; fresh Host generation; execution artifacts reused" }
                            else { "source build completed; fresh Host generation" })?;
                    }
                    eprintln!(
                        "Watching {} for App changes. Press Ctrl-C to stop.",
                        root.display()
                    );
                }
                Some(false) => {
                    if let Some(batch) = &pending_change {
                        batch.report(&root, batch.work(&root, frontend_enabled), "rejected", revision, compiled, "candidate failed readiness; previous preview retained")?;
                    }
                }
                None => {
                    stop_active(&mut host, &mut frontend_process).await?;
                    return Ok(());
                }
            }
        } else {
            eprintln!("App rebuild failed; edit the source to retry.");
            if let Some(batch) = &pending_change {
                batch.report(&root, batch.work(&root, frontend_enabled), "rejected", revision, compiled, "candidate preparation failed; previous preview retained")?;
            }
        }
        loop {
            let deadline = active.as_ref().map(TimedProof::deadline);
            tokio::select! {
                biased;
                () = sleep_until_optional(deadline) => {
                    expire_active_if_needed(
                        &root,
                        &mut active, &mut host, &mut frontend_process,
                        &mut active_backend_url,
                    ).await?;
                }
                signal = &mut interrupt => {
                    signal?;
                    stop_active(&mut host, &mut frontend_process).await?;
                    return Ok(());
                }
                event = events.recv() => {
                    match event.context("App watcher closed")? {
                        Ok(event) if rebuild_event(&event) => {
                            let mut batch = changes::Batch::new(event);
                            match run_until(active.as_ref().map(TimedProof::deadline), batch.collect(&mut events)).await {
                                Ok(result) => result?,
                                Err(_) => { expire_active_if_needed(
                                    &root, &mut active, &mut host, &mut frontend_process, &mut active_backend_url,
                                ).await?; }
                            }
                            if watch_overflow.swap(false, std::sync::atomic::Ordering::SeqCst) {
                                // A full frontend-only queue cannot prove that a later
                                // backend event was retained. Unknown changes rebuild.
                                batch.paths.insert(root.clone());
                            }
                            if batch.paths.iter().any(|p| frontend::is_config(&root, p)) {
                                eprintln!("Frontend dev configuration changed; restart lenso dev to review and apply its command.");
                            }
                            if batch.work(&root, frontend_enabled) == changes::Work::Frontend {
                                // The explicitly selected frontend owns its HMR/rebuild loop.
                                // Its source edits do not invalidate the Rust Host distribution.
                                batch.report(&root, changes::Work::Frontend, "delegated", revision, false, "frontend dev server owns reload; no Host build or restart")?;
                                continue;
                            }
                            pending_change = Some(batch);
                            // Configuration edits may add a previously unwatched shared source.
                            match watch(&root) {
                                Ok((next, receiver, overflow)) => { watcher = next; events = receiver; watch_overflow = overflow; }
                                Err(error) => eprintln!("App source configuration is invalid: {error:#}"),
                            }
                            break;
                        }
                        Ok(_) => {}
                        Err(error) => {
                            stop_active(&mut host, &mut frontend_process).await?;
                            return Err(error).context("App watcher failed");
                        }
                    }
                }
                () = tokio::time::sleep(Duration::from_millis(200)) => {
                    #[cfg(unix)]
                    let host_exit = if let Some(process) = &mut host {
                        match process.id() { Some(id) => exited_unreaped(id)?, None => false }
                    } else { false };
                    #[cfg(not(unix))]
                    let host_exit = if let Some(process) = &mut host {
                        process.try_wait()?.is_some()
                    } else { false };
                    if host_exit {
                            let process = host.as_mut().context("exited Host")?;
                            if let Some(proof) = &active { process.renew(proof.deadline()); }
                            process.retire().await?;
                            eprintln!("Local Host retired; edit the source to restart.");
                            host = None;
                            if let Some(frontend) = &mut frontend_process { frontend::stop(frontend).await?; }
                            frontend_process = None;
                            active_backend_url = None;
                            active = None;
                            if policy.is_some() { restore_backend_url(&root, None)?; }
                    }
                    if let Some(process) = &mut frontend_process
                        && process.exited_unreaped()? {
                            stop_active_now(&mut host, &mut frontend_process).await?;
                            if policy.is_some() { restore_backend_url(&root, None)?; }
                            bail!("Frontend dev process exited; preview is no longer ready");
                        }
                }
                _ = poll.tick(), if policy.is_some() => {
                    // A successfully built candidate may have failed its first
                    // source read. Retry that exact build on source recovery;
                    // the source can live outside the watched project tree.
                    let target = if built && current_output.as_deref() != Some(output.as_path()) {
                        Some(output.clone())
                    } else {
                        current_output.clone()
                    };
                    let Some(target) = target else {
                        // A failed source build has no distribution to poll;
                        // await an actual source edit rather than rebuilding
                        // the same broken candidate every interval.
                        continue;
                    };
                    let policy = policy.as_deref().expect("poll branch requires policy");
                    if retire_active_on_policy_change(
                        &root,
                        current_output.as_deref(),
                        policy,
                        &mut host,
                        &mut frontend_process,
                        &mut active_backend_url,
                    )
                    .await? {
                        active = None;
                    }
                    match sync_source_with_deadline(
                        &target,
                        LivePreviewGuard {
                            root: &root,
                            active_output: current_output.as_deref(),
                            policy,
                            host: &mut host,
                            frontend: &mut frontend_process,
                            active_backend_url: &mut active_backend_url,
                            active: &mut active,
                        },
                    ).await {
                        Ok(proof) => {
                            update_poll_interval(
                                &mut poll, &mut effective_poll, configured_poll,
                                proof.accepted.max_stale_seconds,
                            ).await;
                            if current_output.as_deref() == Some(target.as_path()) {
                                refresh_active_if_matching(&mut active, &mut host, &proof);
                            }
                            match super::configuration_source::inspect_status(&target) {
                                Ok(status) if !status.pending_publication &&
                                    (status.pending_activation || host.is_none() ||
                                     current_output.as_deref() != Some(target.as_path())) => {
                                    match activate_supervised_candidate(
                                        &target, &args.args,
                                        frontend_config.as_ref(), proof,
                                        LivePreviewGuard {
                                            root: &root,
                                            active_output: current_output.as_deref(),
                                            policy,
                                            host: &mut host,
                                            frontend: &mut frontend_process,
                                            active_backend_url: &mut active_backend_url,
                                            active: &mut active,
                                        },
                                        &mut supervised_dynamic_start_available,
                                    ).await? {
                                        Some(true) => select_output(&mut current_output, &target),
                                        Some(false) => {},
                                        None => {
                                            stop_active(&mut host, &mut frontend_process).await?;
                                            return Ok(());
                                        }
                                    }
                                }
                                Ok(_) => {}
                                Err(error) => eprintln!("Configuration status is invalid; candidate not activated: {error:#}"),
                            }
                        }
                        Err(error) => eprintln!("Configuration source unavailable or rejected; candidate not activated: {error:#}"),
                    }
                }
            }
        }
        // Retain the OS watcher while building, so edits during compilation are queued.
        let _ = &watcher;
        }
    }
    .await;
    finish_dev(result, &mut host, &mut frontend_process).await
}

fn absolute_configuration_policy(path: PathBuf) -> anyhow::Result<PathBuf> {
    Ok(std::path::absolute(path)?)
}

async fn finish_dev(
    result: anyhow::Result<()>,
    host: &mut Option<Host>,
    frontend: &mut Option<frontend::FrontendProcess>,
) -> anyhow::Result<()> {
    let shutdown = stop_active(host, frontend).await;
    match (result, shutdown) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(error), Err(shutdown_error)) => Err(error).context(format!(
            "local preview shutdown also failed: {shutdown_error:#}"
        )),
    }
}

async fn expire_active_if_needed(
    root: &Path,
    active: &mut Option<TimedProof>,
    host: &mut Option<Host>,
    frontend: &mut Option<frontend::FrontendProcess>,
    active_backend_url: &mut Option<String>,
) -> anyhow::Result<bool> {
    if active.as_ref().is_none_or(TimedProof::is_fresh) {
        return Ok(false);
    }
    eprintln!(
        "Configuration source freshness expired; stopping the running preview until an operator verifies retirement and restarts"
    );
    stop_active_now(host, frontend).await?;
    *active_backend_url = None;
    restore_backend_url(root, None)?;
    *active = None;
    Ok(true)
}

fn refresh_active_if_matching(
    active: &mut Option<TimedProof>,
    host: &mut Option<Host>,
    proof: &TimedProof,
) {
    if proof.is_fresh()
        && active.as_ref().is_some_and(|current| {
            current.accepted.source == proof.accepted.source
                && current.accepted.policy_digest == proof.accepted.policy_digest
                && current.accepted.plugin_root_revision == proof.accepted.plugin_root_revision
        })
    {
        *active = Some(proof.clone());
        if let Some(host) = host {
            host.renew(proof.deadline());
        }
    }
}

async fn sync_source_with_deadline(
    output: &Path,
    guard: LivePreviewGuard<'_>,
) -> anyhow::Result<TimedProof> {
    let LivePreviewGuard {
        root,
        active_output,
        policy,
        host,
        frontend,
        active_backend_url,
        active,
    } = guard;
    let output = output.to_path_buf();
    let policy_path = policy.to_path_buf();
    run_source_sync_with_deadline(
        root,
        Some((active_output, policy)),
        move || super::configuration_source::sync_with_proof(&output, &policy_path),
        active,
        host,
        frontend,
        active_backend_url,
    )
    .await
}

async fn run_source_sync_with_deadline<F>(
    root: &Path,
    policy_check: Option<(Option<&Path>, &Path)>,
    sync: F,
    active: &mut Option<TimedProof>,
    host: &mut Option<Host>,
    frontend: &mut Option<frontend::FrontendProcess>,
    active_backend_url: &mut Option<String>,
) -> anyhow::Result<TimedProof>
where
    F: FnOnce() -> anyhow::Result<super::configuration_source::AcceptedSourceProof>
        + Send
        + 'static,
{
    let started_at = Instant::now();
    let mut job = tokio::task::spawn_blocking(move || {
        sync().map(|accepted| TimedProof {
            accepted,
            received_at: started_at,
        })
    });
    let mut check = tokio::time::interval(Duration::from_millis(250));
    check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        let deadline = active.as_ref().map(TimedProof::deadline);
        tokio::select! {
            result = &mut job => {
                if let Some((active_output, policy)) = policy_check
                    && retire_active_on_policy_change(
                        root, active_output, policy, host, frontend, active_backend_url,
                    ).await? { *active = None; }
                expire_active_if_needed(root, active, host, frontend, active_backend_url).await?;
                return result.context("configuration source task failed")?;
            }
            _ = check.tick(), if policy_check.is_some() => {
                let (active_output, policy) = policy_check.expect("policy check branch");
                if retire_active_on_policy_change(
                    root, active_output, policy, host, frontend, active_backend_url,
                ).await? { *active = None; }
            }
            () = async {
                if let Some(deadline) = deadline {
                    tokio::time::sleep_until(deadline).await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {
                expire_active_if_needed(root, active, host, frontend, active_backend_url).await?;
            }
        }
    }
}

fn proof_still_usable(output: &Path, policy: &Path, proof: &TimedProof) -> bool {
    proof.is_fresh()
        && matches!(
            super::configuration_source::proof_matches_current(output, policy, &proof.accepted),
            Ok(true)
        )
}

async fn run_until<T>(
    source_deadline: Option<Instant>,
    operation: impl Future<Output = T>,
) -> Result<T, tokio::time::error::Elapsed> {
    if let Some(deadline) = source_deadline {
        tokio::time::timeout_at(deadline, operation).await
    } else {
        Ok(operation.await)
    }
}

async fn activate_supervised_candidate(
    output: &Path,
    args: &[String],
    config: Option<&frontend::FrontendConfig>,
    proof: TimedProof,
    guard: LivePreviewGuard<'_>,
    dynamic_start_available: &mut bool,
) -> anyhow::Result<Option<bool>> {
    let LivePreviewGuard {
        root,
        active_output,
        policy,
        host,
        frontend: frontend_process,
        active_backend_url,
        active,
    } = guard;
    if !proof_still_usable(output, policy, &proof) {
        if retire_active_on_policy_change(
            root,
            active_output,
            policy,
            host,
            frontend_process,
            active_backend_url,
        )
        .await?
        {
            *active = None;
        }
        expire_active_if_needed(root, active, host, frontend_process, active_backend_url).await?;
        if active.is_none() && (host.is_some() || frontend_process.is_some()) {
            stop_active_now(host, frontend_process).await?;
            *active_backend_url = None;
            restore_backend_url(root, None)?;
        }
        eprintln!("Configuration source proof changed or expired; candidate not activated");
        return Ok(Some(false));
    }
    if active_output == Some(output)
        && host.is_some()
        && active.as_ref().is_some_and(|previous| {
            previous.accepted.source == proof.accepted.source
                && previous.accepted.policy_digest == proof.accepted.policy_digest
                && previous.accepted.plugin_root_revision == proof.accepted.plugin_root_revision
        })
    {
        // This renews authority for the already-running Root; it is not a
        // new activation. Keep the original activation receipt, as app start does.
        refresh_active_if_matching(active, host, &proof);
        return Ok(Some(true));
    }
    let preparation_deadline = active.as_ref().map_or(proof.deadline(), |previous| {
        previous.deadline().min(proof.deadline())
    });
    let preparation = static_prepare_until(
        output,
        Some(preparation_deadline),
        LivePreviewGuard {
            root,
            active_output,
            policy,
            host,
            frontend: frontend_process,
            active_backend_url,
            active,
        },
    )
    .await;
    if retire_active_on_policy_change(
        root,
        active_output,
        policy,
        host,
        frontend_process,
        active_backend_url,
    )
    .await?
    {
        *active = None;
    }
    expire_active_if_needed(root, active, host, frontend_process, active_backend_url).await?;
    if let Err(error) = preparation {
        eprintln!(
            "Configuration candidate failed static preparation; candidate not activated: {error:#}"
        );
        return Ok(Some(false));
    }
    if !proof_still_usable(output, policy, &proof) {
        eprintln!(
            "Configuration source proof changed or expired during preparation; candidate not activated"
        );
        return Ok(Some(false));
    }
    if let Some(config) = config {
        if host.is_some() || frontend_process.is_some() || !*dynamic_start_available {
            eprintln!(
                "Frontend dev has no managed-retirement acknowledgement; automatic replacement remains unsupported. Verify retirement before manually removing the persistent dev fence and restarting."
            );
            return Ok(Some(false));
        }
        Retirement::fence_session(root)?;
        eprintln!(
            "Frontend dev retirement cannot be confirmed automatically; this session's persistent recovery fence will remain after shutdown."
        );
        *dynamic_start_available = false;
        let activation = activate_candidate_with_frontend_until(
            output,
            args,
            FrontendPreview {
                root,
                host,
                frontend_process,
                active_backend_url,
                config,
            },
            true,
            Some(proof.deadline()),
            Some((policy, &proof.accepted)),
        )
        .await?;
        if activation == Some(true) {
            if !proof_still_usable(output, policy, &proof) {
                stop_active_now(host, frontend_process).await?;
                *active_backend_url = None;
                restore_backend_url(root, None)?;
                return Ok(Some(false));
            }
            *active = Some(proof);
        }
        return Ok(activation);
    }
    // Static preparation does not start a Kernel. Actual Ready is the only
    // dynamic startup; retire the old generation before that startup begins.
    if let Some(previous) = host {
        if let Some(previous_proof) = active.as_ref() {
            previous.renew(previous_proof.deadline());
        }
        previous.retire().await?;
    }
    *host = None;
    *active = None;
    *active_backend_url = None;
    restore_backend_url(root, None)?;
    Retirement::check_session(root)?;
    if !proof_still_usable(output, policy, &proof) {
        eprintln!(
            "Configuration source proof changed or expired during retirement; candidate not activated"
        );
        return Ok(Some(false));
    }
    let eligible =
        cfg!(unix) && matches!(proof.accepted.source.kind(), "file_snapshot" | "https_poll");
    *dynamic_start_available = false;
    let retirement = Retirement::new(root, eligible, proof.deadline())?;
    let mut candidate = match launch_configured_until(
        output,
        args,
        true,
        false,
        Some(proof.deadline()),
        Some(retirement),
    )
    .await
    {
        Ok(Some(candidate)) => candidate,
        Ok(None) => return Ok(None),
        Err(error) => {
            return Err(error)
                .context("supervised dev candidate startup failed; session remains fenced");
        }
    };
    if !proof_still_usable(output, policy, &proof) {
        kill_process_group_now(&mut candidate).await?;
        bail!("configuration source changed or expired during readiness; session remains fenced");
    }
    if let Err(error) =
        super::configuration_source::record_distribution_activation(output, policy, &proof.accepted)
    {
        kill_process_group_now(&mut candidate).await?;
        return Err(error).context("record dev activation; session remains fenced");
    }
    if !proof_still_usable(output, policy, &proof) {
        kill_process_group_now(&mut candidate).await?;
        bail!("configuration source changed or expired after readiness; session remains fenced");
    }
    *host = Some(candidate);
    *active = Some(proof);
    *dynamic_start_available = eligible;
    Ok(Some(true))
}

fn effective_poll_interval(configured: Duration, max_stale_seconds: u64) -> Duration {
    configured.min((Duration::from_secs(max_stale_seconds) / 2).max(Duration::from_millis(1)))
}

async fn update_poll_interval(
    poll: &mut tokio::time::Interval,
    current: &mut Duration,
    configured: Duration,
    max_stale_seconds: u64,
) {
    let next = effective_poll_interval(configured, max_stale_seconds);
    if next == *current {
        return;
    }
    if next < configured {
        eprintln!(
            "Configuration poll interval shortened to {} ms to fit max_stale_seconds={max_stale_seconds}",
            next.as_millis()
        );
    }
    *poll = tokio::time::interval(next);
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    poll.tick().await;
    *current = next;
}

fn verify_activation_source(
    output: &Path,
    supervised_configuration: bool,
    source: Option<(&Path, &AcceptedSourceProof)>,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        supervised_configuration == source.is_some(),
        "supervised candidate needs its exact accepted source proof"
    );
    if let Some((policy, proof)) = source {
        anyhow::ensure!(
            super::configuration_source::proof_matches_current(output, policy, proof)?,
            "configuration candidate no longer matches its accepted source proof"
        );
    }
    Ok(())
}

struct FrontendPreview<'a> {
    root: &'a Path,
    host: &'a mut Option<Host>,
    frontend_process: &'a mut Option<frontend::FrontendProcess>,
    active_backend_url: &'a mut Option<String>,
    config: &'a frontend::FrontendConfig,
}

async fn activate_candidate_with_frontend(
    output: &Path,
    args: &[String],
    preview: FrontendPreview<'_>,
    supervised_configuration: bool,
) -> anyhow::Result<Option<bool>> {
    activate_candidate_with_frontend_until(
        output,
        args,
        preview,
        supervised_configuration,
        None,
        None,
    )
    .await
}

async fn activate_candidate_with_frontend_until(
    output: &Path,
    args: &[String],
    preview: FrontendPreview<'_>,
    supervised_configuration: bool,
    source_deadline: Option<Instant>,
    source: Option<(&Path, &AcceptedSourceProof)>,
) -> anyhow::Result<Option<bool>> {
    let FrontendPreview {
        root,
        host,
        frontend_process,
        active_backend_url,
        config,
    } = preview;
    if let Err(error) = verify_activation_source(output, supervised_configuration, source) {
        eprintln!(
            "Configuration candidate changed before startup; candidate not activated: {error:#}"
        );
        return Ok(Some(false));
    }
    let mut candidate = match launch_ready_until(
        output,
        args,
        supervised_configuration,
        true,
        source_deadline,
    )
    .await
    {
        Ok(Some(candidate)) => candidate,
        Ok(None) => return Ok(None),
        Err(error) => {
            eprintln!("App candidate failed actual startup; candidate not activated: {error:#}");
            return Ok(Some(false));
        }
    };
    let next_url = match backend_url(output) {
        Ok(url) => url,
        Err(error) => {
            stop(&mut candidate, true).await?;
            eprintln!(
                "App candidate did not report a usable Web Ingress; candidate not activated: {error:#}"
            );
            return Ok(Some(false));
        }
    };
    let previous_url = active_backend_url.clone();
    let address_file = root.join(".lenso/dev-backend-url");
    let mut staged_frontend = None;
    if source_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        expire_frontend_candidate_now(
            root,
            &mut candidate,
            &mut staged_frontend,
            host,
            frontend_process,
            active_backend_url,
        )
        .await?;
        eprintln!("Configuration source proof expired during frontend startup");
        return Ok(Some(false));
    }
    if frontend_process.is_none() {
        let initial_url = previous_url.as_deref().unwrap_or(&next_url);
        if let Err(error) = write_backend_url(root, initial_url) {
            rollback_frontend_candidate(
                root,
                previous_url.as_deref(),
                &mut candidate,
                &mut staged_frontend,
                host,
                frontend_process,
            )
            .await?;
            eprintln!(
                "Frontend backend URL could not be published; candidate not activated: {error:#}"
            );
            return Ok(Some(false));
        }
        match run_until(
            source_deadline,
            config.launch(root, initial_url, &address_file),
        )
        .await
        {
            Ok(Ok(Some(process))) => staged_frontend = Some(process),
            Ok(Ok(None)) => {
                rollback_frontend_candidate(
                    root,
                    previous_url.as_deref(),
                    &mut candidate,
                    &mut staged_frontend,
                    host,
                    frontend_process,
                )
                .await?;
                return Ok(None);
            }
            Ok(Err(error)) => {
                rollback_frontend_candidate(
                    root,
                    previous_url.as_deref(),
                    &mut candidate,
                    &mut staged_frontend,
                    host,
                    frontend_process,
                )
                .await?;
                eprintln!(
                    "Frontend candidate failed readiness; candidate not activated: {error:#}"
                );
                return Ok(Some(false));
            }
            Err(_) => {
                expire_frontend_candidate_now(
                    root,
                    &mut candidate,
                    &mut staged_frontend,
                    host,
                    frontend_process,
                    active_backend_url,
                )
                .await?;
                eprintln!("Configuration source proof expired during frontend readiness");
                return Ok(Some(false));
            }
        }
    }
    if let Err(error) = write_backend_url(root, &next_url) {
        rollback_frontend_candidate(
            root,
            previous_url.as_deref(),
            &mut candidate,
            &mut staged_frontend,
            host,
            frontend_process,
        )
        .await?;
        eprintln!(
            "Frontend backend URL could not be published; candidate not activated: {error:#}"
        );
        return Ok(Some(false));
    }
    let Some(selected_frontend) = staged_frontend.as_mut().or(frontend_process.as_mut()) else {
        rollback_frontend_candidate(
            root,
            previous_url.as_deref(),
            &mut candidate,
            &mut staged_frontend,
            host,
            frontend_process,
        )
        .await?;
        bail!("frontend candidate is missing after launch");
    };
    match run_until(
        source_deadline,
        config.verify_backend(selected_frontend, &next_url),
    )
    .await
    {
        Ok(Ok(true)) => {}
        Err(_) => {
            expire_frontend_candidate_now(
                root,
                &mut candidate,
                &mut staged_frontend,
                host,
                frontend_process,
                active_backend_url,
            )
            .await?;
            eprintln!("Configuration source proof expired during frontend backend verification");
            return Ok(Some(false));
        }
        Ok(result) => {
            rollback_frontend_candidate(
                root,
                previous_url.as_deref(),
                &mut candidate,
                &mut staged_frontend,
                host,
                frontend_process,
            )
            .await?;
            match result {
                Ok(false) => return Ok(None),
                Err(error) => {
                    eprintln!(
                        "Frontend did not accept candidate backend; candidate not activated: {error:#}"
                    );
                    return Ok(Some(false));
                }
                Ok(true) => unreachable!(),
            }
        }
    }
    match candidate_still_running(&mut candidate) {
        Ok(true) => {}
        result => {
            rollback_frontend_candidate(
                root,
                previous_url.as_deref(),
                &mut candidate,
                &mut staged_frontend,
                host,
                frontend_process,
            )
            .await?;
            match result {
                Ok(false) => eprintln!(
                    "App candidate exited during frontend readiness; candidate not activated"
                ),
                Err(error) => eprintln!(
                    "App candidate state could not be checked before switch; candidate not activated: {error:#}"
                ),
                Ok(true) => unreachable!(),
            }
            return Ok(Some(false));
        }
    }
    let old_stopped = if let Some(previous_host) = host {
        match stop(previous_host, false).await {
            Ok(()) => true,
            Err(error) => {
                eprintln!("Previous Host shutdown failed: {error:#}");
                match previous_host.try_wait() {
                    Ok(status) => status.is_some(),
                    Err(inspect_error) => {
                        eprintln!("Previous Host state could not be inspected: {inspect_error:#}");
                        false
                    }
                }
            }
        }
    } else {
        true
    };
    if !old_stopped {
        rollback_frontend_candidate(
            root,
            previous_url.as_deref(),
            &mut candidate,
            &mut staged_frontend,
            host,
            frontend_process,
        )
        .await?;
        eprintln!("Retaining the previous Host because it did not stop cleanly");
        return Ok(Some(false));
    }
    if source_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        expire_frontend_candidate_now(
            root,
            &mut candidate,
            &mut staged_frontend,
            host,
            frontend_process,
            active_backend_url,
        )
        .await?;
        eprintln!("Configuration source proof expired before frontend activation");
        return Ok(Some(false));
    }
    if let Some((policy, proof)) = source
        && let Err(error) =
            super::configuration_source::record_distribution_activation(output, policy, proof)
    {
        eprintln!("Configuration activation receipt failed; stopping the candidate: {error:#}");
        expire_frontend_candidate_now(
            root,
            &mut candidate,
            &mut staged_frontend,
            host,
            frontend_process,
            active_backend_url,
        )
        .await?;
        return Ok(Some(false));
    }
    if source_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        expire_frontend_candidate_now(
            root,
            &mut candidate,
            &mut staged_frontend,
            host,
            frontend_process,
            active_backend_url,
        )
        .await?;
        eprintln!("Configuration source proof expired before frontend activation");
        return Ok(Some(false));
    }
    *host = Some(candidate);
    if let Some(process) = staged_frontend {
        *frontend_process = Some(process);
    }
    *active_backend_url = Some(next_url.clone());
    eprintln!(
        "App preview ready at {} (API: {next_url})",
        config.preview_url()
    );
    eprintln!(
        "Frontend development does not update checked-in Host static assets; run an explicit frontend build to update them."
    );
    Ok(Some(true))
}

fn candidate_still_running(candidate: &mut Child) -> anyhow::Result<bool> {
    #[cfg(unix)]
    {
        let Some(group_id) = candidate.id() else {
            return Ok(false);
        };
        if !exited_unreaped(group_id)? {
            return Ok(true);
        }
        signal_process_group_now(candidate)?;
        let _ = candidate.try_wait()?;
        Ok(false)
    }
    #[cfg(not(unix))]
    {
        Ok(candidate.try_wait()?.is_none())
    }
}

async fn rollback_frontend_candidate(
    root: &Path,
    previous_url: Option<&str>,
    candidate: &mut Child,
    staged_frontend: &mut Option<frontend::FrontendProcess>,
    host: &mut Option<Host>,
    frontend_process: &mut Option<frontend::FrontendProcess>,
) -> anyhow::Result<()> {
    // A failed directory sync may still have published the new URL. Always
    // restore before retiring the candidate, but never let a restore error
    // skip cleanup of the candidate's owned processes.
    let restored = restore_backend_url(root, previous_url);
    let candidate_stopped = stop(candidate, true).await;
    let staged_stopped = if let Some(process) = staged_frontend {
        frontend::stop(process).await
    } else {
        Ok(())
    };
    if let Err(error) = restored {
        // The old frontend cannot safely remain live when its proxy target is
        // unknown. Stop the old pair instead of reporting a healthy preview.
        if let Err(shutdown_error) = stop_active(host, frontend_process).await {
            eprintln!(
                "Previous preview shutdown after failed backend URL restoration: {shutdown_error:#}"
            );
        }
        if let Err(shutdown_error) = candidate_stopped {
            eprintln!("Rejected Host shutdown failed: {shutdown_error:#}");
        }
        if let Err(shutdown_error) = staged_stopped {
            eprintln!("Rejected frontend shutdown failed: {shutdown_error:#}");
        }
        return Err(error).context("restore previous frontend backend URL");
    }
    candidate_stopped.context("stop rejected Host")?;
    staged_stopped.context("stop rejected frontend")?;
    Ok(())
}

async fn expire_frontend_candidate_now(
    root: &Path,
    candidate: &mut Child,
    staged_frontend: &mut Option<frontend::FrontendProcess>,
    host: &mut Option<Host>,
    frontend_process: &mut Option<frontend::FrontendProcess>,
    active_backend_url: &mut Option<String>,
) -> anyhow::Result<()> {
    let signal = signal_process_group_now(candidate);
    let previous = stop_active_now(host, frontend_process).await;
    let staged = if let Some(process) = staged_frontend.as_mut() {
        frontend::stop_now(process).await
    } else {
        Ok(())
    };
    let reaped = reap_killed(candidate).await;
    *active_backend_url = None;
    let address = restore_backend_url(root, None);
    signal?;
    previous?;
    staged?;
    reaped?;
    address
}

fn restore_backend_url(root: &Path, previous_url: Option<&str>) -> anyhow::Result<()> {
    if let Some(previous_url) = previous_url {
        return write_backend_url(root, previous_url);
    }
    let directory = root.join(".lenso");
    match fs::remove_file(directory.join("dev-backend-url")) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    fs::File::open(directory)?.sync_all()?;
    Ok(())
}

fn write_backend_url(root: &Path, address: &str) -> anyhow::Result<()> {
    use std::io::Write;
    let directory = root.join(".lenso");
    let mut staged = tempfile::NamedTempFile::new_in(&directory)?;
    writeln!(staged, "{address}")?;
    staged.as_file().sync_all()?;
    staged.persist(directory.join("dev-backend-url"))?;
    fs::File::open(directory)?.sync_all()?;
    Ok(())
}

fn lock_dev(root: &Path) -> anyhow::Result<super::local_lock::LocalLock> {
    super::local_lock::acquire(
        root,
        "dev.lock",
        "another lenso app dev session may already own this App development lock",
    )
}

fn backend_url(output: &Path) -> anyhow::Result<String> {
    let path = output.join(".lenso/dev-web-address");
    let metadata = fs::symlink_metadata(&path).context("read App Web Ingress address")?;
    anyhow::ensure!(
        metadata.file_type().is_file() && metadata.len() <= 512,
        "App Web Ingress address receipt is not a bounded regular file"
    );
    let address = fs::read_to_string(path)?;
    let address = address.trim_end_matches('\n');
    let url = url::Url::parse(address).context("parse App Web Ingress address")?;
    let loopback = match url.host() {
        Some(url::Host::Ipv4(address)) => address == std::net::Ipv4Addr::LOCALHOST,
        Some(url::Host::Ipv6(address)) => address == std::net::Ipv6Addr::LOCALHOST,
        _ => false,
    };
    anyhow::ensure!(
        url.scheme() == "http"
            && loopback
            && url.port().is_some()
            && url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none(),
        "frontend development requires a loopback Web Ingress with an exact port"
    );
    Ok(address.to_owned())
}

async fn stop_active(
    host: &mut Option<Host>,
    frontend: &mut Option<frontend::FrontendProcess>,
) -> anyhow::Result<()> {
    let frontend_stopped = if let Some(process) = frontend {
        let result = frontend::stop(process).await;
        *frontend = None;
        result
    } else {
        Ok(())
    };
    let host_stopped = if let Some(process) = host {
        let result = process.retire().await;
        *host = None;
        result
    } else {
        Ok(())
    };
    frontend_stopped?;
    host_stopped?;
    Ok(())
}

/// Revoke a running preview without its normal graceful-shutdown allowance.
async fn stop_active_now(
    host: &mut Option<Host>,
    frontend: &mut Option<frontend::FrontendProcess>,
) -> anyhow::Result<()> {
    let host_signal = if let Some(process) = host.as_mut() {
        signal_process_group_now(process)
    } else {
        Ok(())
    };
    let frontend_result = if let Some(process) = frontend.as_mut() {
        frontend::stop_now(process).await
    } else {
        Ok(())
    };
    let host_result = if let Some(process) = host.as_mut() {
        reap_killed(process).await
    } else {
        Ok(())
    };
    *host = None;
    *frontend = None;
    host_signal?;
    frontend_result?;
    host_result
}

fn signal_process_group_now(process: &mut Child) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        let result = match process.id() {
            Some(id) => signal_process_group_id_now(id),
            None => Ok(()),
        };
        if result.is_err() {
            let _ = process.start_kill();
        }
        result
    }
    #[cfg(not(unix))]
    {
        process.start_kill().map_err(anyhow::Error::from)
    }
}

#[cfg(unix)]
fn signal_process_group_id_now(id: u32) -> anyhow::Result<()> {
    signal_process_group_id(id, nix::sys::signal::Signal::SIGKILL)
}

#[cfg(unix)]
fn signal_process_group_id(id: u32, signal: nix::sys::signal::Signal) -> anyhow::Result<()> {
    use nix::{sys::signal::killpg, unistd::Pid};
    let group = Pid::from_raw(i32::try_from(id)?);
    match killpg(group, signal) {
        Ok(()) | Err(nix::errno::Errno::ESRCH) => Ok(()),
        // Darwin reports EPERM for a zombie-only group. Do not treat a
        // permission failure as revocation while any member is live.
        #[cfg(target_os = "macos")]
        Err(nix::errno::Errno::EPERM) if exited_unreaped(id)? && darwin_group_only_zombies(id)? => {
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(target_os = "macos")]
pub(super) fn darwin_group_only_zombies(group_id: u32) -> anyhow::Result<bool> {
    use nix::libc;
    let mut pids = vec![0_i32; 64];
    let inspection_deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
    'snapshot: loop {
        let capacity = i32::try_from(pids.len() * std::mem::size_of::<i32>())?;
        // PROC_PGRP_ONLY is 2 in the macOS SDK's sys/proc_info.h.
        let bytes = unsafe { libc::proc_listpids(2, group_id, pids.as_mut_ptr().cast(), capacity) };
        anyhow::ensure!(bytes >= 0, "cannot enumerate supervised process group");
        if bytes >= capacity {
            anyhow::ensure!(
                pids.len() < 16384,
                "supervised process group is too large to inspect"
            );
            pids.resize(pids.len() * 2, 0);
            continue;
        }
        let mut saw_leader = false;
        for &pid in &pids[..usize::try_from(bytes)? / std::mem::size_of::<i32>()] {
            if pid <= 0 {
                continue;
            }
            if u32::try_from(pid)? == group_id {
                // WNOWAIT above has already established this exact owned
                // leader is a zombie; libproc omits its BSD info on Darwin.
                saw_leader = true;
                continue;
            }
            let mut info = std::mem::MaybeUninit::<libc::proc_bsdshortinfo>::zeroed();
            let info_size = i32::try_from(std::mem::size_of::<libc::proc_bsdshortinfo>())?;
            let returned = unsafe {
                libc::proc_pidinfo(
                    pid,
                    libc::PROC_PIDT_SHORTBSDINFO,
                    0,
                    info.as_mut_ptr().cast(),
                    info_size,
                )
            };
            // Enumeration can race with a member's exit. An ambiguous member
            // is not proof of a stopped group. Re-enumerate briefly so a
            // vanished member can disappear from the snapshot; persistent
            // ambiguity still fails closed.
            if returned == 0 {
                if std::time::Instant::now() >= inspection_deadline {
                    return Ok(false);
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
                continue 'snapshot;
            }
            anyhow::ensure!(
                returned == info_size,
                "cannot inspect supervised process-group member {pid}"
            );
            let info = unsafe { info.assume_init() };
            if info.pbsi_pgid != group_id {
                continue;
            }
            if info.pbsi_status != libc::SZOMB {
                return Ok(false);
            }
        }
        anyhow::ensure!(
            saw_leader,
            "supervised process-group leader disappeared during inspection"
        );
        return Ok(true);
    }
}

#[cfg(unix)]
fn exited_unreaped(group_id: u32) -> anyhow::Result<bool> {
    use nix::libc;
    let id = i32::try_from(group_id)?;
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
        return Err(error).context("observe local process exit without reaping");
    }
    Ok(unsafe { info.assume_init().si_pid() } == id)
}

#[cfg(unix)]
async fn wait_for_exit_unreaped(group_id: u32) -> anyhow::Result<()> {
    loop {
        if exited_unreaped(group_id)? {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn reap_killed(process: &mut Child) -> anyhow::Result<()> {
    match tokio::time::timeout(Duration::from_secs(2), process.wait()).await {
        Ok(status) => {
            status?;
            Ok(())
        }
        Err(_) => {
            process.start_kill()?;
            tokio::time::timeout(Duration::from_secs(2), process.wait())
                .await
                .context("local process did not exit after immediate stop")??;
            Ok(())
        }
    }
}

async fn kill_process_group_now(process: &mut Child) -> anyhow::Result<()> {
    let signal = signal_process_group_now(process);
    let reap = reap_killed(process).await;
    signal?;
    reap
}

async fn retire_active_on_policy_change(
    root: &Path,
    active_output: Option<&Path>,
    policy: &Path,
    host: &mut Option<Host>,
    frontend: &mut Option<frontend::FrontendProcess>,
    active_backend_url: &mut Option<String>,
) -> anyhow::Result<bool> {
    if host.is_none() && frontend.is_none() {
        return Ok(false);
    }
    let changed = match active_output {
        Some(output) => {
            match super::configuration_source::policy_changed_since_active(output, policy) {
                Ok(changed) => changed,
                Err(error) => {
                    eprintln!(
                        "Cannot verify active configuration policy; stopping the running generation: {error:#}"
                    );
                    true
                }
            }
        }
        None => true,
    };
    if changed {
        eprintln!(
            "Host configuration policy changed; stopping the running generation until an operator verifies retirement and restarts"
        );
        stop_active_now(host, frontend).await?;
        *active_backend_url = None;
        restore_backend_url(root, None)?;
    }
    Ok(changed)
}

fn select_output(current_output: &mut Option<PathBuf>, output: &Path) {
    if current_output.as_deref() == Some(output) {
        return;
    }
    if let Some(previous) = current_output.replace(output.to_path_buf())
        && let Err(error) = fs::remove_dir_all(&previous)
    {
        eprintln!(
            "Previous inactive generation could not be removed at {}: {error}",
            previous.display()
        );
    }
}

async fn activate_candidate(
    output: &Path,
    args: &[String],
    host: &mut Option<Host>,
    supervised_configuration: bool,
    frontend_enabled: bool,
) -> anyhow::Result<Option<bool>> {
    activate_candidate_until(
        output,
        args,
        host,
        supervised_configuration,
        frontend_enabled,
        None,
        None,
    )
    .await
}

async fn activate_candidate_until(
    output: &Path,
    args: &[String],
    host: &mut Option<Host>,
    supervised_configuration: bool,
    frontend_enabled: bool,
    source_deadline: Option<Instant>,
    source: Option<(&Path, &AcceptedSourceProof)>,
) -> anyhow::Result<Option<bool>> {
    if let Err(error) = verify_activation_source(output, supervised_configuration, source) {
        eprintln!(
            "Configuration candidate changed before startup; candidate not activated: {error:#}"
        );
        return Ok(Some(false));
    }
    let mut candidate = match launch_ready_until(
        output,
        args,
        supervised_configuration,
        frontend_enabled,
        source_deadline,
    )
    .await
    {
        Ok(Some(candidate)) => candidate,
        Ok(None) => return Ok(None),
        Err(error) => {
            eprintln!("App candidate failed actual startup; candidate not activated: {error:#}");
            return Ok(Some(false));
        }
    };
    if source_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        kill_process_group_now(&mut candidate).await?;
        eprintln!("Configuration source proof expired before Host activation");
        return Ok(Some(false));
    }
    let old_stopped = if let Some(previous_host) = host {
        match stop(previous_host, false).await {
            Ok(()) => true,
            Err(error) => {
                eprintln!("Previous Host shutdown failed: {error:#}");
                previous_host.try_wait()?.is_some()
            }
        }
    } else {
        true
    };
    if !old_stopped {
        stop(&mut candidate, true).await?;
        eprintln!("Retaining the previous Host because it did not stop cleanly");
        return Ok(Some(false));
    }
    if !candidate_still_running(&mut candidate)? {
        eprintln!("App candidate exited before activation; candidate not activated");
        return Ok(Some(false));
    }
    if let Some((policy, proof)) = source
        && let Err(error) =
            super::configuration_source::record_distribution_activation(output, policy, proof)
    {
        eprintln!("Configuration activation receipt failed; stopping the candidate: {error:#}");
        kill_process_group_now(&mut candidate).await?;
        return Ok(Some(false));
    }
    if source_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        kill_process_group_now(&mut candidate).await?;
        eprintln!("Configuration source proof expired before Host activation");
        return Ok(Some(false));
    }
    *host = Some(candidate);
    Ok(Some(true))
}

#[cfg(test)]
async fn launch_ready(
    output: &Path,
    args: &[String],
    defer_activation: bool,
    frontend_enabled: bool,
) -> anyhow::Result<Option<Host>> {
    launch_ready_until(output, args, defer_activation, frontend_enabled, None).await
}

async fn launch_ready_until(
    output: &Path,
    args: &[String],
    defer_activation: bool,
    frontend_enabled: bool,
    source_deadline: Option<Instant>,
) -> anyhow::Result<Option<Host>> {
    launch_configured_until(
        output,
        args,
        defer_activation,
        frontend_enabled,
        source_deadline,
        None,
    )
    .await
}

async fn launch_configured_until(
    output: &Path,
    args: &[String],
    defer_activation: bool,
    frontend_enabled: bool,
    source_deadline: Option<Instant>,
    mut retirement: Option<Retirement>,
) -> anyhow::Result<Option<Host>> {
    // A fresh path is essential when restarting a Host from one distribution:
    // an old readiness file must never certify a new process.
    let marker = tempfile::NamedTempFile::new_in(output.join(".lenso"))?.into_temp_path();
    fs::remove_file(&marker)?;
    let mut candidate = command(output.join(".lenso/host"));
    candidate
        .args(super::local_host::host_arguments(output)?)
        .arg("--ready-file")
        .arg(&marker);
    if frontend_enabled {
        let address = output.join(".lenso/dev-web-address");
        match fs::remove_file(&address) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("clear previous App Web Ingress address"),
        }
        candidate.arg("--web-address-file").arg(address);
    }
    if defer_activation {
        candidate.arg("--defer-activation");
    }
    if !args.is_empty() {
        candidate.arg("--").args(args);
    }
    if let Some(retirement) = &mut retirement {
        retirement.configure(&mut candidate)?;
    }
    let child = match candidate.spawn() {
        Ok(child) => child,
        Err(error) => {
            if let Some(retirement) = &mut retirement {
                retirement.clear_unstarted()?;
            }
            return Err(error).context("start generated local Host");
        }
    };
    let mut candidate = Host::new(child, retirement);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        if source_deadline.is_some_and(|source_deadline| Instant::now() >= source_deadline) {
            kill_process_group_now(&mut candidate).await?;
            bail!("configuration source proof expired during Host startup");
        }
        #[cfg(unix)]
        if let Some(group_id) = candidate.id()
            && exited_unreaped(group_id)?
        {
            signal_process_group_now(&mut candidate)?;
            let status = candidate.wait().await?;
            bail!("generated local Host exited before readiness: {status}");
        }
        #[cfg(not(unix))]
        if let Some(status) = candidate.try_wait()? {
            bail!("generated local Host exited before readiness: {status}");
        }
        match fs::symlink_metadata(&marker) {
            Ok(metadata) => {
                if !metadata.file_type().is_file()
                    || fs::read(&marker)? != b"lenso.local-host-ready.v1\n"
                {
                    kill_process_group_now(&mut candidate).await?;
                    bail!("generated local Host returned an invalid readiness receipt");
                }
                if frontend_enabled && let Err(error) = backend_url(output) {
                    kill_process_group_now(&mut candidate).await?;
                    return Err(error)
                        .context("generated local Host did not report a usable Web Ingress");
                }
                return Ok(Some(candidate));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                stop(&mut candidate, true).await?;
                return Err(error).context("inspect generated local Host readiness");
            }
        }
        if tokio::time::Instant::now() >= deadline {
            kill_process_group_now(&mut candidate).await?;
            bail!("generated local Host did not become ready within 60 seconds");
        }
        tokio::select! {
            biased;
            () = sleep_until_optional(source_deadline) => {
                kill_process_group_now(&mut candidate).await?;
                bail!("configuration source proof expired during Host startup");
            }
            signal = tokio::signal::ctrl_c() => {
                signal?;
                stop(&mut candidate, true).await?;
                return Ok(None);
            }
            () = tokio::time::sleep(Duration::from_millis(25)) => {}
        }
    }
}

async fn preflight(output: &Path) -> anyhow::Result<()> {
    preflight_until(output, None).await
}

async fn preflight_until(output: &Path, source_deadline: Option<Instant>) -> anyhow::Result<()> {
    host_check_until(output, source_deadline, "--check", "readiness", None).await
}

struct LivePreviewGuard<'a> {
    root: &'a Path,
    active_output: Option<&'a Path>,
    policy: &'a Path,
    host: &'a mut Option<Host>,
    frontend: &'a mut Option<frontend::FrontendProcess>,
    active_backend_url: &'a mut Option<String>,
    active: &'a mut Option<TimedProof>,
}

impl LivePreviewGuard<'_> {
    async fn still_valid(&mut self) -> anyhow::Result<bool> {
        if retire_active_on_policy_change(
            self.root,
            self.active_output,
            self.policy,
            self.host,
            self.frontend,
            self.active_backend_url,
        )
        .await?
        {
            *self.active = None;
            return Ok(false);
        }
        if expire_active_if_needed(
            self.root,
            self.active,
            self.host,
            self.frontend,
            self.active_backend_url,
        )
        .await?
        {
            return Ok(false);
        }
        Ok(true)
    }
}

async fn static_prepare_until(
    output: &Path,
    source_deadline: Option<Instant>,
    guard: LivePreviewGuard<'_>,
) -> anyhow::Result<()> {
    host_check_until(
        output,
        source_deadline,
        "--prepare",
        "static preparation",
        Some(guard),
    )
    .await
}

async fn host_check_until(
    output: &Path,
    source_deadline: Option<Instant>,
    mode: &str,
    phase: &str,
    mut guard: Option<LivePreviewGuard<'_>>,
) -> anyhow::Result<()> {
    let mut candidate = command(output.join(".lenso/host"));
    candidate
        .args(super::local_host::host_arguments(output)?)
        .arg(mode);
    let mut candidate = candidate
        .spawn()
        .with_context(|| format!("start App candidate {phase} check"))?;
    let readiness_deadline = Instant::now() + Duration::from_secs(60);
    let mut next_policy_check = Instant::now();
    #[cfg(unix)]
    let group_id = candidate.id().context("candidate process ID")?;
    loop {
        let now = Instant::now();
        if source_deadline.is_some_and(|deadline| now >= deadline) {
            kill_process_group_now(&mut candidate).await?;
            bail!("configuration source proof expired during candidate {phase}")
        }
        if now >= readiness_deadline {
            kill_process_group_now(&mut candidate).await?;
            bail!("App candidate {phase} did not complete within 60 seconds")
        }
        if now >= next_policy_check
            && let Some(guard) = &mut guard
        {
            match guard.still_valid().await {
                Ok(true) => {}
                Ok(false) => {
                    kill_process_group_now(&mut candidate).await?;
                    bail!("active preview invalidated during static preparation")
                }
                Err(error) => {
                    kill_process_group_now(&mut candidate).await?;
                    return Err(error).context("check active preview during static preparation");
                }
            }
            next_policy_check = Instant::now() + Duration::from_millis(250);
        }
        #[cfg(unix)]
        if exited_unreaped(group_id)? {
            signal_process_group_now(&mut candidate)?;
            let status = candidate.wait().await?;
            if status.success() {
                return Ok(());
            }
            bail!("App candidate {phase} check failed: {status}")
        }
        #[cfg(not(unix))]
        if let Some(status) = candidate.try_wait()? {
            if status.success() {
                return Ok(());
            }
            bail!("App candidate {phase} check failed: {status}")
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn sleep_until_optional(deadline: Option<Instant>) {
    if let Some(deadline) = deadline {
        tokio::time::sleep_until(deadline).await;
    } else {
        std::future::pending::<()>().await;
    }
}

fn command(executable: PathBuf) -> Command {
    let mut command = Command::new(executable);
    command.kill_on_drop(true);
    #[cfg(unix)]
    {
        command.process_group(0);
    }
    command
}

async fn stop(child: &mut Child, whole_group: bool) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use nix::{
            sys::signal::{Signal, kill, killpg},
            unistd::Pid,
        };
        let Some(child_id) = child.id() else {
            return Ok(());
        };
        let id = i32::try_from(child_id)?;
        let pid = Pid::from_raw(id);
        if !exited_unreaped(child_id)? {
            let result = if whole_group {
                killpg(pid, Signal::SIGTERM)
            } else {
                kill(pid, Signal::SIGTERM)
            };
            if let Err(error) = result
                && error != nix::errno::Errno::ESRCH
            {
                return Err(error.into());
            }
        }
        let within_budget =
            tokio::time::timeout(Duration::from_secs(12), wait_for_exit_unreaped(child_id)).await;
        let timed_out = match within_budget {
            Ok(Ok(())) => false,
            Ok(Err(error)) => return Err(error),
            Err(_) => true,
        };
        // The child remains unreaped until all group descendants are stopped.
        signal_process_group_id(child_id, Signal::SIGKILL)?;
        let status = tokio::time::timeout(Duration::from_secs(2), child.wait())
            .await
            .context("local process did not exit after group stop")??;
        if timed_out {
            bail!("local process did not stop within its shutdown budget");
        }
        if !whole_group && !status.success() {
            bail!("local Host shutdown failed: {status}");
        }
    }
    #[cfg(not(unix))]
    {
        let _ = whole_group;
        child.kill().await?;
        child.wait().await?;
    }
    Ok(())
}

fn watch(
    root: &Path,
) -> anyhow::Result<(
    notify::RecommendedWatcher,
    mpsc::Receiver<notify::Result<notify::Event>>,
    std::sync::Arc<std::sync::atomic::AtomicBool>,
)> {
    let (sender, receiver) = mpsc::channel(128);
    let overflow = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let queue_overflow = overflow.clone();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event.as_ref().is_ok_and(|event| !rebuild_event(event)) {
            return;
        }
        if matches!(
            sender.try_send(event),
            Err(mpsc::error::TrySendError::Full(_))
        ) {
            queue_overflow.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    })?;
    watcher.watch(root, RecursiveMode::Recursive)?;
    let config = root.join("lenso.toml");
    if config.is_file() {
        let document: toml::Value = toml::from_str(&fs::read_to_string(config)?)?;
        if let Some(sources) = document
            .get("plugin_sources")
            .and_then(toml::Value::as_array)
        {
            for source in sources {
                let source = source
                    .as_str()
                    .context("plugin_sources entries must be paths")?;
                let mut path = root.to_path_buf();
                for part in Path::new(source).components() {
                    if part.as_os_str().to_string_lossy().contains(['*', '?', '[']) {
                        break;
                    }
                    path.push(part);
                }
                while !path.is_dir() {
                    if !path.pop() {
                        bail!("cannot watch local source {source}");
                    }
                }
                let path = fs::canonicalize(path)?;
                if !path.starts_with(root) {
                    watcher.watch(&path, RecursiveMode::Recursive)?;
                }
            }
        }
    }
    watch_dependencies(root, &mut watcher)?;
    Ok((watcher, receiver, overflow))
}
fn watch_dependencies(root: &Path, watcher: &mut notify::RecommendedWatcher) -> anyhow::Result<()> {
    let path = root.join(".lenso/host-cache/watch-roots.json");
    if path.is_file() {
        let paths: Vec<PathBuf> = serde_json::from_slice(&fs::read(path)?)?;
        for path in paths {
            if !path.starts_with(root) && path.is_dir() {
                watcher.watch(&path, RecursiveMode::Recursive)?;
            }
        }
    }
    Ok(())
}

fn relevant(path: &Path) -> bool {
    !path.components().any(|part| {
        part.as_os_str().to_str().is_some_and(|name| {
            name.starts_with(".lenso-")
                || [
                    ".git",
                    ".lenso",
                    "target",
                    "node_modules",
                    "dist",
                    "build",
                    ".next",
                    ".venv",
                    "__pycache__",
                ]
                .contains(&name)
        })
    })
}

fn rebuild_event(event: &notify::Event) -> bool {
    let write_closed = matches!(
        event.kind,
        notify::EventKind::Access(notify::event::AccessKind::Close(
            notify::event::AccessMode::Write
        ))
    );
    (write_closed || !event.kind.is_access()) && event.paths.iter().any(|path| relevant(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accepted_proof(
        max_stale_seconds: u64,
    ) -> super::super::configuration_source::AcceptedSourceProof {
        super::super::configuration_source::AcceptedSourceProof {
            source: lenso_app_authoring::PluginConfigurationAuthoritySource::new(
                "file_snapshot",
                "test",
            )
            .unwrap(),
            policy_digest: "sha256:test".into(),
            revision: 1,
            snapshot_digest: "sha256:snapshot".into(),
            plugin_root_revision: "root-revision".into(),
            max_stale_seconds,
        }
    }

    #[test]
    fn poll_interval_is_shorter_than_the_stale_limit() {
        assert_eq!(
            effective_poll_interval(Duration::from_secs(3600), 300),
            Duration::from_secs(150)
        );
        assert_eq!(
            effective_poll_interval(Duration::from_secs(3600), 1),
            Duration::from_millis(500)
        );
    }

    #[tokio::test]
    async fn slow_source_read_cannot_start_a_new_stale_window() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".lenso")).unwrap();
        let mut active = None;
        let mut host = None;
        let mut frontend = None;
        let mut active_backend_url = None;
        let proof = run_source_sync_with_deadline(
            root.path(),
            None,
            || {
                std::thread::sleep(Duration::from_millis(1100));
                Ok(accepted_proof(1))
            },
            &mut active,
            &mut host,
            &mut frontend,
            &mut active_backend_url,
        )
        .await
        .unwrap();
        assert!(!proof.is_fresh());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn blocked_source_read_does_not_delay_policy_retirement() {
        use nix::{errno::Errno, sys::signal::kill, unistd::Pid};
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".lenso")).unwrap();
        fs::write(
            root.path().join(".lenso/dev-backend-url"),
            "http://127.0.0.1:3001/\n",
        )
        .unwrap();
        let mut process = command(PathBuf::from("/bin/sleep"));
        process.arg("30");
        let process = process.spawn().unwrap();
        let pid = process.id().unwrap();
        let observed = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(600)).await;
            kill(Pid::from_raw(i32::try_from(pid).unwrap()), None) == Err(Errno::ESRCH)
        });
        let mut active = Some(TimedProof {
            accepted: accepted_proof(10),
            received_at: Instant::now(),
        });
        let mut host = Some(process.into());
        let mut frontend = None;
        let mut active_backend_url = Some("http://127.0.0.1:3001/".into());
        let policy = root.path().join("policy.json");
        let _ = run_source_sync_with_deadline(
            root.path(),
            Some((None, &policy)),
            || {
                std::thread::sleep(Duration::from_millis(1100));
                Ok(accepted_proof(10))
            },
            &mut active,
            &mut host,
            &mut frontend,
            &mut active_backend_url,
        )
        .await
        .unwrap();
        assert!(observed.await.unwrap());
        assert!(host.is_none());
        assert!(active.is_none());
        assert!(!root.path().join(".lenso/dev-backend-url").exists());
    }

    #[cfg(unix)]
    #[test]
    fn dev_policy_path_keeps_symlink_visible_to_host_validation() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let policy = root.path().join("policy.json");
        let link = root.path().join("policy-link.json");
        fs::write(&policy, b"{}").unwrap();
        symlink(&policy, &link).unwrap();
        let selected = absolute_configuration_policy(link.clone()).unwrap();
        assert_eq!(selected, link);
        assert!(
            fs::symlink_metadata(selected)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[cfg(unix)]
    async fn assert_pid_stopped(pid: i32) {
        use nix::{
            errno::Errno,
            sys::signal::{Signal, kill},
            unistd::Pid,
        };
        let pid = Pid::from_raw(pid);
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if kill(pid, None) == Err(Errno::ESRCH) {
                return;
            }
            if Instant::now() >= deadline {
                let _ = kill(pid, Some(Signal::SIGKILL));
                panic!("Host process-group child remained live");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn expired_active_proof_kills_host_process_group() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".lenso")).unwrap();
        fs::write(
            root.path().join(".lenso/dev-backend-url"),
            "http://127.0.0.1:3001/\n",
        )
        .unwrap();
        let script = root.path().join("group.sh");
        let child_pid = root.path().join("child.pid");
        fs::write(
            &script,
            "sleep 30 &\nprintf '%s\\n' \"$!\" > \"$1\"\nwait\n",
        )
        .unwrap();
        let mut command = command(PathBuf::from("/bin/sh"));
        command.arg(&script).arg(&child_pid);
        let process = command.spawn().unwrap();
        let mut host = Some(process.into());
        let deadline = Instant::now() + Duration::from_secs(2);
        while !child_pid.is_file() {
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let pid: i32 = fs::read_to_string(&child_pid)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let mut frontend = None;
        let mut active_backend_url = Some("http://127.0.0.1:3001/".to_owned());
        let mut active = Some(TimedProof {
            accepted: accepted_proof(1),
            received_at: Instant::now() - Duration::from_secs(2),
        });
        assert!(
            expire_active_if_needed(
                root.path(),
                &mut active,
                &mut host,
                &mut frontend,
                &mut active_backend_url,
            )
            .await
            .unwrap()
        );
        assert!(host.is_none());
        assert!(active.is_none());
        assert!(active_backend_url.is_none());
        assert!(!root.path().join(".lenso/dev-backend-url").exists());
        assert_pid_stopped(pid).await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn proof_expiry_interrupts_side_effecting_preflight() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let control = root.path().join(".lenso");
        fs::create_dir(&control).unwrap();
        fs::write(control.join("host-mode"), "native").unwrap();
        let child_pid = root.path().join("child.pid");
        let host = control.join("host");
        fs::write(
            &host,
            format!(
                "#!/bin/sh\nsleep 30 &\nprintf '%s\\n' \"$!\" > \"{}\"\nwait\n",
                child_pid.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&host, fs::Permissions::from_mode(0o755)).unwrap();
        let error = preflight_until(
            root.path(),
            Some(Instant::now() + Duration::from_millis(200)),
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("proof expired"));
        let pid: i32 = fs::read_to_string(&child_pid)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_pid_stopped(pid).await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn exited_preflight_leader_stops_its_live_descendant_before_reaping() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let control = root.path().join(".lenso");
        fs::create_dir(&control).unwrap();
        fs::write(control.join("host-mode"), "native").unwrap();
        let child_pid = root.path().join("child.pid");
        let host = control.join("host");
        fs::write(
            &host,
            format!(
                "#!/bin/sh\nsleep 30 &\nprintf '%s\\n' \"$!\" > \"{}\"\nexit 23\n",
                child_pid.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&host, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(preflight(root.path()).await.is_err());
        let pid: i32 = fs::read_to_string(&child_pid)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_pid_stopped(pid).await;
    }

    #[cfg(unix)]
    fn mock_host(output: &Path, ready: bool) {
        use std::os::unix::fs::PermissionsExt;

        let host = output.join(".lenso/host");
        let source = if ready {
            "#!/bin/sh\ntest \"$1\" = --prepare && exit 0\ntest \"$1\" = --ready-file || exit 24\ntrap 'printf %s \"$LENSO_MANAGED_SHUTDOWN_TOKEN\" > \"$LENSO_MANAGED_SHUTDOWN_RECEIPT\"; exit 0' TERM\nprintf 'lenso.local-host-ready.v1\\n' > \"$2\"\nwhile :; do sleep 0.05; done\n"
        } else {
            "#!/bin/sh\nexit 23\n"
        };
        fs::write(&host, source).unwrap();
        fs::set_permissions(host, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(unix)]
    fn mock_host_dynamic_failure(output: &Path, marker: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let host = output.join(".lenso/host");
        fs::write(
            &host,
            format!(
                "#!/bin/sh\ntest \"$1\" = --prepare && exit 0\nprintf 'dynamic-started\\n' > \"{}\"\nexit 23\n",
                marker.display()
            ),
        )
        .unwrap();
        fs::set_permissions(host, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn managed_dev_requires_receipt_and_successful_exit_to_clear_root_fence() {
        use std::os::unix::fs::PermissionsExt as _;
        for cleanup in [
            "exit 0",
            "printf wrong > \"$LENSO_MANAGED_SHUTDOWN_RECEIPT\"; exit 0",
            "printf %s \"$LENSO_MANAGED_SHUTDOWN_TOKEN\" > \"$LENSO_MANAGED_SHUTDOWN_RECEIPT\"; exit 23",
        ] {
            let root = tempfile::tempdir().unwrap();
            fs::create_dir(root.path().join(".lenso")).unwrap();
            let distribution = tempfile::tempdir().unwrap();
            fs::create_dir(distribution.path().join(".lenso")).unwrap();
            fs::write(distribution.path().join(".lenso/host-mode"), "native").unwrap();
            let executable = distribution.path().join(".lenso/host");
            fs::write(&executable, format!(
                "#!/bin/sh\ntrap '{cleanup}' TERM\nprintf 'lenso.local-host-ready.v1\\n' > \"$2\"\nwhile :; do sleep 0.05; done\n"
            )).unwrap();
            fs::set_permissions(executable, fs::Permissions::from_mode(0o755)).unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            let retirement = Retirement::new(root.path(), true, deadline).unwrap();
            let mut host = launch_configured_until(
                distribution.path(),
                &[],
                true,
                false,
                Some(deadline),
                Some(retirement),
            )
            .await
            .unwrap()
            .unwrap();
            assert!(host.retire().await.is_err());
            drop(host);
            drop(distribution);
            // Deleting temporary distributions must not grant another session.
            assert!(Retirement::check_session(root.path()).is_err());
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn failed_dynamic_dev_start_remains_fenced() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".lenso")).unwrap();
        fs::write(root.path().join(".lenso/host-mode"), "native").unwrap();
        let marker = root.path().join("dynamic-started");
        mock_host_dynamic_failure(root.path(), &marker);
        let deadline = Instant::now() + Duration::from_secs(10);
        let retirement = Retirement::new(root.path(), true, deadline).unwrap();
        assert!(
            launch_configured_until(
                root.path(),
                &[],
                true,
                false,
                Some(deadline),
                Some(retirement),
            )
            .await
            .is_err()
        );
        assert!(marker.exists());
        assert!(Retirement::check_session(root.path()).is_err());
    }

    #[cfg(unix)]
    fn mock_web_host(output: &Path, address: &str) {
        use std::os::unix::fs::PermissionsExt;

        let control = output.join(".lenso");
        fs::create_dir_all(&control).unwrap();
        fs::write(control.join("host-mode"), "native").unwrap();
        let script = format!(
            "#!/bin/sh\ntest \"$1\" = --ready-file || exit 24\ntest \"$3\" = --web-address-file || exit 25\nprintf '{address}\\n' > \"$4\"\nprintf 'lenso.local-host-ready.v1\\n' > \"$2\"\ntrap 'exit 0' TERM\nwhile :; do sleep 1; done\n"
        );
        let host = control.join("host");
        fs::write(&host, script).unwrap();
        fs::set_permissions(host, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn web_address_receipt_accepts_exact_ipv4_and_ipv6_loopback() {
        let output = tempfile::tempdir().unwrap();
        let control = output.path().join(".lenso");
        fs::create_dir(&control).unwrap();
        let receipt = control.join("dev-web-address");
        fs::write(&receipt, b"http://127.0.0.1:3001/\n").unwrap();
        assert_eq!(
            backend_url(output.path()).unwrap(),
            "http://127.0.0.1:3001/"
        );
        fs::write(&receipt, b"http://[::1]:3002/\n").unwrap();
        assert_eq!(backend_url(output.path()).unwrap(), "http://[::1]:3002/");
        fs::write(&receipt, b"http://[::2]:3002/\n").unwrap();
        assert!(backend_url(output.path()).is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn exited_candidate_cannot_replace_a_running_host() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("old");
        let next = root.path().join("next");
        mock_web_host(&old, "http://127.0.0.1:3001/");
        mock_web_host(&next, "http://127.0.0.1:3002/");
        let script = "#!/bin/sh\ntest \"$1\" = --ready-file || exit 24\ntest \"$3\" = --web-address-file || exit 25\nprintf 'http://127.0.0.1:3002/\\n' > \"$4\"\nprintf 'lenso.local-host-ready.v1\\n' > \"$2\"\nsleep 0.3\nexit 23\n";
        let next_host = next.join(".lenso/host");
        fs::write(&next_host, script).unwrap();
        fs::set_permissions(&next_host, fs::Permissions::from_mode(0o755)).unwrap();
        let mut running = launch_ready(&old, &[], false, true).await.unwrap().unwrap();
        let mut candidate = launch_ready(&next, &[], false, true)
            .await
            .unwrap()
            .unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(!candidate_still_running(&mut candidate).unwrap());
        assert!(running.try_wait().unwrap().is_none());
        stop(&mut running, true).await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn failed_frontend_candidate_keeps_previous_host_generation() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".lenso")).unwrap();
        fs::create_dir_all(root.path().join("frontend")).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        fs::write(
            root.path().join("frontend/lenso.dev.toml"),
            format!(
                "schema = 'lenso.frontend-dev.v1'\ncommand = ['/bin/sh', '-c', 'exit 23']\nurl = 'http://127.0.0.1:{port}/'\nbackend_url_mode = 'file'\n"
            ),
        )
        .unwrap();
        let config = frontend::FrontendConfig::load(root.path())
            .unwrap()
            .unwrap();
        let old = root.path().join("old");
        let next = root.path().join("next");
        mock_web_host(&old, "http://127.0.0.1:3001/");
        mock_web_host(&next, "http://127.0.0.1:3002/");
        let mut host = launch_ready(&old, &[], false, true).await.unwrap();
        let old_id = host.as_ref().unwrap().id();
        let mut frontend_process = None;
        let mut active_backend_url = Some("http://127.0.0.1:3001/".to_owned());
        assert_eq!(
            activate_candidate_with_frontend(
                &next,
                &[],
                FrontendPreview {
                    root: root.path(),
                    host: &mut host,
                    frontend_process: &mut frontend_process,
                    active_backend_url: &mut active_backend_url,
                    config: &config,
                },
                false,
            )
            .await
            .unwrap(),
            Some(false)
        );
        assert_eq!(host.as_ref().unwrap().id(), old_id);
        assert!(host.as_mut().unwrap().try_wait().unwrap().is_none());
        assert!(frontend_process.is_none());
        assert_eq!(
            fs::read_to_string(root.path().join(".lenso/dev-backend-url")).unwrap(),
            "http://127.0.0.1:3001/\n"
        );
        stop(host.as_mut().unwrap(), true).await.unwrap();
    }

    #[tokio::test]
    async fn second_dev_session_cannot_change_live_backend_file() {
        let root = tempfile::tempdir().unwrap();
        let control = root.path().join(".lenso");
        fs::create_dir(&control).unwrap();
        let backend = control.join("dev-backend-url");
        fs::write(&backend, b"http://127.0.0.1:3001/\n").unwrap();
        let first = lock_dev(root.path()).unwrap();
        let error = dev(DevArgs {
            root: Some(root.path().to_path_buf()),
            configuration_policy: None,
            configuration_poll_seconds: 10,
            trust_linked_build: Vec::new(),
            args: Vec::new(),
        })
        .await
        .unwrap_err();
        assert!(format!("{error:#}").contains("another lenso app dev session"));
        assert_eq!(fs::read(&backend).unwrap(), b"http://127.0.0.1:3001/\n");
        assert_eq!(fs::read_dir(&control).unwrap().count(), 2);
        drop(first);
        let restored = lock_dev(root.path());
        assert!(
            restored.is_ok(),
            "dev lock stayed unavailable: {restored:?}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn readiness_preflight_uses_candidate_check_and_rejects_failed_startup() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let control = directory.path().join(".lenso");
        fs::create_dir(&control).unwrap();
        fs::write(control.join("host-mode"), "native").unwrap();
        let host = control.join("host");
        fs::write(&host, "#!/bin/sh\ntest \"$1\" = --check\n").unwrap();
        fs::set_permissions(&host, fs::Permissions::from_mode(0o755)).unwrap();
        preflight(directory.path()).await.unwrap();
        fs::write(&host, "#!/bin/sh\nexit 23\n").unwrap();
        assert!(preflight(directory.path()).await.is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn actual_startup_requires_a_live_host_readiness_receipt() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let control = directory.path().join(".lenso");
        fs::create_dir(&control).unwrap();
        fs::write(control.join("host-mode"), "native").unwrap();
        let host = control.join("host");
        fs::write(&host, "#!/bin/sh\nexit 23\n").unwrap();
        fs::set_permissions(&host, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            launch_ready(directory.path(), &[], false, false)
                .await
                .is_err()
        );
        fs::write(
            &host,
            "#!/bin/sh\nif [ \"$1\" = --ready-file ]; then printf 'lenso.local-host-ready.v1\\n' > \"$2\"; exec sleep 30; fi\nexit 24\n",
        )
        .unwrap();
        let mut candidate = launch_ready(directory.path(), &[], false, false)
            .await
            .unwrap()
            .unwrap();
        assert!(candidate.try_wait().unwrap().is_none());
        stop(&mut candidate, true).await.unwrap();
        fs::write(control.join("dev-ready"), b"lenso.local-host-ready.v1\n").unwrap();
        fs::write(&host, "#!/bin/sh\nsleep 0.2\nexit 23\n").unwrap();
        assert!(
            launch_ready(directory.path(), &[], false, false)
                .await
                .is_err()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn supervised_configuration_retires_before_update_and_allows_clean_retry() {
        use lenso_app_plan::authoring::{
            HostCatalog, HostDefaultPlugin, HostPluginRelease, HostSlot, PluginDescriptor,
        };

        let output = tempfile::tempdir().unwrap();
        fs::create_dir(output.path().join(".lenso")).unwrap();
        fs::create_dir_all(output.path().join("intent/.lenso")).unwrap();
        fs::write(output.path().join(".lenso/host-mode"), "native").unwrap();
        mock_host(output.path(), true);
        let descriptor = PluginDescriptor::new("example.agent", "1.0.0", "agent")
            .with_configuration_schema(serde_json::json!({
                "type": "object", "properties": {"greeting": {"type": "string"}},
                "additionalProperties": false
            }));
        let catalog = HostCatalog::new(
            [HostSlot::one("agent")],
            [HostPluginRelease::new(descriptor)],
            [HostDefaultPlugin::new("example.agent", "default")],
        );
        fs::write(
            output.path().join(".lenso/host-catalog.json"),
            serde_json::to_vec(&catalog).unwrap(),
        )
        .unwrap();
        let source = output.path().join("snapshot.json");
        let policy = output.path().join("policy.json");
        fs::write(
            &policy,
            serde_json::to_vec(&serde_json::json!({
                "schema": "lenso.configuration-source-policy.v1",
                "source_reference": "test",
                "source": {"type": "file", "path": source},
                "objects": [{"plugin_id": "example.agent", "instance_key": "default", "fields": ["greeting"]}]
            }))
            .unwrap(),
        )
        .unwrap();
        let write_snapshot = |revision: u64, toml: &str| {
            fs::write(
                &source,
                serde_json::to_vec(&serde_json::json!({
                    "schema": "lenso.plugin-configuration-snapshot.v1",
                    "revision": revision,
                    "configurations": [{"plugin_id": "example.agent", "instance_key": "default", "toml": toml}]
                }))
                .unwrap(),
            )
            .unwrap();
        };
        write_snapshot(1, "greeting = 'first'\n");
        let accepted =
            super::super::configuration_source::sync_with_proof(output.path(), &policy).unwrap();
        let mut host = None;
        let mut frontend_process = None;
        let mut active_backend_url = None;
        let mut active = None;
        let mut dynamic_start_available = true;
        assert!(
            activate_supervised_candidate(
                output.path(),
                &[],
                None,
                TimedProof {
                    accepted,
                    received_at: Instant::now()
                },
                LivePreviewGuard {
                    root: output.path(),
                    active_output: None,
                    policy: &policy,
                    host: &mut host,
                    frontend: &mut frontend_process,
                    active_backend_url: &mut active_backend_url,
                    active: &mut active,
                },
                &mut dynamic_start_available,
            )
            .await
            .unwrap()
            .unwrap()
        );
        assert!(dynamic_start_available);
        let first = host.as_ref().unwrap().id();
        let status = super::super::configuration_source::inspect_status(output.path()).unwrap();
        assert_eq!(status.last_activated_revision, Some(1));

        let mut rejected = active.as_ref().unwrap().clone();
        rejected.accepted.revision = 0;
        assert_eq!(
            activate_supervised_candidate(
                output.path(),
                &[],
                None,
                rejected,
                LivePreviewGuard {
                    root: output.path(),
                    active_output: Some(output.path()),
                    policy: &policy,
                    host: &mut host,
                    frontend: &mut frontend_process,
                    active_backend_url: &mut active_backend_url,
                    active: &mut active,
                },
                &mut dynamic_start_available,
            )
            .await
            .unwrap(),
            Some(false)
        );
        assert_eq!(host.as_ref().unwrap().id(), first);
        assert!(active.as_ref().unwrap().is_fresh());

        write_snapshot(2, "greeting = 'second'\n");
        let accepted =
            super::super::configuration_source::sync_with_proof(output.path(), &policy).unwrap();
        assert!(
            fs::read_to_string(
                output
                    .path()
                    .join("intent/plugins/example.agent/default.toml")
            )
            .unwrap()
            .contains("second")
        );
        mock_host(output.path(), false);
        assert!(
            !activate_supervised_candidate(
                output.path(),
                &[],
                None,
                TimedProof {
                    accepted,
                    received_at: Instant::now()
                },
                LivePreviewGuard {
                    root: output.path(),
                    active_output: Some(output.path()),
                    policy: &policy,
                    host: &mut host,
                    frontend: &mut frontend_process,
                    active_backend_url: &mut active_backend_url,
                    active: &mut active,
                },
                &mut dynamic_start_available,
            )
            .await
            .unwrap()
            .unwrap()
        );
        assert_eq!(host.as_ref().unwrap().id(), first);
        assert!(active.as_ref().unwrap().is_fresh());
        use nix::{errno::Errno, sys::signal::kill, unistd::Pid};
        assert_eq!(
            kill(Pid::from_raw(i32::try_from(first.unwrap()).unwrap()), None),
            Ok(())
        );
        let status = super::super::configuration_source::inspect_status(output.path()).unwrap();
        assert_eq!(status.desired_revision, Some(2));
        assert_eq!(status.last_activated_revision, Some(1));
        assert!(status.pending_activation);

        mock_host(output.path(), true);
        let executable = output.path().join(".lenso/host");
        let script = fs::read_to_string(&executable).unwrap().replace(
            "test \"$1\" = --ready-file",
            &format!(
                "if kill -0 {} 2>/dev/null; then exit 47; fi\ntest \"$1\" = --ready-file",
                first.unwrap()
            ),
        );
        fs::write(executable, script).unwrap();
        let accepted =
            super::super::configuration_source::sync_with_proof(output.path(), &policy).unwrap();
        assert!(
            activate_supervised_candidate(
                output.path(),
                &[],
                None,
                TimedProof {
                    accepted,
                    received_at: Instant::now()
                },
                LivePreviewGuard {
                    root: output.path(),
                    active_output: Some(output.path()),
                    policy: &policy,
                    host: &mut host,
                    frontend: &mut frontend_process,
                    active_backend_url: &mut active_backend_url,
                    active: &mut active,
                },
                &mut dynamic_start_available,
            )
            .await
            .unwrap()
            .unwrap()
        );
        assert_ne!(host.as_ref().unwrap().id(), first);
        assert_eq!(
            kill(Pid::from_raw(i32::try_from(first.unwrap()).unwrap()), None),
            Err(Errno::ESRCH)
        );
        let second = host.as_ref().unwrap().id();
        write_snapshot(3, "greeting = 'second'\n");
        let same_root = TimedProof {
            accepted: super::super::configuration_source::sync_with_proof(output.path(), &policy)
                .unwrap(),
            received_at: Instant::now(),
        };
        assert_eq!(
            activate_supervised_candidate(
                output.path(),
                &[],
                None,
                same_root,
                LivePreviewGuard {
                    root: output.path(),
                    active_output: Some(output.path()),
                    policy: &policy,
                    host: &mut host,
                    frontend: &mut frontend_process,
                    active_backend_url: &mut active_backend_url,
                    active: &mut active,
                },
                &mut dynamic_start_available,
            )
            .await
            .unwrap(),
            Some(true)
        );
        assert_eq!(host.as_ref().unwrap().id(), second);
        let status = super::super::configuration_source::inspect_status(output.path()).unwrap();
        assert_eq!(status.desired_revision, Some(3));
        assert_eq!(status.last_activated_revision, Some(2));
        assert!(status.desired_matches_last_activated_root_and_policy);
        assert!(status.pending_activation);
        stop_active(&mut host, &mut frontend_process).await.unwrap();
        active = None;
        Retirement::check_session(output.path()).unwrap();

        let accepted =
            super::super::configuration_source::sync_with_proof(output.path(), &policy).unwrap();
        let pending_proof = TimedProof {
            accepted,
            received_at: Instant::now(),
        };
        mock_host(output.path(), true);
        let mut next_session_dynamic_start_available = true;
        assert!(
            activate_supervised_candidate(
                output.path(),
                &[],
                None,
                pending_proof,
                LivePreviewGuard {
                    root: output.path(),
                    active_output: Some(output.path()),
                    policy: &policy,
                    host: &mut host,
                    frontend: &mut frontend_process,
                    active_backend_url: &mut active_backend_url,
                    active: &mut active,
                },
                &mut next_session_dynamic_start_available,
            )
            .await
            .unwrap()
            .unwrap()
        );
        assert!(next_session_dynamic_start_available);
        assert_ne!(host.as_ref().unwrap().id(), first);
        let status = super::super::configuration_source::inspect_status(output.path()).unwrap();
        assert_eq!(status.last_activated_revision, Some(3));
        assert!(!status.pending_activation);

        write_snapshot(4, "unauthorized = 'value'\n");
        assert!(super::super::configuration_source::sync(output.path(), &policy).is_err());
        fs::remove_file(&source).unwrap();
        assert!(super::super::configuration_source::sync(output.path(), &policy).is_err());
        let status = super::super::configuration_source::inspect_status(output.path()).unwrap();
        assert_eq!(status.desired_revision, Some(3));
        assert_eq!(status.last_activated_revision, Some(3));
        assert!(host.as_mut().unwrap().try_wait().unwrap().is_none());

        assert!(
            !retire_active_on_policy_change(
                output.path(),
                Some(output.path()),
                &policy,
                &mut host,
                &mut frontend_process,
                &mut active_backend_url,
            )
            .await
            .unwrap()
        );
        assert!(host.as_mut().unwrap().try_wait().unwrap().is_none());
        write_snapshot(3, "greeting = 'second'\n");
        fs::write(
            &policy,
            serde_json::to_vec(&serde_json::json!({
                "schema": "lenso.configuration-source-policy.v1",
                "source_reference": "test",
                "source": {"type": "file", "path": source},
                "objects": [{"plugin_id": "example.agent", "instance_key": "default", "fields": ["greeting", "token"]}]
            }))
            .unwrap(),
        )
        .unwrap();
        super::super::configuration_source::sync(output.path(), &policy).unwrap();
        assert!(
            retire_active_on_policy_change(
                output.path(),
                Some(output.path()),
                &policy,
                &mut host,
                &mut frontend_process,
                &mut active_backend_url,
            )
            .await
            .unwrap()
        );
        assert!(host.is_none());
        assert!(Retirement::check_session(output.path()).is_err());
    }

    #[test]
    fn app_watch_includes_discovery_and_intent_but_excludes_generated_output() {
        for path in [
            "/app/app/new/Cargo.toml",
            "/shared/plugins/new/src/lib.rs",
            "/app/plugins/a/instance.json",
            "/app/lenso.toml",
            "/app/app/web/public/icon.svg",
        ] {
            assert!(relevant(Path::new(path)));
        }
        for path in [
            "/app/.lenso/dev/generation/runtime/bun",
            "/app/app/p/target/debug/a",
            "/app/node_modules/a",
            "/app/dist/plugins/a",
        ] {
            assert!(!relevant(Path::new(path)));
        }
    }

    #[test]
    fn app_watch_ignores_read_access_but_rebuilds_on_write_close() {
        use notify::event::{AccessKind, AccessMode, ModifyKind};

        let source = PathBuf::from("/app/src/lib.rs");
        let opened =
            notify::Event::new(notify::EventKind::Access(AccessKind::Open(AccessMode::Any)))
                .add_path(source.clone());
        let closed_after_read = notify::Event::new(notify::EventKind::Access(AccessKind::Close(
            AccessMode::Read,
        )))
        .add_path(source.clone());
        let closed_after_write = notify::Event::new(notify::EventKind::Access(AccessKind::Close(
            AccessMode::Write,
        )))
        .add_path(source.clone());
        let modified =
            notify::Event::new(notify::EventKind::Modify(ModifyKind::Any)).add_path(source);

        assert!(!rebuild_event(&opened));
        assert!(!rebuild_event(&closed_after_read));
        assert!(rebuild_event(&closed_after_write));
        assert!(rebuild_event(&modified));
    }
}
