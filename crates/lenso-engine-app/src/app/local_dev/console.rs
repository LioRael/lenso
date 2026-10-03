//! Selected Console conventions classify page-only edits. Their generated
//! provider still belongs to the convention compiler, never generic TS reuse.
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::Context;
use sha2::{Digest, Sha256};

#[derive(PartialEq, Eq)]
pub(super) struct Inputs {
    selection: Vec<u8>,
    files: BTreeMap<PathBuf, String>,
}

impl Inputs {
    pub fn capture(root: &Path) -> anyhow::Result<Option<Self>> {
        let session = lenso_engine::discovery::DiscoverySession::new(root)?;
        let report = lenso_app_authoring::discovery::discover_in(root, &session)?;
        let plan = lenso_app_authoring::discovery::conventions::plan_in(&report, &session)?;
        let compilations: Vec<_> = plan
            .compilations
            .iter()
            .filter(|compilation| {
                compilation.convention == "lenso.console.pages"
                    && compilation.entry.starts_with(root.join("app"))
            })
            .collect();
        if compilations.is_empty() {
            return Ok(None);
        }
        let mut files = BTreeMap::new();
        for compilation in &compilations {
            let mut scope = session.scope(&compilation.entry)?;
            let mut pending = vec![String::new()];
            let mut count = 0;
            while let Some(directory) = pending.pop() {
                for entry in scope.directory(&directory)? {
                    let filename = Path::new(&entry.path)
                        .file_name()
                        .context("Console source filename")?
                        .to_string_lossy();
                    if filename.starts_with('.')
                        || ["node_modules", "dist", "target"].contains(&filename.as_ref())
                    {
                        continue;
                    }
                    count += 1;
                    anyhow::ensure!(count <= 4096, "Console source exceeds 4096 entries");
                    if entry.kind.is_dir() {
                        pending.push(entry.path);
                    } else if entry.kind.is_file() {
                        let bytes = scope.read(&entry.path, 64 * 1024 * 1024)?;
                        files.insert(
                            compilation.entry.join(entry.path),
                            Sha256::digest(bytes)
                                .iter()
                                .map(|byte| format!("{byte:02x}"))
                                .collect(),
                        );
                    } else {
                        anyhow::bail!("Console source contains a symlink or special file");
                    }
                }
            }
        }
        Ok(Some(Self {
            selection: serde_json::to_vec(&plan)?,
            files,
        }))
    }

    pub fn page_edit(
        &self,
        root: &Path,
        paths: &std::collections::BTreeSet<PathBuf>,
    ) -> anyhow::Result<bool> {
        let Some(next) = Self::capture(root)? else {
            return Ok(false);
        };
        Ok(self.page_difference(&next, paths))
    }

    fn page_difference(&self, next: &Self, paths: &std::collections::BTreeSet<PathBuf>) -> bool {
        // A service may import shared frontend code. Until its compiler exposes
        // the dependency partition, a service-bearing Workspace stays structural.
        if self.selection != next.selection
            || !self.files.keys().eq(next.files.keys())
            || self
                .files
                .keys()
                .any(|path| path.file_name().is_some_and(|name| name == "services.ts"))
            || paths.iter().any(|path| !self.files.contains_key(path))
        {
            return false;
        }
        let changed: Vec<_> = self
            .files
            .iter()
            .filter(|(path, digest)| next.files.get(*path) != Some(*digest))
            .map(|(path, _)| path)
            .collect();
        !changed.is_empty()
            && changed.iter().all(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "tsx" || extension == "css")
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_selected_console_support_classifies_existing_page_edits() {
        let root = tempfile::tempdir().unwrap();
        let write = |path: &str, text: &str| {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("lenso.toml", "plugin_sources = ['support']\n");
        write(
            "support/Cargo.toml",
            "[package]\nname='console-support'\nversion='1.0.0'\nedition='2024'\n[package.metadata.lenso]\nplugin-id='lenso.console.web'\nroot-slot='console'\n[[package.metadata.lenso.conventions]]\nid='lenso.console.pages'\nentries=['console']\ncompiler={program='bun',args=['compiler.mjs']}\n",
        );
        write("support/src/lib.rs", "// read-only discovery fixture\n");
        write(
            "app/orders/console/page.tsx",
            "export default function Page(){return null;}\n",
        );
        assert!(Inputs::capture(root.path()).unwrap().is_none());
        write("plugins/lenso.console.web/default.toml", "");
        let before = Inputs::capture(root.path()).unwrap().unwrap();
        write(
            "app/orders/console/page.tsx",
            "export default function Page(){return 'edited';}\n",
        );
        let paths =
            std::collections::BTreeSet::from([root.path().join("app/orders/console/page.tsx")]);
        assert!(before.page_edit(root.path(), &paths).unwrap());
        write(
            "app/orders/console/orders/page.tsx",
            "export default function Page(){return null;}\n",
        );
        assert!(!before.page_edit(root.path(), &paths).unwrap());
        write("plugins/lenso.console.web/default.disabled", "");
        assert!(Inputs::capture(root.path()).unwrap().is_none());
    }

    #[test]
    fn component_edits_do_not_hide_route_membership_services_or_mixed_changes() {
        let page = PathBuf::from("/app/app/orders/console/page.tsx");
        let mut before = Inputs {
            selection: Vec::new(),
            files: BTreeMap::from([(page.clone(), "before".into())]),
        };
        let mut after = Inputs {
            selection: Vec::new(),
            files: BTreeMap::from([(page.clone(), "after".into())]),
        };
        let paths = std::collections::BTreeSet::from([page.clone()]);
        assert!(before.page_difference(&after, &paths));
        let service = page.with_file_name("services.ts");
        after.files.insert(service.clone(), "same".into());
        assert!(
            !before.page_difference(&after, &paths),
            "adding services changes structure"
        );
        before.files.insert(service, "same".into());
        assert!(
            !before.page_difference(&after, &paths),
            "shared service imports cannot be inferred"
        );
        before.files.remove(&page.with_file_name("services.ts"));
        after.files.remove(&page.with_file_name("services.ts"));
        after
            .files
            .insert(page.with_file_name("layout.tsx"), "added".into());
        assert!(
            !before.page_difference(&after, &paths),
            "new layers change route structure"
        );
        after.files.remove(&page.with_file_name("layout.tsx"));
        let mut mixed = paths;
        mixed.insert(PathBuf::from("/app/src/plugin.rs"));
        assert!(!before.page_difference(&after, &mixed));
    }
}
