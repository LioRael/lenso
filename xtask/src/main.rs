use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process,
};

const NOTICE_LINE_LIMIT: usize = 600;
const DEFAULT_LINE_LIMIT: usize = 1_000;
const DEBT_FILE: &str = "scripts/rust-module-size-debt.txt";
const SOURCE_ROOTS: &[&str] = &["crates"];
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

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args_os().skip(1);
    let usage = "usage: cargo xtask <check-rust-module-size [--report-notices] | check-core-repository-boundary>";
    let Some(command) = arguments.next() else {
        return Err(usage.to_owned());
    };

    match command.to_str() {
        Some("check-rust-module-size") => {
            let report_notices = match arguments.next() {
                None => false,
                Some(flag) if flag == OsStr::new("--report-notices") => true,
                Some(argument) => {
                    return Err(format!(
                        "unexpected argument `{}`; {usage}",
                        argument.to_string_lossy()
                    ));
                }
            };
            if arguments.next().is_some() {
                return Err(usage.to_owned());
            }
            check_rust_module_sizes(report_notices)
        }
        Some("check-core-repository-boundary") => {
            if arguments.next().is_some() {
                return Err(usage.to_owned());
            }
            check_core_repository_boundary()
        }
        _ => Err(format!(
            "unknown xtask command `{}`; {usage}",
            command.to_string_lossy()
        )),
    }
}

fn repository_root() -> Result<&'static Path, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask manifest has no repository parent".to_owned())
}

fn check_rust_module_sizes(report_notices: bool) -> Result<(), String> {
    let repository_root = repository_root()?;
    let debt_path = repository_root.join(DEBT_FILE);
    let debt = fs::read_to_string(&debt_path).map_err(|error| {
        format!(
            "could not read {}: {error}",
            display_path(repository_root, &debt_path)
        )
    })?;
    let debt = parse_debt(&debt)?;

    let mut source_files = Vec::new();
    for source_root in SOURCE_ROOTS {
        collect_rust_files(&repository_root.join(source_root), &mut source_files)?;
    }
    source_files.sort();

    let mut failed = false;
    for source_file in source_files {
        let relative_path = source_file
            .strip_prefix(repository_root)
            .map_err(|error| format!("could not relativize source path: {error}"))?;
        let relative_path = relative_path.to_string_lossy().replace('\\', "/");
        let line_count = physical_line_count(&source_file)?;
        let line_limit = debt
            .get(relative_path.as_str())
            .copied()
            .unwrap_or(DEFAULT_LINE_LIMIT);

        if line_count > line_limit {
            eprintln!("error: {relative_path} has {line_count} lines (limit: {line_limit})");
            failed = true;
        } else if report_notices && line_count > NOTICE_LINE_LIMIT {
            eprintln!(
                "notice: {relative_path} has {line_count} lines; review for a cohesive split"
            );
        }
    }

    if failed {
        return Err(
            "Rust module size check failed. Split by responsibility; do not raise a limit without architecture rationale."
                .to_owned(),
        );
    }

    Ok(())
}

fn check_core_repository_boundary() -> Result<(), String> {
    let repository_root = repository_root()?;
    let mut failures = Vec::new();

    let crates_root = repository_root.join("crates");
    let mut actual_directories = fs::read_dir(&crates_root)
        .map_err(|error| format!("could not read {}: {error}", crates_root.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("could not inspect {}: {error}", crates_root.display()))?
        .into_iter()
        .filter_map(|entry| {
            entry
                .file_type()
                .ok()
                .filter(std::fs::FileType::is_dir)
                .map(|_| entry.file_name().to_string_lossy().into_owned())
        })
        .collect::<Vec<_>>();
    actual_directories.sort();
    let expected_directories = CORE_DIRECTORIES
        .iter()
        .map(|directory| (*directory).to_owned())
        .collect::<Vec<_>>();
    if actual_directories != expected_directories {
        failures.push(format!(
            "crates/ must contain only portable core directories; expected {expected_directories:?}, found {actual_directories:?}"
        ));
    }
    if repository_root.join("fixtures").exists() {
        failures.push("fixtures/ is owned by outer repositories".to_owned());
    }

    for rule in CORE_PACKAGE_RULES {
        let package_root = repository_root.join(rule.directory);
        let manifest_path = package_root.join("Cargo.toml");
        let manifest = fs::read_to_string(&manifest_path).map_err(|error| {
            format!(
                "could not read {}: {error}",
                display_path(repository_root, &manifest_path)
            )
        })?;
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
                collect_rust_files(&source_directory, &mut rust_files)?;
            }
        }
        rust_files.sort();
        for rust_file in rust_files {
            let contents = fs::read_to_string(&rust_file).map_err(|error| {
                format!(
                    "could not read {}: {error}",
                    display_path(repository_root, &rust_file)
                )
            })?;
            for crate_name in referenced_lenso_crates(&contents) {
                if !rule.allowed_crates.contains(&crate_name.as_str()) {
                    failures.push(format!(
                        "{} references forbidden crate `{crate_name}`",
                        display_path(repository_root, &rust_file)
                    ));
                }
            }
        }
    }

    if failures.is_empty() {
        return Ok(());
    }
    failures.sort();
    failures.dedup();
    Err(format!(
        "Core repository boundary check failed:\n{}",
        failures.join("\n")
    ))
}

fn path_dependencies(manifest: &str) -> BTreeSet<String> {
    manifest
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let (name, value) = line.split_once('=')?;
            let name = name.trim();
            (value.trim_start().starts_with('{')
                && inline_dependency_field(value, "path").is_some())
            .then(|| package_name_from_inline_dependency(value).unwrap_or_else(|| name.to_owned()))
        })
        .collect()
}

fn package_name_from_inline_dependency(value: &str) -> Option<String> {
    let package = inline_dependency_field(value, "package")?;

    package
        .strip_prefix('"')
        .and_then(|package| package.split_once('"'))
        .map(|(package, _)| package.to_owned())
}

fn inline_dependency_field<'a>(value: &'a str, expected: &str) -> Option<&'a str> {
    value.split(',').find_map(|field| {
        let (name, value) = field.split_once('=')?;
        (name.trim().trim_start_matches('{').trim() == expected).then_some(value.trim())
    })
}

fn referenced_lenso_crates(source: &str) -> BTreeSet<String> {
    source
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .filter(|token| token.starts_with("lenso_") && token.len() > "lenso_".len())
        .map(ToOwned::to_owned)
        .collect()
}

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| format!("could not read {}: {error}", directory.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("could not inspect {}: {error}", directory.display()))?;
    entries.sort_by_key(fs::DirEntry::file_name);

    for entry in entries {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;

        if file_type.is_dir() {
            collect_rust_files(&path, files)?;
        } else if file_type.is_file()
            && path.extension() == Some(OsStr::new("rs"))
            && !is_excluded(&path)
        {
            files.push(path);
        }
    }

    Ok(())
}

fn is_excluded(path: &Path) -> bool {
    path.file_name() == Some(OsStr::new("generated.rs"))
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::Normal(name)
                    if name == OsStr::new("generated") || name == OsStr::new("snapshots")
            )
        })
}

fn physical_line_count(path: &Path) -> Result<usize, String> {
    let contents =
        fs::read(path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
    Ok(contents
        .split(|byte| *byte == b'\n')
        .count()
        .saturating_sub(1))
}

fn parse_debt(contents: &str) -> Result<BTreeMap<String, usize>, String> {
    let mut debt = BTreeMap::new();

    for (line_number, line) in contents.lines().enumerate() {
        let line_number = line_number + 1;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let mut fields = line.split_whitespace();
        let Some(path) = fields.next() else {
            continue;
        };
        let Some(limit) = fields.next() else {
            return Err(format!("{DEBT_FILE}:{line_number}: expected a line limit"));
        };
        let limit = limit.parse::<usize>().map_err(|error| {
            format!("{DEBT_FILE}:{line_number}: invalid line limit `{limit}`: {error}")
        })?;

        if debt.insert(path.to_owned(), limit).is_some() {
            return Err(format!(
                "{DEBT_FILE}:{line_number}: duplicate path `{path}`"
            ));
        }
    }

    Ok(debt)
}

fn display_path(repository_root: &Path, path: &Path) -> String {
    path.strip_prefix(repository_root).map_or_else(
        |_| path.display().to_string(),
        |relative| relative.to_string_lossy().replace('\\', "/"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_comments_and_debt_limits() {
        let debt =
            parse_debt("# Existing debt\n\ncrates/example.rs 1200\nfixtures/example.rs 1400\n")
                .expect("valid debt file");

        assert_eq!(debt.get("crates/example.rs"), Some(&1200));
        assert_eq!(debt.get("fixtures/example.rs"), Some(&1400));
    }

    #[test]
    fn rejects_duplicate_debt_paths() {
        let error = parse_debt("crates/example.rs 1200\ncrates/example.rs 1300\n")
            .expect_err("duplicate debt paths should fail");

        assert!(error.contains("duplicate path `crates/example.rs`"));
    }

    #[test]
    fn excludes_generated_and_snapshot_paths() {
        assert!(is_excluded(Path::new("fixtures/example/src/generated.rs")));
        assert!(is_excluded(Path::new(
            "fixtures/example/generated/value.rs"
        )));
        assert!(is_excluded(Path::new(
            "fixtures/example/snapshots/value.rs"
        )));
        assert!(!is_excluded(Path::new("fixtures/example/src/module.rs")));
    }

    #[test]
    fn counts_physical_lines_like_wc_l() {
        let path = env::temp_dir().join(format!("lenso-module-size-test-{}", process::id()));
        fs::write(&path, b"first\nsecond\nlast").expect("write test source");

        assert_eq!(physical_line_count(&path).expect("read test source"), 2);

        fs::remove_file(path).expect("remove test source");
    }

    #[test]
    fn finds_path_dependencies_and_resolves_package_aliases() {
        let dependencies = path_dependencies(
            "lenso-kernel = { path = \"../lenso-kernel\" }\n\
             kernel = { package = \"lenso-kernel\", path = \"../lenso-kernel\" }\n\
             description = \"mentions a path but is not a dependency\"\n\
             serde.workspace = true\n",
        );

        assert_eq!(dependencies, BTreeSet::from(["lenso-kernel".to_owned()]));
    }

    #[test]
    fn finds_referenced_lenso_crates_without_matching_product_words() {
        let crates = referenced_lenso_crates(
            "use lenso_app_plan::ResolvedAppPlan; // lenso-kernel is not a Rust crate token",
        );

        assert_eq!(crates, BTreeSet::from(["lenso_app_plan".to_owned()]));
    }
}
