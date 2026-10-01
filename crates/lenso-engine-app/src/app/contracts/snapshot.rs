use super::*;

pub(super) fn safe_output(root: &Path, relative: &Path) -> anyhow::Result<PathBuf> {
    safe_path(root, relative, false)
}
pub(super) fn safe_directory(root: &Path, relative: &Path) -> anyhow::Result<PathBuf> {
    safe_path(root, relative, true)
}
fn safe_path(root: &Path, relative: &Path, directory: bool) -> anyhow::Result<PathBuf> {
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        bail!(
            "contract output must stay inside its package: {}",
            relative.display()
        );
    }
    let mut path = root.to_path_buf();
    for component in relative.components() {
        path.push(component);
        if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            bail!(
                "contract output cannot traverse symlinks: {}",
                path.display()
            );
        }
    }
    if path.is_dir() && !directory {
        bail!("contract output is a directory: {}", path.display());
    }
    if path.is_file() && directory {
        bail!("contract root is a file: {}", path.display());
    }
    Ok(path)
}
pub(super) fn snapshot_files(descriptor: &Path) -> anyhow::Result<BTreeMap<PathBuf, Vec<u8>>> {
    snapshot_files_in(
        descriptor,
        &lenso_engine::discovery::DiscoverySession::new(
            descriptor.parent().context("descriptor parent")?,
        )?,
    )
}

pub(super) fn snapshot_files_in(
    descriptor: &Path,
    inputs: &lenso_engine::discovery::DiscoverySession,
) -> anyhow::Result<BTreeMap<PathBuf, Vec<u8>>> {
    let mut snapshot = lenso_engine::Snapshot::default();
    lenso_engine_contracts::snapshot_contract_in(
        &mut inputs.scope(descriptor.parent().context("descriptor parent")?)?,
        descriptor
            .file_name()
            .context("descriptor filename")?
            .to_str()
            .context("UTF8 descriptor")?,
        "capability.json",
        &mut snapshot,
    )?;
    Ok(snapshot
        .files()
        .iter()
        .map(|(path, bytes)| (PathBuf::from(path), bytes.clone()))
        .collect())
}
pub(super) fn snapshot(descriptor: &Path, destination: &Path) -> anyhow::Result<()> {
    snapshot_in(
        descriptor,
        destination,
        &lenso_engine::discovery::DiscoverySession::new(
            descriptor.parent().context("descriptor parent")?,
        )?,
    )
}

pub(super) fn snapshot_in(
    descriptor: &Path,
    destination: &Path,
    inputs: &lenso_engine::discovery::DiscoverySession,
) -> anyhow::Result<()> {
    let changes = snapshot_files_in(descriptor, inputs)?
        .into_iter()
        .map(|(relative, bytes)| (destination.join(relative), bytes))
        .collect();
    install(&changes)
}
