//! Cooperative retirement is evidence of managed cleanup, not OS containment.

use std::{
    fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context as _, bail, ensure};
use sha2::{Digest as _, Sha256};
use tokio::{
    process::{Child, Command},
    time::Instant,
};

const UNCERTAIN_RECEIPT: &[u8] = b"lenso.supervised-start-uncertain.v1\n";

pub(super) struct CrashFence {
    path: PathBuf,
    pub(super) marked: bool,
}

impl CrashFence {
    pub(super) fn new(distribution: &Path) -> anyhow::Result<Self> {
        Self::at(distribution.join(".lenso/supervised-start.uncertain"))
    }

    pub(super) fn at(path: PathBuf) -> anyhow::Result<Self> {
        match fs::symlink_metadata(&path) {
            Ok(_) => bail!(
                "previous supervised App run is unconfirmed; verify every Host and descendant for this App has stopped, then manually remove {} before restarting",
                path.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("inspect supervised start crash fence"),
        }
        Ok(Self {
            path,
            marked: false,
        })
    }

    pub(super) fn mark(&mut self) -> anyhow::Result<()> {
        ensure!(
            !self.marked,
            "supervised start crash fence is already marked"
        );
        let directory = self
            .path
            .parent()
            .context("supervised start fence directory")?;
        let mut stage = tempfile::NamedTempFile::new_in(directory)?;
        stage.write_all(UNCERTAIN_RECEIPT)?;
        stage.as_file().sync_all()?;
        stage.persist_noclobber(&self.path)?;
        fs::File::open(directory)?.sync_all()?;
        self.marked = true;
        Ok(())
    }

    pub(super) fn clear(&mut self) -> anyhow::Result<()> {
        ensure!(self.marked, "supervised start crash fence was not marked");
        let metadata = fs::symlink_metadata(&self.path)?;
        ensure!(
            metadata.file_type().is_file()
                && metadata.len() == u64::try_from(UNCERTAIN_RECEIPT.len())?
                && fs::read(&self.path)? == UNCERTAIN_RECEIPT,
            "supervised start crash fence changed after staging"
        );
        fs::remove_file(&self.path)?;
        fs::File::open(
            self.path
                .parent()
                .context("supervised start fence directory")?,
        )?
        .sync_all()?;
        self.marked = false;
        Ok(())
    }
}

pub(super) struct Receipt {
    directory: tempfile::TempDir,
    token: String,
}

impl Receipt {
    pub(super) fn new(from: &Path) -> anyhow::Result<Self> {
        Ok(Self {
            directory: tempfile::Builder::new()
                .prefix("managed-shutdown-")
                .tempdir_in(from.join(".lenso"))?,
            token: hex::encode(Sha256::digest(uuid::Uuid::now_v7().as_bytes())),
        })
    }

    pub(super) fn configure(&self, command: &mut Command) {
        command
            .env("LENSO_MANAGED_SHUTDOWN_TOKEN", &self.token)
            .env(
                "LENSO_MANAGED_SHUTDOWN_RECEIPT",
                self.directory.path().join("clean"),
            );
    }

    fn verify(&self) -> anyhow::Result<()> {
        let path = self.directory.path().join("clean");
        ensure!(
            fs::symlink_metadata(&path)?.file_type().is_file(),
            "managed shutdown receipt is not a regular file"
        );
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
        }
        let file = options.open(path)?;
        ensure!(
            file.metadata()?.is_file(),
            "managed shutdown receipt is not a regular file"
        );
        let mut bytes = Vec::new();
        file.take(65).read_to_end(&mut bytes)?;
        ensure!(
            bytes == self.token.as_bytes(),
            "managed shutdown receipt does not match this Generation"
        );
        Ok(())
    }
}

pub(super) async fn confirm(
    child: &mut Child,
    group_id: u32,
    receipt: Option<&Receipt>,
    deadline: Instant,
    signal: bool,
) -> anyhow::Result<()> {
    let receipt = receipt.context("this source or platform cannot confirm managed retirement")?;
    #[cfg(unix)]
    if signal && !exited_unreaped(group_id)? {
        use nix::{sys::signal, unistd::Pid};
        // Let the Host shut its adapters down. Signalling the entire group
        // first can kill a Guest before the adapter receives its acknowledgement.
        signal::kill(
            Pid::from_raw(i32::try_from(group_id)?),
            signal::Signal::SIGTERM,
        )?;
    }
    #[cfg(not(unix))]
    let _ = signal;
    let deadline = (Instant::now() + Duration::from_secs(10)).min(deadline);
    tokio::time::timeout_at(deadline, wait_for_exit_unreaped(child, group_id))
        .await
        .context("supervised Host did not stop cooperatively before its deadline")??;
    receipt.verify().context("confirm managed Host shutdown")?;
    #[cfg(unix)]
    ensure!(
        group_only_zombies(group_id)?,
        "supervised process group still has live members after shutdown"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
pub(super) fn group_only_zombies(group_id: u32) -> anyhow::Result<bool> {
    super::local_dev::darwin_group_only_zombies(group_id)
}

#[cfg(target_os = "linux")]
pub(super) fn group_only_zombies(group_id: u32) -> anyhow::Result<bool> {
    for entry in fs::read_dir("/proc").context("enumerate supervised process group")? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().parse::<u32>().is_err() {
            continue;
        }
        let stat = match fs::read_to_string(entry.path().join("stat")) {
            Ok(stat) => stat,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error).context("inspect supervised process-group member"),
        };
        let (_, fields) = stat
            .rsplit_once(") ")
            .context("invalid supervised process-group member status")?;
        let mut fields = fields.split_whitespace();
        let state = fields.next().context("missing process state")?;
        let _parent = fields.next().context("missing parent process ID")?;
        let member_group: u32 = fields.next().context("missing process-group ID")?.parse()?;
        if member_group == group_id && state != "Z" && state != "X" {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
pub(super) fn group_only_zombies(group_id: u32) -> anyhow::Result<bool> {
    use nix::{errno::Errno, sys::signal::kill, unistd::Pid};
    match kill(Pid::from_raw(-i32::try_from(group_id)?), None) {
        Err(Errno::ESRCH) => Ok(true),
        Ok(()) | Err(Errno::EPERM) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

#[cfg(unix)]
pub(super) async fn wait_for_exit_unreaped(
    _child: &mut Child,
    group_id: u32,
) -> anyhow::Result<()> {
    loop {
        if exited_unreaped(group_id)? {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[cfg(unix)]
pub(super) fn exited_unreaped(group_id: u32) -> anyhow::Result<bool> {
    use nix::libc;
    let id = i32::try_from(group_id)?;
    // Keep the group leader's PID reserved through group inspection.
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
pub(super) async fn wait_for_exit_unreaped(
    child: &mut Child,
    _group_id: u32,
) -> anyhow::Result<()> {
    child.wait().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt() -> (tempfile::TempDir, Receipt) {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".lenso")).unwrap();
        let receipt = Receipt::new(root.path()).unwrap();
        (root, receipt)
    }

    #[test]
    fn shutdown_requires_exact_generation_receipt() {
        let (_root, receipt) = receipt();
        let path = receipt.directory.path().join("clean");
        assert!(receipt.verify().is_err());
        fs::write(&path, receipt.token.as_bytes()).unwrap();
        receipt.verify().unwrap();
        for bytes in [
            String::new(),
            "a".repeat(64),
            format!("{}\n", receipt.token),
            "a".repeat(4096),
        ] {
            fs::write(&path, bytes).unwrap();
            assert!(receipt.verify().is_err());
        }
    }

    #[test]
    fn previous_generation_receipt_does_not_authorize_retirement() {
        let (root, first) = receipt();
        let second = Receipt::new(root.path()).unwrap();
        assert_ne!(first.token, second.token);
        fs::write(second.directory.path().join("clean"), &first.token).unwrap();
        assert!(second.verify().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn receipt_cannot_be_a_symlink() {
        let (_root, receipt) = receipt();
        let target = receipt.directory.path().join("other");
        fs::write(&target, &receipt.token).unwrap();
        std::os::unix::fs::symlink(target, receipt.directory.path().join("clean")).unwrap();
        assert!(receipt.verify().is_err());
    }
}
