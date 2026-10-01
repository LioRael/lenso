//! Versioned official App authoring; old Cargo/build-script projects stay explicit.
use anyhow::{Context, ensure};
use lenso_engine::{discovery::DiscoverySession, publication::FileResource};
use lenso_engine_web::{WebOptions, compile, read_sources_in};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(crate) fn stage_project(root: &Path, destination: &Path) -> anyhow::Result<PathBuf> {
    let manifest: toml::Value = toml::from_str(&fs::read_to_string(root.join("Cargo.toml"))?)?;
    if manifest
        .get("package")
        .and_then(|package| package.get("metadata"))
        .and_then(|metadata| metadata.get("lenso"))
        .and_then(|lenso| lenso.get("web"))
        .is_none()
    {
        return Ok(root.to_path_buf());
    }
    let output = crate::app::cargo_command()
        .args(["metadata", "--format-version=1", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .output()?;
    ensure!(
        output.status.success(),
        "read Web Cargo graph: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Value = serde_json::from_slice(&output.stdout)?;
    let manifest_path = fs::canonicalize(root.join("Cargo.toml"))?;
    let package = metadata["packages"]
        .as_array()
        .context("Cargo packages")?
        .iter()
        .find(|package| {
            package["manifest_path"]
                .as_str()
                .is_some_and(|path| Path::new(path) == manifest_path)
        })
        .context("selected Web package")?;
    Ok(stage(package, &metadata, destination)?.unwrap_or_else(|| root.to_path_buf()))
}

pub(super) fn stage(
    package: &Value,
    metadata: &Value,
    destination: &Path,
) -> anyhow::Result<Option<PathBuf>> {
    if package.pointer("/metadata/lenso/web").is_none() {
        return Ok(None);
    }
    let root = Path::new(
        package["manifest_path"]
            .as_str()
            .context("Cargo manifest path")?,
    )
    .parent()
    .context("Cargo source root")?;
    stage_in(
        package,
        metadata,
        destination,
        &DiscoverySession::new(root)?,
    )
}

pub(super) fn stage_in(
    package: &Value,
    metadata: &Value,
    destination: &Path,
    inputs: &DiscoverySession,
) -> anyhow::Result<Option<PathBuf>> {
    let Some(config) = package.pointer("/metadata/lenso/web") else {
        return Ok(None);
    };
    let mut config = config
        .as_object()
        .context("lenso.web must be a table")?
        .clone();
    ensure!(
        config.remove("preset").as_ref().and_then(Value::as_str) == Some("v1"),
        "lenso.web requires preset = 'v1'"
    );
    let manifest_path = Path::new(
        package["manifest_path"]
            .as_str()
            .context("Cargo manifest path")?,
    );
    let root = manifest_path.parent().context("Cargo source root")?;
    let mut session = inputs.scope(root)?;
    let mut manifest: toml::Value = toml::from_str(std::str::from_utf8(
        &session.read("Cargo.toml", 4 * 1024 * 1024)?,
    )?)?;
    ensure!(
        !root.join("build.rs").exists()
            && manifest
                .get("package")
                .and_then(|package| package.get("build"))
                .is_none(),
        "Web App preset owns lowering; use the existing explicit build API for a custom build script"
    );
    let entry = manifest
        .get("lib")
        .and_then(|lib| lib.get("path"))
        .and_then(toml::Value::as_str)
        .unwrap_or("src/lib.rs")
        .to_owned();
    let authored = session.read(&entry, 1024 * 1024)?;
    let source = std::str::from_utf8(&authored)?;
    let parsed = syn::parse_file(source)?;
    if !config.contains_key("provider") {
        let providers = parsed
            .items
            .iter()
            .filter_map(|item| {
                let syn::Item::Struct(provider) = item else {
                    return None;
                };
                provider
                    .attrs
                    .iter()
                    .any(|attr| {
                        attr.path()
                            .segments
                            .last()
                            .is_some_and(|segment| segment.ident == "plugin")
                    })
                    .then(|| provider.ident.to_string())
            })
            .collect::<Vec<_>>();
        ensure!(
            providers.len() == 1,
            "Web preset requires one root Plugin provider or an explicit provider option"
        );
        config.insert("provider".into(), Value::String(providers[0].clone()));
    }
    // Missing conventional directories are optional; explicit roots remain strict.
    for (key, default) in [("roots", "src/routes"), ("filesystem_roots", "src/app")] {
        if !config.contains_key(key) {
            config.insert(
                key.into(),
                if root.join(default).exists() {
                    serde_json::json!([default])
                } else {
                    serde_json::json!([])
                },
            );
        }
    }
    let options: WebOptions = serde_json::from_value(Value::Object(config))?;
    let generation = compile(read_sources_in(&mut session, &options)?, options)?;
    let output: FileResource =
        serde_json::from_value(generation.outputs["web/routes"]["bindings"].value.clone())?;
    let lowered = std::str::from_utf8(&output.bytes)?;
    let combined = format!("{source}\n{lowered}");
    // Snapshot/mirror the package, preserving relative module and data paths.
    let mut snapshot = Vec::new();
    collect(&mut session, "", &mut snapshot, 0)?;
    let workspace_root = Path::new(
        metadata["workspace_root"]
            .as_str()
            .context("Cargo workspace root")?,
    );
    let workspace: toml::Value = if workspace_root == root {
        manifest.clone()
    } else {
        toml::from_str(std::str::from_utf8(
            &inputs
                .scope(workspace_root)?
                .read("Cargo.toml", 4 * 1024 * 1024)?,
        )?)?
    };
    // Resolve workspace package inheritance before the staged package becomes standalone.
    if let Some(fields) = manifest
        .get_mut("package")
        .and_then(toml::Value::as_table_mut)
    {
        for (name, value) in fields.iter_mut() {
            if value.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
                *value = workspace
                    .get("workspace")
                    .and_then(|workspace| workspace.get("package"))
                    .and_then(|package| package.get(name))
                    .context("inherited Cargo package field")?
                    .clone();
            }
        }
    }
    rebase_dependencies(&mut manifest, root, workspace_root, &workspace)?;
    let mut staged_workspace = toml::map::Map::new();
    if manifest
        .get("lints")
        .and_then(|lints| lints.get("workspace"))
        .and_then(toml::Value::as_bool)
        == Some(true)
    {
        let lints = workspace
            .get("workspace")
            .and_then(|workspace| workspace.get("lints"))
            .context("inherited Cargo lints")?;
        staged_workspace.insert("lints".into(), lints.clone());
    }
    manifest
        .as_table_mut()
        .context("Cargo manifest table")?
        .insert("workspace".into(), toml::Value::Table(staged_workspace));
    let manifest = toml::to_string(&manifest)?;
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    digest.update(manifest.as_bytes());
    digest.update(combined.as_bytes());
    let mut inputs = Vec::new();
    for path in snapshot {
        let bytes = session.read(&path, 4 * 1024 * 1024)?;
        digest.update((path.len() as u64).to_le_bytes());
        digest.update(path.as_bytes());
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(&bytes);
        inputs.push((path, bytes));
    }
    let destination = destination.join(
        digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
    );
    fs::create_dir_all(&destination)?;
    for (path, bytes) in inputs {
        if path != entry && path != "Cargo.toml" {
            install(&destination.join(&path), &bytes)?;
        }
    }
    install(&destination.join("Cargo.toml"), manifest.as_bytes())?;
    install(&destination.join(&entry), combined.as_bytes())?;
    install(
        &destination.join(".lenso/web-inventory.json"),
        &serde_json::to_vec_pretty(&generation.outputs["web/routes"]["routes"].value)?,
    )?;
    install(
        &destination.join(".lenso/acquisition.json"),
        &serde_json::to_vec_pretty(&session.stats())?,
    )?;
    Ok(Some(destination.to_path_buf()))
}

fn rebase_dependencies(
    value: &mut toml::Value,
    root: &Path,
    workspace_root: &Path,
    workspace: &toml::Value,
) -> anyhow::Result<()> {
    if let Some(table) = value.as_table_mut() {
        for (key, child) in table {
            if key == "dependencies" || key == "build-dependencies" || key == "dev-dependencies" {
                if let Some(dependencies) = child.as_table_mut() {
                    for (name, dependency) in dependencies.iter_mut() {
                        let mut base = root;
                        if dependency.get("workspace").and_then(toml::Value::as_bool) == Some(true)
                        {
                            let local = dependency
                                .as_table()
                                .context("inherited dependency table")?
                                .clone();
                            *dependency = workspace
                                .get("workspace")
                                .and_then(|workspace| workspace.get("dependencies"))
                                .and_then(|dependencies| dependencies.get(name))
                                .context("inherited Cargo dependency")?
                                .clone();
                            if let Some(version) = dependency.as_str() {
                                *dependency = toml::Value::Table(toml::map::Map::from_iter([(
                                    "version".into(),
                                    toml::Value::String(version.into()),
                                )]));
                            }
                            let inherited = dependency
                                .as_table_mut()
                                .context("workspace dependency table")?;
                            for (field, value) in local {
                                if field == "workspace" {
                                    continue;
                                }
                                if field == "features" {
                                    inherited
                                        .entry(field)
                                        .or_insert_with(|| toml::Value::Array(vec![]))
                                        .as_array_mut()
                                        .context("dependency features")?
                                        .extend(
                                            value
                                                .as_array()
                                                .context("dependency features")?
                                                .clone(),
                                        );
                                } else {
                                    inherited.insert(field, value);
                                }
                            }
                            base = workspace_root;
                        }
                        if let Some(path) = dependency.get_mut("path") {
                            let absolute = fs::canonicalize(
                                base.join(path.as_str().context("Cargo dependency path")?),
                            )?;
                            *path = toml::Value::String(
                                absolute.to_str().context("UTF8 dependency path")?.into(),
                            );
                        }
                    }
                }
            } else {
                rebase_dependencies(child, root, workspace_root, workspace)?;
            }
        }
    }
    Ok(())
}

fn collect(
    session: &mut DiscoverySession,
    directory: &str,
    selected: &mut Vec<String>,
    depth: usize,
) -> anyhow::Result<()> {
    ensure!(depth <= 32, "Web package exceeds depth limit");
    for entry in session.directory(directory)? {
        let name = Path::new(&entry.path)
            .file_name()
            .and_then(|name| name.to_str())
            .context("Web package filename")?;
        if [
            ".git",
            ".lenso",
            "target",
            "node_modules",
            "dist",
            "Cargo.lock",
        ]
        .contains(&name)
        {
            continue;
        }
        ensure!(
            !entry.kind.is_symlink() && (entry.kind.is_dir() || entry.kind.is_file()),
            "Web package contains symlink/special file: {}",
            entry.path
        );
        if entry.kind.is_dir() {
            collect(session, &entry.path, selected, depth + 1)?;
        } else {
            selected.push(entry.path);
        }
    }
    Ok(())
}

fn install(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    if fs::read(path).ok().as_deref() == Some(bytes) {
        return Ok(());
    }
    fs::create_dir_all(path.parent().context("staged Web parent")?)?;
    fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture(root: &Path) -> (Value, Value) {
        fs::create_dir_all(root.join("src/app/health")).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname = 'fixture-web'\nversion = '0.1.0'\nedition = '2024'\n[package.metadata.lenso.web]\npreset = 'v1'\n").unwrap();
        fs::write(
            root.join("src/lib.rs"),
            "#[lenso::plugin(id = \"fixture.web\", root_slot = \"web\")]\npub struct Http;\n",
        )
        .unwrap();
        fs::write(
            root.join("src/app/health/route.rs"),
            "#[get] async fn read(&self) {}\n",
        )
        .unwrap();
        (
            json!({"name":"fixture-web","manifest_path":root.join("Cargo.toml"),"metadata":{"lenso":{"web":{"preset":"v1"}}}}),
            json!({"workspace_root":root}),
        )
    }

    #[test]
    fn stages_default_without_glue_reuses_reads_and_never_replays_removed_files() {
        let source = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let (package, metadata) = fixture(source.path());
        let session = DiscoverySession::new(source.path()).unwrap();
        let report = lenso_app_authoring::discovery::discover_in(source.path(), &session).unwrap();
        assert_eq!(report.candidates.len(), 1);
        lenso_app_authoring::discovery::conventions::plan_in(&report, &session).unwrap();
        let first = stage_in(&package, &metadata, output.path(), &session)
            .unwrap()
            .unwrap();
        assert!(!source.path().join("build.rs").exists());
        assert!(
            !fs::read_to_string(source.path().join("src/lib.rs"))
                .unwrap()
                .contains("endpoint")
        );
        let combined = fs::read_to_string(first.join("src/lib.rs")).unwrap();
        assert!(combined.contains("endpoint") && combined.contains("/health"));
        let stats: Value =
            serde_json::from_slice(&fs::read(first.join(".lenso/acquisition.json")).unwrap())
                .unwrap();
        assert_eq!(stats["file_reads"], 3);
        assert_eq!(stats["directory_reads"], 4);
        let modified = fs::metadata(first.join("src/lib.rs"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(
            stage(&package, &metadata, output.path()).unwrap().unwrap(),
            first
        );
        assert_eq!(
            fs::metadata(first.join("src/lib.rs"))
                .unwrap()
                .modified()
                .unwrap(),
            modified
        );
        fs::write(source.path().join("src/app/health/extra.txt"), "extra").unwrap();
        let second = stage(&package, &metadata, output.path()).unwrap().unwrap();
        assert_ne!(first, second);
        fs::remove_file(source.path().join("src/app/health/extra.txt")).unwrap();
        assert_eq!(
            stage(&package, &metadata, output.path()).unwrap().unwrap(),
            first
        );
        assert!(!first.join("src/app/health/extra.txt").exists());
        let mut changed = package.clone();
        changed["metadata"]["lenso"]["web"]["scopes"] = json!({"src/app":["authenticate"]});
        let third = stage(&changed, &metadata, output.path()).unwrap().unwrap();
        assert_ne!(first, third);
        assert!(
            fs::read_to_string(third.join("src/lib.rs"))
                .unwrap()
                .contains("authenticate")
        );
    }

    #[test]
    fn default_preset_compiles_through_the_official_endpoint_macro() {
        let source = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let (package, metadata) = fixture(source.path());
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let mut manifest: toml::Value =
            toml::from_str(&fs::read_to_string(source.path().join("Cargo.toml")).unwrap()).unwrap();
        manifest.as_table_mut().unwrap().insert("dependencies".into(), toml::Value::try_from(serde_json::json!({
            "lenso": {"path": crates.join("lenso")},
            "lenso-capability-http-endpoint": {"path": crates.join("lenso-capability-http-endpoint")}
        })).unwrap());
        manifest.as_table_mut().unwrap().insert(
            "workspace".into(),
            toml::Value::try_from(serde_json::json!({"lints":{"rust":{"unsafe_code":"forbid"}}}))
                .unwrap(),
        );
        manifest.as_table_mut().unwrap().insert(
            "lints".into(),
            toml::Value::try_from(serde_json::json!({"workspace":true})).unwrap(),
        );
        fs::write(
            source.path().join("Cargo.toml"),
            toml::to_string(&manifest).unwrap(),
        )
        .unwrap();
        fs::write(source.path().join("src/lib.rs"), "use lenso_capability_http_endpoint::prelude::*;\n#[lenso::plugin(id = \"fixture.web\", root_slot = \"web\")]\n#[derive(Clone, Debug, Default)] pub struct Http {}\n").unwrap();
        fs::write(source.path().join("src/app/health/route.rs"), "#[get] async fn read(&self) -> Result<Json<String>, Problem> { Ok(Json(\"ok\".into())) }\n").unwrap();
        let staged = stage(&package, &metadata, output.path()).unwrap().unwrap();
        let checked = crate::app::cargo_command()
            .args(["check", "--offline", "--manifest-path"])
            .arg(staged.join("Cargo.toml"))
            .output()
            .unwrap();
        assert!(
            checked.status.success(),
            "{}",
            String::from_utf8_lossy(&checked.stderr)
        );
        assert!(!source.path().join("build.rs").exists());
    }

    #[test]
    fn old_projects_stay_explicit_and_custom_build_ownership_cannot_overlap() {
        assert!(
            stage(&json!({}), &json!({}), Path::new("unused"))
                .unwrap()
                .is_none()
        );
        let source = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let (package, metadata) = fixture(source.path());
        fs::write(source.path().join("build.rs"), "fn main() {}\n").unwrap();
        assert!(stage(&package, &metadata, output.path()).is_err());
    }
}
