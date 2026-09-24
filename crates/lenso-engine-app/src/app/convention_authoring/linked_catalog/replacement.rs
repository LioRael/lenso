//! Explicit version switch for an already selected linked-Cargo Plugin source.
//! The old source and Plugin Root remain untouched; only App source selection changes.

use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, bail, ensure};
use tempfile::NamedTempFile;
use toml_edit::{DocumentMut, Item, Value};

use super::{SOURCE_LOCK, SourceLock, adoption, source_digest, verify_archive_cargo_lock};

#[derive(Clone, Copy, Eq, PartialEq)]
enum CommitCheckpoint {
    SourceReady,
    WorkspaceExcluded,
    ConfigSelected,
}

pub(super) struct PreparedLinkedReplacement {
    _app_lock: fs::File,
    root: PathBuf,
    previous: PathBuf,
    destination: PathBuf,
    config: PathBuf,
    config_before: Vec<u8>,
    config_after: Vec<u8>,
    staged_config: Option<NamedTempFile>,
    rollback_config: Option<NamedTempFile>,
    workspace_manifest: PathBuf,
    workspace_before: Option<Vec<u8>>,
    workspace_after: Option<Vec<u8>>,
    staged_workspace: Option<NamedTempFile>,
    rollback_workspace: Option<NamedTempFile>,
    workspace_exclude_owned: bool,
    workspace_published: bool,
    config_published: bool,
}

impl PreparedLinkedReplacement {
    pub(super) fn new_locked(
        root: &Path,
        previous: &Path,
        destination: &Path,
        plugin_id: &str,
        app_lock: fs::File,
    ) -> anyhow::Result<Self> {
        ensure!(
            previous != destination,
            "replacement requires a different exact version"
        );
        super::super::preflight_source_adoption(root, plugin_id)?;
        super::super::writable_path(root, Path::new("Cargo.toml"))?;
        super::super::writable_path(root, Path::new("lenso.toml"))?;
        super::super::writable_path(
            root,
            &Path::new("plugins").join(plugin_id).join("default.toml"),
        )?;
        let previous_relative = previous
            .strip_prefix(root)
            .context("prior linked source is outside App root")?;
        let destination_relative = destination
            .strip_prefix(root)
            .context("new linked source is outside App root")?;
        ensure!(
            previous_relative.parent() == destination_relative.parent(),
            "replacement must retain the same Plugin ID"
        );
        let prior_lock = verify_prior_source(previous, plugin_id)?;
        let intent = root.join("plugins").join(plugin_id);
        ensure!(
            fs::symlink_metadata(&intent)?.file_type().is_dir(),
            "linked Plugin Root is not a regular directory"
        );
        ensure!(
            fs::symlink_metadata(intent.join("default.toml"))?
                .file_type()
                .is_file(),
            "linked Plugin Root default is not a regular file"
        );

        let workspace_manifest = root.join("Cargo.toml");
        let workspace_before = adoption::read_optional_regular(&workspace_manifest)?;
        adoption::remove_workspace_exclude(
            workspace_before.as_deref(),
            previous_relative,
            prior_lock.workspace_exclude_owned,
        )?;
        let app_owns_workspace = workspace_before
            .as_deref()
            .map(|bytes| {
                adoption::workspace_document(bytes)
                    .map(|document| document.get("workspace").is_some())
            })
            .transpose()?
            .unwrap_or(false);
        if !app_owns_workspace {
            adoption::ensure_no_enclosing_workspace(root)?;
        }
        let (workspace_after, workspace_exclude_owned) =
            adoption::add_workspace_exclude(workspace_before.as_deref(), destination_relative)?;
        let (staged_workspace, rollback_workspace) = if workspace_after != workspace_before {
            (
                Some(stage(
                    root,
                    workspace_after
                        .as_deref()
                        .context("updated workspace manifest missing")?,
                )?),
                Some(stage(
                    root,
                    workspace_before
                        .as_deref()
                        .context("prior workspace manifest missing")?,
                )?),
            )
        } else {
            (None, None)
        };

        let config = root.join("lenso.toml");
        let config_before = adoption::read_optional_regular(&config)?
            .context("selected linked source needs lenso.toml")?;
        let mut document: DocumentMut = std::str::from_utf8(&config_before)?
            .parse()
            .context("parse App lenso.toml")?;
        let sources = document
            .get_mut("plugin_sources")
            .and_then(Item::as_array_mut)
            .context("lenso.toml plugin_sources array")?;
        let old = previous_relative
            .to_str()
            .context("prior source path UTF-8")?;
        let new = destination_relative
            .to_str()
            .context("new source path UTF-8")?;
        ensure!(
            sources
                .iter()
                .filter(|value| value.as_str() == Some(old))
                .count()
                == 1,
            "prior linked Cargo source is not uniquely selected"
        );
        ensure!(
            !sources.iter().any(|value| value.as_str() == Some(new)),
            "new linked Cargo source is already selected"
        );
        let selected = sources
            .iter_mut()
            .find(|value| value.as_str() == Some(old))
            .context("prior linked Cargo source selection disappeared")?;
        let decor = selected.decor().clone();
        *selected = Value::from(new);
        *selected.decor_mut() = decor;
        let config_after = document.to_string().into_bytes();
        let staged_config = stage(root, &config_after)?;
        let rollback_config = stage(root, &config_before)?;
        Ok(Self {
            _app_lock: app_lock,
            root: root.to_path_buf(),
            previous: previous.to_path_buf(),
            destination: destination.to_path_buf(),
            config,
            config_before,
            config_after,
            staged_config: Some(staged_config),
            rollback_config: Some(rollback_config),
            workspace_manifest,
            workspace_before,
            workspace_after,
            staged_workspace,
            rollback_workspace,
            workspace_exclude_owned,
            workspace_published: false,
            config_published: false,
        })
    }

    pub(super) fn workspace_exclude_owned(&self) -> bool {
        self.workspace_exclude_owned
    }

    pub(super) fn commit(self, source_stage: &Path) -> anyhow::Result<()> {
        self.commit_with(source_stage, |_| Ok(()))
    }

    fn commit_with(
        mut self,
        source_stage: &Path,
        mut checkpoint: impl FnMut(CommitCheckpoint) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let result = self.publish(source_stage, &mut checkpoint);
        if let Err(error) = result {
            if let Err(rollback) = self.rollback_selection() {
                bail!(
                    "linked Cargo replacement failed: {error:#}; rollback incomplete: {rollback:#}"
                );
            }
            return Err(error.context("linked Cargo selection restored; verified new source may remain unselected for exact retry"));
        }
        Ok(())
    }

    fn publish(
        &mut self,
        source_stage: &Path,
        checkpoint: &mut impl FnMut(CommitCheckpoint) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        self.ensure_config_is(&self.config_before)?;
        self.ensure_workspace_is(&self.workspace_before)?;
        verify_prior_source(
            &self.previous,
            self.previous
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .context("prior Plugin ID path")?,
        )?;
        super::super::writable_path(&self.root, self.destination.strip_prefix(&self.root)?)?;
        if fs::symlink_metadata(&self.destination).is_ok() {
            ensure!(
                super::same_tree(source_stage, &self.destination)?,
                "existing linked Cargo source differs from signed archive"
            );
        } else {
            fs::create_dir_all(self.destination.parent().context("linked source parent")?)?;
            super::super::super::build::publish_new_output(source_stage, &self.destination)?;
        }
        checkpoint(CommitCheckpoint::SourceReady)?;

        self.ensure_config_is(&self.config_before)?;
        self.ensure_workspace_is(&self.workspace_before)?;
        if let Some(staged) = self.staged_workspace.take() {
            staged.persist(&self.workspace_manifest)?;
            self.workspace_published = true;
        }
        checkpoint(CommitCheckpoint::WorkspaceExcluded)?;

        self.ensure_workspace_is(&self.workspace_after)?;
        self.ensure_config_is(&self.config_before)?;
        let staged = self
            .staged_config
            .take()
            .context("replacement config was not staged")?;
        staged.persist(&self.config)?;
        self.config_published = true;
        checkpoint(CommitCheckpoint::ConfigSelected)?;
        Ok(())
    }

    fn rollback_selection(&mut self) -> anyhow::Result<()> {
        let config_result = (|| {
            if self.config_published
                && adoption::read_optional_regular(&self.config)? == Some(self.config_after.clone())
            {
                let staged = self
                    .rollback_config
                    .take()
                    .context("config rollback was not staged")?;
                staged.persist(&self.config)?;
            }
            self.ensure_config_is(&self.config_before)
        })();
        let workspace_result = (|| {
            if self.workspace_before != self.workspace_after {
                if self.workspace_published
                    && adoption::read_optional_regular(&self.workspace_manifest)?
                        == self.workspace_after
                {
                    let staged = self
                        .rollback_workspace
                        .take()
                        .context("workspace rollback was not staged")?;
                    staged.persist(&self.workspace_manifest)?;
                }
                self.ensure_workspace_is(&self.workspace_before)?;
            }
            Ok::<_, anyhow::Error>(())
        })();
        match (config_result, workspace_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(config), Ok(())) => Err(config),
            (Ok(()), Err(workspace)) => Err(workspace),
            (Err(config), Err(workspace)) => {
                bail!("config rollback: {config:#}; workspace rollback: {workspace:#}")
            }
        }
    }

    fn ensure_config_is(&self, expected: &[u8]) -> anyhow::Result<()> {
        ensure!(
            adoption::read_optional_regular(&self.config)?.as_deref() == Some(expected),
            "lenso.toml changed during linked Cargo replacement; preserving concurrent edit"
        );
        Ok(())
    }

    fn ensure_workspace_is(&self, expected: &Option<Vec<u8>>) -> anyhow::Result<()> {
        ensure!(
            adoption::read_optional_regular(&self.workspace_manifest)? == *expected,
            "Cargo.toml changed during linked Cargo replacement; preserving concurrent edit"
        );
        Ok(())
    }
}

fn stage(root: &Path, bytes: &[u8]) -> anyhow::Result<NamedTempFile> {
    let mut staged = NamedTempFile::new_in(root)?;
    staged.write_all(bytes)?;
    Ok(staged)
}

fn verify_prior_source(source: &Path, plugin_id: &str) -> anyhow::Result<SourceLock> {
    ensure!(
        source.is_dir() && !source.is_symlink(),
        "prior linked Cargo source is not a regular directory"
    );
    let bytes = adoption::read_optional_regular(&source.join(SOURCE_LOCK))?
        .context("prior linked Cargo source lock is missing")?;
    ensure!(
        bytes.len() <= 4096,
        "prior linked Cargo source lock exceeds size limit"
    );
    let lock: SourceLock = serde_json::from_slice(&bytes)?;
    ensure!(
        lock.schema_version == 1
            && lock.plugin_id == plugin_id
            && source.file_name().and_then(|name| name.to_str()) == Some(lock.version.as_str()),
        "prior linked Cargo source lock identity changed"
    );
    ensure!(
        source_digest(source)? == lock.source_digest,
        "prior linked Cargo source has user changes; preserve and review it before replacement"
    );
    verify_archive_cargo_lock(source, &lock)?;
    Ok(lock)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_stage(root: &Path, version: &str) -> tempfile::TempDir {
        let stage = tempfile::tempdir_in(root).unwrap();
        fs::create_dir_all(stage.path().join("src")).unwrap();
        fs::write(
            stage.path().join("Cargo.toml"),
            format!("[package]\nname = 'example-web-plugin'\nversion = '{version}'\n"),
        )
        .unwrap();
        fs::write(stage.path().join("src/lib.rs"), b"pub struct Plugin;\n").unwrap();
        let lock = SourceLock {
            schema_version: 1,
            plugin_id: "example.web".into(),
            version: version.into(),
            crate_digest: format!("sha256:{version}"),
            source_digest: source_digest(stage.path()).unwrap(),
            archive_cargo_lock_digest: None,
            workspace_exclude_owned: true,
            v6: None,
        };
        fs::write(
            stage.path().join(SOURCE_LOCK),
            serde_json::to_vec_pretty(&lock).unwrap(),
        )
        .unwrap();
        stage
    }

    fn app() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("vendor/lenso/example.web/0.4.5");
        let new = root.path().join("vendor/lenso/example.web/0.4.6");
        fs::create_dir_all(old.parent().unwrap()).unwrap();
        let stage = source_stage(root.path(), "0.4.5");
        fs::rename(stage.path(), &old).unwrap();
        let (workspace, _) = adoption::add_workspace_exclude(
            Some(b"[workspace]\n"),
            Path::new("vendor/lenso/example.web/0.4.5"),
        )
        .unwrap();
        fs::write(root.path().join("Cargo.toml"), workspace.unwrap()).unwrap();
        fs::write(
            root.path().join("lenso.toml"),
            b"plugin_sources = ['vendor/lenso/example.web/0.4.5', 'app/other']\n# App-owned setting note\n",
        )
        .unwrap();
        fs::create_dir_all(root.path().join("plugins/example.web")).unwrap();
        fs::write(
            root.path().join("plugins/example.web/default.toml"),
            b"custom_setting = 'keep me'\n",
        )
        .unwrap();
        fs::create_dir_all(root.path().join("dist")).unwrap();
        fs::write(root.path().join("dist/old-host"), b"runnable-generation").unwrap();
        (root, old, new)
    }

    fn selected(root: &Path) -> Vec<String> {
        toml::from_str::<toml::Value>(&fs::read_to_string(root.join("lenso.toml")).unwrap())
            .unwrap()["plugin_sources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry.as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn replacement_preserves_old_source_custom_intent_and_built_distribution() {
        let (root, old, new) = app();
        let previous_bytes = fs::read(old.join(SOURCE_LOCK)).unwrap();
        let prepared = PreparedLinkedReplacement::new_locked(
            root.path(),
            &old,
            &new,
            "example.web",
            adoption::lock_app(root.path()).unwrap(),
        )
        .unwrap();
        assert!(prepared.workspace_exclude_owned());
        let stage = source_stage(root.path(), "0.4.6");
        prepared.commit(stage.path()).unwrap();
        assert_eq!(
            selected(root.path()),
            ["vendor/lenso/example.web/0.4.6", "app/other"]
        );
        assert!(
            fs::read_to_string(root.path().join("lenso.toml"))
                .unwrap()
                .contains("# App-owned setting note")
        );
        assert_eq!(fs::read(old.join(SOURCE_LOCK)).unwrap(), previous_bytes);
        assert!(new.join(SOURCE_LOCK).is_file());
        assert_eq!(
            fs::read(root.path().join("plugins/example.web/default.toml")).unwrap(),
            b"custom_setting = 'keep me'\n"
        );
        assert_eq!(
            fs::read(root.path().join("dist/old-host")).unwrap(),
            b"runnable-generation"
        );
        let workspace = fs::read_to_string(root.path().join("Cargo.toml")).unwrap();
        assert!(workspace.contains("vendor/lenso/example.web/0.4.6"));
    }

    #[test]
    fn failed_selection_rolls_back_and_exact_retry_converges() {
        for failure_point in [
            CommitCheckpoint::SourceReady,
            CommitCheckpoint::WorkspaceExcluded,
            CommitCheckpoint::ConfigSelected,
        ] {
            let (root, old, new) = app();
            let config_before = fs::read(root.path().join("lenso.toml")).unwrap();
            let workspace_before = fs::read(root.path().join("Cargo.toml")).unwrap();
            let first = PreparedLinkedReplacement::new_locked(
                root.path(),
                &old,
                &new,
                "example.web",
                adoption::lock_app(root.path()).unwrap(),
            )
            .unwrap();
            let stage = source_stage(root.path(), "0.4.6");
            let result = first.commit_with(stage.path(), |step| {
                if step == failure_point {
                    bail!("injected I/O failure");
                }
                Ok(())
            });
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("selection restored")
            );
            assert_eq!(
                fs::read(root.path().join("lenso.toml")).unwrap(),
                config_before
            );
            assert_eq!(
                fs::read(root.path().join("Cargo.toml")).unwrap(),
                workspace_before
            );
            assert!(old.join(SOURCE_LOCK).is_file());
            assert_eq!(
                fs::read(root.path().join("plugins/example.web/default.toml")).unwrap(),
                b"custom_setting = 'keep me'\n"
            );

            let retry = PreparedLinkedReplacement::new_locked(
                root.path(),
                &old,
                &new,
                "example.web",
                adoption::lock_app(root.path()).unwrap(),
            )
            .unwrap();
            let retry_stage = source_stage(root.path(), "0.4.6");
            retry.commit(retry_stage.path()).unwrap();
            assert_eq!(selected(root.path())[0], "vendor/lenso/example.web/0.4.6");
        }
    }

    #[test]
    fn edited_old_or_preexisting_new_source_blocks_replacement() {
        let (root, old, new) = app();
        fs::write(old.join("src/lib.rs"), b"user edit\n").unwrap();
        let blocked = PreparedLinkedReplacement::new_locked(
            root.path(),
            &old,
            &new,
            "example.web",
            adoption::lock_app(root.path()).unwrap(),
        );
        assert!(blocked.err().unwrap().to_string().contains("user changes"));
        assert!(!new.exists());

        fs::write(old.join("src/lib.rs"), b"pub struct Plugin;\n").unwrap();
        let workspace = fs::read_to_string(root.path().join("Cargo.toml")).unwrap();
        fs::write(
            root.path().join("Cargo.toml"),
            workspace.replace("# lenso:linked-cargo-exclude", "# edited exclusion"),
        )
        .unwrap();
        let edited_workspace = PreparedLinkedReplacement::new_locked(
            root.path(),
            &old,
            &new,
            "example.web",
            adoption::lock_app(root.path()).unwrap(),
        );
        assert!(
            edited_workspace
                .err()
                .unwrap()
                .to_string()
                .contains("exclusion was edited")
        );
        fs::write(root.path().join("Cargo.toml"), workspace).unwrap();
        fs::create_dir_all(&new).unwrap();
        fs::write(new.join("Cargo.toml"), b"edited preexisting source\n").unwrap();
        let prepared = PreparedLinkedReplacement::new_locked(
            root.path(),
            &old,
            &new,
            "example.web",
            adoption::lock_app(root.path()).unwrap(),
        )
        .unwrap();
        let stage = source_stage(root.path(), "0.4.6");
        assert!(prepared.commit(stage.path()).is_err());
        assert_eq!(selected(root.path())[0], "vendor/lenso/example.web/0.4.5");
        assert_eq!(
            fs::read(new.join("Cargo.toml")).unwrap(),
            b"edited preexisting source\n"
        );
    }

    #[test]
    fn concurrent_config_edit_is_preserved_and_workspace_is_rolled_back() {
        let (root, old, new) = app();
        let workspace_before = fs::read(root.path().join("Cargo.toml")).unwrap();
        let prepared = PreparedLinkedReplacement::new_locked(
            root.path(),
            &old,
            &new,
            "example.web",
            adoption::lock_app(root.path()).unwrap(),
        )
        .unwrap();
        let stage = source_stage(root.path(), "0.4.6");
        let user_config =
            b"plugin_sources = ['vendor/lenso/example.web/0.4.5']\n# concurrent edit\n";
        let failed = prepared.commit_with(stage.path(), |step| {
            if step == CommitCheckpoint::WorkspaceExcluded {
                fs::write(root.path().join("lenso.toml"), user_config)?;
            }
            Ok(())
        });
        assert!(
            failed
                .unwrap_err()
                .to_string()
                .contains("rollback incomplete")
        );
        assert_eq!(
            fs::read(root.path().join("lenso.toml")).unwrap(),
            user_config
        );
        assert_eq!(
            fs::read(root.path().join("Cargo.toml")).unwrap(),
            workspace_before
        );
        assert!(old.join(SOURCE_LOCK).is_file());
    }

    #[test]
    fn same_bytes_concurrent_edits_before_publication_are_not_rolled_back() {
        let (root, old, new) = app();
        let prepared = PreparedLinkedReplacement::new_locked(
            root.path(),
            &old,
            &new,
            "example.web",
            adoption::lock_app(root.path()).unwrap(),
        )
        .unwrap();
        let expected_config = prepared.config_after.clone();
        let expected_workspace = prepared.workspace_after.clone().unwrap();
        let stage = source_stage(root.path(), "0.4.6");
        let failed = prepared.commit_with(stage.path(), |step| {
            if step == CommitCheckpoint::SourceReady {
                fs::write(root.path().join("lenso.toml"), &expected_config)?;
                fs::write(root.path().join("Cargo.toml"), &expected_workspace)?;
                bail!("concurrent same-bytes edit");
            }
            Ok(())
        });
        assert!(
            failed
                .unwrap_err()
                .to_string()
                .contains("rollback incomplete")
        );
        assert_eq!(
            fs::read(root.path().join("lenso.toml")).unwrap(),
            expected_config
        );
        assert_eq!(
            fs::read(root.path().join("Cargo.toml")).unwrap(),
            expected_workspace
        );
        assert!(old.join(SOURCE_LOCK).is_file());
    }
}
