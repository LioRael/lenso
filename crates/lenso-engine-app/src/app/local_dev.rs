//! Local development rebuilds complete App generations. A failed build leaves
//! the running generation alone; replacement waits for the new Host Ready Gate
//! before draining the previous Host.
use anyhow::{Context, bail};
use clap::Args;
use notify::{RecursiveMode, Watcher};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{
    process::{Child, Command},
    sync::mpsc,
};

mod frontend;

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
    /// Arguments for installed terminal support, rerun after each successful rebuild.
    #[arg(last = true)]
    args: Vec<String>,
}

pub async fn dev(args: DevArgs) -> anyhow::Result<()> {
    anyhow::ensure!(
        (1..=3600).contains(&args.configuration_poll_seconds),
        "configuration poll interval must be between 1 and 3600 seconds"
    );
    let root = crate::plugins::project_root(args.root)?;
    let policy = args
        .configuration_policy
        .map(fs::canonicalize)
        .transpose()?;
    fs::create_dir_all(root.join(".lenso"))?;
    let _dev_lock = lock_dev(&root)?;
    let generations = tempfile::Builder::new()
        .prefix("dev-")
        .tempdir_in(root.join(".lenso"))?;
    let frontend_config = frontend::FrontendConfig::load(&root)?;
    let frontend_enabled = frontend_config.is_some();
    let (mut watcher, mut events) = watch(&root)?;
    let mut host: Option<Child> = None;
    let mut frontend_process: Option<frontend::FrontendProcess> = None;
    let mut active_backend_url: Option<String> = None;
    let mut revision = 0;
    let mut current_output: Option<PathBuf> = None;
    let mut poll = tokio::time::interval(Duration::from_secs(args.configuration_poll_seconds));
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    poll.tick().await;
    let result: anyhow::Result<()> = async {
        loop {
        revision += 1;
        let output = generations.path().join(format!("generation-{revision}"));
        let mut build = command(std::env::current_exe()?);
        build
            .args(["app", "build", "--root"])
            .arg(&root)
            .arg("--out")
            .arg(&output);
        let mut child = build.spawn().context("start local App build")?;
        let status = tokio::select! {
            status = child.wait() => status?,
            signal = tokio::signal::ctrl_c() => {
                signal?;
                stop(&mut child, true).await?;
                stop_active(&mut host, &mut frontend_process).await?;
                return Ok(());
            }
        };
        let built = status.success();
        watch_dependencies(&root, &mut watcher)?;
        if let Some(policy) = &policy {
            retire_active_on_policy_change(
                current_output.as_deref(),
                policy,
                &mut host,
                &mut frontend_process,
                &mut active_backend_url,
            )
            .await?;
        }
        let ready = if built {
            if let Some(policy) = &policy
                && let Err(error) = super::configuration_source::sync(&output, policy)
            {
                eprintln!(
                    "App configuration source rejected; candidate not activated: {error:#}"
                );
                false
            } else {
                match preflight(&output).await {
                    Ok(()) => true,
                    Err(error) => {
                        eprintln!(
                            "App candidate failed readiness; candidate not activated: {error:#}"
                        );
                        false
                    }
                }
            }
        } else {
            eprintln!("App rebuild failed; edit the source to retry.");
            false
        };
        if ready {
            let activation = if let Some(config) = &frontend_config {
                activate_candidate_with_frontend(
                    &root,
                    &output,
                    &args.args,
                    &mut host,
                    &mut frontend_process,
                    &mut active_backend_url,
                    policy.is_some(),
                    config,
                )
                .await?
            } else {
                activate_candidate(&output, &args.args, &mut host, policy.is_some(), false).await?
            };
            match activation {
                Some(true) => {
                    select_output(&mut current_output, &output);
                    eprintln!(
                        "Watching {} for App changes. Press Ctrl-C to stop.",
                        root.display()
                    );
                }
                Some(false) => {}
                None => {
                    stop_active(&mut host, &mut frontend_process).await?;
                    return Ok(());
                }
            }
        }
        loop {
            tokio::select! {
                signal = tokio::signal::ctrl_c() => {
                    signal?;
                    stop_active(&mut host, &mut frontend_process).await?;
                    return Ok(());
                }
                event = events.recv() => {
                    match event.context("App watcher closed")? {
                        Ok(event) if rebuild_event(&event) => {
                            if event.paths.iter().any(|p| frontend::is_config(&root, p)) {
                                eprintln!("Frontend dev configuration changed; restart lenso dev to review and apply its command.");
                            }
                            if frontend_enabled
                                && event.paths.iter().filter(|p| relevant(p)).all(|p| frontend::is_frontend(&root, p))
                            {
                                // The explicitly selected frontend owns its HMR/rebuild loop.
                                // Its source edits do not invalidate the Rust Host distribution.
                                continue;
                            }
                            tokio::time::sleep(Duration::from_millis(150)).await;
                            while events.try_recv().is_ok() {}
                            // Configuration edits may add a previously unwatched shared source.
                            match watch(&root) {
                                Ok((next, receiver)) => { watcher = next; events = receiver; }
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
                    if let Some(process) = &mut host
                        && let Some(status) = process.try_wait()? {
                            eprintln!("Local Host exited ({status}); edit the source to restart.");
                            host = None;
                            if let Some(frontend) = &mut frontend_process { frontend::stop(frontend).await?; }
                            frontend_process = None;
                            active_backend_url = None;
                        }
                    if let Some(process) = &mut frontend_process
                        && let Some(status) = process.try_wait()? {
                            stop_active(&mut host, &mut frontend_process).await?;
                            bail!("Frontend dev process exited ({status}); preview is no longer ready");
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
                    retire_active_on_policy_change(
                        current_output.as_deref(),
                        policy,
                        &mut host,
                        &mut frontend_process,
                        &mut active_backend_url,
                    )
                    .await?;
                    match super::configuration_source::sync(&target, policy) {
                        Ok(()) => match super::configuration_source::inspect_status(&target) {
                            Ok(status) if !status.pending_publication && (status.pending_activation || host.is_none() || current_output.as_deref() != Some(target.as_path())) => {
                                match preflight(&target).await {
                                    Ok(()) => {
                                        let activation = if let Some(config) = &frontend_config {
                                            activate_candidate_with_frontend(
                                                &root, &target, &args.args, &mut host,
                                                &mut frontend_process, &mut active_backend_url,
                                                true, config,
                                            ).await?
                                        } else {
                                            activate_candidate(&target, &args.args, &mut host, true, false).await?
                                        };
                                        match activation {
                                        Some(true) => {
                                            select_output(&mut current_output, &target);
                                        },
                                        Some(false) => {},
                                        None => {
                                            stop_active(&mut host, &mut frontend_process).await?;
                                            return Ok(());
                                        }
                                        }
                                    },
                                    Err(error) => eprintln!("Configuration candidate failed readiness; candidate not activated: {error:#}"),
                                }
                            }
                            Ok(_) => {}
                            Err(error) => eprintln!("Configuration status is invalid; candidate not activated: {error:#}"),
                        },
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

async fn finish_dev(
    result: anyhow::Result<()>,
    host: &mut Option<Child>,
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

async fn activate_candidate_with_frontend(
    root: &Path,
    output: &Path,
    args: &[String],
    host: &mut Option<Child>,
    frontend_process: &mut Option<frontend::FrontendProcess>,
    active_backend_url: &mut Option<String>,
    supervised_configuration: bool,
    config: &frontend::FrontendConfig,
) -> anyhow::Result<Option<bool>> {
    let revision = if supervised_configuration {
        match super::configuration_source::desired_root_revision(output) {
            Ok(revision) => revision,
            Err(error) => {
                eprintln!(
                    "Configuration candidate changed before startup; candidate not activated: {error:#}"
                );
                return Ok(Some(false));
            }
        }
    } else {
        None
    };
    let mut candidate = match launch_ready(output, args, supervised_configuration, true).await {
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
        match config.launch(root, initial_url, &address_file).await {
            Ok(Some(process)) => staged_frontend = Some(process),
            Ok(None) => {
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
            Err(error) => {
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
    match config.verify_backend(selected_frontend, &next_url).await {
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
    if let Some(revision) = revision
        && let Err(error) =
            super::configuration_source::record_distribution_activation(output, &revision)
    {
        eprintln!(
            "Local Host is ready, but configuration activation could not be recorded: {error:#}"
        );
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
    let group_id = candidate.id();
    match candidate.try_wait()? {
        None => Ok(true),
        Some(_) => {
            #[cfg(unix)]
            if let Some(group_id) = group_id {
                use nix::{
                    sys::signal::{Signal, killpg},
                    unistd::Pid,
                };
                let group = Pid::from_raw(i32::try_from(group_id)?);
                if let Err(error) = killpg(group, Signal::SIGKILL)
                    && error != nix::errno::Errno::ESRCH
                {
                    return Err(error.into());
                }
            }
            Ok(false)
        }
    }
}

async fn rollback_frontend_candidate(
    root: &Path,
    previous_url: Option<&str>,
    candidate: &mut Child,
    staged_frontend: &mut Option<frontend::FrontendProcess>,
    host: &mut Option<Child>,
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

fn lock_dev(root: &Path) -> anyhow::Result<fs::File> {
    let directory = root.join(".lenso");
    anyhow::ensure!(
        fs::symlink_metadata(&directory)?.file_type().is_dir(),
        "App development control directory must be a real directory"
    );
    let path = directory.join("dev.lock");
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(nix::libc::O_NOFOLLOW);
    }
    let file = match options.create_new(true).open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            anyhow::ensure!(
                fs::symlink_metadata(&path)?.file_type().is_file(),
                "App development lock must be a regular file"
            );
            let mut existing = fs::OpenOptions::new();
            existing.read(true).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                existing.custom_flags(nix::libc::O_NOFOLLOW);
            }
            existing.open(&path)?
        }
        Err(error) => return Err(error).context("open App development lock"),
    };
    anyhow::ensure!(
        file.metadata()?.is_file() && fs::symlink_metadata(&path)?.file_type().is_file(),
        "App development lock must remain a regular file"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let opened = file.metadata()?;
        let current = fs::symlink_metadata(&path)?;
        anyhow::ensure!(
            opened.dev() == current.dev() && opened.ino() == current.ino(),
            "App development lock path changed while opening"
        );
    }
    file.try_lock()
        .context("another lenso app dev session may already own this App development lock")?;
    Ok(file)
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
    host: &mut Option<Child>,
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
        let result = stop(process, false).await;
        *host = None;
        result
    } else {
        Ok(())
    };
    frontend_stopped?;
    host_stopped?;
    Ok(())
}

async fn retire_active_on_policy_change(
    active_output: Option<&Path>,
    policy: &Path,
    host: &mut Option<Child>,
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
            "Host configuration policy changed; stopping the running generation until a new candidate is ready"
        );
        stop_active(host, frontend).await?;
        *active_backend_url = None;
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
    host: &mut Option<Child>,
    supervised_configuration: bool,
    frontend_enabled: bool,
) -> anyhow::Result<Option<bool>> {
    let revision = if supervised_configuration {
        match super::configuration_source::desired_root_revision(output) {
            Ok(revision) => revision,
            Err(error) => {
                eprintln!(
                    "Configuration candidate no longer matches its published Root; candidate not activated: {error:#}"
                );
                return Ok(Some(false));
            }
        }
    } else {
        None
    };
    let mut candidate = match launch_ready(output, args, supervised_configuration, frontend_enabled)
        .await
    {
        Ok(Some(candidate)) => candidate,
        Ok(None) => return Ok(None),
        Err(error) => {
            eprintln!("App candidate failed actual startup; candidate not activated: {error:#}");
            return Ok(Some(false));
        }
    };
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
    if let Some(revision) = revision
        && let Err(error) =
            super::configuration_source::record_distribution_activation(output, &revision)
    {
        // The candidate is already live. Keep it supervised and report that
        // its activation receipt could not be fenced against current intent.
        eprintln!(
            "Local Host is ready, but configuration activation could not be recorded: {error:#}"
        );
    }
    *host = Some(candidate);
    Ok(Some(true))
}

async fn launch_ready(
    output: &Path,
    args: &[String],
    defer_activation: bool,
    frontend_enabled: bool,
) -> anyhow::Result<Option<Child>> {
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
    let mut candidate = candidate.spawn().context("start generated local Host")?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(status) = candidate.try_wait()? {
            bail!("generated local Host exited before readiness: {status}");
        }
        match fs::symlink_metadata(&marker) {
            Ok(metadata) => {
                if !metadata.file_type().is_file()
                    || fs::read(&marker)? != b"lenso.local-host-ready.v1\n"
                {
                    stop(&mut candidate, true).await?;
                    bail!("generated local Host returned an invalid readiness receipt");
                }
                if frontend_enabled && let Err(error) = backend_url(output) {
                    stop(&mut candidate, true).await?;
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
            stop(&mut candidate, true).await?;
            bail!("generated local Host did not become ready within 60 seconds");
        }
        tokio::select! {
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
    let mut candidate = command(output.join(".lenso/host"));
    candidate
        .args(super::local_host::host_arguments(output)?)
        .arg("--check");
    let mut candidate = candidate
        .spawn()
        .context("start App candidate readiness check")?;
    match tokio::time::timeout(Duration::from_secs(60), candidate.wait()).await {
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(Ok(status)) => bail!("App candidate readiness check failed: {status}"),
        Ok(Err(error)) => Err(error).context("wait for App candidate readiness"),
        Err(_) => {
            stop(&mut candidate, true).await?;
            bail!("App candidate did not become ready within 60 seconds")
        }
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
    if child.try_wait()?.is_some() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        use nix::{
            sys::signal::{Signal, kill, killpg},
            unistd::Pid,
        };
        let id = i32::try_from(child.id().context("child process ID")?)?;
        let pid = Pid::from_raw(id);
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
        match tokio::time::timeout(Duration::from_secs(12), child.wait()).await {
            Ok(result) => {
                let status = result?;
                if !whole_group && !status.success() {
                    bail!("local Host shutdown failed: {status}");
                }
            }
            Err(_) => {
                let _ = killpg(pid, Signal::SIGKILL);
                child.wait().await?;
                bail!("local process did not stop within its shutdown budget");
            }
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
)> {
    let (sender, receiver) = mpsc::channel(128);
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event.as_ref().is_ok_and(|event| !rebuild_event(event)) {
            return;
        }
        // A full queue already guarantees a rebuild; coalesce further events.
        let _ = sender.try_send(event);
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
    Ok((watcher, receiver))
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

    #[cfg(unix)]
    fn mock_host(output: &Path, ready: bool) {
        use std::os::unix::fs::PermissionsExt;

        let host = output.join(".lenso/host");
        let source = if ready {
            "#!/bin/sh\nif [ \"$1\" = --check ]; then exit 0; fi\ntest \"$1\" = --ready-file || exit 24\nprintf 'lenso.local-host-ready.v1\\n' > \"$2\"\ntrap 'exit 0' TERM\nwhile :; do sleep 1; done\n"
        } else {
            "#!/bin/sh\nexit 23\n"
        };
        fs::write(&host, source).unwrap();
        fs::set_permissions(host, fs::Permissions::from_mode(0o755)).unwrap();
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
                root.path(),
                &next,
                &[],
                &mut host,
                &mut frontend_process,
                &mut active_backend_url,
                false,
                &config,
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
            args: Vec::new(),
        })
        .await
        .unwrap_err();
        assert!(format!("{error:#}").contains("another lenso app dev session"));
        assert_eq!(fs::read(&backend).unwrap(), b"http://127.0.0.1:3001/\n");
        assert_eq!(fs::read_dir(&control).unwrap().count(), 2);
        drop(first);
        assert!(lock_dev(root.path()).is_ok());
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
    async fn configuration_candidate_moves_receipt_only_after_ready_replacement() {
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
        super::super::configuration_source::sync(output.path(), &policy).unwrap();
        let mut host = None;
        assert!(
            activate_candidate(output.path(), &[], &mut host, true, false)
                .await
                .unwrap()
                .unwrap()
        );
        let first = host.as_ref().unwrap().id();
        let status = super::super::configuration_source::inspect_status(output.path()).unwrap();
        assert_eq!(status.last_activated_revision, Some(1));

        write_snapshot(2, "greeting = 'second'\n");
        super::super::configuration_source::sync(output.path(), &policy).unwrap();
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
            !activate_candidate(output.path(), &[], &mut host, true, false)
                .await
                .unwrap()
                .unwrap()
        );
        assert_eq!(host.as_ref().unwrap().id(), first);
        assert!(host.as_mut().unwrap().try_wait().unwrap().is_none());
        let status = super::super::configuration_source::inspect_status(output.path()).unwrap();
        assert_eq!(status.desired_revision, Some(2));
        assert_eq!(status.last_activated_revision, Some(1));
        assert!(status.pending_activation);

        mock_host(output.path(), true);
        assert!(
            activate_candidate(output.path(), &[], &mut host, true, false)
                .await
                .unwrap()
                .unwrap()
        );
        assert_ne!(host.as_ref().unwrap().id(), first);
        let status = super::super::configuration_source::inspect_status(output.path()).unwrap();
        assert_eq!(status.last_activated_revision, Some(2));
        assert!(!status.pending_activation);

        write_snapshot(3, "unauthorized = 'value'\n");
        assert!(super::super::configuration_source::sync(output.path(), &policy).is_err());
        fs::remove_file(&source).unwrap();
        assert!(super::super::configuration_source::sync(output.path(), &policy).is_err());
        let status = super::super::configuration_source::inspect_status(output.path()).unwrap();
        assert_eq!(status.desired_revision, Some(2));
        assert_eq!(status.last_activated_revision, Some(2));
        assert!(host.as_mut().unwrap().try_wait().unwrap().is_none());

        let mut frontend_process = None;
        let mut active_backend_url = None;
        assert!(
            !retire_active_on_policy_change(
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
        write_snapshot(2, "greeting = 'second'\n");
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
