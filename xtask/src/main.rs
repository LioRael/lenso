use std::{
    collections::BTreeMap,
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process,
};

const NOTICE_LINE_LIMIT: usize = 600;
const DEFAULT_LINE_LIMIT: usize = 1_000;
const DEBT_FILE: &str = "scripts/rust-module-size-debt.txt";
const SOURCE_ROOTS: &[&str] = &["crates", "fixtures"];

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args_os().skip(1);
    let usage = "usage: cargo xtask check-rust-module-size [--report-notices]";
    let Some(command) = arguments.next() else {
        return Err(usage.to_owned());
    };

    if command != OsStr::new("check-rust-module-size") {
        return Err(format!(
            "unknown xtask command `{}`; {usage}",
            command.to_string_lossy()
        ));
    }

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

fn check_rust_module_sizes(report_notices: bool) -> Result<(), String> {
    let repository_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask manifest has no repository parent".to_owned())?;
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
}
