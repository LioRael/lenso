use std::{
    collections::BTreeMap,
    fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
};

use anyhow::{Context as _, bail, ensure};
use ed25519_dalek::VerifyingKey;
use lenso_plugin_catalog::{
    Trust,
    linked_cargo::{self, LinkedCargoIntegration},
};
use serde::Deserialize;

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
                    .all(|part| !matches!(part, "" | "." | "..")),
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
