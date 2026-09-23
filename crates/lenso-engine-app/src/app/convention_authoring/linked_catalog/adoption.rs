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

enum StagedIntent {
    New(TempDir),
    MissingDefault(NamedTempFile),
    Existing,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum CommitCheckpoint {
    AfterSource,
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
    intent: PathBuf,
    staged_intent: StagedIntent,
}

impl PreparedLinkedAdoption {
    pub(super) fn new(root: &Path, destination: &Path, plugin_id: &str) -> anyhow::Result<Self> {
        let app_lock = lock_app(root)?;
        super::super::preflight_source_adoption(root, plugin_id)?;
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
            intent,
            staged_intent,
        })
    }

    pub(super) fn commit(self, source_stage: &Path) -> anyhow::Result<()> {
        self.commit_with(source_stage, |_| Ok(()))
    }

    fn commit_with(
        mut self,
        source_stage: &Path,
        mut checkpoint: impl FnMut(CommitCheckpoint) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        self.ensure_config_is(&self.config_before)?;
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

        self.ensure_config_is(&self.config_before)?;
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

fn read_optional_regular(path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
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
