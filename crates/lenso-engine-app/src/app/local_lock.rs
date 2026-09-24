use std::{fs, path::Path};

use anyhow::{Context, ensure};

#[derive(Debug)]
pub(super) struct LocalLock(fs::File);

impl Drop for LocalLock {
    fn drop(&mut self) {
        // A forked process can briefly inherit the open file description
        // before exec closes its descriptor. Unlock before closing ours so
        // the next App session does not inherit that transient lock window.
        let _ = self.0.unlock();
    }
}

pub(super) fn acquire(
    root: &Path,
    name: &str,
    occupied_message: &'static str,
) -> anyhow::Result<LocalLock> {
    let directory = root.join(".lenso");
    ensure!(
        fs::symlink_metadata(&directory)?.file_type().is_dir(),
        "App control directory must be a real directory"
    );
    let path = directory.join(name);
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
            ensure!(
                fs::symlink_metadata(&path)?.file_type().is_file(),
                "App session lock must be a regular file"
            );
            options.create_new(false).open(&path)?
        }
        Err(error) => return Err(error).context("open App session lock"),
    };
    ensure!(
        file.metadata()?.is_file() && fs::symlink_metadata(&path)?.file_type().is_file(),
        "App session lock must remain a regular file"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let opened = file.metadata()?;
        let current = fs::symlink_metadata(&path)?;
        ensure!(
            opened.dev() == current.dev() && opened.ino() == current.ino(),
            "App session lock path changed while opening"
        );
    }
    file.try_lock().context(occupied_message)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let opened = file.metadata()?;
        let current = fs::symlink_metadata(&path)?;
        ensure!(
            current.file_type().is_file()
                && opened.dev() == current.dev()
                && opened.ino() == current.ino(),
            "App session lock path changed after acquisition"
        );
    }
    Ok(LocalLock(file))
}
