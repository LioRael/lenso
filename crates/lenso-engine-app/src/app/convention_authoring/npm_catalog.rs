//! Adopt an exact signed npm distribution as a Bun source candidate.
//! Package installation never runs lifecycle scripts; building its Plugin code
//! still needs an explicit operator trust declaration for the archive digest.

use std::{
    collections::BTreeSet,
    fs,
    io::{Cursor, Read as _, Write as _},
    path::{Component, Path, PathBuf},
};

use anyhow::{Context as _, bail, ensure};
use flate2::bufread::GzDecoder;
use lenso_app_authoring::discovery::Candidate;
use lenso_plugin_catalog::{Distribution, DistributionKind, digest, package};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::{AddArgs, linked_catalog};

const SOURCE_LOCK: &str = ".lenso-npm-source.json";
const SOURCE_ARCHIVE: &str = ".lenso-npm-archive.tgz";
const MAX_ARCHIVE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_UNPACKED_BYTES: u64 = 128 * 1024 * 1024;
const MAX_FILES: usize = 4096;
const MAX_CHECKPOINT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TAR_PADDING_BYTES: u64 = 1024 * 1024;
const MAX_TAR_BYTES: u64 = MAX_UNPACKED_BYTES + (MAX_FILES as u64) * 1024 + MAX_TAR_PADDING_BYTES;

#[cfg(unix)]
fn file_mode(metadata: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt as _;
    metadata.permissions().mode() & 0o777
}

#[cfg(not(unix))]
fn file_mode(metadata: &fs::Metadata) -> u32 {
    u32::from(metadata.permissions().readonly())
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct SourceLock {
    schema_version: u32,
    plugin_id: String,
    release_version: String,
    package: String,
    package_version: String,
    distribution_id: String,
    registry_url: String,
    archive_digest: String,
    source_digest: String,
    dependency_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    linked_base: Option<LinkedBaseLock>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct LinkedBaseLock {
    catalog_id: String,
    release_identity: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredCheckpoint {
    schema_version: u32,
    checkpoint: package::PackageCheckpoint,
}

fn checkpoint_path(root: &Path, catalog_id: &str) -> PathBuf {
    root.join(".lenso").join(format!(
        "npm-package-{}.json",
        hex::encode(Sha256::digest(catalog_id.as_bytes()))
    ))
}

fn read_checkpoint(
    root: &Path,
    catalog_id: &str,
) -> anyhow::Result<Option<package::PackageCheckpoint>> {
    let path = checkpoint_path(root, catalog_id);
    super::writable_path(root, path.strip_prefix(root)?)?;
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        metadata.file_type().is_file() && metadata.len() <= MAX_CHECKPOINT_BYTES,
        "npm package checkpoint must be a bounded regular file"
    );
    let stored: StoredCheckpoint = serde_json::from_slice(&fs::read(&path)?)
        .context("npm package checkpoint is invalid; refusing to forget accepted history")?;
    ensure!(
        stored.schema_version == 1 && stored.checkpoint.catalog_id == catalog_id,
        "npm package checkpoint schema or catalog changed"
    );
    Ok(Some(stored.checkpoint))
}

fn persist_checkpoint(
    root: &Path,
    checkpoint: &package::PackageCheckpoint,
    previous: Option<&package::PackageCheckpoint>,
) -> anyhow::Result<()> {
    let path = checkpoint_path(root, &checkpoint.catalog_id);
    super::writable_path(root, path.strip_prefix(root)?)?;
    fs::create_dir_all(path.parent().context("npm checkpoint parent")?)?;
    ensure!(
        read_checkpoint(root, &checkpoint.catalog_id)?.as_ref() == previous,
        "npm package checkpoint changed during verification"
    );
    if previous == Some(checkpoint) {
        return Ok(());
    }
    let bytes = serde_json::to_vec(&StoredCheckpoint {
        schema_version: 1,
        checkpoint: checkpoint.clone(),
    })?;
    ensure!(
        u64::try_from(bytes.len())? <= MAX_CHECKPOINT_BYTES,
        "npm package checkpoint exceeds size limit"
    );
    let mut staged = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    staged.write_all(&bytes)?;
    staged.as_file().sync_all()?;
    ensure!(
        read_checkpoint(root, &checkpoint.catalog_id)?.as_ref() == previous,
        "npm package checkpoint changed during publication"
    );
    if previous.is_some() {
        staged.persist(&path)?;
    } else {
        staged.persist_noclobber(&path)?;
    }
    fs::File::open(path.parent().unwrap())?.sync_all()?;
    Ok(())
}

fn read_archive(path: &Path) -> anyhow::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.file_type().is_file() && metadata.len() <= MAX_ARCHIVE_BYTES,
        "npm archive must be a bounded regular file"
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_ARCHIVE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        !bytes.is_empty() && u64::try_from(bytes.len())? <= MAX_ARCHIVE_BYTES,
        "npm archive exceeds size limit"
    );
    Ok(bytes)
}

fn unpack_archive(bytes: &[u8], stage: &Path) -> anyhow::Result<()> {
    let mut decoder = GzDecoder::new(Cursor::new(bytes));
    let mut tar_bytes = Vec::new();
    decoder
        .by_ref()
        .take(MAX_TAR_BYTES + 1)
        .read_to_end(&mut tar_bytes)?;
    ensure!(
        u64::try_from(tar_bytes.len())? <= MAX_TAR_BYTES,
        "npm archive exceeds total decompressed tar size limit"
    );
    ensure!(
        decoder.into_inner().position() == bytes.len() as u64,
        "npm archive has trailing compressed data"
    );
    let mut archive = tar::Archive::new(tar_bytes.as_slice());
    let mut files = 0;
    let mut total = 0_u64;
    let mut seen = BTreeSet::new();
    for entry in archive.entries().context("invalid npm archive")? {
        let mut entry = entry.context("invalid npm archive entry")?;
        files += 1;
        ensure!(files <= MAX_FILES, "npm archive has too many entries");
        total = total
            .checked_add(entry.size())
            .context("npm archive size overflow")?;
        ensure!(
            total <= MAX_UNPACKED_BYTES,
            "npm archive exceeds uncompressed size limit"
        );
        let path = std::str::from_utf8(&entry.path_bytes())?.to_owned();
        let kind = entry.header().entry_type();
        if path == "package" || path == "package/" {
            ensure!(kind.is_dir(), "npm package root must be a directory");
            continue;
        }
        let relative = path
            .strip_prefix("package/")
            .context("npm archive entry is outside package/")?;
        let relative = if kind.is_dir() {
            relative.strip_suffix('/').unwrap_or(relative)
        } else {
            relative
        };
        ensure!(
            !relative.is_empty()
                && !relative.contains('\\')
                && !relative.chars().any(char::is_control)
                && relative
                    .split('/')
                    .all(|part| !matches!(part, "" | "." | ".."))
                && relative != SOURCE_LOCK
                && relative != SOURCE_ARCHIVE
                && !relative.split('/').any(|part| part == "node_modules"),
            "npm archive has an invalid or reserved path"
        );
        ensure!(
            seen.insert(relative.to_owned()),
            "npm archive repeats an entry path"
        );
        let destination = stage.join(relative);
        if kind.is_dir() {
            create_archive_directory(stage, &destination)?;
            continue;
        }
        ensure!(kind.is_file(), "npm archive has a non-file entry");
        create_archive_directory(stage, destination.parent().context("npm file parent")?)?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)?;
        ensure!(
            std::io::copy(&mut entry, &mut output)? == entry.size(),
            "npm archive entry is truncated"
        );
        output.flush()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = if entry.header().mode()? & 0o111 == 0 {
                0o644
            } else {
                0o755
            };
            fs::set_permissions(&destination, fs::Permissions::from_mode(mode))?;
        }
    }
    ensure!(files > 0, "npm archive is empty");
    let padding = archive.into_inner();
    ensure!(
        u64::try_from(padding.len())? <= MAX_TAR_PADDING_BYTES
            && padding.iter().all(|byte| *byte == 0),
        "npm archive has nonzero or excessive tar data after end marker"
    );
    Ok(())
}

fn create_archive_directory(stage: &Path, destination: &Path) -> anyhow::Result<()> {
    let mut current = stage.to_path_buf();
    for component in destination.strip_prefix(stage)?.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) => ensure!(
                metadata.file_type().is_dir(),
                "npm archive directory conflicts with a non-directory"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current)?;
            }
            Err(error) => return Err(error.into()),
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&current, fs::Permissions::from_mode(0o755))?;
        }
    }
    Ok(())
}

fn source_digest(root: &Path) -> anyhow::Result<String> {
    let mut pending = vec![root.to_path_buf()];
    let mut paths = Vec::new();
    let mut total = 0_u64;
    while let Some(path) = pending.pop() {
        if path == root.join("node_modules") {
            ensure!(
                fs::symlink_metadata(&path)?.file_type().is_dir(),
                "npm node_modules must be a directory"
            );
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_dir() {
            paths.push((path.clone(), b'd'));
            for entry in fs::read_dir(&path)? {
                pending.push(entry?.path());
            }
        } else if metadata.file_type().is_file() {
            if path == root.join(SOURCE_LOCK) || path == root.join(SOURCE_ARCHIVE) {
                continue;
            }
            total = total
                .checked_add(metadata.len())
                .context("npm source size overflow")?;
            ensure!(total <= MAX_UNPACKED_BYTES, "npm source exceeds size limit");
            paths.push((path, b'f'));
        } else {
            bail!("npm source contains a symlink or special file");
        }
        ensure!(
            pending.len() + paths.len() <= MAX_FILES + 1,
            "npm source has too many entries"
        );
    }
    paths.sort_by(|left, right| left.0.cmp(&right.0));
    let mut hasher = Sha256::new();
    for (path, kind) in paths {
        let relative = path
            .strip_prefix(root)?
            .to_str()
            .context("npm source path UTF-8")?;
        let bytes = if kind == b'f' {
            fs::read(&path)?
        } else {
            Vec::new()
        };
        hasher.update([kind]);
        hasher.update(file_mode(&fs::symlink_metadata(&path)?).to_be_bytes());
        hasher.update((relative.len() as u64).to_be_bytes());
        hasher.update(relative.as_bytes());
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }
    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}

fn dependency_digest(root: &Path) -> anyhow::Result<String> {
    let dependencies = root.join("node_modules");
    let metadata = match fs::symlink_metadata(&dependencies) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok("none".into()),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        metadata.file_type().is_dir(),
        "npm node_modules must be a directory"
    );
    let mut pending = vec![dependencies.clone()];
    let mut paths = Vec::new();
    let mut total = 0_u64;
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_dir() {
            paths.push((path.clone(), b'd'));
            for entry in fs::read_dir(&path)? {
                pending.push(entry?.path());
            }
        } else if metadata.file_type().is_file() {
            total = total
                .checked_add(metadata.len())
                .context("npm dependency size overflow")?;
            ensure!(
                total <= 512 * 1024 * 1024,
                "npm dependencies exceed size limit"
            );
            paths.push((path, b'f'));
        } else if metadata.file_type().is_symlink() {
            ensure!(
                fs::canonicalize(&path)?.starts_with(&dependencies),
                "npm dependency symlink escapes node_modules"
            );
            paths.push((path, b'l'));
        } else {
            bail!("npm dependencies contain a special file");
        }
        ensure!(
            pending.len() + paths.len() <= 100_000,
            "npm dependencies have too many entries"
        );
    }
    paths.sort_by(|left, right| left.0.cmp(&right.0));
    let mut hasher = Sha256::new();
    for (path, kind) in paths {
        let relative = path
            .strip_prefix(&dependencies)?
            .to_str()
            .context("npm dependency path UTF-8")?;
        let bytes = match kind {
            b'f' => fs::read(&path)?,
            b'l' => fs::read_link(&path)?
                .to_str()
                .context("npm dependency link UTF-8")?
                .as_bytes()
                .to_vec(),
            _ => Vec::new(),
        };
        hasher.update([kind]);
        if kind != b'l' {
            hasher.update(file_mode(&fs::symlink_metadata(&path)?).to_be_bytes());
        }
        hasher.update((relative.len() as u64).to_be_bytes());
        hasher.update(relative.as_bytes());
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }
    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}

fn build_digest(lock: &SourceLock) -> String {
    let mut hasher = Sha256::new();
    for value in [
        if lock.linked_base.is_some() {
            "lenso/npm-build/v3"
        } else {
            "lenso/npm-build/v2"
        },
        &lock.plugin_id,
        &lock.release_version,
        &lock.package,
        &lock.package_version,
        &lock.distribution_id,
        &lock.registry_url,
        &lock.archive_digest,
        &lock.source_digest,
        &lock.dependency_digest,
    ] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    if let Some(base) = &lock.linked_base {
        for value in [&base.catalog_id, &base.release_identity] {
            hasher.update((value.len() as u64).to_be_bytes());
            hasher.update(value.as_bytes());
        }
    }
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn read_lock(root: &Path) -> anyhow::Result<SourceLock> {
    let path = root.join(SOURCE_LOCK);
    let metadata = fs::symlink_metadata(&path)?;
    ensure!(
        metadata.file_type().is_file() && metadata.len() <= 4096,
        "npm source lock must be a bounded regular file"
    );
    let lock: SourceLock = serde_json::from_slice(&fs::read(path)?)?;
    ensure!(lock.schema_version == 1, "unsupported npm source lock");
    Ok(lock)
}

fn verify_manifest_identity(root: &Path, lock: &SourceLock) -> anyhow::Result<()> {
    let path = root.join("package.json");
    let metadata = fs::symlink_metadata(&path)?;
    ensure!(
        metadata.file_type().is_file() && metadata.len() <= 1024 * 1024,
        "adopted npm package.json must be a bounded regular file"
    );
    let manifest: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
    ensure!(
        manifest["name"].as_str() == Some(lock.package.as_str())
            && manifest["version"].as_str() == Some(lock.package_version.as_str())
            && manifest["lenso"]["pluginId"].as_str() == Some(lock.plugin_id.as_str())
            && manifest["lenso"]["releaseVersion"].as_str() == Some(lock.release_version.as_str())
            && manifest["lenso"]["runtime"].as_str() == Some("bun"),
        "adopted npm manifest differs from its signed source lock"
    );
    Ok(())
}

fn native_target_matches(targets: &[String]) -> bool {
    let target = lenso_app_authoring::native_host_target();
    targets.is_empty() || targets.iter().any(|item| item == "*" || item == target)
}

struct SelectedPackage {
    verified: package::VerifiedPackageSnapshot,
    previous: Option<package::PackageCheckpoint>,
    distribution: Distribution,
}

fn select_package(
    root: &Path,
    snapshot: &Path,
    trust_path: &Path,
    plugin_id: &str,
    release_version: &str,
    distribution_id: Option<&str>,
) -> anyhow::Result<SelectedPackage> {
    let trust = linked_catalog::read_trust(trust_path)?;
    let previous = read_checkpoint(root, &trust.catalog_id)?;
    let now = linked_catalog::now()?;
    let verified = package::verify(
        &linked_catalog::read_envelope(snapshot)?,
        &trust,
        previous.as_ref(),
        now,
    )?;
    let release = verified
        .snapshot()
        .releases
        .iter()
        .find(|release| release.plugin_id == plugin_id && release.version == release_version)
        .context("exact npm Plugin release is not in signed snapshot")?;
    let selected = release
        .distributions
        .iter()
        .filter(|distribution| {
            distribution.kind == DistributionKind::NpmPackage
                && distribution_id.is_none_or(|id| distribution.id == id)
                && native_target_matches(&distribution.targets)
        })
        .collect::<Vec<_>>();
    let [distribution] = selected.as_slice() else {
        bail!(
            "select exactly one signed npm distribution for Host target {}; use --distribution when needed",
            lenso_app_authoring::native_host_target()
        );
    };
    verified.select_npm(plugin_id, release_version, &distribution.id, now)?;
    let distribution = (*distribution).clone();
    Ok(SelectedPackage {
        verified,
        previous,
        distribution,
    })
}

pub(in crate::app) fn preview(
    root: &Path,
    snapshot: &Path,
    trust: &Path,
    tgz: &Path,
    plugin_id: &str,
    release_version: &str,
) -> anyhow::Result<serde_json::Value> {
    lenso_app_authoring::identity::validate_plugin_id_v1(plugin_id)?;
    lenso_app_authoring::identity::validate_release_version(release_version)?;
    let selected = select_package(root, snapshot, trust, plugin_id, release_version, None)?;
    let archive_digest = digest(&read_archive(tgz)?);
    ensure!(
        selected.distribution.integrity.as_deref() == Some(archive_digest.as_str()),
        "npm archive digest does not match signed distribution"
    );
    super::preflight_source_adoption(root, plugin_id)?;
    let expected_source = root
        .join("vendor/lenso/npm")
        .join(plugin_id)
        .join(release_version);
    let app_selection = match selected_source(root, plugin_id)? {
        None => "not_selected",
        Some(candidate)
            if candidate.project == expected_source
                && candidate.release_version == release_version =>
        {
            "exact_version_selected"
        }
        Some(_) => "different_source_selected",
    };
    Ok(serde_json::json!({
        "schema_version": 1,
        "kind": "lenso.signed-npm-adoption-preview",
        "plugin_id": plugin_id,
        "version": release_version,
        "catalog_id": selected.verified.snapshot().catalog_id,
        "catalog_revision": selected.verified.snapshot().revision,
        "catalog_payload_digest": selected.verified.checkpoint().payload_digest,
        "distribution_id": selected.distribution.id,
        "package": selected.distribution.package,
        "package_version": selected.distribution.version,
        "archive_digest": archive_digest,
        "host_target": lenso_app_authoring::native_host_target(),
        "signed_metadata": "verified",
        "archive_bytes": "digest_verified",
        "source_manifest": "not_verified",
        "dependencies": "not_installed",
        "app_selection": app_selection,
        "build_trust": "required_separately",
        "activation": "not_observed"
    }))
}

pub(super) fn add(root: &Path, args: &AddArgs) -> anyhow::Result<()> {
    let package_only = args.package_snapshot.is_some()
        && args.linked_snapshot.is_none()
        && args.release_details.is_none();
    let linked_details = args.package_snapshot.is_none()
        && args.linked_snapshot.is_some()
        && args.release_details.is_some();
    ensure!(
        (package_only || linked_details)
            && args.tgz.is_some()
            && args.trust.is_some()
            && args.portable_snapshot.is_none()
            && args.crate_archive.is_none()
            && args.bundle.is_none()
            && args.archive.is_none()
            && args.origin.is_none()
            && args.content_snapshot.is_none()
            && args.content_id.is_none()
            && args.content_archive.is_none()
            && args.content_destination.is_none()
            && !args.content_preview,
        "npm adoption needs --trust and --tgz with either --package-snapshot or both --linked-snapshot and --release-details"
    );
    let (plugin_id, release_version) = args
        .source
        .split_once('@')
        .context("npm source must be exact PLUGIN_ID@RELEASE_VERSION")?;
    let app_lock = linked_catalog::adoption::lock_app(root)?;
    let selected_package = if package_only {
        Some(select_package(
            root,
            args.package_snapshot.as_deref().unwrap(),
            args.trust.as_deref().unwrap(),
            plugin_id,
            release_version,
            args.distribution.as_deref(),
        )?)
    } else {
        None
    };
    let selected_linked = if linked_details {
        Some(linked_catalog::select_linked_npm_details(
            root,
            &app_lock,
            args,
            plugin_id,
            release_version,
        )?)
    } else {
        None
    };
    let distribution = selected_package
        .as_ref()
        .map(|selected| &selected.distribution)
        .or_else(|| {
            selected_linked
                .as_ref()
                .map(|selected| &selected.distribution)
        })
        .context("signed npm distribution is missing")?;
    let bytes = read_archive(args.tgz.as_deref().unwrap())?;
    let archive_digest = digest(&bytes);
    ensure!(
        distribution.integrity.as_deref() == Some(archive_digest.as_str()),
        "npm archive digest does not match signed distribution"
    );
    let parent = root.join("vendor/lenso/npm").join(plugin_id);
    let destination = parent.join(release_version);
    super::writable_path(root, destination.strip_prefix(root)?)?;
    let stage = tempfile::Builder::new()
        .prefix(".npm-source-")
        .tempdir_in(root)?;
    unpack_archive(&bytes, stage.path())?;
    let manifest_path = stage.path().join("package.json");
    let manifest_meta = fs::symlink_metadata(&manifest_path)?;
    ensure!(
        manifest_meta.file_type().is_file() && manifest_meta.len() <= 1024 * 1024,
        "npm package.json must be a bounded regular file"
    );
    let manifest: serde_json::Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    ensure!(
        manifest["name"].as_str() == Some(distribution.package.as_str())
            && manifest["version"].as_str() == Some(distribution.version.as_str())
            && manifest["lenso"]["pluginId"].as_str() == Some(plugin_id)
            && manifest["lenso"]["releaseVersion"].as_str() == Some(release_version)
            && manifest["lenso"]["runtime"].as_str() == Some("bun"),
        "npm package identity or Bun Plugin declaration differs from signed distribution"
    );
    ensure!(
        fs::symlink_metadata(stage.path().join("bun.lock"))?
            .file_type()
            .is_file(),
        "signed npm Plugin needs a root bun.lock for frozen dependency installation"
    );
    let report = lenso_app_authoring::discovery::discover(stage.path())?;
    let [candidate] = report.candidates.as_slice() else {
        bail!("npm archive must contain exactly one Bun Plugin source");
    };
    ensure!(
        candidate.plugin_id == plugin_id
            && candidate.release_version == release_version
            && candidate.format == "bun",
        "npm Plugin logical release identity differs from signed snapshot"
    );
    if let Some(selected) = &selected_package {
        persist_checkpoint(
            root,
            selected.verified.checkpoint(),
            selected.previous.as_ref(),
        )?;
    }
    if let Some(selected) = &selected_linked {
        selected.persist(root, &app_lock)?;
    }
    let expected_source_digest = source_digest(stage.path())?;
    if !args.no_install {
        let install_home = tempfile::tempdir_in(root)?;
        let status = super::super::build_command("bun")
            .args([
                "install",
                "--ignore-scripts",
                "--frozen-lockfile",
                "--backend=copyfile",
                "--linker=hoisted",
            ])
            .env("HOME", install_home.path())
            .env(
                "BUN_INSTALL_CACHE_DIR",
                install_home.path().join("bun-cache"),
            )
            .current_dir(stage.path())
            .status()?;
        ensure!(
            status.success(),
            "frozen Bun dependency installation failed"
        );
        ensure!(
            source_digest(stage.path())? == expected_source_digest,
            "Bun installation changed signed npm source or bun.lock"
        );
    }
    let lock = SourceLock {
        schema_version: 1,
        plugin_id: plugin_id.to_owned(),
        release_version: release_version.to_owned(),
        package: distribution.package.clone(),
        package_version: distribution.version.clone(),
        distribution_id: distribution.id.clone(),
        registry_url: distribution
            .registry_url
            .clone()
            .context("npm registry URL")?,
        archive_digest,
        source_digest: expected_source_digest,
        dependency_digest: dependency_digest(stage.path())?,
        linked_base: selected_linked.as_ref().map(|selected| LinkedBaseLock {
            catalog_id: selected.catalog_id.clone(),
            release_identity: selected.base_release_identity.clone(),
        }),
    };
    fs::write(stage.path().join(SOURCE_ARCHIVE), &bytes)?;
    fs::write(
        stage.path().join(SOURCE_LOCK),
        serde_json::to_vec_pretty(&lock)?,
    )?;
    commit_source(
        root,
        &destination,
        candidate,
        &lock,
        stage.path(),
        args.replace,
        app_lock,
    )?;
    if args.no_install {
        println!(
            "Adopted {}@{} from npm {}@{} at {} without dependencies; run `bun install --ignore-scripts --frozen-lockfile --backend=copyfile --linker=hoisted` there before approving the build digest",
            plugin_id,
            release_version,
            lock.package,
            lock.package_version,
            destination.display()
        );
    } else {
        println!(
            "Adopted {}@{} from npm {}@{} at {}; approve build code with --trust-adopted-build '{}@{}={}'",
            plugin_id,
            release_version,
            lock.package,
            lock.package_version,
            destination.display(),
            plugin_id,
            release_version,
            build_digest(&lock)
        );
    }
    Ok(())
}

fn selected_source(root: &Path, plugin_id: &str) -> anyhow::Result<Option<Candidate>> {
    let report = lenso_app_authoring::discovery::discover(root)?;
    Ok(report
        .candidates
        .into_iter()
        .find(|candidate| candidate.plugin_id == plugin_id))
}

fn commit_source(
    root: &Path,
    destination: &Path,
    candidate: &Candidate,
    lock: &SourceLock,
    stage: &Path,
    replace: bool,
    _app_lock: fs::File,
) -> anyhow::Result<()> {
    let selected = selected_source(root, &lock.plugin_id)?;
    let previous = if let Some(selected) = selected {
        if selected.project == destination && selected.release_version == lock.release_version {
            None
        } else {
            ensure!(
                replace,
                "App already selects this Plugin; use --replace with exact signed inputs"
            );
            let prior = root
                .join("vendor/lenso/npm")
                .join(&lock.plugin_id)
                .join(&selected.release_version);
            ensure!(
                selected.project == prior,
                "cannot replace a Plugin selected from a different source"
            );
            verify_source(&prior, &selected)?;
            Some(prior)
        }
    } else {
        ensure!(
            !replace,
            "--replace requires an existing selected npm Plugin"
        );
        None
    };
    ensure!(
        candidate.plugin_id == lock.plugin_id && candidate.release_version == lock.release_version,
        "staged npm Plugin identity changed"
    );
    super::preflight_source_adoption(root, &lock.plugin_id)?;
    let config = root.join("lenso.toml");
    let before = linked_catalog::adoption::read_optional_regular(&config)?;
    let mut document: toml::Value = match &before {
        Some(bytes) => toml::from_str(std::str::from_utf8(bytes)?)?,
        None => toml::Value::Table(Default::default()),
    };
    let sources = document
        .as_table_mut()
        .context("lenso.toml table")?
        .entry("plugin_sources")
        .or_insert_with(|| toml::Value::Array(vec![]))
        .as_array_mut()
        .context("lenso.toml plugin_sources array")?;
    let relative = destination
        .strip_prefix(root)?
        .to_str()
        .context("npm source path UTF-8")?;
    if let Some(previous) = &previous {
        let prior = previous
            .strip_prefix(root)?
            .to_str()
            .context("prior npm source path UTF-8")?;
        ensure!(
            sources
                .iter()
                .filter(|value| value.as_str() == Some(prior))
                .count()
                == 1,
            "prior npm Plugin source is not uniquely selected"
        );
        sources.retain(|value| value.as_str() != Some(prior));
    }
    let occurrences = sources
        .iter()
        .filter(|value| value.as_str() == Some(relative))
        .count();
    ensure!(
        occurrences <= 1,
        "npm Plugin source is selected more than once"
    );
    if occurrences == 0 {
        sources.push(toml::Value::String(relative.to_owned()));
    }
    let after = toml::to_string_pretty(&document)?.into_bytes();
    let intent = root.join("plugins").join(&lock.plugin_id);
    super::writable_path(root, intent.strip_prefix(root)?)?;
    let intent_exists = match fs::symlink_metadata(&intent) {
        Ok(metadata) if metadata.file_type().is_dir() => true,
        Ok(_) => bail!("npm Plugin Root intent is not a regular directory"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    ensure!(
        previous.is_some() || !intent_exists || destination.exists(),
        "unselected Plugin Root intent already exists; preserve its owner changes"
    );
    if destination.exists() {
        ensure!(
            fs::symlink_metadata(destination)?.file_type().is_dir()
                && read_lock(destination)? == *lock
                && source_digest(destination)? == lock.source_digest
                && dependency_digest(destination)? == lock.dependency_digest,
            "existing npm source differs from the signed archive"
        );
        verify_archived_source(destination, lock)?;
    } else {
        fs::create_dir_all(destination.parent().context("npm source parent")?)?;
        super::super::build::publish_new_output(stage, destination)?;
    }
    ensure!(
        linked_catalog::adoption::read_optional_regular(&config)? == before,
        "lenso.toml changed during npm adoption; preserving concurrent edit"
    );
    if before.as_deref() != Some(after.as_slice()) {
        let mut staged = tempfile::NamedTempFile::new_in(root)?;
        staged.write_all(&after)?;
        if before.is_some() {
            staged.persist(&config)?;
        } else {
            staged.persist_noclobber(&config)?;
        }
    }
    if !intent_exists {
        let staged = tempfile::Builder::new()
            .prefix(".npm-intent-")
            .tempdir_in(root)?;
        fs::write(
            staged.path().join("default.toml"),
            "# Explicit local Plugin adoption\n",
        )?;
        fs::create_dir_all(root.join("plugins"))?;
        super::super::build::publish_new_output(staged.path(), &intent)?;
    }
    Ok(())
}

fn verify_source(path: &Path, candidate: &Candidate) -> anyhow::Result<SourceLock> {
    ensure!(
        fs::symlink_metadata(path)?.file_type().is_dir(),
        "adopted npm source must be a directory"
    );
    let lock = read_lock(path)?;
    ensure!(
        lock.plugin_id == candidate.plugin_id
            && lock.release_version == candidate.release_version
            && candidate.format == "bun",
        "adopted npm source identity changed: {}",
        path.display()
    );
    verify_source_contents(path, &lock)?;
    Ok(lock)
}

fn verify_source_contents(path: &Path, lock: &SourceLock) -> anyhow::Result<String> {
    let installed_dependencies = dependency_digest(path)?;
    ensure!(
        source_digest(path)? == lock.source_digest
            && (lock.dependency_digest == "none"
                || installed_dependencies == lock.dependency_digest),
        "adopted npm source changed after adoption: {}",
        path.display()
    );
    verify_manifest_identity(path, lock)?;
    verify_archived_source(path, &lock)?;
    Ok(installed_dependencies)
}

fn verify_archived_source(path: &Path, lock: &SourceLock) -> anyhow::Result<()> {
    let archive = read_archive(&path.join(SOURCE_ARCHIVE))?;
    ensure!(
        digest(&archive) == lock.archive_digest,
        "adopted npm archive changed after adoption"
    );
    let unpacked = tempfile::tempdir()?;
    unpack_archive(&archive, unpacked.path())?;
    fs::set_permissions(unpacked.path(), fs::symlink_metadata(path)?.permissions())?;
    ensure!(
        source_digest(unpacked.path())? == lock.source_digest,
        "adopted npm source no longer corresponds to its exact signed archive"
    );
    Ok(())
}

pub(super) fn verify_sources(root: &Path, candidates: &[Candidate]) -> anyhow::Result<()> {
    let npm_root = fs::canonicalize(root)?.join("vendor/lenso/npm");
    let config = root.join("lenso.toml");
    let document: toml::Value = match fs::read_to_string(&config) {
        Ok(bytes) => toml::from_str(&bytes)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let sources = document
        .get("plugin_sources")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten();
    let mut selected = BTreeSet::new();
    for source in sources {
        let relative = source
            .as_str()
            .context("Plugin source must be a path string")?;
        let path = Path::new(relative);
        if !path.starts_with("vendor/lenso/npm") {
            continue;
        }
        ensure!(
            path.components()
                .all(|part| matches!(part, Component::Normal(_)))
                && path.components().count() == 5,
            "signed npm source must be one direct vendor/lenso/npm/PLUGIN_ID/VERSION path"
        );
        super::writable_path(root, path)?;
        let full = root.join(path);
        ensure!(
            fs::symlink_metadata(&full)?.file_type().is_dir(),
            "selected signed npm source must be a regular directory"
        );
        ensure!(
            selected.insert(full.clone()),
            "duplicate signed npm source selection"
        );
        let candidate = candidates
            .iter()
            .find(|candidate| candidate.project == full)
            .context("selected signed npm source was not discovered")?;
        let lock = verify_source(&full, candidate)?;
        ensure!(
            full == npm_root.join(&lock.plugin_id).join(&lock.release_version),
            "signed npm source path differs from its logical Plugin identity"
        );
    }
    for candidate in candidates {
        if candidate.project.starts_with(&npm_root) {
            ensure!(
                selected.contains(&candidate.project),
                "npm source candidate is not an exact signed selection"
            );
        }
    }
    Ok(())
}

pub(super) fn build_declaration(npm_root: &Path, source: &Path) -> anyhow::Result<Option<String>> {
    let Ok(relative) = source.strip_prefix(npm_root) else {
        return Ok(None);
    };
    let mut components = relative.components();
    let (Some(Component::Normal(plugin_id)), Some(Component::Normal(version))) =
        (components.next(), components.next())
    else {
        bail!("adopted npm build source lacks exact Plugin ID/version");
    };
    let source = npm_root.join(plugin_id).join(version);
    let lock = read_lock(&source)?;
    ensure!(
        source.file_name().and_then(|part| part.to_str()) == Some(lock.release_version.as_str())
            && source
                .parent()
                .and_then(Path::file_name)
                .and_then(|part| part.to_str())
                == Some(lock.plugin_id.as_str()),
        "adopted npm source changed before build"
    );
    let installed_dependencies = verify_source_contents(&source, &lock)?;
    let mut build_lock = lock;
    build_lock.dependency_digest = installed_dependencies;
    Ok(Some(format!(
        "{}@{}={}",
        build_lock.plugin_id,
        build_lock.release_version,
        build_digest(&build_lock)
    )))
}

fn copy_build_tree(source: &Path, destination: &Path, dependencies: &Path) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_dir() {
        fs::create_dir(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_build_tree(
                &entry.path(),
                &destination.join(entry.file_name()),
                dependencies,
            )?;
        }
        fs::set_permissions(destination, metadata.permissions())?;
    } else if metadata.file_type().is_file() {
        fs::copy(source, destination)?;
    } else if metadata.file_type().is_symlink() {
        ensure!(
            source.starts_with(dependencies),
            "adopted npm source has a symlink outside node_modules"
        );
        let target = fs::read_link(source)?;
        ensure!(
            !target.is_absolute(),
            "adopted npm dependency symlink must be relative for isolated builds"
        );
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, destination)?;
        #[cfg(windows)]
        if fs::metadata(source)?.is_dir() {
            std::os::windows::fs::symlink_dir(&target, destination)?;
        } else {
            std::os::windows::fs::symlink_file(&target, destination)?;
        }
        #[cfg(not(any(unix, windows)))]
        bail!("adopted npm dependency symlinks are unsupported on this Host");
    } else {
        bail!("adopted npm source contains a special file");
    }
    Ok(())
}

pub(crate) fn isolated_build_source(
    source: &Path,
) -> anyhow::Result<(tempfile::TempDir, PathBuf, String)> {
    let source = fs::canonicalize(source)?;
    let lock = read_lock(&source)?;
    let app_root = source
        .ancestors()
        .nth(5)
        .context("adopted npm source is outside its App")?;
    ensure!(
        source
            == app_root
                .join("vendor/lenso/npm")
                .join(&lock.plugin_id)
                .join(&lock.release_version),
        "adopted npm source path differs from its logical identity"
    );
    let installed_dependencies = verify_source_contents(&source, &lock)?;
    ensure!(
        installed_dependencies != "none",
        "adopted npm build requires local installed dependencies"
    );
    let temporary = tempfile::Builder::new()
        .prefix("lenso-npm-build-")
        .tempdir()?;
    let temporary_root = fs::canonicalize(temporary.path())?;
    let isolated = temporary_root.join("source");
    ensure!(
        !temporary_root.starts_with(app_root),
        "adopted npm build staging must be outside the App"
    );
    for ancestor in temporary_root.ancestors() {
        match fs::symlink_metadata(ancestor.join("node_modules")) {
            Ok(_) => bail!("adopted npm build staging has ancestor dependencies"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    copy_build_tree(&source, &isolated, &source.join("node_modules"))?;
    ensure!(
        verify_source_contents(&isolated, &lock)? == installed_dependencies,
        "adopted npm dependencies changed while staging the build"
    );
    let mut build_lock = lock;
    build_lock.dependency_digest = installed_dependencies;
    let declaration = format!(
        "{}@{}={}",
        build_lock.plugin_id,
        build_lock.release_version,
        build_digest(&build_lock)
    );
    Ok((temporary, isolated, declaration))
}

pub(crate) fn is_adopted_source(source: &Path) -> anyhow::Result<bool> {
    let source = fs::canonicalize(source)?;
    let under_vendor = source.ancestors().any(|ancestor| {
        ancestor
            .parent()
            .and_then(Path::parent)
            .is_some_and(|parent| parent.ends_with("vendor/lenso/npm"))
    });
    let lock_present = match fs::symlink_metadata(source.join(SOURCE_LOCK)) {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    Ok(under_vendor || lock_present)
}

pub(super) fn unadopt(root: &Path, source: &str) -> anyhow::Result<()> {
    let (plugin_id, release_version) = source
        .split_once('@')
        .context("npm source must be exact PLUGIN_ID@RELEASE_VERSION")?;
    lenso_app_authoring::identity::validate_plugin_id_v1(plugin_id)?;
    lenso_app_authoring::identity::validate_release_version(release_version)?;
    let _app_lock = linked_catalog::adoption::lock_app(root)?;
    let relative = Path::new("vendor/lenso/npm")
        .join(plugin_id)
        .join(release_version);
    let source_path = root.join(&relative);
    let intent_relative = Path::new("plugins").join(plugin_id);
    let intent_path = root.join(&intent_relative);
    for path in [
        relative.as_path(),
        intent_relative.as_path(),
        Path::new("lenso.toml"),
        Path::new(".lenso/trash/npm"),
    ] {
        super::writable_path(root, path)?;
    }
    let report = lenso_app_authoring::discovery::discover(root)?;
    let candidate = report
        .candidates
        .iter()
        .find(|candidate| candidate.plugin_id == plugin_id)
        .context("npm Plugin is not selected")?;
    ensure!(
        candidate.project == source_path && candidate.release_version == release_version,
        "selected npm Plugin does not match the exact requested source"
    );
    verify_source(&source_path, candidate)?;
    ensure!(
        fs::symlink_metadata(&intent_path)?.file_type().is_dir(),
        "npm Plugin Root intent must be a regular directory"
    );
    let mut default = false;
    for entry in fs::read_dir(&intent_path)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().context("Plugin Root intent filename UTF-8")?;
        ensure!(
            name == "default.toml" || name == "default.disabled",
            "npm Plugin Root intent has user changes; preserve it before unadopting"
        );
        let metadata = fs::symlink_metadata(entry.path())?;
        ensure!(
            metadata.file_type().is_file() && (name != "default.disabled" || metadata.len() == 0),
            "npm Plugin Root intent has user changes; preserve it before unadopting"
        );
        if name == "default.toml" {
            ensure!(
                fs::read_to_string(entry.path())? == "# Explicit local Plugin adoption\n",
                "npm Plugin Root intent has user changes; preserve it before unadopting"
            );
            default = true;
        }
    }
    ensure!(default, "npm Plugin Root default intent is missing");
    let config_path = root.join("lenso.toml");
    let config_before = fs::read(&config_path)?;
    let mut document: toml::Value = toml::from_str(std::str::from_utf8(&config_before)?)?;
    let sources = document
        .get_mut("plugin_sources")
        .and_then(toml::Value::as_array_mut)
        .context("lenso.toml plugin_sources array")?;
    let selected = relative.to_str().context("npm source path UTF-8")?;
    ensure!(
        sources
            .iter()
            .filter(|value| value.as_str() == Some(selected))
            .count()
            == 1,
        "npm Plugin source is not uniquely selected by lenso.toml"
    );
    sources.retain(|value| value.as_str() != Some(selected));
    let config_after = toml::to_string_pretty(&document)?.into_bytes();
    let mut staged = tempfile::NamedTempFile::new_in(root)?;
    staged.write_all(&config_after)?;
    let trash_parent = root.join(".lenso/trash/npm");
    fs::create_dir_all(&trash_parent)?;
    let trash = tempfile::Builder::new()
        .prefix("unadopt-")
        .tempdir_in(&trash_parent)?
        .keep();
    ensure!(
        fs::read(&config_path)? == config_before,
        "lenso.toml changed during npm unadopt; preserving concurrent edit"
    );
    let mut source_moved = false;
    let mut intent_moved = false;
    let mut config_published = false;
    let result = (|| -> anyhow::Result<()> {
        super::super::build::publish_new_output(&source_path, &trash.join("source"))?;
        source_moved = true;
        super::super::build::publish_new_output(&intent_path, &trash.join("plugin-root"))?;
        intent_moved = true;
        ensure!(
            fs::read(&config_path)? == config_before,
            "lenso.toml changed during npm unadopt; preserving concurrent edit"
        );
        staged.persist(&config_path)?;
        config_published = true;
        Ok(())
    })();
    if let Err(error) = result {
        if let Err(rollback_error) = linked_catalog::rollback_unadopt(
            root,
            &trash,
            &source_path,
            &intent_path,
            &config_path,
            &config_before,
            &config_after,
            source_moved,
            intent_moved,
            config_published,
        ) {
            return Err(error.context(format!(
                "npm unadopt rollback incomplete; recover from {}: {rollback_error:#}",
                trash.display()
            )));
        }
        return Err(error);
    }
    println!(
        "Unadopted {plugin_id}@{release_version}; source and Plugin Root intent are recoverable at {}",
        trash.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use ed25519_dalek::SigningKey;
    use flate2::{Compression, write::GzEncoder};
    use lenso_plugin_catalog::{
        Availability, Distribution, ReleaseDetails, ReleaseDetailsSnapshot,
        linked_cargo::{self, LinkedCargoIntegration, LinkedCargoRelease, LinkedCargoSnapshot},
        package::PackageRelease,
        sign_release_details,
    };

    use super::*;

    fn archive() -> Vec<u8> {
        let gzip = GzEncoder::new(Vec::new(), Compression::default());
        let mut archive = tar::Builder::new(gzip);
        for (path, bytes) in [
            (
                "package/package.json",
                br#"{"name":"@example/notes","version":"4.5.6","lenso":{"pluginId":"example.notes","releaseVersion":"1.2.3","runtime":"bun","rootSlot":"tools","source":"index.ts"}}"#.as_slice(),
            ),
            ("package/bun.lock", b"{}".as_slice()),
            ("package/index.ts", b"export default {};".as_slice()),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            archive
                .append_data(&mut header, path, Cursor::new(bytes))
                .unwrap();
        }
        archive.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn linked_release_details_select_exact_npm_alternative_without_cargo_or_portable() {
        let app = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(app.path()).unwrap();
        let archive = archive();
        let mut tar = tar::Archive::new(GzDecoder::new(Cursor::new(&archive)));
        let mut entries = Vec::new();
        for entry in tar.entries().unwrap() {
            let mut entry = entry.unwrap();
            let path = entry.path().unwrap().to_path_buf();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            if path == Path::new("package/package.json") {
                let mut manifest: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                manifest["version"] = "1.2.3".into();
                bytes = serde_json::to_vec(&manifest).unwrap();
            }
            entries.push((path, bytes));
        }
        let mut repacked = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
        for (path, bytes) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            repacked
                .append_data(&mut header, path, Cursor::new(bytes))
                .unwrap();
        }
        let archive = repacked.into_inner().unwrap().finish().unwrap();
        let tgz = root.join("plugin.tgz");
        fs::write(&tgz, &archive).unwrap();

        let key = SigningKey::from_bytes(&[47; 32]);
        let trust = root.join("trust.json");
        fs::write(
            &trust,
            serde_json::to_vec(&serde_json::json!({
                "catalog_id": "linked-npm-test",
                "key_id": "key",
                "public_key_hex": hex::encode(key.verifying_key().to_bytes())
            }))
            .unwrap(),
        )
        .unwrap();
        let now = linked_catalog::now().unwrap();
        let base = LinkedCargoRelease {
            plugin_id: "example.notes".into(),
            version: "1.2.3".into(),
            publisher_id: "example".into(),
            title: "Notes".into(),
            summary: "Notes Plugin".into(),
            source_url: "https://example.test/source".into(),
            source_revision: "a".repeat(40),
            license: "MIT".into(),
            package: "example-notes-plugin".into(),
            registry_url: "https://crates.io".into(),
            crate_digest: digest(b"exact-crate"),
            integration: LinkedCargoIntegration::LinkedPlugin,
            targets: vec![lenso_app_authoring::native_host_target().into()],
            availability: Availability::Listed,
            documentation: vec![],
        };
        let linked = root.join("linked.json");
        fs::write(
            &linked,
            linked_cargo::sign(
                &LinkedCargoSnapshot::new(
                    "linked-npm-test".into(),
                    1,
                    now - 5,
                    now + 3600,
                    vec![base.clone()],
                ),
                "key",
                &key,
            )
            .unwrap(),
        )
        .unwrap();
        let details = root.join("details.json");
        let release = ReleaseDetails {
            plugin_id: base.plugin_id.clone(),
            version: base.version.clone(),
            base_release_identity: base.immutable_identity().unwrap(),
            distributions: vec![
                Distribution {
                    id: "cargo".into(),
                    kind: DistributionKind::CargoPackage,
                    package: base.package.clone(),
                    version: base.version.clone(),
                    integrity: Some(base.crate_digest.clone()),
                    registry_url: Some(base.registry_url.clone()),
                    artifact: None,
                    targets: base.targets.clone(),
                },
                Distribution {
                    id: "bun".into(),
                    kind: DistributionKind::NpmPackage,
                    package: "@example/notes".into(),
                    version: base.version.clone(),
                    integrity: Some(digest(&archive)),
                    registry_url: Some("https://registry.npmjs.org".into()),
                    artifact: None,
                    targets: vec![],
                },
            ],
            documentation: vec![],
        };
        let details_snapshot = ReleaseDetailsSnapshot::new(
            "linked-npm-test".into(),
            1,
            now - 5,
            now + 3600,
            vec![release.clone()],
        );
        let sign_details = |snapshot: &ReleaseDetailsSnapshot| {
            fs::write(
                &details,
                sign_release_details(snapshot, "key", &key).unwrap(),
            )
            .unwrap();
        };
        let args = AddArgs {
            source: "example.notes@1.2.3".into(),
            root: Some(root.clone()),
            no_install: true,
            linked_snapshot: Some(linked),
            portable_snapshot: None,
            package_snapshot: None,
            release_details: Some(details.clone()),
            distribution: Some("bun".into()),
            trust: Some(trust),
            crate_archive: None,
            tgz: Some(tgz.clone()),
            bundle: None,
            archive: None,
            origin: None,
            replace: false,
            content_snapshot: None,
            content_id: None,
            content_archive: None,
            content_destination: None,
            content_preview: false,
        };
        let mut wrong_base = details_snapshot.clone();
        wrong_base.releases[0].base_release_identity = digest(b"wrong-base");
        sign_details(&wrong_base);
        assert!(super::super::add(args.clone()).is_err());
        assert!(!root.join("vendor/lenso/npm/example.notes").exists());

        sign_details(&details_snapshot);
        fs::write(&tgz, b"wrong archive").unwrap();
        assert!(super::super::add(args.clone()).is_err());
        assert!(!root.join("vendor/lenso/npm/example.notes").exists());
        fs::write(&tgz, &archive).unwrap();
        super::super::add(args.clone()).unwrap();
        let source = root.join("vendor/lenso/npm/example.notes/1.2.3");
        let lock = read_lock(&source).unwrap();
        assert_eq!(
            lock.linked_base.as_ref().unwrap().release_identity,
            release.base_release_identity
        );
        let mut unrelated_admission = lock.clone();
        unrelated_admission.linked_base = None;
        assert_ne!(build_digest(&lock), build_digest(&unrelated_admission));
        assert!(root.join(".lenso").is_dir());
        let report = lenso_app_authoring::discovery::discover(&root).unwrap();
        verify_sources(&root, &report.candidates).unwrap();
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(report.candidates[0].format, "bun");
        assert!(
            build_declaration(&root.join("vendor/lenso/npm"), &source)
                .unwrap()
                .unwrap()
                .starts_with("example.notes@1.2.3=sha256:")
        );
        let package_snapshot = root.join("package-only.json");
        fs::write(
            &package_snapshot,
            package::sign(
                &package::PackageSnapshot::new(
                    "linked-npm-test".into(),
                    1,
                    now - 5,
                    now + 3600,
                    vec![PackageRelease {
                        plugin_id: base.plugin_id.clone(),
                        version: base.version.clone(),
                        publisher_id: base.publisher_id.clone(),
                        title: base.title.clone(),
                        summary: base.summary.clone(),
                        source_url: base.source_url.clone(),
                        source_revision: base.source_revision.clone(),
                        license: base.license.clone(),
                        distributions: vec![release.distributions[1].clone()],
                        availability: Availability::Listed,
                        documentation: vec![],
                    }],
                ),
                "key",
                &key,
            )
            .unwrap(),
        )
        .unwrap();
        let mut conflicting_channel = args.clone();
        conflicting_channel.linked_snapshot = None;
        conflicting_channel.release_details = None;
        conflicting_channel.package_snapshot = Some(package_snapshot);
        conflicting_channel.replace = true;
        assert!(super::super::add(conflicting_channel).is_err());
        assert_eq!(read_lock(&source).unwrap(), lock);
        add(&root, &args).unwrap();
        unadopt(&root, "example.notes@1.2.3").unwrap();
        assert!(!source.exists());
    }

    #[cfg(unix)]
    #[test]
    fn archive_nested_directory_modes_are_independent_of_umask_and_tar_headers() {
        use std::os::unix::fs::PermissionsExt as _;

        let gzip = GzEncoder::new(Vec::new(), Compression::default());
        let mut archive = tar::Builder::new(gzip);
        let mut directory = tar::Header::new_gnu();
        directory.set_size(0);
        directory.set_mode(0o700);
        directory.set_entry_type(tar::EntryType::Directory);
        directory.set_cksum();
        archive
            .append_data(&mut directory, "package/nested/explicit/", Cursor::new([]))
            .unwrap();
        for path in [
            "package/nested/explicit/file.ts",
            "package/nested/implicit/file.ts",
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(1);
            header.set_mode(0o644);
            header.set_cksum();
            archive
                .append_data(&mut header, path, Cursor::new(b"x"))
                .unwrap();
        }
        let bytes = archive.into_inner().unwrap().finish().unwrap();
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        for path in ["nested", "nested/explicit", "nested/implicit"] {
            let directory = second.path().join(path);
            fs::create_dir_all(&directory).unwrap();
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        }
        unpack_archive(&bytes, first.path()).unwrap();
        unpack_archive(&bytes, second.path()).unwrap();
        for stage in [first.path(), second.path()] {
            for path in ["nested", "nested/explicit", "nested/implicit"] {
                assert_eq!(
                    file_mode(&fs::symlink_metadata(stage.join(path)).unwrap()),
                    0o755
                );
            }
        }
        assert_eq!(
            source_digest(first.path()).unwrap(),
            source_digest(second.path()).unwrap()
        );
    }

    #[test]
    fn signed_npm_archive_adopts_requires_exact_build_trust_and_unadopts() {
        let app = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(app.path()).unwrap();
        let archive = archive();
        let archive_path = app.path().join("plugin.tgz");
        fs::write(&archive_path, &archive).unwrap();
        let key = SigningKey::from_bytes(&[23; 32]);
        let now = linked_catalog::now().unwrap();
        let snapshot = package::PackageSnapshot::new(
            "catalog".into(),
            1,
            now - 10,
            now + 3600,
            vec![PackageRelease {
                plugin_id: "example.notes".into(),
                version: "1.2.3".into(),
                publisher_id: "example".into(),
                title: "Notes".into(),
                summary: "Notes Plugin".into(),
                source_url: "https://example.test/source".into(),
                source_revision: "a".repeat(40),
                license: "MIT".into(),
                distributions: vec![Distribution {
                    id: "npm".into(),
                    kind: DistributionKind::NpmPackage,
                    package: "@example/notes".into(),
                    version: "4.5.6".into(),
                    integrity: Some(digest(&archive)),
                    registry_url: Some("https://registry.npmjs.org".into()),
                    artifact: None,
                    targets: vec![],
                }],
                availability: Availability::Listed,
                documentation: vec![],
            }],
        );
        let snapshot_path = app.path().join("package-snapshot.json");
        fs::write(
            &snapshot_path,
            package::sign(&snapshot, "key", &key).unwrap(),
        )
        .unwrap();
        let trust_path = app.path().join("trust.json");
        fs::write(
            &trust_path,
            serde_json::to_vec(&serde_json::json!({
                "catalog_id": "catalog",
                "key_id": "key",
                "public_key_hex": hex::encode(key.verifying_key().to_bytes())
            }))
            .unwrap(),
        )
        .unwrap();
        let args = AddArgs {
            source: "example.notes@1.2.3".into(),
            root: None,
            no_install: true,
            linked_snapshot: None,
            portable_snapshot: None,
            package_snapshot: Some(snapshot_path),
            release_details: None,
            distribution: Some("npm".into()),
            trust: Some(trust_path),
            crate_archive: None,
            tgz: Some(archive_path.clone()),
            bundle: None,
            archive: None,
            origin: None,
            replace: false,
            content_snapshot: None,
            content_id: None,
            content_archive: None,
            content_destination: None,
            content_preview: false,
        };
        let mut tampered = archive.clone();
        tampered[0] ^= 1;
        fs::write(&archive_path, tampered).unwrap();
        assert!(
            preview(
                &root,
                args.package_snapshot.as_deref().unwrap(),
                args.trust.as_deref().unwrap(),
                &archive_path,
                "example.notes",
                "1.2.3",
            )
            .is_err()
        );
        assert!(add(&root, &args).is_err());
        assert!(!app.path().join("vendor/lenso/npm/example.notes").exists());
        fs::write(&archive_path, &archive).unwrap();
        let inspected = preview(
            &root,
            args.package_snapshot.as_deref().unwrap(),
            args.trust.as_deref().unwrap(),
            &archive_path,
            "example.notes",
            "1.2.3",
        )
        .unwrap();
        assert_eq!(inspected["archive_bytes"], "digest_verified");
        assert_eq!(inspected["dependencies"], "not_installed");
        assert_eq!(inspected["app_selection"], "not_selected");
        assert_eq!(inspected["activation"], "not_observed");
        assert!(!app.path().join("vendor/lenso/npm/example.notes").exists());
        add(&root, &args).unwrap();
        add(&root, &args).unwrap();
        let report = lenso_app_authoring::discovery::discover(&root).unwrap();
        assert_eq!(report.candidates[0].release_version, "1.2.3");
        verify_sources(&root, &report.candidates).unwrap();
        let source = root.join("vendor/lenso/npm/example.notes/1.2.3");
        let declaration = build_declaration(&root.join("vendor/lenso/npm"), &source)
            .unwrap()
            .unwrap();
        assert!(
            linked_catalog::require_linked_build_trust(&root, &report.candidates, &[], false, &[])
                .is_err()
        );
        linked_catalog::require_linked_build_trust(
            &root,
            &report.candidates,
            &[],
            false,
            &[declaration],
        )
        .unwrap();
        fs::write(source.join("index.ts"), b"export default {changed: true};").unwrap();
        assert!(verify_sources(&root, &report.candidates).is_err());
        fs::write(source.join("index.ts"), b"export default {};").unwrap();
        unadopt(&root, "example.notes@1.2.3").unwrap();
        assert!(!source.exists());
        assert!(!app.path().join("plugins/example.notes").exists());
        assert!(
            lenso_app_authoring::discovery::discover(&root)
                .unwrap()
                .candidates
                .is_empty()
        );
    }

    #[test]
    fn unpack_rejects_symlink_entries() {
        let stage = tempfile::tempdir().unwrap();
        let mut archive = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_link_name("../../escape").unwrap();
        header.set_cksum();
        archive
            .append_data(&mut header, "package/link", Cursor::new([]))
            .unwrap();
        let bytes = archive.into_inner().unwrap().finish().unwrap();
        assert!(unpack_archive(&bytes, stage.path()).is_err());
    }

    #[test]
    fn unpack_rejects_ambiguous_directories_and_compressed_tail() {
        let stage = tempfile::tempdir().unwrap();
        let mut trailing = archive();
        trailing.extend_from_slice(b"another gzip member or hidden data");
        assert!(unpack_archive(&trailing, stage.path()).is_err());

        let gzip = GzEncoder::new(Vec::new(), Compression::default());
        let mut archive = tar::Builder::new(gzip);
        for _ in 0..2 {
            let mut header = tar::Header::new_gnu();
            header.set_size(0);
            header.set_mode(0o755);
            header.set_entry_type(tar::EntryType::Directory);
            header.set_cksum();
            archive
                .append_data(&mut header, "package/src/", Cursor::new([]))
                .unwrap();
        }
        let bytes = archive.into_inner().unwrap().finish().unwrap();
        assert!(unpack_archive(&bytes, stage.path()).is_err());
    }

    #[test]
    fn adopted_build_requires_local_dependencies_and_copies_outside_app() {
        let app = tempfile::tempdir().unwrap();
        let source = app.path().join("vendor/lenso/npm/example.notes/1.2.3");
        fs::create_dir_all(&source).unwrap();
        let bytes = archive();
        unpack_archive(&bytes, &source).unwrap();
        let lock = SourceLock {
            schema_version: 1,
            plugin_id: "example.notes".into(),
            release_version: "1.2.3".into(),
            package: "@example/notes".into(),
            package_version: "4.5.6".into(),
            distribution_id: "npm".into(),
            registry_url: "https://registry.npmjs.org".into(),
            archive_digest: digest(&bytes),
            source_digest: source_digest(&source).unwrap(),
            dependency_digest: "none".into(),
            linked_base: None,
        };
        fs::write(source.join(SOURCE_ARCHIVE), &bytes).unwrap();
        fs::write(source.join(SOURCE_LOCK), serde_json::to_vec(&lock).unwrap()).unwrap();
        let npm_root = app.path().join("vendor/lenso/npm");
        let before_install = build_declaration(&npm_root, &source).unwrap().unwrap();
        assert!(
            isolated_build_source(&source)
                .unwrap_err()
                .to_string()
                .contains("local installed dependencies")
        );

        fs::create_dir(source.join("node_modules")).unwrap();
        fs::write(source.join("node_modules/local.js"), b"export default 1;").unwrap();
        let after_install = build_declaration(&npm_root, &source).unwrap().unwrap();
        assert_ne!(after_install, before_install);
        fs::create_dir(app.path().join("node_modules")).unwrap();
        fs::write(app.path().join("node_modules/poison.js"), b"poison").unwrap();
        let (_temporary, isolated, approved) = isolated_build_source(&source).unwrap();
        assert!(!isolated.starts_with(app.path()));
        assert_eq!(
            dependency_digest(&isolated).unwrap(),
            dependency_digest(&source).unwrap()
        );
        assert_eq!(approved, after_install);
        assert!(!isolated.join("node_modules/poison.js").exists());
        let direct = crate::plugin::materialize(
            &source,
            &app.path().join("unchecked.lenso-plugin"),
            crate::plugin::BuildProfile::Development,
        )
        .err()
        .unwrap();
        assert!(direct.to_string().contains("not trusted"));

        fs::remove_file(source.join(SOURCE_LOCK)).unwrap();
        assert!(is_adopted_source(&source).unwrap());
        assert!(
            crate::plugin::materialize(
                &source,
                &app.path().join("missing-lock.lenso-plugin"),
                crate::plugin::BuildProfile::Development,
            )
            .is_err()
        );

        let mut pinned = lock.clone();
        pinned.dependency_digest = dependency_digest(&source).unwrap();
        fs::write(source.join("node_modules/local.js"), b"export default 2;").unwrap();
        assert!(verify_source_contents(&source, &pinned).is_err());

        let nested = source.join("subdir");
        fs::create_dir(&nested).unwrap();
        fs::copy(source.join("package.json"), nested.join("package.json")).unwrap();
        assert!(is_adopted_source(&nested).unwrap());
        assert!(isolated_build_source(&nested).is_err());
    }

    #[test]
    fn signed_package_provenance_changes_build_trust_and_must_match_manifest() {
        let stage = tempfile::tempdir().unwrap();
        let bytes = archive();
        unpack_archive(&bytes, stage.path()).unwrap();
        let lock = SourceLock {
            schema_version: 1,
            plugin_id: "example.notes".into(),
            release_version: "1.2.3".into(),
            package: "@example/notes".into(),
            package_version: "4.5.6".into(),
            distribution_id: "npm".into(),
            registry_url: "https://registry.npmjs.org".into(),
            archive_digest: digest(&bytes),
            source_digest: source_digest(stage.path()).unwrap(),
            dependency_digest: "none".into(),
            linked_base: None,
        };
        verify_manifest_identity(stage.path(), &lock).unwrap();
        let approved = build_digest(&lock);
        let mut changed = lock.clone();
        changed.package = "@example/imposter".into();
        assert_ne!(build_digest(&changed), approved);
        assert!(verify_manifest_identity(stage.path(), &changed).is_err());
        let mut changed = lock.clone();
        changed.distribution_id = "other".into();
        assert_ne!(build_digest(&changed), approved);
        let mut changed = lock.clone();
        changed.registry_url = "https://other.example".into();
        assert_ne!(build_digest(&changed), approved);
    }

    #[test]
    fn empty_directories_and_directory_modes_change_source_and_dependency_digests() {
        let root = tempfile::tempdir().unwrap();
        let before_source = source_digest(root.path()).unwrap();
        let source_dir = root.path().join("empty");
        fs::create_dir(&source_dir).unwrap();
        let with_source_dir = source_digest(root.path()).unwrap();
        assert_ne!(before_source, with_source_dir);

        let dependencies = root.path().join("node_modules");
        fs::create_dir(&dependencies).unwrap();
        let before_dependency = dependency_digest(root.path()).unwrap();
        let dependency_dir = dependencies.join("empty");
        fs::create_dir(&dependency_dir).unwrap();
        let with_dependency_dir = dependency_digest(root.path()).unwrap();
        assert_ne!(before_dependency, with_dependency_dir);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let changed_mode = |path: &Path| {
                let current = file_mode(&fs::symlink_metadata(path).unwrap());
                if current == 0o700 { 0o755 } else { 0o700 }
            };
            fs::set_permissions(
                &source_dir,
                fs::Permissions::from_mode(changed_mode(&source_dir)),
            )
            .unwrap();
            fs::set_permissions(
                &dependency_dir,
                fs::Permissions::from_mode(changed_mode(&dependency_dir)),
            )
            .unwrap();
            assert_ne!(source_digest(root.path()).unwrap(), with_source_dir);
            assert_ne!(dependency_digest(root.path()).unwrap(), with_dependency_dir);
        }
    }
}
