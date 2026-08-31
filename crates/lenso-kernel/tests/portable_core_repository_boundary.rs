use std::{
    collections::BTreeSet,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

const CORE_DIRECTORIES: &[&str] = &[
    "lenso-app-plan",
    "lenso-kernel",
    "lenso-runtime-conformance",
];

const CORE_PACKAGE_RULES: &[CorePackageRule] = &[
    CorePackageRule {
        directory: "crates/lenso-app-plan",
        allowed_packages: &["lenso-app-plan"],
        allowed_crates: &["lenso_app_plan"],
    },
    CorePackageRule {
        directory: "crates/lenso-kernel",
        allowed_packages: &[
            "lenso-app-plan",
            "lenso-kernel",
            "lenso-runtime-conformance",
        ],
        allowed_crates: &[
            "lenso_app_plan",
            "lenso_kernel",
            "lenso_runtime_conformance",
        ],
    },
    CorePackageRule {
        directory: "crates/lenso-runtime-conformance",
        allowed_packages: &[
            "lenso-app-plan",
            "lenso-kernel",
            "lenso-runtime-conformance",
        ],
        allowed_crates: &[
            "lenso_app_plan",
            "lenso_kernel",
            "lenso_runtime_conformance",
        ],
    },
];

#[derive(Clone, Copy, Debug)]
struct CorePackageRule {
    directory: &'static str,
    allowed_packages: &'static [&'static str],
    allowed_crates: &'static [&'static str],
}

#[test]
fn repository_contains_only_portable_core_packages() {
    let repository_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("Kernel crate must live below the repository root");
    let mut failures = Vec::new();

    let crates_root = repository_root.join("crates");
    let mut actual_directories = fs::read_dir(&crates_root)
        .expect("read crates directory")
        .map(|entry| entry.expect("inspect crate entry"))
        .filter(|entry| entry.file_type().expect("inspect crate type").is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    actual_directories.sort();

    if actual_directories != CORE_DIRECTORIES {
        failures.push(format!(
            "crates/ must contain only portable core packages; expected {CORE_DIRECTORIES:?}, found {actual_directories:?}"
        ));
    }
    if repository_root.join("fixtures").exists() {
        failures.push("fixtures/ is owned by outer repositories".to_owned());
    }

    for rule in CORE_PACKAGE_RULES {
        let package_root = repository_root.join(rule.directory);
        let manifest_path = package_root.join("Cargo.toml");
        let manifest = fs::read_to_string(&manifest_path).expect("read package manifest");

        for dependency in path_dependencies(&manifest) {
            if !rule.allowed_packages.contains(&dependency.as_str()) {
                failures.push(format!(
                    "{} has forbidden path dependency `{dependency}`",
                    display_path(repository_root, &manifest_path)
                ));
            }
        }

        let mut rust_files = Vec::new();
        for source_directory in [package_root.join("src"), package_root.join("tests")] {
            if source_directory.is_dir() {
                collect_rust_files(&source_directory, &mut rust_files);
            }
        }
        rust_files.sort();

        for rust_file in rust_files {
            let source = fs::read_to_string(&rust_file).expect("read Rust source");
            for crate_name in referenced_lenso_crates(&source) {
                if !rule.allowed_crates.contains(&crate_name.as_str()) {
                    failures.push(format!(
                        "{} references forbidden crate `{crate_name}`",
                        display_path(repository_root, &rust_file)
                    ));
                }
            }
        }
    }

    failures.sort();
    failures.dedup();
    assert!(
        failures.is_empty(),
        "portable-core repository boundary failed:\n{}",
        failures.join("\n")
    );
}

fn path_dependencies(manifest: &str) -> BTreeSet<String> {
    manifest
        .lines()
        .filter_map(|line| {
            let (name, value) = line.trim().split_once('=')?;
            (value.trim_start().starts_with('{')
                && inline_dependency_field(value, "path").is_some())
            .then(|| {
                inline_dependency_field(value, "package")
                    .and_then(quoted_value)
                    .unwrap_or_else(|| name.trim().to_owned())
            })
        })
        .collect()
}

fn inline_dependency_field<'a>(value: &'a str, expected: &str) -> Option<&'a str> {
    value.split(',').find_map(|field| {
        let (name, value) = field.split_once('=')?;
        (name.trim().trim_start_matches('{').trim() == expected).then_some(value.trim())
    })
}

fn quoted_value(value: &str) -> Option<String> {
    value
        .strip_prefix('"')
        .and_then(|value| value.split_once('"'))
        .map(|(value, _)| value.to_owned())
}

fn referenced_lenso_crates(source: &str) -> BTreeSet<String> {
    source
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .filter(|token| token.starts_with("lenso_") && token.len() > "lenso_".len())
        .map(ToOwned::to_owned)
        .collect()
}

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
    let mut entries = fs::read_dir(directory)
        .expect("read source directory")
        .map(|entry| entry.expect("inspect source entry"))
        .collect::<Vec<_>>();
    entries.sort_by_key(fs::DirEntry::file_name);

    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type().expect("inspect source entry type");
        if file_type.is_dir() {
            collect_rust_files(&path, files);
        } else if file_type.is_file()
            && path.extension() == Some(OsStr::new("rs"))
            && path.file_name() != Some(OsStr::new("generated.rs"))
            && !path.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::Normal(name)
                        if name == OsStr::new("generated") || name == OsStr::new("snapshots")
                )
            })
        {
            files.push(path);
        }
    }
}

fn display_path(repository_root: &Path, path: &Path) -> String {
    path.strip_prefix(repository_root).map_or_else(
        |_| path.display().to_string(),
        |relative| relative.to_string_lossy().replace('\\', "/"),
    )
}
