use std::{fs, io::Read as _, path::Path};

#[cfg(not(unix))]
use anyhow::bail;
use anyhow::{Context as _, ensure};
use lenso_plugin_catalog::Checkpoint;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

const MAX_CHECKPOINT_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredCheckpoint {
    schema_version: u32,
    checkpoint: Checkpoint,
}

fn file_name(catalog_id: &str) -> String {
    format!(
        "portable-{}.json",
        hex::encode(Sha256::digest(catalog_id.as_bytes()))
    )
}

fn decode(bytes: &[u8], catalog_id: &str) -> anyhow::Result<Checkpoint> {
    let stored: StoredCheckpoint = serde_json::from_slice(bytes)
        .context("signed Portable checkpoint is invalid; refusing to forget accepted history")?;
    ensure!(
        stored.schema_version == 1 && stored.checkpoint.catalog_id == catalog_id,
        "signed Portable checkpoint schema or catalog differs; refusing to forget accepted history"
    );
    Ok(stored.checkpoint)
}

#[cfg(unix)]
fn state_dir(root_lock: &fs::File, create: bool) -> anyhow::Result<Option<fs::File>> {
    use rustix::{
        fs::{Mode, OFlags, mkdirat, openat},
        io::Errno,
    };

    if create {
        match mkdirat(root_lock, ".lenso", Mode::RUSR | Mode::WUSR | Mode::XUSR) {
            Ok(()) => root_lock.sync_all()?,
            Err(Errno::EXIST) => {}
            Err(error) => {
                return Err(error).context("create App signed Portable checkpoint directory");
            }
        }
    }

    let descriptor = match openat(
        root_lock,
        ".lenso",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(descriptor) => descriptor,
        Err(Errno::NOENT) if !create => return Ok(None),
        Err(error) => return Err(error).context("open non-symlink App checkpoint directory"),
    };
    Ok(Some(fs::File::from(descriptor)))
}

#[cfg(unix)]
fn read_in_dir(dir: &fs::File, name: &str, catalog_id: &str) -> anyhow::Result<Option<Checkpoint>> {
    use rustix::{
        fs::{Mode, OFlags, openat},
        io::Errno,
    };
    use std::os::unix::fs::MetadataExt as _;

    let descriptor = match openat(
        dir,
        name,
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(descriptor) => descriptor,
        Err(Errno::NOENT) => return Ok(None),
        Err(error) => return Err(error).context("open non-symlink signed Portable checkpoint"),
    };
    let mut file = fs::File::from(descriptor);
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.nlink() == 1,
        "signed Portable checkpoint must be a single-link regular file"
    );
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_CHECKPOINT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_CHECKPOINT_BYTES,
        "signed Portable checkpoint exceeds size limit"
    );
    Ok(Some(decode(&bytes, catalog_id)?))
}

#[cfg(unix)]
pub(super) fn read(
    _root: &Path,
    root_lock: &fs::File,
    catalog_id: &str,
) -> anyhow::Result<Option<Checkpoint>> {
    let Some(dir) = state_dir(root_lock, false)? else {
        return Ok(None);
    };
    read_in_dir(&dir, &file_name(catalog_id), catalog_id)
}

#[cfg(unix)]
pub(super) fn persist(
    _root: &Path,
    root_lock: &fs::File,
    checkpoint: &Checkpoint,
    previous: Option<&Checkpoint>,
) -> anyhow::Result<()> {
    use rustix::fs::{AtFlags, Mode, OFlags, openat, renameat, unlinkat};
    use std::io::Write as _;

    let dir = state_dir(root_lock, true)?
        .context("App signed Portable checkpoint directory is missing")?;
    let name = file_name(&checkpoint.catalog_id);
    ensure!(
        read_in_dir(&dir, &name, &checkpoint.catalog_id)?.as_ref() == previous,
        "signed Portable checkpoint changed during verification"
    );
    if previous == Some(checkpoint) {
        return Ok(());
    }
    let bytes = serde_json::to_vec(&StoredCheckpoint {
        schema_version: 1,
        checkpoint: checkpoint.clone(),
    })?;
    ensure!(
        bytes.len() as u64 <= MAX_CHECKPOINT_BYTES,
        "signed Portable checkpoint exceeds size limit"
    );
    let temporary = format!(".{name}.{}.tmp", uuid::Uuid::now_v7());
    let descriptor = openat(
        &dir,
        temporary.as_str(),
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    )?;
    let mut file = fs::File::from(descriptor);
    let result = (|| {
        file.write_all(&bytes)?;
        file.sync_all()?;
        ensure!(
            read_in_dir(&dir, &name, &checkpoint.catalog_id)?.as_ref() == previous,
            "signed Portable checkpoint changed during publication"
        );
        renameat(&dir, temporary.as_str(), &dir, name.as_str())?;
        dir.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = unlinkat(&dir, temporary.as_str(), AtFlags::empty());
    }
    result
}

#[cfg(not(unix))]
pub(super) fn read(
    _root: &Path,
    _root_lock: &fs::File,
    _catalog_id: &str,
) -> anyhow::Result<Option<Checkpoint>> {
    bail!("durable signed Portable checkpoints are unsupported on this platform")
}

#[cfg(not(unix))]
pub(super) fn persist(
    _root: &Path,
    _root_lock: &fs::File,
    _checkpoint: &Checkpoint,
    _previous: Option<&Checkpoint>,
) -> anyhow::Result<()> {
    bail!("durable signed Portable checkpoints are unsupported on this platform")
}
