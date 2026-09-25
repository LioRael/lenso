use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{Display, Formatter, Write as _},
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context as _, Result, ensure};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use super::{
    BuildProvenanceFacts, BuildSourceFacts, GeneratedArtifactFacts, PluginFacts, SourceLocation,
};

const MAX_LOCK_BYTES: u64 = 8 * 1024 * 1024;
const MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_GENERATED_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TOTAL_GENERATED_BYTES: u64 = 64 * 1024 * 1024;
const MAX_BUILD_SOURCES: usize = 256;
const MAX_GENERATED_ARTIFACTS: usize = 64;

#[derive(Debug)]
struct ProvenancePath(PathBuf);

impl Display for ProvenancePath {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "build provenance file `{}`", self.0.display())
    }
}

pub(super) fn failure_source(error: &anyhow::Error, root: &Path) -> SourceLocation {
    let relative = error
        .downcast_ref::<ProvenancePath>()
        .map_or_else(|| Path::new("local-sources.json"), |path| path.0.as_path());
    SourceLocation {
        path: root.join(relative),
    }
}

#[derive(Deserialize)]
struct DistributionLock {
    schema: String,
    target: String,
    files: Vec<LockedFile>,
}

#[derive(Deserialize)]
struct LockedFile {
    path: String,
    role: String,
    sha256: String,
    size: u64,
}

#[derive(Deserialize)]
struct LocalSources {
    schema: String,
    target: String,
    sources: Vec<BuildSourceRecord>,
    source_digests: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct BuildSourceRecord {
    plugin_id: String,
    release_version: String,
    role: String,
    #[serde(default)]
    surface_owner: Option<String>,
}

pub(super) fn inspect(
    root: &Path,
    host_target: &str,
    plugins: &[PluginFacts],
) -> Result<Option<BuildProvenanceFacts>> {
    let source_path = root.join("local-sources.json");
    let lock_path = root.join(".lenso/distribution.lock.json");
    let source_present = present(&source_path)?;
    let lock_present = present(&lock_path)
        .with_context(|| ProvenancePath(PathBuf::from(".lenso/distribution.lock.json")))?;
    if !source_present && !lock_present {
        return Ok(None);
    }
    let lock: DistributionLock = (|| {
        ensure!(lock_present, "build provenance has no distribution lock");
        ensure_regular_file_path(root, Path::new(".lenso/distribution.lock.json"))?;
        let lock: DistributionLock =
            serde_json::from_slice(&super::read_bounded(&lock_path, MAX_LOCK_BYTES)?)?;
        ensure!(
            lock.schema == "lenso.local-host-distribution.v1"
                || lock.schema == "lenso.host-distribution.v1",
            "unsupported distribution lock schema"
        );
        ensure!(lock.target == host_target, "Host target changed");
        ensure!(lock.files.len() <= 2048, "distribution has too many files");
        Ok(lock)
    })()
    .with_context(|| ProvenancePath(PathBuf::from(".lenso/distribution.lock.json")))?;

    let (source_entry, generated) = (|| {
        let mut seen = BTreeSet::new();
        let mut source_entry = None;
        let mut generated = Vec::new();
        for file in &lock.files {
            ensure!(safe_relative(&file.path), "invalid distribution file path");
            ensure!(
                seen.insert(file.path.as_str()),
                "duplicate distribution file"
            );
            if file.path == "local-sources.json" {
                ensure!(file.role == "source_provenance", "source role changed");
                source_entry = Some(file);
            } else if file.path.starts_with(".lenso/generated-host/") {
                ensure!(
                    file.role == "build_provenance",
                    "generated Host role changed"
                );
                generated.push(file);
            }
        }
        ensure!(
            generated.len() <= MAX_GENERATED_ARTIFACTS,
            "too many generated Host files"
        );
        ensure!(
            generated
                .iter()
                .try_fold(0_u64, |total, file| total.checked_add(file.size))
                .is_some_and(|total| total <= MAX_TOTAL_GENERATED_BYTES),
            "generated Host files exceed the size limit"
        );
        Ok((source_entry, generated))
    })()
    .with_context(|| ProvenancePath(PathBuf::from(".lenso/distribution.lock.json")))?;
    let mut generated_artifacts = Vec::with_capacity(generated.len());
    for file in generated {
        verified_file(root, file, MAX_GENERATED_BYTES)
            .with_context(|| ProvenancePath(PathBuf::from(&file.path)))?;
        generated_artifacts.push(GeneratedArtifactFacts {
            path: file.path.clone(),
            owner: "lenso_host_build",
            role: file.role.clone(),
            sha256: file.sha256.clone(),
            size: file.size,
        });
    }
    generated_artifacts.sort_by(|left, right| left.path.cmp(&right.path));

    let Some(source_entry) = source_entry else {
        ensure!(!source_present, "unlocked local source provenance");
        return Ok(None);
    };
    ensure!(source_present, "locked source provenance is missing");
    let source_bytes = verified_file(root, source_entry, MAX_SOURCE_BYTES)
        .with_context(|| ProvenancePath(PathBuf::from("local-sources.json")))?;
    let sources: LocalSources = serde_json::from_slice(&source_bytes)
        .with_context(|| ProvenancePath(PathBuf::from("local-sources.json")))?;
    ensure!(
        sources.schema == "lenso.local-sources.v1",
        "source schema changed"
    );
    ensure!(sources.target == host_target, "source target changed");
    ensure!(
        sources.sources.len() <= MAX_BUILD_SOURCES,
        "too many build sources"
    );
    ensure!(
        sources.source_digests.len() == sources.sources.len(),
        "source digest inventory changed"
    );

    let adopted = plugins
        .iter()
        .map(|plugin| (plugin.plugin_id.as_str(), plugin.release_version.as_str()))
        .collect::<BTreeSet<_>>();
    let mut remaining_digests = sources.source_digests;
    let mut build_sources = Vec::with_capacity(sources.sources.len());
    for source in sources.sources {
        ensure!(
            !source.plugin_id.is_empty()
                && source.plugin_id.len() <= 256
                && !source.release_version.is_empty()
                && source.release_version.len() <= 128,
            "invalid build source identity"
        );
        ensure!(
            source.role == "app_owned" || source.role == "shared",
            "invalid build source role"
        );
        ensure!(
            source
                .surface_owner
                .as_ref()
                .is_none_or(|owner| owner.len() <= 256),
            "invalid source owner"
        );
        let digest = remaining_digests
            .remove(&source.plugin_id)
            .context("missing or duplicate build source digest")?;
        ensure!(valid_digest(&digest), "invalid build source digest");
        build_sources.push(BuildSourceFacts {
            matches_adopted_coordinates: adopted
                .contains(&(source.plugin_id.as_str(), source.release_version.as_str())),
            plugin_id: source.plugin_id,
            release_version: source.release_version,
            status: "host_build_input",
            role: source.role,
            source_digest: digest,
            surface_owner: source.surface_owner,
        });
    }
    ensure!(remaining_digests.is_empty(), "unused build source digests");
    build_sources.sort_by(|left, right| left.plugin_id.cmp(&right.plugin_id));

    Ok(Some(BuildProvenanceFacts {
        source_location: SourceLocation { path: source_path },
        build_sources,
        generated_artifacts,
    }))
}

fn present(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn safe_relative(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 4096
        && !value.contains('\\')
        && Path::new(value)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn valid_digest(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn sha256_digest(bytes: &[u8]) -> String {
    let mut digest = String::with_capacity(71);
    digest.push_str("sha256:");
    for byte in Sha256::digest(bytes) {
        write!(&mut digest, "{byte:02x}").expect("writing to String cannot fail");
    }
    digest
}

fn ensure_regular_file_path(root: &Path, relative: &Path) -> Result<fs::Metadata> {
    let mut path = root.to_path_buf();
    let mut components = relative.components().peekable();
    while let Some(component) = components.next() {
        let Component::Normal(part) = component else {
            anyhow::bail!("invalid distribution file path");
        };
        path.push(part);
        let metadata = fs::symlink_metadata(&path)?;
        if components.peek().is_some() {
            ensure!(
                metadata.file_type().is_dir(),
                "distribution parent is not a directory"
            );
        } else {
            ensure!(
                metadata.file_type().is_file(),
                "distribution file is not regular"
            );
            return Ok(metadata);
        }
    }
    anyhow::bail!("empty distribution file path")
}

fn verified_file(root: &Path, file: &LockedFile, max_size: u64) -> Result<Vec<u8>> {
    ensure!(
        file.size <= max_size && valid_digest(&file.sha256),
        "invalid locked file"
    );
    let path = root.join(&file.path);
    let metadata = ensure_regular_file_path(root, Path::new(&file.path))?;
    ensure!(
        metadata.len() == file.size,
        "locked file is not a matching regular file"
    );
    let bytes = super::read_bounded(&path, max_size)?;
    ensure!(bytes.len() as u64 == file.size, "locked file size changed");
    let digest = sha256_digest(&bytes);
    ensure!(digest == file.sha256, "locked file digest changed");
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use serde_json::json;

    use super::{failure_source, inspect, sha256_digest};
    use crate::app::facts::{PluginFacts, SourceLocation};

    fn fixture(root: &Path) {
        fs::create_dir_all(root.join(".lenso/generated-host/src")).unwrap();
        let sources = serde_json::to_vec(&json!({
            "schema": "lenso.local-sources.v1",
            "target": "aarch64-apple-darwin",
            "sources": [
                {"plugin_id":"example.active","release_version":"1.2.3","role":"app_owned","project":"/private/do-not-echo"},
                {"plugin_id":"example.available","release_version":"2.0.0","role":"shared","surface_owner":"example.surface"}
            ],
            "source_digests": {
                "example.active": format!("sha256:{}", "a".repeat(64)),
                "example.available": format!("sha256:{}", "b".repeat(64))
            }
        }))
        .unwrap();
        let generated = b"generated Host source";
        fs::write(root.join("local-sources.json"), &sources).unwrap();
        fs::write(root.join(".lenso/generated-host/src/main.rs"), generated).unwrap();
        let lock = json!({
            "schema": "lenso.local-host-distribution.v1",
            "target": "aarch64-apple-darwin",
            "files": [
                {"path":"local-sources.json","role":"source_provenance","size":sources.len(),"sha256":sha256_digest(&sources)},
                {"path":".lenso/generated-host/src/main.rs","role":"build_provenance","size":generated.len(),"sha256":sha256_digest(generated)}
            ]
        });
        fs::write(
            root.join(".lenso/distribution.lock.json"),
            serde_json::to_vec(&lock).unwrap(),
        )
        .unwrap();
    }

    fn adopted() -> Vec<PluginFacts> {
        vec![PluginFacts {
            plugin_id: "example.active".into(),
            release_version: "1.2.3".into(),
            release_source: "host_catalog",
            source_location: SourceLocation {
                path: "host-build.json".into(),
            },
            instances: Vec::new(),
        }]
    }

    #[test]
    fn reports_locked_host_inputs_without_claiming_every_source_is_adopted() {
        let root = tempfile::tempdir().unwrap();
        fixture(root.path());

        let facts = inspect(root.path(), "aarch64-apple-darwin", &adopted())
            .unwrap()
            .unwrap();
        assert_eq!(facts.build_sources.len(), 2);
        assert!(
            facts
                .build_sources
                .iter()
                .all(|source| source.status == "host_build_input")
        );
        assert!(facts.build_sources[0].matches_adopted_coordinates);
        assert!(!facts.build_sources[1].matches_adopted_coordinates);
        assert_eq!(facts.generated_artifacts.len(), 1);
        assert_eq!(facts.generated_artifacts[0].owner, "lenso_host_build");
        let json = serde_json::to_string(&facts).unwrap();
        assert!(!json.contains("/private/do-not-echo"));
    }

    #[test]
    fn old_distributions_without_provenance_remain_readable() {
        let root = tempfile::tempdir().unwrap();
        assert!(
            inspect(root.path(), "aarch64-apple-darwin", &[])
                .unwrap()
                .is_none()
        );
        fs::create_dir(root.path().join(".lenso")).unwrap();
        fs::write(
            root.path().join(".lenso/distribution.lock.json"),
            serde_json::to_vec(&json!({
                "schema":"lenso.host-distribution.v1",
                "target":"aarch64-apple-darwin",
                "files":[]
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(
            inspect(root.path(), "aarch64-apple-darwin", &[])
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn legacy_generated_files_without_local_sources_are_still_verified() {
        let root = tempfile::tempdir().unwrap();
        let generated_path = ".lenso/generated-host/src/main.rs";
        fs::create_dir_all(root.path().join(".lenso/generated-host/src")).unwrap();
        let generated = b"legacy generated Host source";
        fs::write(root.path().join(generated_path), generated).unwrap();
        let lock = json!({
            "schema": "lenso.host-distribution.v1",
            "target": "aarch64-apple-darwin",
            "files": [{
                "path": generated_path,
                "role": "build_provenance",
                "size": generated.len(),
                "sha256": sha256_digest(generated)
            }]
        });
        fs::write(
            root.path().join(".lenso/distribution.lock.json"),
            serde_json::to_vec(&lock).unwrap(),
        )
        .unwrap();

        assert!(
            inspect(root.path(), "aarch64-apple-darwin", &[])
                .unwrap()
                .is_none()
        );

        fs::write(
            root.path().join(generated_path),
            b"tampered generated Host source",
        )
        .unwrap();
        let error = inspect(root.path(), "aarch64-apple-darwin", &[]).unwrap_err();
        assert_eq!(
            failure_source(&error, root.path()).path,
            root.path().join(generated_path)
        );

        fs::remove_file(root.path().join(generated_path)).unwrap();
        let error = inspect(root.path(), "aarch64-apple-darwin", &[]).unwrap_err();
        assert_eq!(
            failure_source(&error, root.path()).path,
            root.path().join(generated_path)
        );
    }

    #[test]
    fn present_but_changed_or_unlocked_provenance_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        fixture(root.path());
        fs::write(root.path().join("local-sources.json"), b"changed").unwrap();
        assert!(inspect(root.path(), "aarch64-apple-darwin", &adopted()).is_err());

        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("local-sources.json"), b"unlocked").unwrap();
        assert!(inspect(root.path(), "aarch64-apple-darwin", &adopted()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_generated_file_is_rejected() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        fixture(root.path());
        fs::remove_file(root.path().join(".lenso/generated-host/src/main.rs")).unwrap();
        symlink(
            root.path().join("local-sources.json"),
            root.path().join(".lenso/generated-host/src/main.rs"),
        )
        .unwrap();
        assert!(inspect(root.path(), "aarch64-apple-darwin", &adopted()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_generated_parent_is_rejected() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fixture(root.path());
        fs::rename(
            root.path().join(".lenso/generated-host/src"),
            outside.path().join("src"),
        )
        .unwrap();
        symlink(
            outside.path().join("src"),
            root.path().join(".lenso/generated-host/src"),
        )
        .unwrap();
        assert!(inspect(root.path(), "aarch64-apple-darwin", &adopted()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_lock_parent_is_rejected() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fixture(root.path());
        fs::rename(root.path().join(".lenso"), outside.path().join(".lenso")).unwrap();
        symlink(outside.path().join(".lenso"), root.path().join(".lenso")).unwrap();
        assert!(inspect(root.path(), "aarch64-apple-darwin", &adopted()).is_err());
    }
}
