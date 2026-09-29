//! Root intent selects already-reachable Cargo Plugins without changing their package identities.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use anyhow::{Context as _, ensure};
use serde_json::{Value, json};

#[derive(Default)]
pub(super) struct Sources {
    declared: BTreeSet<String>,
    selected: BTreeMap<String, Value>,
}

impl Sources {
    pub(super) fn new(candidate_ids: impl IntoIterator<Item = String>) -> Self {
        Self {
            declared: candidate_ids.into_iter().collect(),
            ..Self::default()
        }
    }

    pub(super) fn select(&mut self, root: &Path, package: &Value) -> anyhow::Result<()> {
        let Some(id) = package
            .pointer("/metadata/lenso/plugin-id")
            .and_then(Value::as_str)
        else {
            return Ok(());
        };
        if self.declared.contains(id) {
            return Ok(());
        }
        lenso_app_authoring::identity::classify_existing_plugin_id(id)?;
        let directory = root.join("plugins").join(id);
        let metadata = match fs::symlink_metadata(&directory) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "selected linked Plugin Root must be a regular directory"
        );
        let has_intent = fs::read_dir(directory)?.try_fold(
            false,
            |selected, entry| -> anyhow::Result<bool> {
                let entry = entry?;
                let kind = entry.file_type()?;
                ensure!(
                    !kind.is_symlink(),
                    "linked Plugin Root intent cannot be a symbolic link"
                );
                let path = entry.path();
                Ok(selected
                    || kind.is_file()
                        && path.extension().is_some_and(|extension| {
                            extension == "toml" || extension == "disabled"
                        }))
            },
        )?;
        if !has_intent {
            return Ok(());
        }
        ensure!(
            package["targets"]
                .as_array()
                .is_some_and(|targets| targets.iter().any(|target| target["kind"]
                    .as_array()
                    .is_some_and(|kinds| kinds
                        .iter()
                        .any(|kind| kind == "lib" || kind == "rlib")))),
            "Root-selected linked Plugin `{id}` needs a Rust library target"
        );
        if let Some(previous) = self.selected.get(id) {
            ensure!(
                previous["id"] == package["id"],
                "Root-selected linked Plugin `{id}` has competing Cargo package identities"
            );
        } else {
            ensure!(
                self.selected.len() < 256,
                "Root-selected linked Plugin limit exceeded"
            );
            self.selected.insert(id.to_owned(), package.clone());
        }
        Ok(())
    }

    pub(super) fn packages(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.selected
            .iter()
            .map(|(id, package)| (id.as_str(), package))
    }

    pub(super) fn record(&self, stage: &Path) -> anyhow::Result<()> {
        let sources = self.packages().map(|(id, package)| -> anyhow::Result<Value> {
            Ok(json!({"plugin_id":id,"cargo_package_id":package["id"],"version":package["version"],"dependency":super::dependency(package)?}))
        }).collect::<anyhow::Result<Vec<_>>>()?;
        fs::write(
            stage.join(".lenso/root-linked-sources.json"),
            serde_json::to_vec_pretty(&sources)?,
        )
        .context("record Root-selected linked Cargo sources")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(id: &str, source: &str) -> Value {
        json!({"id":format!("{source}#owner@1.0.0"),"name":"owner","version":"1.0.0","source":source,"manifest_path":"/immutable/owner/Cargo.toml","metadata":{"lenso":{"plugin-id":id,"root-slot":"owners"}},"targets":[{"kind":["lib"]}]})
    }

    #[test]
    fn explicit_root_intent_selects_exact_git_package_without_enabling_unselected_dependencies() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("plugins/example.selected")).unwrap();
        fs::write(
            root.path().join("plugins/example.selected/primary.toml"),
            "",
        )
        .unwrap();
        let source = format!(
            "git+https://example.test/owner?rev={}#{}",
            "a".repeat(40),
            "a".repeat(40)
        );
        let selected = package("example.selected", &source);
        let mut sources = Sources::new(["example.app".into()]);
        sources.select(root.path(), &selected).unwrap();
        sources.select(root.path(), &selected).unwrap();
        sources
            .select(root.path(), &package("example.unselected", &source))
            .unwrap();
        let packages = sources.packages().collect::<Vec<_>>();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].0, "example.selected");
        let dependency = super::super::dependency(packages[0].1).unwrap();
        assert_eq!(dependency["git"], "https://example.test/owner");
        assert_eq!(dependency["rev"], "a".repeat(40));
        assert!(dependency.get("path").is_none());
    }

    #[test]
    fn empty_directory_does_not_adopt_a_dependency_but_disabled_intent_is_declared() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("plugins/example.owner");
        fs::create_dir_all(&directory).unwrap();
        let mut sources = Sources::default();
        let owner = package(
            "example.owner",
            "registry+https://github.com/rust-lang/crates.io-index",
        );
        sources.select(root.path(), &owner).unwrap();
        assert_eq!(sources.packages().count(), 0);
        fs::write(directory.join("default.disabled"), "").unwrap();
        sources.select(root.path(), &owner).unwrap();
        assert_eq!(sources.packages().count(), 1);
    }

    #[test]
    fn one_selected_plugin_cannot_join_two_cargo_identities() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("plugins/example.owner")).unwrap();
        fs::write(root.path().join("plugins/example.owner/default.toml"), "").unwrap();
        let mut sources = Sources::default();
        sources
            .select(
                root.path(),
                &package(
                    "example.owner",
                    "git+https://example.test/a#1111111111111111111111111111111111111111",
                ),
            )
            .unwrap();
        assert!(
            sources
                .select(
                    root.path(),
                    &package(
                        "example.owner",
                        "git+https://example.test/b#2222222222222222222222222222222222222222"
                    )
                )
                .is_err()
        );
    }

    #[test]
    fn selected_owner_declares_only_explicit_instances_and_keeps_closed_admission() {
        use lenso_app_authoring::host_authoring::{GeneratedHostBuild, LocalPluginInput};
        use lenso_app_plan::authoring::PluginDescriptor;
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("plugins/example.owner");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("primary.toml"), "").unwrap();
        let authority = GeneratedHostBuild::lower_local(
            "example.app",
            vec![LocalPluginInput {
                descriptor: PluginDescriptor::new("example.owner", "1.0.0", "owners"),
                manifest_digest: format!("sha256:{}", "a".repeat(64)),
                app_owned: false,
                source: "exact reachable Cargo Plugin".into(),
            }],
        )
        .unwrap();
        let (_, resolved) = authority.clone().with_local_root(root.path()).unwrap();
        assert_eq!(resolved.plan().plugin_instances().len(), 1);
        assert_eq!(
            resolved.plan().plugin_instances()[0].instance_key(),
            "example.owner/primary"
        );
        fs::create_dir_all(root.path().join("plugins/example.unselected")).unwrap();
        fs::write(
            root.path().join("plugins/example.unselected/default.toml"),
            "",
        )
        .unwrap();
        assert!(authority.with_local_root(root.path()).is_err());
    }
}
