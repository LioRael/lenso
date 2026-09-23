use std::{
    collections::BTreeMap,
    fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
};

use anyhow::{Context as _, bail, ensure};
use ed25519_dalek::VerifyingKey;
use lenso_app_authoring::discovery::Candidate;
use lenso_plugin_catalog::{
    Trust,
    linked_cargo::{self, LinkedCargoIntegration},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::AddArgs;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustFile {
    catalog_id: String,
    key_id: String,
    public_key_hex: String,
}

const MAX_CRATE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_UNPACKED_BYTES: u64 = 128 * 1024 * 1024;
const MAX_FILES: usize = 4096;
const SOURCE_LOCK: &str = ".lenso-linked-source.json";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SourceLock {
    schema_version: u32,
    plugin_id: String,
    version: String,
    crate_digest: String,
    source_digest: String,
}

pub(super) fn add(root: &Path, args: &AddArgs) -> anyhow::Result<()> {
    let snapshot_path = args
        .linked_snapshot
        .as_ref()
        .context("--linked-snapshot required")?;
    let trust_path = args.trust.as_ref().context("--trust required")?;
    let crate_path = args.crate_archive.as_ref().context("--crate required")?;
    let (plugin_id, version) = args
        .source
        .split_once('@')
        .context("linked Cargo source must be an exact PLUGIN_ID@VERSION")?;
    let trust_file: TrustFile = serde_json::from_slice(&fs::read(trust_path)?)?;
    let key: [u8; 32] = hex::decode(trust_file.public_key_hex)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("public trust key must have 32 bytes"))?;
    let trust = Trust {
        catalog_id: trust_file.catalog_id,
        keys: BTreeMap::from([(trust_file.key_id, VerifyingKey::from_bytes(&key)?)]),
    };
    let envelope = fs::read(snapshot_path)?;
    ensure!(
        envelope.len() <= lenso_plugin_catalog::MAX_ENVELOPE_BYTES,
        "linked Cargo snapshot exceeds size limit"
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let verified = linked_cargo::verify(&envelope, &trust, None, now)?;
    let release = verified.select(plugin_id, version, now)?;
    ensure!(
        release.integration == LinkedCargoIntegration::LinkedPlugin,
        "Host integration required: this release cannot be added as a generic linked Plugin"
    );
    let target = lenso_app_authoring::native_host_target();
    ensure!(
        release.targets.iter().any(|candidate| candidate == target),
        "linked Cargo release does not support Host target {target}"
    );
    ensure!(
        release.registry_url == "https://crates.io",
        "linked Cargo registry is unsupported; use an authorized custom Host"
    );
    let mut archive = Vec::new();
    fs::File::open(crate_path)?
        .take(MAX_CRATE_BYTES + 1)
        .read_to_end(&mut archive)?;
    ensure!(
        !archive.is_empty() && u64::try_from(archive.len())? <= MAX_CRATE_BYTES,
        "crate archive exceeds size limit"
    );
    ensure!(
        lenso_plugin_catalog::digest(&archive) == release.crate_digest,
        "crate archive digest does not match signed catalog"
    );
    let parent = root.join("vendor/lenso").join(plugin_id);
    super::writable_path(root, Path::new("vendor/lenso"))?;
    super::writable_path(
        root,
        &Path::new("vendor/lenso").join(plugin_id).join(version),
    )?;
    fs::create_dir_all(&parent)?;
    let stage = tempfile::Builder::new()
        .prefix(".linked-cargo-")
        .tempdir_in(&parent)?;
    unpack(&archive, stage.path(), release)?;
    let report = lenso_app_authoring::discovery::discover(stage.path())?;
    let [candidate] = report.candidates.as_slice() else {
        bail!("linked Cargo archive must contain one Plugin source package");
    };
    ensure!(
        candidate.plugin_id == release.plugin_id
            && candidate.release_version == release.version
            && candidate.format == "cargo"
            && candidate
                .implementations
                .iter()
                .any(|entry| entry.runtime == "native-linked"),
        "linked Cargo source does not match an adoptable native Plugin"
    );
    let lock = SourceLock {
        schema_version: 1,
        plugin_id: release.plugin_id.clone(),
        version: release.version.clone(),
        crate_digest: release.crate_digest.clone(),
        source_digest: source_digest(stage.path())?,
    };
    fs::write(
        stage.path().join(SOURCE_LOCK),
        serde_json::to_vec_pretty(&lock)?,
    )?;
    let destination = parent.join(version);
    if destination.try_exists()? {
        ensure!(
            same_tree(stage.path(), &destination)?,
            "existing linked Cargo source differs from signed archive"
        );
    } else {
        fs::rename(stage.path(), &destination)?;
    }
    super::add(AddArgs {
        source: destination
            .to_str()
            .context("linked source path UTF-8")?
            .to_owned(),
        root: Some(root.to_path_buf()),
        no_install: args.no_install,
        linked_snapshot: None,
        trust: None,
        crate_archive: None,
    })?;
    println!(
        "Linked Cargo {}@{} selected for Host compilation; review its build-time code before app build",
        plugin_id, version
    );
    Ok(())
}

pub(crate) fn verify_sources(root: &Path, candidates: &[Candidate]) -> anyhow::Result<()> {
    let vendor_root = fs::canonicalize(root)?.join("vendor/lenso");
    for candidate in candidates {
        if !candidate.project.starts_with(&vendor_root) {
            continue;
        }
        let lock_path = candidate.project.join(SOURCE_LOCK);
        let bytes = fs::read(&lock_path).with_context(|| {
            format!(
                "linked Cargo source lacks lock: {}",
                candidate.project.display()
            )
        })?;
        ensure!(
            bytes.len() <= 4096,
            "linked Cargo source lock exceeds size limit"
        );
        let lock: SourceLock = serde_json::from_slice(&bytes)?;
        ensure!(
            lock.schema_version == 1
                && lock.plugin_id == candidate.plugin_id
                && lock.version == candidate.release_version,
            "linked Cargo source identity changed: {}",
            candidate.project.display()
        );
        ensure!(
            source_digest(&candidate.project)? == lock.source_digest,
            "linked Cargo source changed after adoption: {}",
            candidate.project.display()
        );
    }
    Ok(())
}

fn source_digest(root: &Path) -> anyhow::Result<String> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    let mut total = 0u64;
    while let Some(path) = pending.pop() {
        ensure!(
            pending.len() + files.len() <= MAX_FILES,
            "linked Cargo source has too many files"
        );
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            if path == root.join("target") {
                continue;
            }
            for entry in fs::read_dir(path)? {
                pending.push(entry?.path());
                ensure!(
                    pending.len() + files.len() <= MAX_FILES,
                    "linked Cargo source has too many files"
                );
            }
        } else if metadata.is_file() {
            if path != root.join(SOURCE_LOCK) && path != root.join("Cargo.lock") {
                total = total
                    .checked_add(metadata.len())
                    .context("linked Cargo source size overflow")?;
                ensure!(
                    total <= MAX_UNPACKED_BYTES,
                    "linked Cargo source exceeds size limit"
                );
                files.push(path);
            }
        } else {
            bail!("linked Cargo source contains a symlink or special file");
        }
    }
    files.sort();
    let mut hasher = Sha256::new();
    for path in files {
        let relative = path
            .strip_prefix(root)?
            .to_str()
            .context("linked Cargo path UTF-8")?;
        let bytes = fs::read(&path)?;
        hasher.update((relative.len() as u64).to_be_bytes());
        hasher.update(relative.as_bytes());
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }
    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}

pub(super) fn unadopt(root: &Path, source: &str) -> anyhow::Result<()> {
    let (plugin_id, version) = source
        .split_once('@')
        .context("linked Cargo source must be an exact PLUGIN_ID@VERSION")?;
    lenso_app_authoring::identity::validate_plugin_id_v1(plugin_id)?;
    lenso_app_authoring::identity::validate_release_version(version)?;
    let relative = Path::new("vendor/lenso").join(plugin_id).join(version);
    let source_path = root.join(&relative);
    let intent_relative = Path::new("plugins").join(plugin_id);
    let intent_path = root.join(&intent_relative);
    for path in [
        relative.as_path(),
        intent_relative.as_path(),
        Path::new("lenso.toml"),
        Path::new(".lenso/trash/linked-cargo"),
    ] {
        super::writable_path(root, path)?;
    }
    ensure!(source_path.is_dir(), "linked Cargo source is not adopted");
    let lock_bytes =
        fs::read(source_path.join(SOURCE_LOCK)).context("linked Cargo source lock is missing")?;
    ensure!(
        lock_bytes.len() <= 4096,
        "linked Cargo source lock exceeds size limit"
    );
    let lock: SourceLock = serde_json::from_slice(&lock_bytes)?;
    ensure!(
        lock.schema_version == 1 && lock.plugin_id == plugin_id && lock.version == version,
        "linked Cargo source lock does not match requested identity"
    );
    ensure!(
        intent_path.is_dir(),
        "linked Cargo Plugin Root intent is missing"
    );
    let entries = fs::read_dir(&intent_path)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<Vec<_>, _>>()?;
    ensure!(
        entries
            .iter()
            .all(|name| name == "default.toml" || name == "default.disabled")
            && entries.iter().any(|name| name == "default.toml")
            && fs::read_to_string(intent_path.join("default.toml"))?
                == "# Explicit local Plugin adoption\n",
        "Plugin Root intent has user changes; remove it explicitly before unadopting source"
    );
    let config_path = root.join("lenso.toml");
    let mut document: toml::Value = toml::from_str(&fs::read_to_string(&config_path)?)?;
    let sources = document
        .get_mut("plugin_sources")
        .and_then(toml::Value::as_array_mut)
        .context("lenso.toml plugin_sources array")?;
    let selected = relative.to_str().context("linked source path UTF-8")?;
    ensure!(
        sources
            .iter()
            .filter(|value| value.as_str() == Some(selected))
            .count()
            == 1,
        "linked Cargo source is not uniquely selected by lenso.toml"
    );
    sources.retain(|value| value.as_str() != Some(selected));
    let mut staged = tempfile::NamedTempFile::new_in(root)?;
    staged.write_all(toml::to_string_pretty(&document)?.as_bytes())?;
    let trash_parent = root.join(".lenso/trash/linked-cargo");
    fs::create_dir_all(&trash_parent)?;
    let trash = tempfile::Builder::new()
        .prefix("unadopt-")
        .tempdir_in(&trash_parent)?
        .keep();
    fs::rename(&source_path, trash.join("source"))?;
    if let Err(error) = fs::rename(&intent_path, trash.join("plugin-root")) {
        fs::rename(trash.join("source"), &source_path)?;
        return Err(error.into());
    }
    if let Err(error) = staged.persist(&config_path) {
        fs::rename(trash.join("plugin-root"), &intent_path)?;
        fs::rename(trash.join("source"), &source_path)?;
        return Err(error.into());
    }
    println!(
        "Unadopted {plugin_id}@{version}; source and Plugin Root intent are recoverable at {}",
        trash.display()
    );
    Ok(())
}

fn unpack(
    bytes: &[u8],
    stage: &Path,
    release: &linked_cargo::LinkedCargoRelease,
) -> anyhow::Result<()> {
    let root = format!("{}-{}/", release.package, release.version);
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    let mut files = 0usize;
    let mut total = 0u64;
    for entry in archive.entries().context("invalid crate archive")? {
        let mut entry = entry.context("invalid crate archive entry")?;
        files += 1;
        ensure!(files <= MAX_FILES, "crate archive has too many files");
        total = total
            .checked_add(entry.size())
            .context("crate size overflow")?;
        ensure!(
            total <= MAX_UNPACKED_BYTES,
            "crate archive exceeds uncompressed size limit"
        );
        ensure!(
            entry.header().entry_type().is_file(),
            "crate archive has a non-file entry"
        );
        let path = std::str::from_utf8(&entry.path_bytes())?.to_owned();
        ensure!(
            path.starts_with(&root),
            "crate archive path is outside its package root"
        );
        let relative = &path[root.len()..];
        ensure!(
            !relative.is_empty()
                && relative
                    .split('/')
                    .all(|part| !matches!(part, "" | "." | ".."))
                && !matches!(relative, SOURCE_LOCK | "Cargo.lock")
                && !relative.starts_with("target/"),
            "crate archive has an invalid path"
        );
        let destination = stage.join(relative);
        fs::create_dir_all(destination.parent().context("crate file parent")?)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)?;
        let copied = std::io::copy(&mut entry, &mut file)?;
        ensure!(copied == entry.size(), "crate archive entry is truncated");
        file.flush()?;
    }
    let manifest = fs::read_to_string(stage.join("Cargo.toml"))
        .context("crate archive is missing Cargo.toml")?;
    ensure!(
        manifest.len() <= 1024 * 1024,
        "crate manifest exceeds size limit"
    );
    let manifest: toml::Value = toml::from_str(&manifest)?;
    ensure!(
        manifest["package"]["name"].as_str() == Some(release.package.as_str()),
        "crate package name does not match signed release"
    );
    ensure!(
        manifest["package"]["version"].as_str() == Some(release.version.as_str()),
        "crate package version does not match signed release"
    );
    ensure!(
        manifest["package"]["metadata"]["lenso"]["plugin-id"].as_str()
            == Some(release.plugin_id.as_str()),
        "crate Plugin ID does not match signed release"
    );
    Ok(())
}

fn same_tree(expected: &Path, actual: &Path) -> anyhow::Result<bool> {
    if !actual.is_dir() || actual.is_symlink() {
        return Ok(false);
    }
    let mut expected_entries = fs::read_dir(expected)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<Vec<_>, _>>()?;
    let mut actual_entries = fs::read_dir(actual)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<Vec<_>, _>>()?;
    expected_entries.sort();
    actual_entries.sort();
    if expected_entries != actual_entries {
        return Ok(false);
    }
    for name in expected_entries {
        let expected_child: PathBuf = expected.join(&name);
        let actual_child: PathBuf = actual.join(&name);
        let metadata = fs::symlink_metadata(&actual_child)?;
        if metadata.file_type().is_symlink() {
            return Ok(false);
        }
        if expected_child.is_dir() {
            if !same_tree(&expected_child, &actual_child)? {
                return Ok(false);
            }
        } else if !metadata.is_file()
            || metadata.len() != fs::metadata(&expected_child)?.len()
            || fs::read(&expected_child)? != fs::read(&actual_child)?
        {
            return Ok(false);
        }
    }
    Ok(true)
}
