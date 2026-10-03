//! Project source App intent before handing it to the strict runtime Root reader.
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::bail;

pub(super) fn project(
    root: &Path,
    destination: &Path,
    inputs: &lenso_engine::discovery::DiscoverySession,
) -> anyhow::Result<()> {
    let source = root.join("plugins");
    let files = lenso_app_authoring::discovery::source_files_in(root, inputs)?
        .into_iter()
        // A module path cannot consume an Instance, marker or authority file.
        // Source extensions supplement exact module/entry provenance.
        .filter(|path| {
            path.extension().is_some_and(|extension| {
                ["rs", "ts", "tsx", "js", "mjs", "mts", "cts"]
                    .iter()
                    .any(|allowed| extension == *allowed)
            })
        })
        .filter_map(|path| match is_intent_file(&source, &path) {
            Ok(true) => None,
            Ok(false) => Some(Ok(path)),
            Err(error) => Some(Err(error)),
        })
        .collect::<anyhow::Result<BTreeSet<_>>>()?;
    copy(&source, destination, &files, 0, &mut 0)?;
    Ok(())
}

/// A source input may also be an explicitly configured Instance resource.
/// Its source identity cannot erase resource or Bundle intent.
fn is_intent_file(root: &Path, file: &Path) -> anyhow::Result<bool> {
    let Ok(relative) = file.strip_prefix(root) else {
        return Ok(false);
    };
    let mut components = relative.components();
    let Some(plugin) = components.next() else {
        return Ok(false);
    };
    let Some(container) = components.next() else {
        return Ok(false);
    };
    if components.next().is_none() {
        return Ok(false);
    }
    if container.as_os_str() == "plugin.lenso-plugin" {
        return Ok(true);
    }
    let mut instance = container.as_os_str().to_os_string();
    instance.push(".toml");
    Ok(root.join(plugin).join(instance).try_exists()?)
}

/// Returns whether this subtree contained an exact discovered source input.
/// Only directories emptied by removing those inputs are pruned; unknown files
/// and unrelated empty directories remain for runtime validation to reject.
fn copy(
    source: &Path,
    destination: &Path,
    files: &BTreeSet<PathBuf>,
    depth: usize,
    bytes: &mut u64,
) -> anyhow::Result<bool> {
    if depth > 32 {
        bail!("Plugin Root exceeds 32 directory levels");
    }
    let metadata = fs::symlink_metadata(source)?;
    if metadata.is_dir() {
        fs::create_dir(destination)?;
        let mut projected_source = false;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            projected_source |= copy(
                &entry.path(),
                &destination.join(entry.file_name()),
                files,
                depth + 1,
                bytes,
            )?;
        }
        if depth > 0 && projected_source && fs::read_dir(destination)?.next().is_none() {
            fs::remove_dir(destination)?;
        }
        Ok(projected_source)
    } else if metadata.is_file() {
        if files.contains(source) {
            return Ok(true);
        }
        *bytes += metadata.len();
        if *bytes > 256 * 1024 * 1024 {
            bail!("Plugin Root snapshot exceeds 256 MiB");
        }
        fs::copy(source, destination)?;
        Ok(false)
    } else {
        bail!(
            "Plugin Root contains a symlink or special file: {}",
            source.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lenso_app_plan::authoring::{HostCatalog, HostPluginRelease, HostSlot, PluginDescriptor};

    fn write(root: &Path, path: &str, contents: &str) {
        let file = root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, contents).unwrap();
    }

    fn source(root: &Path, container: &str) {
        write(
            root,
            "Cargo.toml",
            "[package]\nname='fixture'\nversion='1.0.0'\n",
        );
        write(
            root,
            "src/lib.rs",
            &format!("#[path=\"../plugins/{container}/plugin.rs\"] pub mod health;"),
        );
        write(
            root,
            &format!("plugins/{container}/plugin.rs"),
            "#[lenso::plugin(id=\"example.health\", root_slot=\"web\")] pub struct Plugin {}",
        );
        write(root, "plugins/example.health/default.toml", "");
    }

    fn project_to(root: &Path, destination: &Path) -> anyhow::Result<()> {
        let inputs = lenso_engine::discovery::DiscoverySession::new(root)?;
        project(root, &destination.join("plugins"), &inputs)
    }

    fn resolve(root: &Path) -> anyhow::Result<lenso_app_plan::authoring::ResolvedApp> {
        resolve_with_descriptor(
            root,
            PluginDescriptor::new("example.health", "1.0.0", "web"),
        )
    }

    fn resolve_with_descriptor(
        root: &Path,
        descriptor: PluginDescriptor,
    ) -> anyhow::Result<lenso_app_plan::authoring::ResolvedApp> {
        let catalog = HostCatalog::new(
            [HostSlot::many("web")],
            [HostPluginRelease::new(descriptor)],
            [],
        );
        write(
            root,
            ".lenso/host-catalog.json",
            &serde_json::to_string(&catalog)?,
        );
        lenso_app_authoring::load_resolved_app(root)
    }

    #[test]
    fn declared_bun_entry_projects_beside_instances_without_consuming_resources() {
        let root = tempfile::tempdir().unwrap();
        let built = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "package.json",
            r#"{
            "name":"fixture", "version":"1.0.0",
            "lenso":{"pluginId":"example.health", "rootSlot":"web", "runtime":"bun",
                "source":"plugins/example.health/plugin.ts"}
        }"#,
        );
        write(
            root.path(),
            "plugins/example.health/plugin.ts",
            "export default {};\n",
        );
        write(root.path(), "plugins/example.health/default.toml", "");
        write(root.path(), "plugins/example.health/secondary.toml", "");
        write(
            root.path(),
            "plugins/example.health/default/plugin.ts",
            "resource bytes",
        );
        project_to(root.path(), built.path()).unwrap();
        assert!(
            !built
                .path()
                .join("plugins/example.health/plugin.ts")
                .exists()
        );
        assert_eq!(
            fs::read(
                built
                    .path()
                    .join("plugins/example.health/default/plugin.ts")
            )
            .unwrap(),
            b"resource bytes"
        );
        assert_eq!(resolve(built.path()).unwrap().instances().len(), 2);

        let invalid = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "plugins/example.health/unclaimed.ts",
            "export {};\n",
        );
        project_to(root.path(), invalid.path()).unwrap();
        assert!(
            format!("{:#}", resolve(invalid.path()).unwrap_err()).contains("unknown Plugin file")
        );
    }

    #[test]
    fn two_instances_keep_independent_configuration_beside_one_source() {
        let root = tempfile::tempdir().unwrap();
        let built = tempfile::tempdir().unwrap();
        source(root.path(), "example.health");
        write(
            root.path(),
            "plugins/example.health/default.toml",
            "label = 'first'",
        );
        write(
            root.path(),
            "plugins/example.health/secondary.toml",
            "label = 'second'",
        );
        project_to(root.path(), built.path()).unwrap();
        let app = resolve_with_descriptor(
            built.path(),
            PluginDescriptor::new("example.health", "1.0.0", "web")
                .with_configuration_schema(serde_json::json!({
                    "type":"object", "properties":{"label":{"type":"string"}},
                    "required":["label"], "additionalProperties":false
                }))
                .with_configuration_defaults(serde_json::json!({"label":"default"})),
        )
        .unwrap();
        assert_eq!(app.instances().len(), 2);
        assert_eq!(
            app.plan().plugin_instances()[0].configuration(),
            r#"{"label":"first"}"#
        );
        assert_eq!(
            app.plan().plugin_instances()[1].configuration(),
            r#"{"label":"second"}"#
        );
        assert!(
            !built
                .path()
                .join("plugins/example.health/plugin.rs")
                .exists()
        );
        assert!(
            built
                .path()
                .join("plugins/example.health/default.toml")
                .is_file()
        );
        assert!(
            built
                .path()
                .join("plugins/example.health/secondary.toml")
                .is_file()
        );
    }

    #[test]
    fn source_beside_instance_projects_to_strict_runtime_intent() {
        let root = tempfile::tempdir().unwrap();
        let built = tempfile::tempdir().unwrap();
        source(root.path(), "example.health");
        write(root.path(), "plugins/example.health/default.disabled", "");
        write(
            root.path(),
            "plugins/example.health/default/data.rs",
            "resource bytes",
        );
        assert!(format!("{:#}", resolve(root.path()).unwrap_err()).contains("unknown Plugin file"));
        project_to(root.path(), built.path()).unwrap();
        assert!(
            !built
                .path()
                .join("plugins/example.health/plugin.rs")
                .exists()
        );
        assert_eq!(
            fs::read(built.path().join("plugins/example.health/default/data.rs")).unwrap(),
            b"resource bytes"
        );
        assert!(
            built
                .path()
                .join("plugins/example.health/default.disabled")
                .is_file()
        );
        assert!(resolve(built.path()).unwrap().instances().is_empty());
        fs::remove_file(built.path().join("plugins/example.health/default.disabled")).unwrap();
        assert_eq!(resolve(built.path()).unwrap().instances().len(), 1);
        write(
            built.path(),
            "plugins/example.health/plugin.rs",
            "runtime is strict",
        );
        assert!(
            format!("{:#}", resolve(built.path()).unwrap_err()).contains("unknown Plugin file")
        );
    }

    #[test]
    fn prunes_only_source_emptied_containers_and_keeps_unknown_files() {
        let root = tempfile::tempdir().unwrap();
        let built = tempfile::tempdir().unwrap();
        source(root.path(), "health");
        project_to(root.path(), built.path()).unwrap();
        assert!(!built.path().join("plugins/health").exists());
        assert_eq!(resolve(built.path()).unwrap().instances().len(), 1);
        let invalid = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "plugins/health/unreachable.rs",
            "pub fn helper() {}",
        );
        project_to(root.path(), invalid.path()).unwrap();
        assert!(
            format!("{:#}", resolve(invalid.path()).unwrap_err()).contains("unknown Plugin file")
        );
        fs::remove_file(root.path().join("plugins/health/unreachable.rs")).unwrap();
        fs::create_dir(root.path().join("plugins/health/unclaimed")).unwrap();
        let orphan = tempfile::tempdir().unwrap();
        project_to(root.path(), orphan.path()).unwrap();
        assert!(
            format!("{:#}", resolve(orphan.path()).unwrap_err())
                .contains("orphan Plugin resource directory")
        );
    }

    #[test]
    fn ordinary_modules_without_plugin_declarations_do_not_gain_exemptions() {
        let root = tempfile::tempdir().unwrap();
        let built = tempfile::tempdir().unwrap();
        source(root.path(), "example.health");
        write(
            root.path(),
            "plugins/example.health/plugin.rs",
            "pub struct Plugin {}",
        );
        project_to(root.path(), built.path()).unwrap();
        assert!(
            built
                .path()
                .join("plugins/example.health/plugin.rs")
                .is_file()
        );
        assert!(
            format!("{:#}", resolve(built.path()).unwrap_err()).contains("unknown Plugin file")
        );
    }

    #[test]
    fn module_references_preserve_explicit_instance_resource_bytes() {
        let root = tempfile::tempdir().unwrap();
        let built = tempfile::tempdir().unwrap();
        source(root.path(), "example.health");
        write(
            root.path(),
            "src/lib.rs",
            "#[path=\"../plugins/example.health/plugin.rs\"] pub mod health; #[path=\"../plugins/example.health/default/data.rs\"] pub mod helper;",
        );
        write(
            root.path(),
            "plugins/example.health/default/data.rs",
            "pub fn data() {}",
        );
        project_to(root.path(), built.path()).unwrap();
        assert_eq!(
            fs::read(built.path().join("plugins/example.health/default/data.rs")).unwrap(),
            b"pub fn data() {}"
        );
        assert_eq!(resolve(built.path()).unwrap().instances().len(), 1);
    }

    #[test]
    fn module_references_cannot_strip_bundle_contents() {
        let root = tempfile::tempdir().unwrap();
        let built = tempfile::tempdir().unwrap();
        source(root.path(), "example.health");
        write(
            root.path(),
            "src/lib.rs",
            "#[path=\"../plugins/example.health/plugin.rs\"] pub mod health; #[path=\"../plugins/example.health/plugin.lenso-plugin/data.rs\"] pub mod helper;",
        );
        write(
            root.path(),
            "plugins/example.health/plugin.lenso-plugin/data.rs",
            "pub fn data() {}",
        );
        project_to(root.path(), built.path()).unwrap();
        assert_eq!(
            fs::read(
                built
                    .path()
                    .join("plugins/example.health/plugin.lenso-plugin/data.rs")
            )
            .unwrap(),
            b"pub fn data() {}"
        );
        assert!(
            resolve(built.path()).is_err(),
            "projection must preserve an invalid Bundle for admission to reject"
        );
    }

    #[test]
    fn module_paths_cannot_consume_instance_intent_files() {
        let root = tempfile::tempdir().unwrap();
        let built = tempfile::tempdir().unwrap();
        source(root.path(), "example.health");
        write(
            root.path(),
            "src/lib.rs",
            "#[path=\"../plugins/example.health/default.toml\"] pub mod health;",
        );
        write(
            root.path(),
            "plugins/example.health/default.toml",
            "#[lenso::plugin(id=\"example.health\", root_slot=\"web\")] pub struct Plugin {}",
        );
        project_to(root.path(), built.path()).unwrap();
        assert!(
            built
                .path()
                .join("plugins/example.health/default.toml")
                .is_file()
        );
        assert!(resolve(built.path()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn source_symlinks_do_not_bypass_snapshot_checks() {
        let root = tempfile::tempdir().unwrap();
        let built = tempfile::tempdir().unwrap();
        source(root.path(), "health");
        fs::rename(
            root.path().join("plugins/health/plugin.rs"),
            root.path().join("src/health.rs"),
        )
        .unwrap();
        std::os::unix::fs::symlink(
            "../../src/health.rs",
            root.path().join("plugins/health/plugin.rs"),
        )
        .unwrap();
        assert!(project_to(root.path(), built.path()).is_err());
    }
}
