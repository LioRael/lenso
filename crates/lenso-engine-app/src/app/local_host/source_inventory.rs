//! Source availability retained by target-normal local Cargo dependencies.
use std::{collections::BTreeSet, path::PathBuf};

use anyhow::Context as _;
use serde_json::Value;

#[derive(Default)]
pub(super) struct Sources {
    projects: BTreeSet<PathBuf>,
}

impl Sources {
    pub(super) fn new(projects: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            projects: projects.into_iter().collect(),
        }
    }

    // Called only after the generator admits a target-normal, non-proc-macro
    // package. Registry/Git dependencies are not local source discovery inputs.
    pub(super) fn include(&mut self, package: &Value) -> anyhow::Result<()> {
        if package["source"].is_null() {
            let manifest = package["manifest_path"]
                .as_str()
                .context("linked source Cargo manifest")?;
            self.projects.insert(
                std::path::Path::new(manifest)
                    .parent()
                    .context("linked source Cargo directory")?
                    .to_path_buf(),
            );
        }
        Ok(())
    }

    pub(super) fn exclude_in(
        &self,
        selected: &BTreeSet<&str>,
        inputs: &lenso_engine::discovery::DiscoverySession,
    ) -> anyhow::Result<Vec<String>> {
        let mut excluded = BTreeSet::new();
        for project in &self.projects {
            // Only independently source-declared native Plugins qualify for
            // availability filtering. Metadata alone, or unrelated workspace
            // members, must never authorize hiding an unexpected registration.
            if lenso_app_authoring::discovery::source_files_in(project, inputs)?.is_empty() {
                continue;
            }
            let project = std::fs::canonicalize(project)?;
            for candidate in
                lenso_app_authoring::discovery::discover_in(&project, inputs)?.candidates
            {
                if candidate.project == project
                    && candidate.native_link.is_some()
                    && !selected.contains(candidate.plugin_id.as_str())
                {
                    excluded.insert(candidate.plugin_id);
                }
            }
        }
        Ok(excluded.into_iter().collect())
    }
}
