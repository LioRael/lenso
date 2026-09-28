use std::{
    fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context as _, bail, ensure};
use clap::Args;
use lenso_app_authoring::{
    BundleMutation,
    bundle_archive::{
        PluginArchiveDownloadPolicy, PluginArchiveIdentity, PluginReleaseIdentity,
        VerifiedPluginArchive,
    },
};
use lenso_plugin_catalog::{Artifact, Release, verify};
use serde::{Deserialize, Serialize};

pub(crate) mod checkpoint;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PortableSourceLock {
    schema_version: u32,
    catalog_id: String,
    plugin_id: String,
    version: String,
    artifact: Artifact,
}

#[derive(Clone, Debug, Args)]
#[command(disable_version_flag = true)]
pub struct SignedInstallArgs {
    /// Exact Plugin ID in the signed Portable snapshot.
    plugin_id: String,
    /// Exact immutable Release version; no latest selection.
    #[arg(long)]
    version: String,
    /// Exact signed Portable snapshot envelope.
    #[arg(long)]
    snapshot: PathBuf,
    /// Public trust configuration for this catalog.
    #[arg(long)]
    trust: PathBuf,
    /// Local downloaded `.lenso-plugin` archive matching the signed bytes.
    #[arg(long, conflicts_with = "origin")]
    archive: Option<PathBuf>,
    /// Independently allowed HTTPS artifact origin; fetches the signed URL.
    #[arg(long, conflicts_with = "archive")]
    origin: Option<String>,
    /// Replace an already installed root Bundle after candidate validation.
    #[arg(long)]
    replace: bool,
    /// Built App Plugin Root. Defaults to the current directory.
    #[arg(long)]
    root: Option<PathBuf>,
}

#[derive(Clone, Copy)]
pub(crate) struct SignedReleaseInput<'a> {
    pub plugin_id: &'a str,
    pub version: &'a str,
    pub snapshot: &'a Path,
    pub trust: &'a Path,
    pub archive: Option<&'a Path>,
    pub origin: Option<&'a str>,
}

pub(super) fn install(args: SignedInstallArgs) -> anyhow::Result<()> {
    let root = fs::canonicalize(super::project_root(args.root.clone())?)?;
    ensure!(root.is_dir(), "App Plugin Root must be a directory");
    let input = SignedReleaseInput {
        plugin_id: &args.plugin_id,
        version: &args.version,
        snapshot: &args.snapshot,
        trust: &args.trust,
        archive: args.archive.as_deref(),
        origin: args.origin.as_deref(),
    };
    validate_input(input)?;
    let selected = {
        let lock = lock_app(&root)?;
        select(&root, &lock, input)?.0
    };
    let archive = acquire(input, &selected)?;

    let app_lock = lock_app(&root)?;
    let (current, accepted, previous) = select(&root, &app_lock, input)?;
    ensure!(
        current == selected,
        "signed Portable release changed during acquisition; retry the exact selection"
    );
    let destination = root
        .join("plugins")
        .join(&args.plugin_id)
        .join("plugin.lenso-plugin");
    if destination.exists() {
        if same_tree(&archive.directory(), &destination)? {
            preflight_store(&root, &args.plugin_id, &args.version)?;
            checkpoint::persist(&root, &app_lock, &accepted, previous.as_ref())?;
            retain_verified_archive(&root, &args.plugin_id, &args.version, &archive)?;
            println!(
                "Signed Portable Plugin `{}` {} is already installed.",
                args.plugin_id, args.version
            );
            return Ok(());
        }
        ensure!(
            args.replace,
            "installed Plugin Bundle differs; use --replace for an exact signed update"
        );
    }
    let mutation = if args.replace {
        BundleMutation::Replace
    } else {
        BundleMutation::Add
    };
    let prepared = archive.prepare_mutation(&root, mutation)?;
    preflight_store(&root, &args.plugin_id, &args.version)?;
    if args.replace {
        let current = lenso_plugin_bundle::verify_bundle_directory(prepared.destination())?;
        preflight_store(&root, &args.plugin_id, &current.release_version)?;
        retain_previous_signed_bundle(&root, &args.plugin_id, prepared.destination())?;
    }
    checkpoint::persist(&root, &app_lock, &accepted, previous.as_ref())?;
    retain_verified_archive(&root, &args.plugin_id, &args.version, &archive)?;
    prepared.commit()?;
    println!(
        "{} signed Portable Plugin `{}` {}. Run `lenso app check` and start the candidate Host to verify Ready.",
        if args.replace {
            "Replaced"
        } else {
            "Installed"
        },
        args.plugin_id,
        args.version,
    );
    Ok(())
}

fn validate_input(input: SignedReleaseInput<'_>) -> anyhow::Result<()> {
    lenso_app_authoring::identity::validate_plugin_id_v1(input.plugin_id)?;
    lenso_app_authoring::identity::validate_release_version(input.version)?;
    ensure!(
        input.archive.is_some() != input.origin.is_some(),
        "choose exactly one of --archive or --origin"
    );
    Ok(())
}

fn select(
    root: &Path,
    app_lock: &fs::File,
    input: SignedReleaseInput<'_>,
) -> anyhow::Result<(
    Release,
    lenso_plugin_catalog::Checkpoint,
    Option<lenso_plugin_catalog::Checkpoint>,
)> {
    let trust = crate::app::read_signed_portable_trust(input.trust)?;
    let checkpoint = checkpoint::read(root, app_lock, &trust.catalog_id)?;
    let snapshot = crate::app::read_signed_portable_snapshot(input.snapshot)?;
    let now = now()?;
    let verified = verify(&snapshot, &trust, checkpoint.as_ref(), now)?;
    Ok((
        verified
            .select(input.plugin_id, input.version, now)?
            .clone(),
        verified.checkpoint().clone(),
        checkpoint,
    ))
}

fn acquire(
    input: SignedReleaseInput<'_>,
    selected: &Release,
) -> anyhow::Result<VerifiedPluginArchive> {
    let transport = PluginArchiveIdentity {
        size: selected.artifact.size,
        sha256: selected.artifact.digest.clone(),
    };
    let identity = PluginReleaseIdentity {
        plugin_id: selected.plugin_id.clone(),
        release_version: selected.version.clone(),
        manifest_digest: selected.artifact.manifest_digest.clone(),
    };
    if let Some(path) = input.archive {
        VerifiedPluginArchive::read_release(open_regular(path)?, &transport, &identity)
    } else {
        let origin = input.origin.context("--origin required")?;
        PluginArchiveDownloadPolicy::new(&[origin.to_owned()])?.download_release(
            &selected.artifact.url,
            &transport,
            &identity,
        )
    }
}

fn now() -> anyhow::Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

fn open_regular(path: &Path) -> anyhow::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};
        options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
        let file = options
            .open(path)
            .with_context(|| format!("open Portable archive {}", path.display()))?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file() && metadata.nlink() == 1,
            "Portable archive must be a single-link regular file"
        );
        Ok(file)
    }
    #[cfg(not(unix))]
    {
        ensure!(
            fs::symlink_metadata(path)?.is_file(),
            "Portable archive must be a regular file"
        );
        let file = options.open(path)?;
        ensure!(
            file.metadata()?.is_file(),
            "Portable archive must be a regular file"
        );
        Ok(file)
    }
}

fn retain_verified_archive(
    root: &Path,
    plugin_id: &str,
    version: &str,
    archive: &VerifiedPluginArchive,
) -> anyhow::Result<()> {
    preflight_store(root, plugin_id, version)?;
    let directory = super::plugin_store(root, plugin_id);
    fs::create_dir_all(&directory)?;
    let destination = directory.join(format!("{version}.lenso-plugin"));
    publish_immutable_archive(&destination, archive)
}

fn preflight_store(root: &Path, plugin_id: &str, version: &str) -> anyhow::Result<()> {
    let relative = Path::new(".lenso/plugin-store")
        .join(plugin_id)
        .join(format!("{version}.lenso-plugin"));
    crate::app::convention_authoring::writable_path(root, &relative)
}

fn retain_previous_signed_bundle(
    root: &Path,
    plugin_id: &str,
    bundle: &Path,
) -> anyhow::Result<()> {
    let current = lenso_plugin_bundle::verify_bundle_directory(bundle)?;
    ensure!(
        current.plugin_id == plugin_id,
        "current Plugin identity changed"
    );
    let retained = super::plugin_store(root, plugin_id)
        .join(format!("{}.lenso-plugin", current.release_version));
    match fs::symlink_metadata(&retained) {
        Ok(_) => {
            let _ = open_regular(&retained)?;
            lenso_app_authoring::bundle_archive::with_bundle_directory(&retained, |directory| {
                let previous = lenso_plugin_bundle::verify_bundle_directory(directory)?;
                ensure!(
                    previous.plugin_id == current.plugin_id
                        && previous.release_version == current.release_version
                        && previous.manifest_digest == current.manifest_digest
                        && same_tree(directory, bundle)?,
                    "retained signed Portable Release differs from the installed Bundle"
                );
                Ok(())
            })
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => bail!(
            "signed Portable replacement needs the exact retained archive for the current Release; install a fresh Root instead"
        ),
        Err(error) => Err(error.into()),
    }
}

fn publish_immutable_archive(
    destination: &Path,
    archive: &VerifiedPluginArchive,
) -> anyhow::Result<()> {
    let directory = destination.parent().context("Portable archive parent")?;
    let mut staged = tempfile::NamedTempFile::new_in(directory)?;
    std::io::copy(&mut archive.open_archive()?, &mut staged)?;
    staged.flush()?;
    staged.as_file().sync_all()?;
    if fs::symlink_metadata(destination).is_ok() {
        let existing = open_regular(destination)?;
        let length = existing.metadata()?.len();
        ensure!(
            length == staged.as_file().metadata()?.len()
                && super::readers_equal_exact(existing, staged.reopen()?, length)?,
            "retained Portable Release at {} has different immutable bytes",
            destination.display()
        );
        return Ok(());
    }
    staged
        .persist_noclobber(destination)
        .map_err(|error| error.error)?;
    Ok(())
}

/// A source App keeps the exact signed Bundle as an explicit local shared source.
/// It does not grant built-root mutation authority or execute the Plugin.
pub(crate) fn adopt_source(
    root: &Path,
    source: &str,
    snapshot: &Path,
    trust: &Path,
    local_archive: Option<&Path>,
    origin: Option<&str>,
    replace: bool,
) -> anyhow::Result<()> {
    ensure!(root.is_dir(), "source App root must be a directory");
    ensure!(
        !replace,
        "signed Portable source replacement requires explicit app unadopt, then app add"
    );
    let (plugin_id, version) = source
        .split_once('@')
        .context("signed Portable source must be exact PLUGIN_ID@VERSION")?;
    let input = SignedReleaseInput {
        plugin_id,
        version,
        snapshot,
        trust,
        archive: local_archive,
        origin,
    };
    validate_input(input)?;
    let selected = {
        let lock = lock_app(root)?;
        select(root, &lock, input)?.0
    };
    let archive = acquire(input, &selected)?;
    preflight_source_bundle(&archive, plugin_id, version)?;
    let app_lock = lock_app(root)?;
    let (current, accepted, previous) = select(root, &app_lock, input)?;
    ensure!(
        current == selected,
        "signed Portable release changed during acquisition; retry the exact selection"
    );
    let relative = Path::new("vendor/lenso/portable")
        .join(plugin_id)
        .join(format!("{version}.lenso-plugin"));
    let destination = root.join(&relative);
    let source_lock = PortableSourceLock {
        schema_version: 1,
        catalog_id: accepted.catalog_id.clone(),
        plugin_id: plugin_id.to_owned(),
        version: version.to_owned(),
        artifact: selected.artifact.clone(),
    };
    let lock_path = source_lock_path(&destination, version)?;
    crate::app::convention_authoring::preflight_source_adoption(root, plugin_id)?;
    crate::app::convention_authoring::writable_path(root, &relative)?;
    crate::app::convention_authoring::writable_path(
        root,
        lock_path
            .strip_prefix(root)
            .context("Portable lock path outside App")?,
    )?;
    ensure_existing_source_lock(&lock_path, &source_lock)?;
    let discovered = lenso_app_authoring::discovery::discover(root)?;
    if let Some(candidate) = discovered
        .candidates
        .iter()
        .find(|candidate| candidate.plugin_id == plugin_id)
    {
        ensure!(
            candidate.project == destination,
            "Plugin `{plugin_id}` is already supplied by {}; remove that source before adopting the signed Portable release",
            candidate.project.display()
        );
        ensure!(
            candidate.release_version == version,
            "existing source App Plugin `{plugin_id}` has a different version"
        );
    }
    if fs::symlink_metadata(&destination).is_ok() {
        let existing = open_regular(&destination)?;
        let length = existing.metadata()?.len();
        ensure!(
            length == archive.open_archive()?.metadata()?.len()
                && super::readers_equal_exact(existing, archive.open_archive()?, length)?,
            "existing Portable source differs from signed Release archive"
        );
    }
    checkpoint::persist(root, &app_lock, &accepted, previous.as_ref())?;
    let directory = destination.parent().context("Portable source parent")?;
    fs::create_dir_all(directory)?;
    publish_immutable_archive(&destination, &archive)?;
    publish_source_lock(&lock_path, &source_lock)?;
    // Ordinary App source adoption retains the source-App approval boundary and
    // the established Plugin Root intent semantics.
    crate::app::convention_authoring::adopt(
        root.to_path_buf(),
        destination
            .to_str()
            .context("Portable source path UTF-8")?
            .to_owned(),
        false,
    )?;
    println!(
        "Adopted exact signed Portable Release `{plugin_id}@{version}` as a source App Bundle."
    );
    Ok(())
}

fn preflight_source_bundle(
    archive: &VerifiedPluginArchive,
    plugin_id: &str,
    version: &str,
) -> anyhow::Result<()> {
    let probe = tempfile::tempdir()?;
    fs::write(
        probe.path().join("lenso.toml"),
        toml::to_string(&serde_json::json!({
            "plugin_sources": [archive.directory()]
        }))?,
    )?;
    let report = lenso_app_authoring::discovery::discover(probe.path())?;
    let [candidate] = report.candidates.as_slice() else {
        bail!("signed Portable archive must contain one discoverable Bundle candidate");
    };
    ensure!(
        candidate.plugin_id == plugin_id
            && candidate.release_version == version
            && candidate.format == "bundle",
        "signed Portable archive cannot be selected as this source App Plugin"
    );
    Ok(())
}

fn source_lock_path(archive: &Path, version: &str) -> anyhow::Result<PathBuf> {
    Ok(archive
        .parent()
        .context("Portable source parent")?
        .join(format!("{version}.lenso-plugin.lock.json")))
}

fn read_source_lock(path: &Path) -> anyhow::Result<Option<PortableSourceLock>> {
    let mut file = match open_regular(path) {
        Ok(file) => file,
        Err(error)
            if error
                .root_cause()
                .downcast_ref::<std::io::Error>()
                .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(4097)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 4096,
        "Portable source lock exceeds size limit"
    );
    Ok(Some(
        serde_json::from_slice(&bytes).context("decode Portable source lock")?,
    ))
}

fn ensure_existing_source_lock(path: &Path, expected: &PortableSourceLock) -> anyhow::Result<()> {
    if let Some(existing) = read_source_lock(path)? {
        ensure!(
            existing.schema_version == expected.schema_version
                && existing.catalog_id == expected.catalog_id
                && existing.plugin_id == expected.plugin_id
                && existing.version == expected.version
                && existing.artifact == expected.artifact,
            "existing Portable source lock differs from exact signed Release"
        );
    }
    Ok(())
}

fn publish_source_lock(path: &Path, lock: &PortableSourceLock) -> anyhow::Result<()> {
    if read_source_lock(path)?.is_some() {
        return ensure_existing_source_lock(path, lock);
    }
    let mut staged =
        tempfile::NamedTempFile::new_in(path.parent().context("Portable source lock parent")?)?;
    staged.write_all(&serde_json::to_vec_pretty(lock)?)?;
    staged.flush()?;
    staged.as_file().sync_all()?;
    staged
        .persist_noclobber(path)
        .map_err(|error| error.error)?;
    Ok(())
}

pub(crate) fn verify_source_candidate(
    root: &Path,
    candidate: &lenso_app_authoring::discovery::Candidate,
) -> anyhow::Result<()> {
    let (expected, lock) = source_lock_for_candidate(root, candidate)?;
    verify_source_archive(&expected, &lock)?;
    Ok(())
}

/// Copy only privately verified release bytes into the output stage. The
/// staged artifact is checked again, so an App edit between discovery and
/// assembly cannot substitute a different Bundle for the adopted lock.
pub(crate) fn stage_source_archive(
    root: &Path,
    candidate: &lenso_app_authoring::discovery::Candidate,
    destination: &Path,
) -> anyhow::Result<()> {
    let (expected, lock) = source_lock_for_candidate(root, candidate)?;
    let verified = verify_source_archive(&expected, &lock)?;
    let mut staged = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    std::io::copy(&mut verified.open_archive()?, &mut staged)?;
    staged.sync_all()?;
    drop(staged);
    verify_source_archive(destination, &lock)?;
    Ok(())
}

fn source_lock_for_candidate(
    root: &Path,
    candidate: &lenso_app_authoring::discovery::Candidate,
) -> anyhow::Result<(PathBuf, PortableSourceLock)> {
    let expected = fs::canonicalize(root)?
        .join("vendor/lenso/portable")
        .join(&candidate.plugin_id)
        .join(format!("{}.lenso-plugin", candidate.release_version));
    ensure!(
        candidate.project == expected && candidate.format == "bundle",
        "unrecognized source under reserved signed Portable vendor directory: {}",
        candidate.project.display()
    );
    let lock_path = source_lock_path(&expected, &candidate.release_version)?;
    let lock =
        read_source_lock(&lock_path)?.context("signed Portable source lacks exact Release lock")?;
    ensure!(
        lock.schema_version == 1
            && lock.plugin_id == candidate.plugin_id
            && lock.version == candidate.release_version,
        "signed Portable source lock identity changed"
    );
    Ok((expected, lock))
}

fn verify_source_archive(
    path: &Path,
    lock: &PortableSourceLock,
) -> anyhow::Result<VerifiedPluginArchive> {
    let transport = PluginArchiveIdentity {
        size: lock.artifact.size,
        sha256: lock.artifact.digest.clone(),
    };
    let identity = PluginReleaseIdentity {
        plugin_id: lock.plugin_id.clone(),
        release_version: lock.version.clone(),
        manifest_digest: lock.artifact.manifest_digest.clone(),
    };
    VerifiedPluginArchive::read_release(open_regular(path)?, &transport, &identity)
}

/// Unselect the source and generated intent. The signed archive remains an
/// immutable local audit input, but App discovery/build no longer selects it.
pub(crate) fn unadopt_source(root: &Path, source: &str) -> anyhow::Result<()> {
    let (plugin_id, version) = source
        .split_once('@')
        .context("signed Portable source must be exact PLUGIN_ID@VERSION")?;
    lenso_app_authoring::identity::validate_plugin_id_v1(plugin_id)?;
    lenso_app_authoring::identity::validate_release_version(version)?;
    let _app_lock = lock_app(root)?;
    let source_relative = Path::new("vendor/lenso/portable")
        .join(plugin_id)
        .join(format!("{version}.lenso-plugin"));
    let intent_relative = Path::new("plugins").join(plugin_id);
    for relative in [
        source_relative.as_path(),
        intent_relative.as_path(),
        Path::new("lenso.toml"),
        Path::new(".lenso/trash/portable"),
    ] {
        crate::app::convention_authoring::writable_path(root, relative)?;
    }
    let config_path = root.join("lenso.toml");
    let config_before = fs::read(&config_path).context("source App lenso.toml is missing")?;
    let mut document: toml::Value = toml::from_str(std::str::from_utf8(&config_before)?)?;
    let sources = document
        .get_mut("plugin_sources")
        .and_then(toml::Value::as_array_mut)
        .context("lenso.toml plugin_sources array is missing")?;
    let selected = source_relative
        .to_str()
        .context("Portable source path UTF-8")?;
    let occurrences = sources
        .iter()
        .filter(|entry| entry.as_str() == Some(selected))
        .count();
    ensure!(
        occurrences <= 1,
        "signed Portable source is selected more than once"
    );
    let intent = root.join(&intent_relative);
    let intent_exists = match fs::symlink_metadata(&intent) {
        Ok(metadata) if metadata.is_dir() => true,
        Ok(_) => bail!("Portable Plugin Root intent is not a regular directory"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    if occurrences == 0 {
        println!(
            "Signed Portable source `{source}` is already unadopted; exact archive and any unrelated intent are retained."
        );
        return Ok(());
    }
    if occurrences == 1 {
        ensure!(
            open_regular(&root.join(&source_relative)).is_ok(),
            "selected signed Portable source archive is missing or not a regular file"
        );
    }
    if intent_exists {
        let mut entries = fs::read_dir(&intent)?;
        let entry = entries
            .next()
            .transpose()?
            .context("Portable Plugin Root intent is empty")?;
        ensure!(
            entries.next().is_none(),
            "Portable Plugin Root intent has user changes; preserve it"
        );
        ensure!(
            entry.file_name() == "default.toml"
                && entry.file_type()?.is_file()
                && fs::read(entry.path())? == b"# Explicit local Plugin adoption\n",
            "Portable Plugin Root intent has user changes; preserve it"
        );
    }
    let mut staged_config = if occurrences == 1 {
        sources.retain(|entry| entry.as_str() != Some(selected));
        let mut staged = tempfile::NamedTempFile::new_in(root)?;
        staged.write_all(toml::to_string_pretty(&document)?.as_bytes())?;
        staged.flush()?;
        staged.as_file().sync_all()?;
        Some(staged)
    } else {
        None
    };
    ensure!(
        fs::read(&config_path)? == config_before,
        "lenso.toml changed during Portable unadoption"
    );
    if intent_exists {
        let trash = root.join(".lenso/trash/portable");
        fs::create_dir_all(&trash)?;
        let retained = trash.join(format!("{plugin_id}@{version}-{}", uuid::Uuid::now_v7()));
        fs::rename(&intent, &retained)?;
    }
    if let Some(staged) = staged_config.take() {
        ensure!(
            fs::read(&config_path)? == config_before,
            "lenso.toml changed during Portable unadoption; generated intent is in .lenso/trash/portable"
        );
        staged.persist(&config_path).map_err(|error| error.error)?;
    }
    println!(
        "Unadopted signed Portable source `{source}`; exact archive retained under vendor/lenso/portable for audit."
    );
    Ok(())
}

fn same_tree(expected: &Path, actual: &Path) -> anyhow::Result<bool> {
    let expected_meta = fs::symlink_metadata(expected)?;
    let actual_meta = fs::symlink_metadata(actual)?;
    if expected_meta.file_type().is_dir() && actual_meta.file_type().is_dir() {
        let entries = |path: &Path| -> anyhow::Result<Vec<_>> {
            let mut names = fs::read_dir(path)?
                .map(|entry| entry.map(|entry| entry.file_name()))
                .collect::<Result<Vec<_>, _>>()?;
            names.sort();
            Ok(names)
        };
        let names = entries(expected)?;
        if names != entries(actual)? {
            return Ok(false);
        }
        for name in names {
            if !same_tree(&expected.join(&name), &actual.join(&name))? {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    if expected_meta.file_type().is_file() && actual_meta.file_type().is_file() {
        return Ok(expected_meta.len() == actual_meta.len()
            && super::readers_equal_exact(
                fs::File::open(expected)?,
                fs::File::open(actual)?,
                expected_meta.len(),
            )?);
    }
    Ok(false)
}

#[cfg(unix)]
fn lock_app(root: &Path) -> anyhow::Result<fs::File> {
    let file = fs::File::open(root)?;
    ensure!(
        file.metadata()?.is_dir(),
        "App Plugin Root must be a directory"
    );
    file.lock()?;
    Ok(file)
}

#[cfg(not(unix))]
fn lock_app(_root: &Path) -> anyhow::Result<fs::File> {
    bail!("signed Portable installation locking is unsupported on this platform")
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Write as _, path::Path};

    use lenso_app_authoring::bundle_archive::{PluginArchiveIdentity, VerifiedPluginArchive};
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    use super::{
        PortableSourceLock, retain_previous_signed_bundle, stage_source_archive,
        verify_source_candidate,
    };

    fn stored_zip(source: &Path, destination: &Path) {
        fn collect(root: &Path, directory: &Path, files: &mut Vec<std::path::PathBuf>) {
            for entry in fs::read_dir(directory).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    collect(root, &path, files);
                } else {
                    files.push(path.strip_prefix(root).unwrap().to_path_buf());
                }
            }
        }
        let mut files = Vec::new();
        collect(source, source, &mut files);
        files.sort();
        let mut zip = ZipWriter::new(fs::File::create(destination).unwrap());
        for relative in files {
            let name = relative.to_string_lossy().replace('\\', "/");
            zip.start_file(
                name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(&fs::read(source.join(relative)).unwrap())
                .unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    #[ignore = "requires LENSO_TEST_PLUGIN_ARCHIVE from a real CLI pack"]
    fn signed_replacement_keeps_original_stored_publisher_zip_bytes() {
        let bytes = fs::read(std::env::var("LENSO_TEST_PLUGIN_ARCHIVE").unwrap()).unwrap();
        let bundle = VerifiedPluginArchive::read(
            bytes.as_slice(),
            &PluginArchiveIdentity {
                size: bytes.len() as u64,
                sha256: lenso_plugin_catalog::digest(&bytes),
            },
        )
        .unwrap();
        let root = tempfile::tempdir().unwrap();
        let plugin_id = &bundle.bundle().plugin_id;
        let retained = root
            .path()
            .join(".lenso/plugin-store")
            .join(plugin_id)
            .join(format!("{}.lenso-plugin", bundle.bundle().release_version));
        fs::create_dir_all(retained.parent().unwrap()).unwrap();
        stored_zip(&bundle.directory(), &retained);
        let before = fs::read(&retained).unwrap();
        let canonical = root.path().join("canonical.lenso-plugin");
        lenso_app_authoring::bundle_archive::archive_bundle(&bundle.directory(), &canonical)
            .unwrap();
        assert_ne!(before, fs::read(canonical).unwrap());

        retain_previous_signed_bundle(root.path(), plugin_id, &bundle.directory()).unwrap();
        assert_eq!(fs::read(&retained).unwrap(), before);
        fs::remove_file(&retained).unwrap();
        assert!(
            retain_previous_signed_bundle(root.path(), plugin_id, &bundle.directory()).is_err()
        );
        assert!(!retained.exists());
    }

    #[test]
    #[ignore = "requires LENSO_TEST_PLUGIN_ARCHIVE from a real CLI pack"]
    fn staged_source_archive_is_exactly_locked_even_after_source_changes() {
        let bytes = fs::read(std::env::var("LENSO_TEST_PLUGIN_ARCHIVE").unwrap()).unwrap();
        let verified = VerifiedPluginArchive::read(
            bytes.as_slice(),
            &PluginArchiveIdentity {
                size: bytes.len() as u64,
                sha256: lenso_plugin_catalog::digest(&bytes),
            },
        )
        .unwrap();
        let root = tempfile::tempdir().unwrap();
        let id = &verified.bundle().plugin_id;
        let version = &verified.bundle().release_version;
        let source = root
            .path()
            .join("vendor/lenso/portable")
            .join(id)
            .join(format!("{version}.lenso-plugin"));
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, &bytes).unwrap();
        let lock = PortableSourceLock {
            schema_version: 1,
            catalog_id: "portable-test".into(),
            plugin_id: id.clone(),
            version: version.clone(),
            artifact: lenso_plugin_catalog::Artifact {
                url: "https://example.com/plugin.lenso-plugin".into(),
                digest: lenso_plugin_catalog::digest(&bytes),
                size: bytes.len() as u64,
                manifest_digest: verified.bundle().manifest_digest.clone(),
            },
        };
        fs::write(
            source
                .parent()
                .unwrap()
                .join(format!("{version}.lenso-plugin.lock.json")),
            serde_json::to_vec(&lock).unwrap(),
        )
        .unwrap();
        let candidate = lenso_app_authoring::discovery::Candidate {
            composite: None,
            surface_owner: None,
            plugin_id: id.clone(),
            release_version: version.clone(),
            project: source.clone(),
            metadata: source.clone(),
            format: "bundle".into(),
            role: lenso_app_authoring::discovery::SourceRole::Shared,
            implementations: vec![],
            published_resources: vec![],
            evidence: "fixture".into(),
        };
        verify_source_candidate(root.path(), &candidate).unwrap();
        let staged = root.path().join("staged.lenso-plugin");
        stage_source_archive(root.path(), &candidate, &staged).unwrap();
        assert_eq!(fs::read(&staged).unwrap(), bytes);

        fs::write(&source, b"changed after the first verification").unwrap();
        let rejected = root.path().join("rejected.lenso-plugin");
        assert!(stage_source_archive(root.path(), &candidate, &rejected).is_err());
        assert!(!rejected.exists());
    }

    #[test]
    #[ignore = "requires LENSO_TEST_PLUGIN_ARCHIVE from a real CLI pack"]
    fn signed_update_from_stored_to_deflated_keeps_exact_old_archive() {
        use ed25519_dalek::SigningKey;
        use lenso_app_plan::authoring::{HostCatalog, HostSlot};
        use lenso_plugin_catalog::{Artifact, Availability, Release, Snapshot, digest, sign};
        use std::time::{SystemTime, UNIX_EPOCH};

        fn copy_tree(source: &Path, destination: &Path) {
            fs::create_dir_all(destination).unwrap();
            for entry in fs::read_dir(source).unwrap() {
                let entry = entry.unwrap();
                let target = destination.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    copy_tree(&entry.path(), &target);
                } else {
                    fs::copy(entry.path(), target).unwrap();
                }
            }
        }
        fn release(bytes: &[u8], bundle: &VerifiedPluginArchive) -> Release {
            Release {
                plugin_id: bundle.bundle().plugin_id.clone(),
                version: bundle.bundle().release_version.clone(),
                publisher_id: "test".into(),
                title: "Signed update test".into(),
                summary: "Exact ZIP retention".into(),
                description: String::new(),
                presentation: None,
                source_url: "https://example.com/source".into(),
                source_revision: "a".repeat(40),
                license: "MIT".into(),
                availability: Availability::Listed,
                artifact: Artifact {
                    url: "https://example.com/plugin.lenso-plugin".into(),
                    digest: digest(bytes),
                    size: bytes.len() as u64,
                    manifest_digest: bundle.bundle().manifest_digest.clone(),
                },
            }
        }
        fn verified(bytes: &[u8]) -> VerifiedPluginArchive {
            VerifiedPluginArchive::read(
                bytes,
                &PluginArchiveIdentity {
                    size: bytes.len() as u64,
                    sha256: lenso_plugin_catalog::digest(bytes),
                },
            )
            .unwrap()
        }

        let original = fs::read(std::env::var("LENSO_TEST_PLUGIN_ARCHIVE").unwrap()).unwrap();
        let source = verified(&original);
        let temporary = tempfile::tempdir().unwrap();
        let stored = temporary.path().join("publisher-stored.lenso-plugin");
        stored_zip(&source.directory(), &stored);
        let v1_bytes = fs::read(&stored).unwrap();
        let v1 = verified(&v1_bytes);
        let next_dir = temporary.path().join("next-bundle");
        copy_tree(&source.directory(), &next_dir);
        let manifest_path = next_dir.join("lenso-plugin.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest["release_version"] = "0.1.1".into();
        manifest["entry"]["descriptor"]["release_version"] = "0.1.1".into();
        fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let deflated = temporary.path().join("publisher-deflated.lenso-plugin");
        lenso_app_authoring::bundle_archive::archive_bundle(&next_dir, &deflated).unwrap();
        let v2_bytes = fs::read(&deflated).unwrap();
        let v2 = verified(&v2_bytes);
        assert_ne!(v1_bytes, v2_bytes);
        assert_eq!(v2.bundle().release_version, "0.1.1");

        let root = temporary.path().join("app");
        fs::create_dir_all(root.join(".lenso")).unwrap();
        let host = HostCatalog::new([HostSlot::many("tool-providers")], [], []);
        fs::write(
            root.join(".lenso/host-catalog.json"),
            serde_json::to_vec(&host).unwrap(),
        )
        .unwrap();
        let key = SigningKey::from_bytes(&[53; 32]);
        let trust = temporary.path().join("trust.json");
        fs::write(
            &trust,
            serde_json::to_vec(&serde_json::json!({
                "catalog_id":"portable-update-test", "key_id":"test-key",
                "public_key_hex":hex::encode(key.verifying_key().as_bytes())
            }))
            .unwrap(),
        )
        .unwrap();
        let snapshot = temporary.path().join("snapshot.json");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let v1_release = release(&v1_bytes, &v1);
        let v2_release = release(&v2_bytes, &v2);
        fs::write(
            &snapshot,
            sign(
                &Snapshot::new(
                    "portable-update-test".into(),
                    2,
                    now - 60,
                    now + 600,
                    vec![v1_release.clone()],
                ),
                "test-key",
                &key,
            )
            .unwrap(),
        )
        .unwrap();
        let plugin_id = v1.bundle().plugin_id.clone();
        let install_args = |version: &str, archive: &Path, replace| super::SignedInstallArgs {
            plugin_id: plugin_id.clone(),
            version: version.into(),
            snapshot: snapshot.clone(),
            trust: trust.clone(),
            archive: Some(archive.to_path_buf()),
            origin: None,
            replace,
            root: Some(root.clone()),
        };
        super::install(install_args(&v1_release.version, &stored, false)).unwrap();
        let retained = root
            .join(".lenso/plugin-store")
            .join(&plugin_id)
            .join(format!("{}.lenso-plugin", v1_release.version));
        assert_eq!(fs::read(&retained).unwrap(), v1_bytes);

        fs::write(
            &snapshot,
            sign(
                &Snapshot::new(
                    "portable-update-test".into(),
                    3,
                    now - 60,
                    now + 600,
                    vec![v1_release.clone(), v2_release.clone()],
                ),
                "test-key",
                &key,
            )
            .unwrap(),
        )
        .unwrap();
        let missing = temporary.path().join("withheld-original.lenso-plugin");
        fs::rename(&retained, &missing).unwrap();
        assert!(super::install(install_args(&v2_release.version, &deflated, true)).is_err());
        assert_eq!(
            lenso_plugin_bundle::verify_bundle_directory(
                &root
                    .join("plugins")
                    .join(&plugin_id)
                    .join("plugin.lenso-plugin")
            )
            .unwrap()
            .release_version,
            v1_release.version
        );
        fs::rename(missing, &retained).unwrap();

        super::install(install_args(&v2_release.version, &deflated, true)).unwrap();
        assert_eq!(fs::read(&retained).unwrap(), v1_bytes);
        assert_eq!(
            lenso_plugin_bundle::verify_bundle_directory(
                &root
                    .join("plugins")
                    .join(&plugin_id)
                    .join("plugin.lenso-plugin")
            )
            .unwrap()
            .release_version,
            v2_release.version
        );
    }
}
