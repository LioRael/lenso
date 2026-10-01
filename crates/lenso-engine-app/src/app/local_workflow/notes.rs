//! Keep ordinary creation on published inputs until the typed SDK is released.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, ensure};

const SDK: &str = "lenso-capability-http-endpoint";
const TYPED_MANIFEST: &str = include_str!("../../../assets/process-notes/Cargo.toml.template");
const REGISTRY_MANIFEST: &str = include_str!("../../../assets/process-notes/registry-Cargo.toml");

pub(super) fn scaffold(source: Option<&Path>) -> anyhow::Result<Vec<(PathBuf, String)>> {
    let Some(source) = source else {
        return Ok(vec![
            ("Cargo.toml".into(), REGISTRY_MANIFEST.into()),
            (
                "src/main.rs".into(),
                "fn main() { local_starter::serve(); }\n".into(),
            ),
            (
                "src/lib.rs".into(),
                include_str!("../../../assets/process-notes/registry-lib.rs").into(),
            ),
        ]);
    };
    let source = fs::canonicalize(source).context("locate HTTP Endpoint SDK source crate")?;
    let package: toml::Value = toml::from_str(&fs::read_to_string(source.join("Cargo.toml"))?)?;
    let mut manifest: toml::Value = toml::from_str(TYPED_MANIFEST)?;
    let dependency = manifest["dependencies"][SDK]
        .as_table_mut()
        .context("typed starter SDK dependency")?;
    let expected = dependency["version"]
        .as_str()
        .context("typed starter SDK version")?;
    let identity = package
        .get("package")
        .context("--http-sdk-source must be a Cargo package")?;
    ensure!(
        identity.get("name").and_then(toml::Value::as_str) == Some(SDK)
            && identity.get("version").and_then(toml::Value::as_str)
                == Some(expected.trim_start_matches('=')),
        "--http-sdk-source requires {SDK} {expected}"
    );
    ensure!(
        package
            .get("features")
            .and_then(|features| features.get("process"))
            .and_then(toml::Value::as_array)
            .is_some(),
        "--http-sdk-source requires the SDK's `process` feature"
    );
    dependency.insert(
        "path".into(),
        source
            .to_str()
            .context("SDK source path must be UTF-8")?
            .into(),
    );
    Ok(vec![
        ("Cargo.toml".into(), toml::to_string(&manifest)?),
        (
            "src/main.rs".into(),
            include_str!("../../../assets/process-notes/src/main.rs").into(),
        ),
        (
            "src/lib.rs".into(),
            include_str!("../../../assets/process-notes/src/lib.rs").into(),
        ),
        (
            "tests/notes.rs".into(),
            include_str!("../../../assets/process-notes/tests/notes.rs").into(),
        ),
    ])
}

pub(super) fn readme(source_selected: bool) -> &'static str {
    if source_selected {
        concat!(
            include_str!("../../../assets/process-notes/README.md"),
            "\n"
        )
    } else {
        "The root Process Plugin provides `POST /notes` and `GET /notes/{id}`. Notes are in-memory development data and do not survive a restart. Process Plugins are trusted native executables, not sandboxed.\n\nThis starter uses published Process SDK 0.2.0 and HTTP Endpoint 0.3.2. Editing Guest source rebuilds its Process artifact while reusing the precompiled Host. Use the same CLI binary for build and start.\n\nTyped handler authoring is available as a source candidate through `lenso new NAME --http-sdk-source /path/to/lenso/crates/lenso-capability-http-endpoint`. Ordinary creation stays on the registry starter until that SDK is published; `--no-install` only skips installation.\n\nTo remove the Web surface, disable both `local.starter/default` and `lenso.web-ingress/default` in the built Plugin Root, then use `lenso app start --from dist --root dist`.\n\n"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffold_assets_are_not_excluded_as_a_nested_cargo_package() {
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/process-notes");
        assert!(
            !assets.join("Cargo.toml").exists(),
            "Cargo excludes nested packages even when assets/** is explicitly included"
        );
    }

    #[test]
    fn registry_scaffold_does_not_require_candidate_features_or_paths() {
        let files = scaffold(None).unwrap();
        let manifest: toml::Value = toml::from_str(&files[0].1).unwrap();
        assert_eq!(manifest["dependencies"][SDK].as_str(), Some("=0.3.2"));
        assert_eq!(
            manifest["dependencies"]["lenso-process-sdk"].as_str(),
            Some("=0.2.0")
        );
        assert!(manifest.get("patch").is_none());
        assert_eq!(files.len(), 3);
    }

    #[test]
    fn source_selection_checks_identity_version_and_feature() {
        let root = tempfile::tempdir().unwrap();
        for manifest in [
            "",
            "[workspace]\n",
            "[package]\nname='wrong'\nversion='0.3.8'\n[features]\nprocess=[]\n",
            "[package]\nname='lenso-capability-http-endpoint'\nversion='0.3.2'\n[features]\nprocess=[]\n",
            "[package]\nname='lenso-capability-http-endpoint'\nversion='0.3.8'\n",
        ] {
            fs::write(root.path().join("Cargo.toml"), manifest).unwrap();
            assert!(scaffold(Some(root.path())).is_err());
        }
        fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname='lenso-capability-http-endpoint'\nversion='0.3.8'\n[features]\nprocess=[]\n",
        ).unwrap();
        let files = scaffold(Some(root.path())).unwrap();
        let manifest: toml::Value = toml::from_str(&files[0].1).unwrap();
        assert_eq!(
            Path::new(manifest["dependencies"][SDK]["path"].as_str().unwrap()),
            root.path().canonicalize().unwrap()
        );
        assert_eq!(files.len(), 4);
        let candidate_sdk = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(SDK);
        assert!(
            scaffold(Some(&candidate_sdk)).is_ok(),
            "typed starter must accept the SDK version shipped in this cohort"
        );
    }
}
