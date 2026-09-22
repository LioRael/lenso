use std::{
    collections::BTreeMap,
    env,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use lenso_contract_codegen::{
    ProjectionLanguage, check_generated, check_projection, lint_compatibility, write_generated,
    write_projection,
};

fn usage() -> &'static str {
    "usage:\n  lenso-contract-codegen generate <descriptor> <rust-output> <typescript-output>\n  lenso-contract-codegen generate <descriptor> --rust <output>\n  lenso-contract-codegen generate <descriptor> --rust-runtime <output>\n  lenso-contract-codegen generate <descriptor> --rust-plugin <output>\n  lenso-contract-codegen generate <descriptor> --typescript <output>\n  lenso-contract-codegen generate <descriptor> --wit <output>\n  lenso-contract-codegen check <descriptor> <rust-output> <typescript-output>\n  lenso-contract-codegen check <descriptor> --rust <output>\n  lenso-contract-codegen check <descriptor> --rust-runtime <output>\n  lenso-contract-codegen check <descriptor> --rust-plugin <output>\n  lenso-contract-codegen check <descriptor> --typescript <output>\n  lenso-contract-codegen check <descriptor> --wit <output>\n  lenso-contract-codegen workspace <check|generate> [--manifest-path <Cargo.toml>]\n  lenso-contract-codegen lint <old-descriptor> <new-descriptor>"
}

#[derive(Debug)]
struct WorkspaceContract {
    package: String,
    descriptor: PathBuf,
    language: ProjectionLanguage,
    output: PathBuf,
}

fn selected_projection(arguments: &[String]) -> Option<(ProjectionLanguage, &Path)> {
    if arguments.len() != 4 {
        return None;
    }
    let language = match arguments[2].as_str() {
        "--rust" => ProjectionLanguage::Rust,
        "--rust-runtime" => ProjectionLanguage::RustRuntime,
        "--rust-plugin" => ProjectionLanguage::RustPlugin,
        "--typescript" => ProjectionLanguage::TypeScript,
        "--wit" => ProjectionLanguage::Wit,
        _ => return None,
    };
    Some((language, Path::new(&arguments[3])))
}

fn run(arguments: &[String]) -> Result<(), String> {
    let Some(command) = arguments.first().map(String::as_str) else {
        return Err(usage().to_owned());
    };
    match command {
        "generate" => match selected_projection(arguments) {
            Some((language, output)) => {
                write_projection(Path::new(&arguments[1]), language, output)
                    .map_err(|error| error.to_string())
            }
            None if arguments.len() == 4 => write_generated(
                Path::new(&arguments[1]),
                Path::new(&arguments[2]),
                Path::new(&arguments[3]),
            )
            .map_err(|error| error.to_string()),
            None => Err(usage().to_owned()),
        },
        "check" => match selected_projection(arguments) {
            Some((language, output)) => {
                check_projection(Path::new(&arguments[1]), language, output)
                    .map_err(|error| error.to_string())
            }
            None if arguments.len() == 4 => check_generated(
                Path::new(&arguments[1]),
                Path::new(&arguments[2]),
                Path::new(&arguments[3]),
            )
            .map_err(|error| error.to_string()),
            None => Err(usage().to_owned()),
        },
        "lint" if arguments.len() == 3 => {
            lint_compatibility(Path::new(&arguments[1]), Path::new(&arguments[2]))
                .map(|_| ())
                .map_err(|error| error.to_string())
        }
        "workspace" => workspace(arguments),
        _ => Err(usage().to_owned()),
    }
}

fn workspace(arguments: &[String]) -> Result<(), String> {
    let action = arguments.get(1).map(String::as_str);
    if !matches!(action, Some("check" | "generate")) {
        return Err(usage().to_owned());
    }
    let manifest = match arguments.get(2..).unwrap_or_default() {
        [] => Path::new("Cargo.toml"),
        [flag, path] if flag == "--manifest-path" => Path::new(path),
        _ => return Err(usage().to_owned()),
    };
    let contracts = workspace_contracts(manifest)?;
    if contracts.is_empty() {
        return Err(format!(
            "Cargo workspace `{}` declares no `[package.metadata.lenso.contract]` entries",
            manifest.display()
        ));
    }
    let mut failures = Vec::new();
    for contract in contracts {
        let result = match action {
            Some("check") => {
                check_projection(&contract.descriptor, contract.language, &contract.output)
            }
            Some("generate") => {
                write_projection(&contract.descriptor, contract.language, &contract.output)
            }
            _ => unreachable!("workspace action was validated"),
        };
        match result {
            Ok(()) => println!(
                "{} {} -> {}",
                action.expect("workspace action was validated"),
                contract.descriptor.display(),
                contract.output.display()
            ),
            Err(error) => failures.push(format!(
                "package `{}` ({} -> {}): {error}",
                contract.package,
                contract.descriptor.display(),
                contract.output.display()
            )),
        }
    }
    if !failures.is_empty() {
        failures.sort();
        return Err(failures.join("\n"));
    }
    Ok(())
}

fn workspace_contracts(manifest: &Path) -> Result<Vec<WorkspaceContract>, String> {
    let output = Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--manifest-path",
        ])
        .arg(manifest)
        .output()
        .map_err(|error| format!("run Cargo metadata for {}: {error}", manifest.display()))?;
    if !output.status.success() {
        return Err(format!(
            "Cargo metadata failed for {}: {}",
            manifest.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("decode Cargo metadata: {error}"))?;
    let mut packages = metadata["packages"]
        .as_array()
        .ok_or_else(|| "Cargo metadata omitted `packages`".to_owned())?
        .iter()
        .collect::<Vec<_>>();
    packages.sort_by_key(|package| package["manifest_path"].as_str().unwrap_or_default());
    let mut contracts = Vec::new();
    for package in packages {
        let Some(contract) = package
            .pointer("/metadata/lenso/contract")
            .and_then(serde_json::Value::as_object)
        else {
            continue;
        };
        let manifest_path = package["manifest_path"]
            .as_str()
            .ok_or_else(|| "Cargo package omitted `manifest_path`".to_owned())?;
        let root = Path::new(manifest_path)
            .parent()
            .ok_or_else(|| format!("Cargo manifest `{manifest_path}` has no parent"))?;
        let field = |name: &str| {
            contract
                .get(name)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    format!(
                        "package `{}` contract metadata requires string `{name}`",
                        package["name"].as_str().unwrap_or("unknown")
                    )
                })
        };
        let language = match field("projection")? {
            "rust" => ProjectionLanguage::Rust,
            "rust-runtime" => ProjectionLanguage::RustRuntime,
            "rust-plugin" => ProjectionLanguage::RustPlugin,
            "typescript" => ProjectionLanguage::TypeScript,
            "wit" => ProjectionLanguage::Wit,
            value => return Err(format!("unsupported contract projection `{value}`")),
        };
        contracts.push(WorkspaceContract {
            package: package["name"].as_str().unwrap_or("unknown").to_owned(),
            descriptor: normalize_path(&root.join(field("descriptor")?)),
            language,
            output: normalize_path(&root.join(field("output")?)),
        });
    }
    contracts.sort_by(|left, right| {
        left.descriptor
            .cmp(&right.descriptor)
            .then_with(|| left.output.cmp(&right.output))
            .then_with(|| left.package.cmp(&right.package))
    });
    let mut output_owners = BTreeMap::<PathBuf, Vec<&str>>::new();
    for contract in &contracts {
        output_owners
            .entry(contract.output.clone())
            .or_default()
            .push(&contract.package);
    }
    let duplicates = output_owners
        .into_iter()
        .filter(|(_, owners)| owners.len() > 1)
        .map(|(output, owners)| {
            format!(
                "generated output `{}` has multiple owners: {}",
                output.display(),
                owners.join(", ")
            )
        })
        .collect::<Vec<_>>();
    if !duplicates.is_empty() {
        return Err(duplicates.join("\n"));
    }
    Ok(contracts)
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn main() -> ExitCode {
    match run(&env::args().skip(1).collect::<Vec<_>>()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "tests/fixtures/profile/capability.json";

    fn write_workspace(root: &Path, members: &[&str]) -> PathBuf {
        let members = members
            .iter()
            .map(|member| format!("\"{member}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let manifest = root.join("Cargo.toml");
        std::fs::write(
            &manifest,
            format!("[workspace]\nmembers = [{members}]\nresolver = \"3\"\n"),
        )
        .unwrap();
        manifest
    }

    fn write_package(root: &Path, name: &str, projection: &str, output: &str) {
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir(root.join("schemas")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "").unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            format!(
                "[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[package.metadata.lenso.contract]\ndescriptor = \"capability.json\"\nprojection = \"{projection}\"\noutput = \"{output}\"\n"
            ),
        )
        .unwrap();
        let fixture = Path::new(FIXTURE).parent().unwrap();
        std::fs::copy(
            fixture.join("capability.json"),
            root.join("capability.json"),
        )
        .unwrap();
        for entry in std::fs::read_dir(fixture.join("schemas")).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(entry.path(), root.join("schemas").join(entry.file_name())).unwrap();
        }
    }

    fn workspace_arguments(action: &str, manifest: &Path) -> Vec<String> {
        vec![
            "workspace".to_owned(),
            action.to_owned(),
            "--manifest-path".to_owned(),
            manifest.display().to_string(),
        ]
    }

    #[test]
    fn one_language_projection_does_not_require_a_peer_output() {
        let root = std::env::temp_dir().join(format!(
            "lenso-contract-codegen-cli-projection-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("temporary directory should be created");
        let output = root.join("bindings.ts");
        let output = output.to_string_lossy().into_owned();

        run(&[
            "generate".to_owned(),
            FIXTURE.to_owned(),
            "--typescript".to_owned(),
            output.clone(),
        ])
        .expect("TypeScript projection should generate independently");
        run(&[
            "check".to_owned(),
            FIXTURE.to_owned(),
            "--typescript".to_owned(),
            output,
        ])
        .expect("TypeScript projection should check independently");

        assert!(!root.join("bindings.rs").exists());
        std::fs::remove_dir_all(root).expect("temporary directory should be removable");
    }

    #[test]
    fn runtime_codec_projection_is_selectable() {
        let root = std::env::temp_dir().join(format!(
            "lenso-contract-codegen-cli-runtime-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("temporary directory should be created");
        let output = root.join("runtime.rs").to_string_lossy().into_owned();

        run(&[
            "generate".to_owned(),
            FIXTURE.to_owned(),
            "--rust-runtime".to_owned(),
            output.clone(),
        ])
        .expect("runtime codec projection should generate independently");
        run(&[
            "check".to_owned(),
            FIXTURE.to_owned(),
            "--rust-runtime".to_owned(),
            output,
        ])
        .expect("runtime codec projection should check independently");

        std::fs::remove_dir_all(root).expect("temporary directory should be removable");
    }

    #[test]
    fn portable_rust_plugin_projection_is_selectable() {
        let root = std::env::temp_dir().join(format!(
            "lenso-contract-codegen-cli-rust-plugin-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let output = root.join("plugin.rs");
        run(&[
            "generate".to_owned(),
            FIXTURE.to_owned(),
            "--rust-plugin".to_owned(),
            output.display().to_string(),
        ])
        .expect("portable Rust Plugin projection should generate");
        let source = std::fs::read_to_string(&output).unwrap();
        assert!(source.contains("lenso_plugin_sdk::DependencyClient"));
        run(&[
            "check".to_owned(),
            FIXTURE.to_owned(),
            "--rust-plugin".to_owned(),
            output.display().to_string(),
        ])
        .expect("portable Rust Plugin projection should check");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn workspace_commands_discover_contracts_from_cargo_metadata() {
        let root = tempfile::tempdir().expect("temporary workspace should be created");
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::create_dir(root.path().join("schemas")).unwrap();
        std::fs::write(root.path().join("src/lib.rs"), "").unwrap();
        std::fs::write(
            root.path().join("Cargo.toml"),
            r#"[package]
name = "workspace-contract-fixture"
version = "0.0.0"
edition = "2024"

[package.metadata.lenso.contract]
descriptor = "capability.json"
projection = "rust"
output = "src/generated.rs"

[workspace]
"#,
        )
        .unwrap();
        let fixture = Path::new(FIXTURE).parent().unwrap();
        std::fs::copy(
            fixture.join("capability.json"),
            root.path().join("capability.json"),
        )
        .unwrap();
        for entry in std::fs::read_dir(fixture.join("schemas")).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(
                entry.path(),
                root.path().join("schemas").join(entry.file_name()),
            )
            .unwrap();
        }
        let manifest = root.path().join("Cargo.toml").display().to_string();

        run(&[
            "workspace".to_owned(),
            "generate".to_owned(),
            "--manifest-path".to_owned(),
            manifest.clone(),
        ])
        .expect("workspace generation should discover the declared contract");
        run(&[
            "workspace".to_owned(),
            "check".to_owned(),
            "--manifest-path".to_owned(),
            manifest,
        ])
        .expect("workspace check should discover the declared contract");
        assert!(root.path().join("src/generated.rs").is_file());
    }

    #[test]
    fn workspace_commands_check_multiple_packages() {
        let root = tempfile::tempdir().unwrap();
        let manifest = write_workspace(root.path(), &["alpha", "beta"]);
        write_package(
            &root.path().join("alpha"),
            "alpha",
            "rust",
            "src/generated.rs",
        );
        write_package(
            &root.path().join("beta"),
            "beta",
            "rust",
            "src/generated.rs",
        );

        run(&workspace_arguments("generate", &manifest)).unwrap();
        run(&workspace_arguments("check", &manifest)).unwrap();

        assert!(root.path().join("alpha/src/generated.rs").is_file());
        assert!(root.path().join("beta/src/generated.rs").is_file());
    }

    #[test]
    fn workspace_check_reports_stale_and_missing_inputs_deterministically() {
        let root = tempfile::tempdir().unwrap();
        let manifest = write_workspace(root.path(), &["alpha", "beta"]);
        write_package(
            &root.path().join("alpha"),
            "alpha",
            "rust",
            "src/generated.rs",
        );
        write_package(
            &root.path().join("beta"),
            "beta",
            "rust",
            "src/generated.rs",
        );
        run(&workspace_arguments("generate", &manifest)).unwrap();
        std::fs::write(root.path().join("alpha/src/generated.rs"), "stale").unwrap();
        std::fs::remove_file(root.path().join("beta/capability.json")).unwrap();

        let first = run(&workspace_arguments("check", &manifest)).unwrap_err();
        let second = run(&workspace_arguments("check", &manifest)).unwrap_err();

        assert_eq!(first, second);
        assert!(first.contains("package `alpha`"));
        assert!(first.contains("package `beta`"));
    }

    #[test]
    fn workspace_metadata_rejects_an_invalid_projection() {
        let root = tempfile::tempdir().unwrap();
        let manifest = write_workspace(root.path(), &["alpha"]);
        write_package(
            &root.path().join("alpha"),
            "alpha",
            "swift",
            "src/generated.rs",
        );

        let error = workspace_contracts(&manifest).unwrap_err();

        assert_eq!(error, "unsupported contract projection `swift`");
    }

    #[test]
    fn workspace_metadata_rejects_duplicate_output_ownership() {
        let root = tempfile::tempdir().unwrap();
        let manifest = write_workspace(root.path(), &["alpha", "beta"]);
        write_package(
            &root.path().join("alpha"),
            "alpha",
            "rust",
            "../generated.rs",
        );
        write_package(&root.path().join("beta"), "beta", "rust", "../generated.rs");

        let error = workspace_contracts(&manifest).unwrap_err();

        assert!(error.contains("multiple owners: alpha, beta"));
    }
}
