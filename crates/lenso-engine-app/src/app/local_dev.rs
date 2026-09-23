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
    let generations = tempfile::Builder::new()
        .prefix("dev-")
        .tempdir_in(root.join(".lenso"))?;
    let (mut watcher, mut events) = watch(&root)?;
    let mut host: Option<Child> = None;
    let mut revision = 0;
    let mut current_output: Option<PathBuf> = None;
    let mut poll = tokio::time::interval(Duration::from_secs(args.configuration_poll_seconds));
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    poll.tick().await;
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
                if let Some(host) = &mut host { stop(host, false).await?; }
                return Ok(());
            }
        };
        let built = status.success();
        watch_dependencies(&root, &mut watcher)?;
        let ready = if built {
            if let Some(policy) = &policy
                && let Err(error) = super::configuration_source::sync(&output, policy)
            {
                eprintln!(
                    "App configuration source rejected; retaining the running generation: {error:#}"
                );
                false
            } else {
                match preflight(&output).await {
                    Ok(()) => true,
                    Err(error) => {
                        eprintln!(
                            "App candidate failed readiness; retaining the running generation: {error:#}"
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
            match activate_candidate(&output, &args.args, &mut host, policy.is_some()).await? {
                Some(true) => {
                    select_output(&mut current_output, &output);
                    eprintln!(
                        "Watching {} for App changes. Press Ctrl-C to stop.",
                        root.display()
                    );
                }
                Some(false) => {}
                None => {
                    if let Some(host) = &mut host {
                        stop(host, false).await?;
                    }
                    return Ok(());
                }
            }
        }
        loop {
            tokio::select! {
                signal = tokio::signal::ctrl_c() => {
                    signal?;
                    if let Some(host) = &mut host { stop(host, false).await?; }
                    return Ok(());
                }
                event = events.recv() => {
                    match event.context("App watcher closed")? {
                        Ok(event) if event.paths.iter().any(|p| relevant(p)) => {
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
                            if let Some(host) = &mut host { stop(host, false).await?; }
                            return Err(error).context("App watcher failed");
                        }
                    }
                }
                () = tokio::time::sleep(Duration::from_millis(200)) => {
                    if let Some(process) = &mut host
                        && let Some(status) = process.try_wait()? {
                            eprintln!("Local Host exited ({status}); edit the source to restart.");
                            host = None;
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
                    match super::configuration_source::sync(&target, policy) {
                        Ok(()) => match super::configuration_source::inspect_status(&target) {
                            Ok(status) if !status.pending_publication && (status.pending_activation || host.is_none() || current_output.as_deref() != Some(target.as_path())) => {
                                match preflight(&target).await {
                                    Ok(()) => match activate_candidate(&target, &args.args, &mut host, true).await? {
                                        Some(true) => select_output(&mut current_output, &target),
                                        Some(false) => {},
                                        None => {
                                            if let Some(host) = &mut host { stop(host, false).await?; }
                                            return Ok(());
                                        }
                                    },
                                    Err(error) => eprintln!("Configuration candidate failed readiness; retaining the running generation: {error:#}"),
                                }
                            }
                            Ok(_) => {}
                            Err(error) => eprintln!("Configuration status is invalid; retaining the running generation: {error:#}"),
                        },
                        Err(error) => eprintln!("Configuration source unavailable or rejected; retaining the running generation: {error:#}"),
                    }
                }
            }
        }
        // Retain the OS watcher while building, so edits during compilation are queued.
        let _ = &watcher;
    }
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
) -> anyhow::Result<Option<bool>> {
    let revision = if supervised_configuration {
        match super::configuration_source::desired_root_revision(output) {
            Ok(revision) => revision,
            Err(error) => {
                eprintln!(
                    "Configuration candidate no longer matches its published Root; retaining the running generation: {error:#}"
                );
                return Ok(Some(false));
            }
        }
    } else {
        None
    };
    let mut candidate = match launch_ready(output, args, supervised_configuration).await {
        Ok(Some(candidate)) => candidate,
        Ok(None) => return Ok(None),
        Err(error) => {
            eprintln!(
                "App candidate failed actual startup; retaining the running generation: {error:#}"
            );
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
        if event
            .as_ref()
            .is_ok_and(|event| !event.paths.iter().any(|path| relevant(path)))
        {
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
        assert!(launch_ready(directory.path(), &[], false).await.is_err());
        fs::write(
            &host,
            "#!/bin/sh\nif [ \"$1\" = --ready-file ]; then printf 'lenso.local-host-ready.v1\\n' > \"$2\"; exec sleep 30; fi\nexit 24\n",
        )
        .unwrap();
        let mut candidate = launch_ready(directory.path(), &[], false)
            .await
            .unwrap()
            .unwrap();
        assert!(candidate.try_wait().unwrap().is_none());
        stop(&mut candidate, true).await.unwrap();
        fs::write(control.join("dev-ready"), b"lenso.local-host-ready.v1\n").unwrap();
        fs::write(&host, "#!/bin/sh\nsleep 0.2\nexit 23\n").unwrap();
        assert!(launch_ready(directory.path(), &[], false).await.is_err());
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
            activate_candidate(output.path(), &[], &mut host, true)
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
            !activate_candidate(output.path(), &[], &mut host, true)
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
            activate_candidate(output.path(), &[], &mut host, true)
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
        stop(host.as_mut().unwrap(), false).await.unwrap();
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
}
