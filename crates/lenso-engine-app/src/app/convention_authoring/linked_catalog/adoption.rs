//! Recoverable publication of a verified linked-Cargo source into a local App.
//!
//! The source and configuration make the Plugin available; publishing the
//! Plugin Root intent is the final selection step. Retrying the same signed
//! input converges after an I/O interruption without overwriting an existing
//! source or a concurrent intent entry.

use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, bail, ensure};
use tempfile::{NamedTempFile, TempDir};
use toml_edit::{Array, DocumentMut, Value, value};

const OWNED_EXCLUDE: &str = "# lenso:linked-cargo-exclude";
const OWNED_EXCLUDE_LIST: &str = "# lenso:linked-cargo-exclude-list";

enum StagedIntent {
    New(TempDir),
    MissingDefault(NamedTempFile),
    Existing,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum CommitCheckpoint {
    AfterSource,
    AfterSourceLock,
    AfterWorkspace,
    AfterConfig,
}

pub(super) struct PreparedLinkedAdoption {
    _app_lock: fs::File,
    root: PathBuf,
    destination: PathBuf,
    config: PathBuf,
    config_before: Option<Vec<u8>>,
    config_after: Vec<u8>,
    staged_config: Option<NamedTempFile>,
    workspace_manifest: PathBuf,
    workspace_before: Option<Vec<u8>>,
    workspace_after: Option<Vec<u8>>,
    staged_workspace: Option<NamedTempFile>,
    workspace_exclude_owned: bool,
    intent: PathBuf,
    staged_intent: StagedIntent,
}

impl PreparedLinkedAdoption {
    #[cfg(test)]
    pub(super) fn new(root: &Path, destination: &Path, plugin_id: &str) -> anyhow::Result<Self> {
        let app_lock = lock_app(root)?;
        Self::new_locked(root, destination, plugin_id, app_lock)
    }

    pub(super) fn new_locked(
        root: &Path,
        destination: &Path,
        plugin_id: &str,
        app_lock: fs::File,
    ) -> anyhow::Result<Self> {
        super::super::preflight_source_adoption(root, plugin_id)?;
        super::super::writable_path(root, Path::new("Cargo.toml"))?;
        let workspace_manifest = root.join("Cargo.toml");
        let workspace_before = read_optional_regular(&workspace_manifest)?;
        let app_owns_workspace = workspace_before
            .as_deref()
            .map(|bytes| {
                workspace_document(bytes).map(|document| document.get("workspace").is_some())
            })
            .transpose()?
            .unwrap_or(false);
        if !app_owns_workspace {
            ensure_no_enclosing_workspace(root)?;
        }
        let source_relative = destination
            .strip_prefix(root)
            .context("linked source is outside App root")?;
        let (workspace_after, workspace_exclude_owned) =
            add_workspace_exclude(workspace_before.as_deref(), source_relative)?;
        let staged_workspace = if workspace_after != workspace_before {
            let mut staged = NamedTempFile::new_in(root)?;
            staged.write_all(
                workspace_after
                    .as_deref()
                    .context("updated workspace manifest is missing")?,
            )?;
            Some(staged)
        } else {
            None
        };
        let config = root.join("lenso.toml");
        let config_before = read_optional_regular(&config)?;
        let mut document: toml::Value = match &config_before {
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
            .strip_prefix(root)
            .context("linked source is outside App root")?
            .to_str()
            .context("linked source path UTF-8")?;
        let value = toml::Value::String(relative.to_owned());
        let occurrences = sources.iter().filter(|source| **source == value).count();
        ensure!(occurrences <= 1, "linked source is selected more than once");
        let staged_config = if occurrences == 0 {
            sources.push(value);
            let mut staged = NamedTempFile::new_in(root)?;
            staged.write_all(toml::to_string_pretty(&document)?.as_bytes())?;
            Some(staged)
        } else {
            None
        };
        let config_after = if let Some(staged) = &staged_config {
            fs::read(staged.path())?
        } else {
            config_before
                .as_ref()
                .context("selected source needs lenso.toml")?
                .clone()
        };

        let intent = root.join("plugins").join(plugin_id);
        let default = intent.join("default.toml");
        let staged_intent = match fs::symlink_metadata(&intent) {
            Ok(metadata) if metadata.is_dir() => match fs::symlink_metadata(&default) {
                Ok(metadata) if metadata.is_file() => StagedIntent::Existing,
                Ok(_) => bail!("linked Plugin Root default is not a regular file"),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    let mut staged = NamedTempFile::new_in(root)?;
                    staged.write_all(b"# Explicit local Plugin adoption\n")?;
                    StagedIntent::MissingDefault(staged)
                }
                Err(error) => return Err(error.into()),
            },
            Ok(_) => bail!("linked Plugin Root entry is not a directory"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let staged = tempfile::Builder::new()
                    .prefix(".linked-intent-")
                    .tempdir_in(root)?;
                fs::write(
                    staged.path().join("default.toml"),
                    "# Explicit local Plugin adoption\n",
                )?;
                StagedIntent::New(staged)
            }
            Err(error) => return Err(error.into()),
        };
        Ok(Self {
            _app_lock: app_lock,
            root: root.to_path_buf(),
            destination: destination.to_path_buf(),
            config,
            config_before,
            config_after,
            staged_config,
            workspace_manifest,
            workspace_before,
            workspace_after,
            staged_workspace,
            workspace_exclude_owned,
            intent,
            staged_intent,
        })
    }

    pub(super) fn commit(self, source_stage: &Path) -> anyhow::Result<()> {
        self.commit_with(source_stage, |_| Ok(()))
    }

    pub(super) fn workspace_exclude_owned(&self) -> bool {
        self.workspace_exclude_owned
    }

    fn commit_with(
        mut self,
        source_stage: &Path,
        mut checkpoint: impl FnMut(CommitCheckpoint) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        self.ensure_config_is(&self.config_before)?;
        self.ensure_workspace_is(&self.workspace_before)?;
        let source_relative = self.destination.strip_prefix(&self.root)?;
        super::super::writable_path(&self.root, source_relative)?;
        if fs::symlink_metadata(&self.destination).is_ok() {
            ensure!(
                super::same_tree(source_stage, &self.destination)?,
                "existing linked Cargo source differs from signed archive"
            );
        } else {
            fs::create_dir_all(self.destination.parent().context("linked source parent")?)?;
            super::super::super::build::publish_new_output(source_stage, &self.destination)?;
        }
        checkpoint(CommitCheckpoint::AfterSource)?;

        // A prior CLI wrote the same signed source with the canonical V5/V6
        // lock wire. Upgrade that generated lock before selecting the new
        // workspace exclusion; an interrupted upgrade is safe to retry.
        let staged_lock = source_stage.join(super::SOURCE_LOCK);
        if staged_lock.is_file() {
            let expected = fs::read(&staged_lock)?;
            let destination_lock = self.destination.join(super::SOURCE_LOCK);
            let before = read_optional_regular(&destination_lock)?
                .context("linked Cargo source lock disappeared during adoption")?;
            if before != expected {
                ensure!(
                    super::canonical_prior_source_lock_matches(&expected, &before)?,
                    "existing linked Cargo source lock differs from canonical prior adoption"
                );
                let mut staged = NamedTempFile::new_in(&self.destination)?;
                staged.write_all(&expected)?;
                ensure!(
                    read_optional_regular(&destination_lock)?.as_deref() == Some(before.as_slice()),
                    "linked Cargo source lock changed during adoption retry"
                );
                staged.persist(&destination_lock)?;
            }
        }
        checkpoint(CommitCheckpoint::AfterSourceLock)?;

        self.ensure_config_is(&self.config_before)?;
        self.ensure_workspace_is(&self.workspace_before)?;
        if let Some(staged) = self.staged_workspace.take() {
            staged.persist(&self.workspace_manifest)?;
        }
        checkpoint(CommitCheckpoint::AfterWorkspace)?;

        self.ensure_workspace_is(&self.workspace_after)?;
        if let Some(staged) = self.staged_config.take() {
            if self.config_before.is_some() {
                staged.persist(&self.config)?;
            } else {
                staged.persist_noclobber(&self.config)?;
            }
        }
        checkpoint(CommitCheckpoint::AfterConfig)?;

        // A changed configuration must not acquire a fresh selection intent.
        self.ensure_config_is(&Some(self.config_after.clone()))?;
        self.ensure_workspace_is(&self.workspace_after)?;
        let intent_relative = self.intent.strip_prefix(&self.root)?;
        super::super::writable_path(&self.root, intent_relative)?;
        super::super::writable_path(&self.root, &intent_relative.join("default.toml"))?;
        match self.staged_intent {
            StagedIntent::New(staged) => {
                fs::create_dir_all(self.root.join("plugins"))?;
                super::super::super::build::publish_new_output(staged.path(), &self.intent)?;
            }
            StagedIntent::MissingDefault(staged) => {
                staged.persist_noclobber(self.intent.join("default.toml"))?;
            }
            StagedIntent::Existing => {
                ensure!(
                    fs::symlink_metadata(self.intent.join("default.toml"))?.is_file(),
                    "linked Plugin Root default changed during adoption"
                );
            }
        }
        Ok(())
    }

    fn ensure_config_is(&self, expected: &Option<Vec<u8>>) -> anyhow::Result<()> {
        ensure!(
            read_optional_regular(&self.config)? == *expected,
            "lenso.toml changed during linked Cargo adoption; preserving concurrent edit"
        );
        Ok(())
    }

    fn ensure_workspace_is(&self, expected: &Option<Vec<u8>>) -> anyhow::Result<()> {
        ensure!(
            read_optional_regular(&self.workspace_manifest)? == *expected,
            "Cargo.toml changed during linked Cargo adoption; preserving concurrent edit"
        );
        Ok(())
    }
}

fn managed_exclude(value: &Value) -> bool {
    value
        .decor()
        .prefix()
        .and_then(|prefix| prefix.as_str())
        .is_some_and(|prefix| prefix.contains(OWNED_EXCLUDE))
}

pub(super) fn workspace_document(bytes: &[u8]) -> anyhow::Result<DocumentMut> {
    std::str::from_utf8(bytes)?
        .parse::<DocumentMut>()
        .context("parse App Cargo.toml")
}

pub(super) fn ensure_no_enclosing_workspace(root: &Path) -> anyhow::Result<()> {
    for ancestor in root.ancestors().skip(1) {
        let manifest = ancestor.join("Cargo.toml");
        let Some(bytes) = read_optional_regular(&manifest)? else {
            continue;
        };
        if workspace_document(&bytes)?.get("workspace").is_some() {
            bail!(
                "App root is inside enclosing Cargo workspace {}; add an App-root [workspace] or move the App before adopting a linked Cargo source",
                manifest.display()
            );
        }
    }
    Ok(())
}

pub(super) fn add_workspace_exclude(
    before: Option<&[u8]>,
    source_relative: &Path,
) -> anyhow::Result<(Option<Vec<u8>>, bool)> {
    let Some(before) = before else {
        return Ok((None, false));
    };
    let mut document = workspace_document(before)?;
    let Some(workspace) = document.get_mut("workspace") else {
        return Ok((Some(before.to_vec()), false));
    };
    let workspace = workspace
        .as_table_mut()
        .context("App Cargo.toml [workspace] must be a table")?;
    let package_path = source_relative.join("Cargo.toml");
    if let Some(members) = workspace.get("members") {
        let members = members
            .as_array()
            .context("App Cargo.toml workspace.members must be an array")?;
        for member in members.iter() {
            let member = member
                .as_str()
                .context("App Cargo.toml workspace.members must contain strings")?;
            if member_matches(member, source_relative)? {
                bail!(
                    "App Cargo.toml workspace.members explicitly selects linked Cargo source; remove that member before adoption"
                );
            }
        }
    }
    let created_list = !workspace.contains_key("exclude");
    if created_list {
        workspace["exclude"] = value(Array::new());
    }
    let exclude = workspace["exclude"]
        .as_array_mut()
        .context("App Cargo.toml workspace.exclude must be an array")?;
    let relative = source_relative
        .to_str()
        .context("linked source path must be UTF-8")?;
    let mut exact_owned = false;
    let mut covered = false;
    for existing in exclude.iter() {
        let path = existing
            .as_str()
            .context("App Cargo.toml workspace.exclude must contain strings")?;
        if path == relative && managed_exclude(existing) {
            exact_owned = true;
        }
        if package_path.starts_with(Path::new(path)) {
            covered = true;
        }
    }
    if covered {
        return Ok((Some(before.to_vec()), exact_owned));
    }
    let mut added = Value::from(relative);
    added
        .decor_mut()
        .set_prefix(format!("\n    {OWNED_EXCLUDE}\n    "));
    exclude.push_formatted(added);
    if created_list {
        exclude
            .decor_mut()
            .set_suffix(format!(" {OWNED_EXCLUDE_LIST}"));
    }
    Ok((Some(document.to_string().into_bytes()), true))
}

fn member_matches(member: &str, source_relative: &Path) -> anyhow::Result<bool> {
    if member.contains('*') || member.contains('?') || member.contains('[') {
        Ok(glob::Pattern::new(member)?.matches_path(source_relative))
    } else {
        Ok(source_relative.starts_with(Path::new(member)))
    }
}

pub(super) fn remove_workspace_exclude(
    before: Option<&[u8]>,
    source_relative: &Path,
    owned: bool,
) -> anyhow::Result<Option<Vec<u8>>> {
    if !owned {
        return Ok(before.map(ToOwned::to_owned));
    }
    let before = before.context("managed linked Cargo exclusion needs App Cargo.toml")?;
    let mut document = workspace_document(before)?;
    let workspace = document
        .get_mut("workspace")
        .and_then(toml_edit::Item::as_table_mut)
        .context("managed linked Cargo exclusion needs [workspace]")?;
    let exclude = workspace
        .get_mut("exclude")
        .and_then(toml_edit::Item::as_array_mut)
        .context("managed linked Cargo exclusion needs workspace.exclude")?;
    let relative = source_relative
        .to_str()
        .context("linked source path must be UTF-8")?;
    let mut found = None;
    for (index, entry) in exclude.iter().enumerate() {
        if entry.as_str() == Some(relative) {
            ensure!(found.is_none(), "linked Cargo exclusion is duplicated");
            found = Some((index, managed_exclude(entry)));
        }
    }
    let (index, managed) = found.context("managed linked Cargo exclusion is missing")?;
    ensure!(
        managed,
        "linked Cargo exclusion was edited; preserving Cargo.toml and adopted source"
    );
    exclude.remove(index);
    let remove_list = exclude.is_empty()
        && exclude
            .decor()
            .suffix()
            .and_then(|suffix| suffix.as_str())
            .is_some_and(|suffix| suffix.trim() == OWNED_EXCLUDE_LIST);
    if remove_list {
        workspace.remove("exclude");
    }
    Ok(Some(document.to_string().into_bytes()))
}

/// Serialize linked-Cargo mutations for one App without creating a lock file
/// on POSIX. External editors and other App commands do not participate, so
/// their bytes are still checked before each publication step.
pub(super) fn lock_app(root: &Path) -> anyhow::Result<fs::File> {
    #[cfg(unix)]
    let file = fs::File::open(root)?;
    #[cfg(windows)]
    let file = lock_regular_file(root)?;
    #[cfg(any(unix, windows))]
    {
        #[cfg(unix)]
        file.lock()?;
        Ok(file)
    }
    #[cfg(not(any(unix, windows)))]
    {
        bail!("linked Cargo adoption locking is unsupported on this platform")
    }
}

/// Windows cannot open a directory as a lockable File. Its ordinary-file
/// fallback is also exercised by unit tests on POSIX.
#[cfg(any(windows, test))]
fn lock_regular_file(root: &Path) -> anyhow::Result<fs::File> {
    let relative = Path::new(".lenso-linked-adoption.lock");
    super::super::writable_path(root, relative)?;
    let path = root.join(relative);
    let file = match fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            ensure!(
                fs::symlink_metadata(&path)?.is_file(),
                "linked Cargo lock path is not a regular file"
            );
            fs::OpenOptions::new().read(true).write(true).open(&path)?
        }
        Err(error) => return Err(error.into()),
    };
    file.lock()?;
    Ok(file)
}

pub(super) fn read_optional_regular(path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(Some(fs::read(path)?)),
        Ok(_) => bail!("refusing non-regular App config: {}", path.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_stage(root: &Path) -> TempDir {
        let stage = tempfile::tempdir_in(root).unwrap();
        fs::write(stage.path().join("Cargo.toml"), b"[package]\n").unwrap();
        stage
    }

    #[test]
    fn prior_canonical_source_lock_upgrades_on_exact_retry() {
        let root = tempfile::tempdir().unwrap();
        let cargo = root.path().join("Cargo.toml");
        fs::write(&cargo, b"[workspace]\n").unwrap();
        let destination = root.path().join("vendor/lenso/example.web/0.4.5");
        fs::create_dir_all(&destination).unwrap();
        let stage = source_stage(root.path());
        let new_lock = serde_json::to_vec_pretty(&super::super::SourceLock {
            schema_version: 1,
            plugin_id: "example.web".into(),
            version: "0.4.5".into(),
            crate_digest: "sha256:example".into(),
            source_digest: super::super::source_digest(stage.path()).unwrap(),
            archive_cargo_lock_digest: None,
            workspace_exclude_owned: true,
            v6: None,
        })
        .unwrap();
        fs::write(stage.path().join(super::super::SOURCE_LOCK), &new_lock).unwrap();
        fs::copy(
            stage.path().join("Cargo.toml"),
            destination.join("Cargo.toml"),
        )
        .unwrap();
        let old_lock = super::super::legacy_source_lock_bytes(&new_lock).unwrap();
        fs::write(destination.join(super::super::SOURCE_LOCK), &old_lock).unwrap();
        let mut edited_lock = old_lock.clone();
        edited_lock.push(b'\n');
        fs::write(destination.join(super::super::SOURCE_LOCK), edited_lock).unwrap();
        assert!(!super::super::same_tree(stage.path(), &destination).unwrap());
        fs::write(destination.join(super::super::SOURCE_LOCK), old_lock).unwrap();

        let first = PreparedLinkedAdoption::new(root.path(), &destination, "example.web").unwrap();
        let error = first.commit_with(stage.path(), |step| {
            if step == CommitCheckpoint::AfterSourceLock {
                bail!("injected interruption after canonical lock upgrade");
            }
            Ok(())
        });
        assert!(error.is_err());
        assert_eq!(
            fs::read(destination.join(super::super::SOURCE_LOCK)).unwrap(),
            new_lock
        );
        assert!(!fs::read_to_string(&cargo).unwrap().contains(OWNED_EXCLUDE));

        PreparedLinkedAdoption::new(root.path(), &destination, "example.web")
            .unwrap()
            .commit(stage.path())
            .unwrap();
        assert!(fs::read_to_string(&cargo).unwrap().contains(OWNED_EXCLUDE));
        super::super::unadopt(root.path(), "example.web@0.4.5").unwrap();
        assert!(!fs::read_to_string(&cargo).unwrap().contains(OWNED_EXCLUDE));
    }

    #[test]
    fn nested_app_without_own_workspace_rejects_linked_source_early() {
        let enclosing = tempfile::tempdir().unwrap();
        fs::write(enclosing.path().join("Cargo.toml"), b"[workspace]\n").unwrap();
        let app = enclosing.path().join("app");
        fs::create_dir(&app).unwrap();
        let destination = app.join("vendor/lenso/example.web/0.4.5");
        let error = PreparedLinkedAdoption::new(&app, &destination, "example.web")
            .err()
            .unwrap();
        assert!(error.to_string().contains("enclosing Cargo workspace"));
        assert!(!destination.exists());
        assert!(!app.join("lenso.toml").exists());
    }

    #[test]
    fn workspace_exclusion_preserves_user_manifest_and_exact_retry() {
        let root = tempfile::tempdir().unwrap();
        let cargo = root.path().join("Cargo.toml");
        fs::write(
            &cargo,
            b"# user heading\n[package]\nname = \"business\"\nversion = \"0.1.0\"\n# custom workspace\n[workspace]\nmembers = [\"app/*\"]\n",
        )
        .unwrap();
        let destination = root.path().join("vendor/lenso/example.web/0.4.5");
        let first = PreparedLinkedAdoption::new(root.path(), &destination, "example.web").unwrap();
        assert!(first.workspace_exclude_owned());
        let stage = source_stage(root.path());
        let error = first.commit_with(stage.path(), |step| {
            if step == CommitCheckpoint::AfterWorkspace {
                bail!("injected interruption");
            }
            Ok(())
        });
        assert!(error.is_err());
        let after_interruption = fs::read_to_string(&cargo).unwrap();
        assert!(after_interruption.contains("# user heading"));
        assert!(after_interruption.contains(OWNED_EXCLUDE));
        assert!(!root.path().join("plugins/example.web").exists());

        let retry = PreparedLinkedAdoption::new(root.path(), &destination, "example.web").unwrap();
        assert!(retry.workspace_exclude_owned());
        let same_source = source_stage(root.path());
        retry.commit(same_source.path()).unwrap();
        assert_eq!(fs::read_to_string(&cargo).unwrap(), after_interruption);
        assert!(
            root.path()
                .join("plugins/example.web/default.toml")
                .is_file()
        );
    }

    #[test]
    fn concurrent_workspace_edit_is_preserved_without_selection_and_can_retry() {
        let root = tempfile::tempdir().unwrap();
        let cargo = root.path().join("Cargo.toml");
        fs::write(&cargo, b"[workspace]\n").unwrap();
        let destination = root.path().join("vendor/lenso/example.web/0.4.5");
        let prepared =
            PreparedLinkedAdoption::new(root.path(), &destination, "example.web").unwrap();
        let stage = source_stage(root.path());
        let changed = b"# user's concurrent edit\n[workspace]\n";
        let error = prepared.commit_with(stage.path(), |step| {
            if step == CommitCheckpoint::AfterSource {
                fs::write(&cargo, changed)?;
            }
            Ok(())
        });
        assert!(error.is_err());
        assert_eq!(fs::read(&cargo).unwrap(), changed);
        assert!(!root.path().join("lenso.toml").exists());
        assert!(!root.path().join("plugins/example.web").exists());

        let retry = PreparedLinkedAdoption::new(root.path(), &destination, "example.web").unwrap();
        let same_source = source_stage(root.path());
        retry.commit(same_source.path()).unwrap();
        assert!(
            fs::read_to_string(&cargo)
                .unwrap()
                .contains("# user's concurrent edit")
        );
    }

    #[test]
    fn workspace_edit_after_publication_is_not_overwritten_on_retry() {
        let root = tempfile::tempdir().unwrap();
        let cargo = root.path().join("Cargo.toml");
        fs::write(&cargo, b"[workspace]\n").unwrap();
        let destination = root.path().join("vendor/lenso/example.web/0.4.5");
        let prepared =
            PreparedLinkedAdoption::new(root.path(), &destination, "example.web").unwrap();
        let stage = source_stage(root.path());
        let error = prepared.commit_with(stage.path(), |step| {
            if step == CommitCheckpoint::AfterWorkspace {
                let mut bytes = fs::read(&cargo)?;
                bytes.extend_from_slice(b"\n# user edit after workspace publication\n");
                fs::write(&cargo, bytes)?;
            }
            Ok(())
        });
        assert!(error.is_err());
        assert!(!root.path().join("plugins/example.web").exists());
        assert!(!root.path().join("lenso.toml").exists());
        let user_manifest = fs::read(&cargo).unwrap();

        let retry = PreparedLinkedAdoption::new(root.path(), &destination, "example.web").unwrap();
        let same_source = source_stage(root.path());
        retry.commit(same_source.path()).unwrap();
        assert_eq!(fs::read(&cargo).unwrap(), user_manifest);
        assert!(
            root.path()
                .join("plugins/example.web/default.toml")
                .is_file()
        );
    }

    #[test]
    fn workspace_exclude_respects_preexisting_user_coverage() {
        let source = Path::new("vendor/lenso/example.web/0.4.5");
        for existing in [
            "[workspace]\nexclude = [\"vendor/lenso/example.web/0.4.5\"]\n",
            "[workspace]\nexclude = [\"vendor/lenso\"]\n",
        ] {
            let (after, owned) = add_workspace_exclude(Some(existing.as_bytes()), source).unwrap();
            assert_eq!(after.unwrap(), existing.as_bytes());
            assert!(!owned);
        }
    }

    #[test]
    fn unadopt_removes_only_owned_exclude_and_keeps_unrelated_edits() {
        let source = Path::new("vendor/lenso/example.web/0.4.5");
        let original = b"# user heading\n[workspace]\nexclude = [\"other-plugin\"]\n";
        let (added, owned) = add_workspace_exclude(Some(original), source).unwrap();
        assert!(owned);
        let mut modified = String::from_utf8(added.unwrap()).unwrap();
        modified.push_str("\n# user's later note\n");
        let removed = remove_workspace_exclude(Some(modified.as_bytes()), source, owned)
            .unwrap()
            .unwrap();
        let removed = String::from_utf8(removed).unwrap();
        assert!(removed.contains("# user heading"));
        assert!(removed.contains("# user's later note"));
        assert!(removed.contains("other-plugin"));
        assert!(!removed.contains("vendor/lenso/example.web/0.4.5"));

        let user_entry = b"[workspace]\nexclude = [\"vendor/lenso/example.web/0.4.5\"]\n";
        assert_eq!(
            remove_workspace_exclude(Some(user_entry), source, false)
                .unwrap()
                .unwrap(),
            user_entry
        );
    }

    #[test]
    fn edited_managed_exclude_blocks_unadopt_without_rewriting_user_manifest() {
        let source = Path::new("vendor/lenso/example.web/0.4.5");
        let (added, owned) = add_workspace_exclude(Some(b"[workspace]\n"), source).unwrap();
        assert!(owned);
        let edited = String::from_utf8(added.unwrap())
            .unwrap()
            .replace(OWNED_EXCLUDE, "# user's exclusion");
        let error = remove_workspace_exclude(Some(edited.as_bytes()), source, owned).unwrap_err();
        assert!(error.to_string().contains("exclusion was edited"));
        assert!(edited.contains("# user's exclusion"));
    }

    #[test]
    fn interrupted_after_config_recovers_by_exact_retry() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("vendor/lenso/example.web/0.4.5");
        let first = PreparedLinkedAdoption::new(root.path(), &destination, "example.web").unwrap();
        let stage = source_stage(root.path());
        let failure = first.commit_with(stage.path(), |step| {
            if step == CommitCheckpoint::AfterConfig {
                bail!("injected failure before intent publication");
            }
            Ok(())
        });
        assert!(failure.is_err());
        assert!(destination.join("Cargo.toml").is_file());
        assert!(!root.path().join("plugins/example.web").exists());
        assert_eq!(
            toml::from_str::<toml::Value>(
                &fs::read_to_string(root.path().join("lenso.toml")).unwrap()
            )
            .unwrap()["plugin_sources"][0]
                .as_str(),
            Some("vendor/lenso/example.web/0.4.5")
        );

        let retry = PreparedLinkedAdoption::new(root.path(), &destination, "example.web").unwrap();
        let same_source = source_stage(root.path());
        retry.commit(same_source.path()).unwrap();
        assert!(
            root.path()
                .join("plugins/example.web/default.toml")
                .is_file()
        );
        let document: toml::Value =
            toml::from_str(&fs::read_to_string(root.path().join("lenso.toml")).unwrap()).unwrap();
        assert_eq!(document["plugin_sources"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn concurrent_config_edit_is_preserved_without_intent_publication() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("vendor/lenso/example.web/0.4.5");
        let prepared =
            PreparedLinkedAdoption::new(root.path(), &destination, "example.web").unwrap();
        let stage = source_stage(root.path());
        let user_config = b"plugin_sources = []\n# concurrent user edit\n";
        let failure = prepared.commit_with(stage.path(), |step| {
            if step == CommitCheckpoint::AfterSource {
                fs::write(root.path().join("lenso.toml"), user_config)?;
            }
            Ok(())
        });
        assert!(failure.is_err());
        assert_eq!(
            fs::read(root.path().join("lenso.toml")).unwrap(),
            user_config
        );
        assert!(!root.path().join("plugins/example.web").exists());
    }

    #[test]
    fn conflicting_config_keeps_preexisting_disabled_intent_bytes() {
        let root = tempfile::tempdir().unwrap();
        let intent = root.path().join("plugins/example.web");
        fs::create_dir_all(&intent).unwrap();
        fs::write(intent.join("default.toml"), b"# user configuration\n").unwrap();
        fs::write(intent.join("default.disabled"), b"disabled\n").unwrap();
        let destination = root.path().join("vendor/lenso/example.web/0.4.5");
        let prepared =
            PreparedLinkedAdoption::new(root.path(), &destination, "example.web").unwrap();
        let stage = source_stage(root.path());
        let failure = prepared.commit_with(stage.path(), |step| {
            if step == CommitCheckpoint::AfterConfig {
                fs::write(root.path().join("lenso.toml"), b"# changed after config\n")?;
            }
            Ok(())
        });
        assert!(failure.is_err());
        assert_eq!(
            fs::read(root.path().join("lenso.toml")).unwrap(),
            b"# changed after config\n"
        );
        assert_eq!(
            fs::read(intent.join("default.toml")).unwrap(),
            b"# user configuration\n"
        );
        assert_eq!(
            fs::read(intent.join("default.disabled")).unwrap(),
            b"disabled\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn swapped_intent_symlink_is_rejected_before_default_publication() {
        let root = tempfile::tempdir().unwrap();
        let intent = root.path().join("plugins/example.web");
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir_all(&intent).unwrap();
        fs::write(intent.join("default.disabled"), b"disabled\n").unwrap();
        let destination = root.path().join("vendor/lenso/example.web/0.4.5");
        let prepared =
            PreparedLinkedAdoption::new(root.path(), &destination, "example.web").unwrap();
        let stage = source_stage(root.path());
        let failure = prepared.commit_with(stage.path(), |step| {
            if step == CommitCheckpoint::AfterConfig {
                fs::rename(&intent, root.path().join("original-intent"))?;
                std::os::unix::fs::symlink(outside.path(), &intent)?;
            }
            Ok(())
        });
        assert!(failure.is_err());
        assert!(!outside.path().join("default.toml").exists());
        assert_eq!(
            fs::read(root.path().join("original-intent/default.disabled")).unwrap(),
            b"disabled\n"
        );
    }

    #[test]
    fn regular_file_lock_fallback_is_reusable_and_rejects_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let first = lock_regular_file(root.path()).unwrap();
        let lock_path = root.path().join(".lenso-linked-adoption.lock");
        assert!(lock_path.is_file());
        let contender = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock_path)
            .unwrap();
        assert!(contender.try_lock().is_err());
        drop(first);
        let second = lock_regular_file(root.path()).unwrap();
        drop(second);
        #[cfg(unix)]
        {
            fs::remove_file(root.path().join(".lenso-linked-adoption.lock")).unwrap();
            std::os::unix::fs::symlink(
                root.path().join("lenso.toml"),
                root.path().join(".lenso-linked-adoption.lock"),
            )
            .unwrap();
            assert!(lock_regular_file(root.path()).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn app_directory_lock_excludes_a_second_linked_mutation() {
        let root = tempfile::tempdir().unwrap();
        let first = lock_app(root.path()).unwrap();
        let contender = fs::File::open(root.path()).unwrap();
        assert!(contender.try_lock().is_err());
        drop(first);
        assert!(contender.try_lock().is_ok());
    }
}
