//! Exact signed content selection. A copied tree remains App-owned source;
//! copying does not execute an extension or select a runtime Plugin.

use std::{
    fs,
    io::{Read as _, Write as _},
    path::{Component, Path},
};

use anyhow::{Context as _, bail, ensure};
use lenso_plugin_catalog::{
    release_content::{self, BaseKind, Content, ContentKind},
    verify as verify_portable,
};
use serde::Serialize;

use super::{AddArgs, adoption, checkpoint, content_checkpoint, now, read_envelope, read_trust};

const MAX_FILES: usize = 512;
const MAX_UNPACKED_BYTES: u64 = 32 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
const ATTRIBUTION: &str = ".lenso-release-content.json";

#[derive(Serialize)]
struct Preview<'a> {
    schema_version: u32,
    kind: &'static str,
    plugin_id: &'a str,
    version: &'a str,
    content_id: &'a str,
    content_kind: ContentKind,
    base_kind: BaseKind,
    destination: String,
    files: Vec<String>,
    conflict: bool,
    execution: &'static str,
}

pub(crate) fn add(root: &Path, args: &AddArgs) -> anyhow::Result<()> {
    if cfg!(not(unix)) {
        bail!("signed content adoption is unsupported on this platform");
    }
    ensure!(
        args.crate_archive.is_none()
            && args.bundle.is_none()
            && args.archive.is_none()
            && args.origin.is_none()
            && !args.replace,
        "content selection cannot also install or replace a runtime Plugin"
    );
    ensure!(
        args.linked_snapshot.is_some() != args.portable_snapshot.is_some(),
        "content selection needs exactly one signed linked or portable base snapshot"
    );
    let content_snapshot = args
        .content_snapshot
        .as_deref()
        .context("--content-snapshot required")?;
    let content_id = args
        .content_id
        .as_deref()
        .context("--content-id required")?;
    let archive_path = args
        .content_archive
        .as_deref()
        .context("--content-archive required")?;
    let relative = args
        .content_destination
        .as_deref()
        .context("--content-destination required")?;
    let trust_path = args.trust.as_deref().context("--trust required")?;
    let (plugin_id, version) = args
        .source
        .split_once('@')
        .context("content source must be exact PLUGIN_ID@VERSION")?;
    lenso_app_authoring::identity::validate_plugin_id_v1(plugin_id)?;
    lenso_app_authoring::identity::validate_release_version(version)?;
    validate_destination(relative)?;
    super::super::writable_path(root, relative)?;
    let destination = root.join(relative);
    let app_lock = adoption::lock_app(root)?;
    let trust = read_trust(trust_path)?;
    let now = now()?;
    let prior_content = content_checkpoint::read(root, &app_lock, &trust.catalog_id)?;
    let verified_content = release_content::verify(
        &read_envelope(content_snapshot)?,
        &trust,
        prior_content.as_ref(),
        now,
    )?;

    let (release, base_checkpoint) = if let Some(snapshot_path) = args.linked_snapshot.as_deref() {
        let previous = checkpoint::read(root, &app_lock, &trust.catalog_id)?;
        let verified = lenso_plugin_catalog::linked_cargo::verify(
            &read_envelope(snapshot_path)?,
            &trust,
            previous.as_ref(),
            now,
        )?;
        let selected = verified_content.select_linked(&verified, plugin_id, version, now)?;
        (
            selected.clone(),
            BaseCheckpoint::Linked(verified.checkpoint().clone(), previous),
        )
    } else {
        let previous =
            crate::plugins::signed_install::checkpoint::read(root, &app_lock, &trust.catalog_id)?;
        let verified = verify_portable(
            &crate::app::read_signed_portable_snapshot(
                args.portable_snapshot
                    .as_deref()
                    .context("--portable-snapshot required")?,
            )?,
            &trust,
            previous.as_ref(),
            now,
        )?;
        let selected = verified_content.select_portable(&verified, plugin_id, version, now)?;
        (
            selected.clone(),
            BaseCheckpoint::Portable(verified.checkpoint().clone(), previous),
        )
    };
    let content = release.select(content_id)?;
    let archive = read_archive(archive_path, content)?;
    let unpacked = tempfile::tempdir()?;
    let files = unpack(&archive, unpacked.path())?;
    if content.kind == ContentKind::DevelopmentExtension {
        verify_extension_source(unpacked.path(), plugin_id, version)?;
    }
    let conflict = fs::symlink_metadata(&destination).is_ok();
    let preview = Preview {
        schema_version: 1,
        kind: "lenso.signed-release-content-preview",
        plugin_id,
        version,
        content_id,
        content_kind: content.kind,
        base_kind: release.base_kind,
        destination: relative.display().to_string(),
        files,
        conflict,
        execution: "not_selected",
    };
    if args.content_preview {
        println!("{}", serde_json::to_string_pretty(&preview)?);
        return Ok(());
    }
    ensure!(
        !conflict,
        "content destination already exists; user-owned files are never overwritten"
    );
    base_checkpoint.persist(root, &app_lock)?;
    content_checkpoint::persist(
        root,
        &app_lock,
        verified_content.checkpoint(),
        prior_content.as_ref(),
    )?;

    let parent = destination.parent().context("content destination parent")?;
    super::super::writable_path(
        root,
        relative.parent().context("content destination parent")?,
    )?;
    fs::create_dir_all(parent)?;
    let stage = tempfile::Builder::new()
        .prefix(".release-content-")
        .tempdir_in(parent)?;
    copy_tree(unpacked.path(), stage.path(), &preview.files)?;
    fs::write(
        stage.path().join(ATTRIBUTION),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": 1,
            "catalog_id": trust.catalog_id,
            "plugin_id": plugin_id,
            "version": version,
            "base_kind": release.base_kind,
            "base_release_identity": release.base_release_identity,
            "content_id": content_id,
            "content_kind": content.kind,
            "content_digest": content.digest,
            "source_url": content.url,
            "adoption": "editable_copy",
            "execution": "not_selected"
        }))?,
    )?;
    super::super::super::build::publish_new_output(stage.path(), &destination)?;
    println!(
        "Copied signed {}@{} content `{}` to {}; no extension or runtime was selected",
        plugin_id,
        version,
        content_id,
        destination.display()
    );
    if content.kind == ContentKind::DevelopmentExtension {
        println!(
            "Review the copied source, then explicitly run `lenso app add {}` to select this development extension before a build",
            destination.display()
        );
    }
    Ok(())
}

enum BaseCheckpoint {
    Linked(
        lenso_plugin_catalog::linked_cargo::LinkedCargoCheckpoint,
        Option<lenso_plugin_catalog::linked_cargo::LinkedCargoCheckpoint>,
    ),
    Portable(
        lenso_plugin_catalog::Checkpoint,
        Option<lenso_plugin_catalog::Checkpoint>,
    ),
}

impl BaseCheckpoint {
    fn persist(&self, root: &Path, lock: &fs::File) -> anyhow::Result<()> {
        match self {
            Self::Linked(current, previous) => {
                checkpoint::persist(root, lock, current, previous.as_ref())
            }
            Self::Portable(current, previous) => {
                crate::plugins::signed_install::checkpoint::persist(
                    root,
                    lock,
                    current,
                    previous.as_ref(),
                )
            }
        }
    }
}

fn validate_destination(relative: &Path) -> anyhow::Result<()> {
    let raw = relative
        .to_str()
        .context("content destination must be UTF-8")?;
    let parts = relative.components().collect::<Vec<_>>();
    ensure!(
        !raw.contains('\\')
            && !raw.chars().any(char::is_control)
            && raw.split('/').all(|part| !matches!(part, "" | "." | ".."))
            && !parts.is_empty()
            && parts.len() <= 16
            && parts
                .iter()
                .all(|part| matches!(part, Component::Normal(_))),
        "content destination must be a normal App-relative directory"
    );
    let first = parts[0].as_os_str();
    ensure!(
        ![
            "app", "plugins", "vendor", ".lenso", "dist", "intent", "target"
        ]
        .iter()
        .any(|reserved| first == *reserved),
        "content destination cannot enter an auto-discovered or generated App directory"
    );
    Ok(())
}

fn read_archive(path: &Path, content: &Content) -> anyhow::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.file_type().is_file(),
        "content archive must be a regular file"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        ensure!(
            metadata.nlink() == 1,
            "content archive must have one hard link"
        );
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
    }
    let mut file = options.open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() == content.size,
        "content archive size differs from signed reference"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        ensure!(
            metadata.nlink() == 1,
            "content archive must have one hard link"
        );
    }
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(content.size + 1)
        .read_to_end(&mut bytes)?;
    content.verify_bytes(&bytes)?;
    Ok(bytes)
}

fn unpack(bytes: &[u8], stage: &Path) -> anyhow::Result<Vec<String>> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    let mut files = Vec::new();
    let mut total = 0u64;
    for entry in archive.entries().context("invalid content archive")? {
        let mut entry = entry.context("invalid content archive entry")?;
        ensure!(
            entry.header().entry_type().is_file(),
            "content archive contains a non-file entry"
        );
        ensure!(
            entry.size() <= MAX_FILE_BYTES,
            "content archive file exceeds size limit"
        );
        total = total
            .checked_add(entry.size())
            .context("content archive size overflow")?;
        ensure!(
            total <= MAX_UNPACKED_BYTES,
            "content archive exceeds uncompressed size limit"
        );
        let raw = std::str::from_utf8(&entry.path_bytes())?.to_owned();
        ensure!(
            raw.len() <= 1024
                && !raw.chars().any(char::is_control)
                && !raw.contains('\\')
                && !raw.is_empty()
                && raw.split('/').all(|part| !matches!(part, "" | "." | ".."))
                && Path::new(&raw)
                    .components()
                    .all(|part| matches!(part, Component::Normal(_)))
                && raw != ATTRIBUTION,
            "content archive has an invalid path"
        );
        ensure!(
            files.len() < MAX_FILES,
            "content archive has too many files"
        );
        let destination = stage.join(&raw);
        fs::create_dir_all(destination.parent().context("content file parent")?)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)?;
        let copied = std::io::copy(&mut entry, &mut file)?;
        ensure!(copied == entry.size(), "content archive entry is truncated");
        file.flush()?;
        files.push(raw);
    }
    ensure!(!files.is_empty(), "content archive has no files");
    files.sort();
    Ok(files)
}

fn copy_tree(source: &Path, destination: &Path, files: &[String]) -> anyhow::Result<()> {
    for file in files {
        let target = destination.join(file);
        fs::create_dir_all(target.parent().context("content file parent")?)?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        let mut input = fs::File::open(source.join(file))?;
        std::io::copy(&mut input, &mut output)?;
        output.flush()?;
    }
    Ok(())
}

fn verify_extension_source(root: &Path, plugin_id: &str, version: &str) -> anyhow::Result<()> {
    let report = lenso_app_authoring::discovery::discover(root)?;
    let [candidate] = report.candidates.as_slice() else {
        bail!("development extension must contain one Plugin source project");
    };
    ensure!(
        candidate.plugin_id == plugin_id
            && candidate.release_version == version
            && candidate.project == fs::canonicalize(root)?,
        "development extension source differs from signed Plugin identity and version"
    );
    let manifest = match candidate.format.as_str() {
        "bun" => {
            let value: serde_json::Value =
                serde_json::from_slice(&fs::read(root.join("package.json"))?)?;
            value.pointer("/lenso/conventions").cloned()
        }
        "cargo" => {
            let value: toml::Value = toml::from_str(&fs::read_to_string(root.join("Cargo.toml"))?)?;
            value
                .get("package")
                .and_then(|package| package.get("metadata"))
                .and_then(|metadata| metadata.get("lenso"))
                .and_then(|lenso| lenso.get("conventions"))
                .map(serde_json::to_value)
                .transpose()?
        }
        _ => None,
    };
    ensure!(
        manifest
            .as_ref()
            .and_then(serde_json::Value::as_array)
            .is_some_and(|items| !items.is_empty()),
        "development extension needs explicit convention declarations"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::{read_archive, unpack, validate_destination};
    use lenso_plugin_catalog::{
        digest,
        release_content::{Content, ContentKind},
    };

    fn archive(entries: &[(&str, &[u8], tar::EntryType)]) -> Vec<u8> {
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut tar = tar::Builder::new(encoder);
        for &(path, bytes, kind) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(kind);
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, path, bytes).unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn content_paths_reject_escape_generated_roots_and_symlink_archive() {
        for invalid in [
            "../out",
            "/out",
            "app/ext",
            "vendor/ext",
            ".lenso/ext",
            "a/./b",
            "a/\nb",
        ] {
            assert!(
                validate_destination(Path::new(invalid)).is_err(),
                "{invalid}"
            );
        }
        validate_destination(Path::new("frontend/from-release")).unwrap();

        let root = tempfile::tempdir().unwrap();
        let bytes = archive(&[(
            "src/App.tsx",
            b"export const App = 1;",
            tar::EntryType::Regular,
        )]);
        let path = root.path().join("content.tar.gz");
        fs::write(&path, &bytes).unwrap();
        let content = Content {
            id: "template".into(),
            kind: ContentKind::EditableTemplate,
            url: "https://example.test/content.tar.gz".into(),
            digest: digest(&bytes),
            size: bytes.len() as u64,
        };
        read_archive(&path, &content).unwrap();
        let symlink = root.path().join("symlink.tar.gz");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&path, &symlink).unwrap();
            assert!(read_archive(&symlink, &content).is_err());
            let hardlink = root.path().join("hardlink.tar.gz");
            fs::hard_link(&path, &hardlink).unwrap();
            assert!(read_archive(&hardlink, &content).is_err());
        }
        let mut wrong = content.clone();
        wrong.digest = digest(b"wrong");
        assert!(read_archive(&path, &wrong).is_err());
    }

    #[test]
    fn content_archive_rejects_duplicate_and_special_entries() {
        let root = tempfile::tempdir().unwrap();
        let duplicate = archive(&[
            ("src/App.tsx", b"a", tar::EntryType::Regular),
            ("src/App.tsx", b"b", tar::EntryType::Regular),
        ]);
        assert!(unpack(&duplicate, root.path()).is_err());

        let root = tempfile::tempdir().unwrap();
        let symlink = archive(&[("src/link", b"", tar::EntryType::Symlink)]);
        assert!(unpack(&symlink, root.path()).is_err());

        let root = tempfile::tempdir().unwrap();
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(1);
        header.set_mode(0o644);
        header.as_mut_bytes()[..9].copy_from_slice(b"../escape");
        header.set_cksum();
        builder.append(&header, b"x".as_slice()).unwrap();
        let traversal = builder.into_inner().unwrap().finish().unwrap();
        assert!(unpack(&traversal, root.path()).is_err());
        assert!(!root.path().parent().unwrap().join("escape").exists());
    }
}
