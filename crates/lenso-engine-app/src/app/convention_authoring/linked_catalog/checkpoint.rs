use std::{fs, io::Read as _, path::Path};

#[cfg(not(any(unix, windows)))]
use anyhow::bail;
use anyhow::{Context as _, ensure};
use lenso_plugin_catalog::linked_cargo::LinkedCargoCheckpoint;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

const MAX_CHECKPOINT_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredCheckpoint {
    schema_version: u32,
    checkpoint: LinkedCargoCheckpoint,
}

fn file_name(catalog_id: &str) -> String {
    format!(
        "linked-cargo-{}.json",
        hex::encode(Sha256::digest(catalog_id.as_bytes()))
    )
}

fn decode(bytes: &[u8], catalog_id: &str) -> anyhow::Result<LinkedCargoCheckpoint> {
    let stored: StoredCheckpoint = serde_json::from_slice(bytes)
        .context("linked Cargo checkpoint is invalid; refusing to forget accepted history")?;
    ensure!(
        stored.schema_version == 1 && stored.checkpoint.catalog_id == catalog_id,
        "linked Cargo checkpoint schema or catalog differs; refusing to forget accepted history"
    );
    Ok(stored.checkpoint)
}

fn encode(checkpoint: &LinkedCargoCheckpoint) -> anyhow::Result<Vec<u8>> {
    let bytes = serde_json::to_vec(&StoredCheckpoint {
        schema_version: 1,
        checkpoint: checkpoint.clone(),
    })?;
    ensure!(
        u64::try_from(bytes.len())? <= MAX_CHECKPOINT_BYTES,
        "linked Cargo checkpoint exceeds size limit"
    );
    Ok(bytes)
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
                return Err(error).context("create App-owned catalog checkpoint directory");
            }
        }
    }
    match openat(
        root_lock,
        ".lenso",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(descriptor) => Ok(Some(fs::File::from(descriptor))),
        Err(Errno::NOENT) if !create => Ok(None),
        Err(error) => Err(error).context("open non-symlink App catalog checkpoint directory"),
    }
}

#[cfg(unix)]
fn read_in_dir(
    dir: &fs::File,
    name: &str,
    catalog_id: &str,
) -> anyhow::Result<Option<LinkedCargoCheckpoint>> {
    use std::os::unix::fs::MetadataExt as _;

    use rustix::{
        fs::{Mode, OFlags, openat},
        io::Errno,
    };

    let descriptor = match openat(
        dir,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(descriptor) => descriptor,
        Err(Errno::NOENT) => return Ok(None),
        Err(error) => return Err(error).context("open non-symlink linked Cargo checkpoint"),
    };
    let mut file = fs::File::from(descriptor);
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.nlink() == 1,
        "linked Cargo checkpoint must be a single-link regular file"
    );
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_CHECKPOINT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        u64::try_from(bytes.len())? <= MAX_CHECKPOINT_BYTES,
        "linked Cargo checkpoint exceeds size limit"
    );
    Ok(Some(decode(&bytes, catalog_id)?))
}

#[cfg(unix)]
pub(super) fn read(
    _root: &Path,
    root_lock: &fs::File,
    catalog_id: &str,
) -> anyhow::Result<Option<LinkedCargoCheckpoint>> {
    let Some(dir) = state_dir(root_lock, false)? else {
        return Ok(None);
    };
    read_in_dir(&dir, &file_name(catalog_id), catalog_id)
}

#[cfg(unix)]
pub(super) fn persist(
    _root: &Path,
    root_lock: &fs::File,
    checkpoint: &LinkedCargoCheckpoint,
    previous: Option<&LinkedCargoCheckpoint>,
) -> anyhow::Result<()> {
    use std::io::Write as _;

    use rustix::fs::{AtFlags, Mode, OFlags, openat, renameat, unlinkat};

    let dir = state_dir(root_lock, true)?.context("App catalog checkpoint directory is missing")?;
    let name = file_name(&checkpoint.catalog_id);
    let observed = read_in_dir(&dir, &name, &checkpoint.catalog_id)?;
    ensure!(
        observed.as_ref() == previous,
        "linked Cargo checkpoint changed during verification; retry with the latest App state"
    );
    if observed.as_ref() == Some(checkpoint) {
        return Ok(());
    }
    let bytes = encode(checkpoint)?;
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
            "linked Cargo checkpoint changed during publication"
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

#[cfg(windows)]
fn state_path(root: &Path, name: &str, create: bool) -> anyhow::Result<Option<std::path::PathBuf>> {
    let relative = Path::new(".lenso");
    super::super::writable_path(root, relative)?;
    let dir = root.join(relative);
    if create {
        match fs::create_dir(&dir) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    match fs::symlink_metadata(&dir) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => anyhow::bail!("App catalog checkpoint directory must not be a symlink"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !create => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let relative = relative.join(name);
    super::super::writable_path(root, &relative)?;
    Ok(Some(root.join(relative)))
}

#[cfg(windows)]
pub(super) fn read(
    root: &Path,
    _root_lock: &fs::File,
    catalog_id: &str,
) -> anyhow::Result<Option<LinkedCargoCheckpoint>> {
    let Some(path) = state_path(root, &file_name(catalog_id), false)? else {
        return Ok(None);
    };
    let Some(bytes) = super::adoption::read_optional_regular(&path)? else {
        return Ok(None);
    };
    ensure!(
        u64::try_from(bytes.len())? <= MAX_CHECKPOINT_BYTES,
        "linked Cargo checkpoint exceeds size limit"
    );
    Ok(Some(decode(&bytes, catalog_id)?))
}

#[cfg(windows)]
pub(super) fn persist(
    root: &Path,
    root_lock: &fs::File,
    checkpoint: &LinkedCargoCheckpoint,
    previous: Option<&LinkedCargoCheckpoint>,
) -> anyhow::Result<()> {
    use std::io::Write as _;

    let name = file_name(&checkpoint.catalog_id);
    let path =
        state_path(root, &name, true)?.context("App catalog checkpoint directory is missing")?;
    ensure!(
        read(root, root_lock, &checkpoint.catalog_id)?.as_ref() == previous,
        "linked Cargo checkpoint changed during verification"
    );
    if previous == Some(checkpoint) {
        return Ok(());
    }
    let mut stage = tempfile::NamedTempFile::new_in(path.parent().context("checkpoint parent")?)?;
    stage.write_all(&encode(checkpoint)?)?;
    stage.as_file().sync_all()?;
    ensure!(
        read(root, root_lock, &checkpoint.catalog_id)?.as_ref() == previous,
        "linked Cargo checkpoint changed during publication"
    );
    stage.persist(&path)?;
    Ok(())
}

#[cfg(not(any(unix, windows)))]
pub(super) fn read(
    _root: &Path,
    _root_lock: &fs::File,
    _catalog_id: &str,
) -> anyhow::Result<Option<LinkedCargoCheckpoint>> {
    bail!("durable linked Cargo checkpoints are unsupported on this platform")
}

#[cfg(not(any(unix, windows)))]
pub(super) fn persist(
    _root: &Path,
    _root_lock: &fs::File,
    _checkpoint: &LinkedCargoCheckpoint,
    _previous: Option<&LinkedCargoCheckpoint>,
) -> anyhow::Result<()> {
    bail!("durable linked Cargo checkpoints are unsupported on this platform")
}
