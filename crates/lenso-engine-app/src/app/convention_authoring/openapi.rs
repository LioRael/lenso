//! App-selected source link for the optional OpenAPI Plugin.

use std::{fs, io::Write as _, path::Path};

use anyhow::{Context as _, bail};

use super::{preflight_source_adoption, writable_path, write};

/// Bundle only the source link needed to make the existing optional Plugin
/// available to this generated Host. The App's Plugin Root selects it.
pub(super) fn add(root: &Path, no_install: bool) -> anyhow::Result<()> {
    let relative = Path::new("support/lenso-openapi");
    let destination = root.join(relative);
    writable_path(root, relative)?;
    if fs::symlink_metadata(&destination).is_ok() {
        bail!(
            "OpenAPI support already exists at {}",
            destination.display()
        );
    }
    let mut document = preflight_source_adoption(root, "lenso.openapi")?;
    let source = relative.to_str().context("OpenAPI support source UTF-8")?;
    let sources = document
        .as_table_mut()
        .context("lenso.toml table")?
        .entry("plugin_sources")
        .or_insert_with(|| toml::Value::Array(vec![]))
        .as_array_mut()
        .context("plugin_sources array")?;
    if !sources.contains(&toml::Value::String(source.to_owned())) {
        sources.push(toml::Value::String(source.to_owned()));
    }
    let intent = root.join("plugins/lenso.openapi");
    if intent.exists() {
        bail!(
            "OpenAPI Plugin Root entry already exists at {}",
            intent.display()
        );
    }

    let parent = destination.parent().context("OpenAPI support parent")?;
    fs::create_dir_all(parent)?;
    let stage = tempfile::Builder::new()
        .prefix(".lenso-openapi-support-")
        .tempdir_in(parent)?;
    let revision = crate::plugin::LENSO_FRAMEWORK_REVISION;
    write(
        stage.path(),
        "Cargo.toml",
        format!(
            "[package]\nname = \"app-openapi-link\"\nversion = \"0.2.4\"\nedition = \"2024\"\npublish = false\n\n[package.metadata.lenso]\nplugin-id = \"lenso.openapi\"\nroot-slot = \"http-endpoints\"\n\n[dependencies]\nlenso = {{ version = \"=0.5.26\", git = \"https://github.com/LioRael/lenso\", rev = \"{revision}\" }}\nlenso-openapi-plugin = {{ version = \"=0.2.4\", git = \"https://github.com/LioRael/lenso\", rev = \"{revision}\" }}\n\n[patch.crates-io]\nlenso = {{ git = \"https://github.com/LioRael/lenso\", rev = \"{revision}\" }}\nlenso-app-plan = {{ git = \"https://github.com/LioRael/lenso\", rev = \"{revision}\" }}\nlenso-kernel = {{ git = \"https://github.com/LioRael/lenso\", rev = \"{revision}\" }}\nlenso-native-adapter = {{ git = \"https://github.com/LioRael/lenso\", rev = \"{revision}\" }}\n\n[workspace]\n"
        ),
    )?;
    write(
        stage.path(),
        "src/lib.rs",
        "//! Links the optional OpenAPI Plugin into this App's native Host.\n\npub fn link_plugin() { lenso_openapi_plugin::link_plugin(); }\n",
    )?;
    if !no_install {
        let status = super::super::cargo_command()
            .args(["generate-lockfile", "--manifest-path"])
            .arg(stage.path().join("Cargo.toml"))
            .status()?;
        if !status.success() {
            bail!("OpenAPI support lockfile generation failed");
        }
    }
    let intent_parent = intent.parent().context("OpenAPI Plugin Root parent")?;
    fs::create_dir_all(intent_parent)?;
    let intent_stage = tempfile::Builder::new()
        .prefix(".lenso-openapi-intent-")
        .tempdir_in(intent_parent)?;
    fs::write(
        intent_stage.path().join("default.toml"),
        "# Selecting this Instance publishes the public HTTP Endpoint document.\n",
    )?;
    let config = root.join("lenso.toml");
    let mut staged = tempfile::NamedTempFile::new_in(root)?;
    staged.write_all(toml::to_string_pretty(&document)?.as_bytes())?;
    publish_openapi_adoption(
        stage.path(),
        &destination,
        intent_stage.path(),
        &intent,
        staged,
        &config,
    )?;
    println!("Selected optional OpenAPI Plugin at {}.", intent.display());
    Ok(())
}

/// Stage every output before publishing the first directory. A later rename
/// failure moves only this invocation's new directories back to their staging
/// paths, where the owning TempDirs clean them up. Never replace an existing
/// destination during either publication or rollback.
fn publish_openapi_adoption(
    source_stage: &Path,
    source: &Path,
    intent_stage: &Path,
    intent: &Path,
    config_stage: tempfile::NamedTempFile,
    config: &Path,
) -> anyhow::Result<()> {
    super::super::build::publish_new_output(source_stage, source)?;
    if let Err(error) = super::super::build::publish_new_output(intent_stage, intent) {
        super::super::build::publish_new_output(source, source_stage)
            .context("OpenAPI source rollback failed after Plugin Root publication error")?;
        return Err(error).context("publish OpenAPI Plugin Root intent");
    }
    if let Err(error) = config_stage.persist(config) {
        let intent_rollback = super::super::build::publish_new_output(intent, intent_stage);
        let source_rollback = super::super::build::publish_new_output(source, source_stage);
        intent_rollback.context("OpenAPI Plugin Root rollback failed after lenso.toml error")?;
        source_rollback.context("OpenAPI source rollback failed after lenso.toml error")?;
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{add, publish_openapi_adoption};

    #[test]
    fn openapi_support_is_shared_source_selected_only_by_plugin_root() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("plugins")).unwrap();
        add(root.path(), true).unwrap();

        let report = lenso_app_authoring::discovery::discover(root.path()).unwrap();
        let [candidate] = report.candidates.as_slice() else {
            panic!("expected exactly one optional OpenAPI source")
        };
        assert_eq!(candidate.plugin_id, "lenso.openapi");
        assert_eq!(
            candidate.role,
            lenso_app_authoring::discovery::SourceRole::Shared
        );
        assert_eq!(candidate.implementations[0].runtime, "native-linked");
        assert!(
            root.path()
                .join("plugins/lenso.openapi/default.toml")
                .is_file()
        );
        assert!(
            fs::read_to_string(root.path().join("lenso.toml"))
                .unwrap()
                .contains("support/lenso-openapi")
        );
        let manifest = fs::read_to_string(candidate.project.join("Cargo.toml")).unwrap();
        assert!(manifest.contains(crate::plugin::LENSO_FRAMEWORK_REVISION));
        assert!(
            fs::read_to_string(candidate.project.join("src/lib.rs"))
                .unwrap()
                .contains("lenso_openapi_plugin::link_plugin()")
        );
    }

    #[test]
    fn openapi_intent_publication_failure_rolls_back_only_new_source() {
        let root = tempfile::tempdir().unwrap();
        let source_stage = tempfile::tempdir_in(root.path()).unwrap();
        fs::write(source_stage.path().join("Cargo.toml"), "staged source").unwrap();
        let intent_stage = tempfile::tempdir_in(root.path()).unwrap();
        fs::write(intent_stage.path().join("default.toml"), "staged intent").unwrap();
        let source = root.path().join("support/lenso-openapi");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        let intent = root.path().join("plugins/lenso.openapi");
        fs::create_dir_all(&intent).unwrap();
        fs::write(intent.join("user.txt"), "preserve").unwrap();
        let config_stage = tempfile::NamedTempFile::new_in(root.path()).unwrap();

        let error = publish_openapi_adoption(
            source_stage.path(),
            &source,
            intent_stage.path(),
            &intent,
            config_stage,
            &root.path().join("lenso.toml"),
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("publish OpenAPI Plugin Root intent")
        );
        assert!(!source.exists());
        assert_eq!(
            fs::read_to_string(intent.join("user.txt")).unwrap(),
            "preserve"
        );
        assert_eq!(
            fs::read_to_string(source_stage.path().join("Cargo.toml")).unwrap(),
            "staged source"
        );
        assert!(!root.path().join("lenso.toml").exists());
    }

    #[test]
    fn openapi_config_publication_failure_rolls_back_source_and_intent() {
        let root = tempfile::tempdir().unwrap();
        let source_stage = tempfile::tempdir_in(root.path()).unwrap();
        fs::write(source_stage.path().join("Cargo.toml"), "staged source").unwrap();
        let intent_stage = tempfile::tempdir_in(root.path()).unwrap();
        fs::write(intent_stage.path().join("default.toml"), "staged intent").unwrap();
        let source = root.path().join("support/lenso-openapi");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        let intent = root.path().join("plugins/lenso.openapi");
        fs::create_dir_all(intent.parent().unwrap()).unwrap();
        let config = root.path().join("lenso.toml");
        fs::create_dir(&config).unwrap();
        fs::write(config.join("user.txt"), "preserve").unwrap();
        let config_stage = tempfile::NamedTempFile::new_in(root.path()).unwrap();

        assert!(
            publish_openapi_adoption(
                source_stage.path(),
                &source,
                intent_stage.path(),
                &intent,
                config_stage,
                &config,
            )
            .is_err()
        );

        assert!(!source.exists());
        assert!(!intent.exists());
        assert_eq!(
            fs::read_to_string(config.join("user.txt")).unwrap(),
            "preserve"
        );
        assert!(source_stage.path().join("Cargo.toml").exists());
        assert!(intent_stage.path().join("default.toml").exists());
    }
}
