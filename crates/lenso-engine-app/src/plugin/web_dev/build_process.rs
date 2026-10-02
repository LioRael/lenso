//! Own cancellable Cargo invocations for the interactive Web development loop.
use std::{
    env,
    future::Future,
    path::{Path, PathBuf},
    process::Command,
    process::Stdio,
    time::Duration,
};

use anyhow::{Context, bail};
use tokio::{
    io::{AsyncBufReadExt as _, BufReader},
    process::Child,
};

pub(super) async fn target_directory(
    root: &Path,
    shutdown: &mut (impl Future<Output = std::io::Result<()>> + Unpin),
) -> anyhow::Result<Option<PathBuf>> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = crate::app::build_command(cargo);
    command
        .args(["metadata", "--locked", "--format-version", "1", "--no-deps"])
        .current_dir(root);
    let mut output = String::new();
    if !run(
        command,
        "inspect Plugin Cargo target directory",
        shutdown,
        |line| {
            output.push_str(line);
            output.push('\n');
        },
    )
    .await?
    {
        return Ok(None);
    }
    let metadata: super::super::CargoTargetMetadata =
        serde_json::from_str(&output).context("parse Plugin Cargo metadata")?;
    Ok(Some(metadata.target_directory))
}

pub(super) async fn run(
    command: Command,
    action: &str,
    shutdown: &mut (impl Future<Output = std::io::Result<()>> + Unpin),
    mut diagnostic: impl FnMut(&str),
) -> anyhow::Result<bool> {
    let mut command = tokio::process::Command::from(command);
    command.stdout(Stdio::piped()).kill_on_drop(true);
    // Cargo's compiler and build-script children must stop with this invocation,
    // including when only the CLI receives the interrupt.
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command.spawn().with_context(|| action.to_owned())?;
    let output = child.stdout.take().context("read Cargo diagnostics")?;
    let mut lines = BufReader::new(output).lines();
    let status = tokio::select! {
        biased;
        result = shutdown => {
            stop(&mut child).await;
            result.context("listen for Ctrl-C")?;
            return Ok(false);
        }
        result = async {
            while let Some(line) = lines.next_line().await.context("read Cargo diagnostic")? {
                diagnostic(&line);
            }
            child.wait().await.context("wait for Web development build")
        } => match result {
            Ok(status) => status,
            Err(error) => {
                stop(&mut child).await;
                return Err(error);
            }
        }
    };
    if !status.success() {
        bail!("{action} failed with {status}");
    }
    Ok(true)
}

async fn stop(child: &mut Child) {
    #[cfg(unix)]
    let group = child
        .id()
        .and_then(|id| i32::try_from(id).ok())
        .map(nix::unistd::Pid::from_raw);
    #[cfg(unix)]
    if let Some(group) = group {
        let _ = nix::sys::signal::killpg(group, nix::sys::signal::Signal::SIGINT);
    }
    #[cfg(not(unix))]
    let _ = child.start_kill();

    let exited = tokio::time::timeout(Duration::from_secs(4), child.wait()).await;
    // Reap the direct child and remove any compiler/build-script descendants
    // even if Cargo itself exited before them.
    #[cfg(unix)]
    if let Some(group) = group {
        let _ = nix::sys::signal::killpg(group, nix::sys::signal::Signal::SIGKILL);
    }
    if !matches!(exited, Ok(Ok(_))) {
        let _ = child.kill().await;
    }
}
