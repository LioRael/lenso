use anyhow::{Context as _, bail};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN_DOMAIN_DIRS: &[&str] = &["api", "application", "domain", "infrastructure"];
const REQUIRED_OPENAPI_ARTIFACTS: &[&str] = &["contracts/openapi/app-api.v1.yaml"];
const REQUIRED_EVENT_ARTIFACTS: &[&str] =
    &["contracts/events/identity/identity.user_registered.v1.schema.json"];

pub fn run() -> anyhow::Result<()> {
    let root = repo_root();
    let mut failures = Vec::new();

    collect_result(
        check_forbidden_domain_folders(&root),
        "forbidden domain folders",
        &mut failures,
    );
    collect_result(
        check_forbidden_cross_domain_imports(&root),
        "forbidden cross-domain imports",
        &mut failures,
    );
    collect_result(
        check_required_openapi_artifacts(&root),
        "required OpenAPI artifacts",
        &mut failures,
    );
    collect_result(
        check_contract_artifacts_fresh(&root),
        "fresh contract artifacts",
        &mut failures,
    );
    collect_result(
        check_generated_sdk_fresh(&root),
        "fresh generated SDK",
        &mut failures,
    );
    collect_result(
        check_events_are_documented(&root),
        "documented event payloads",
        &mut failures,
    );

    if failures.is_empty() {
        return Ok(());
    }

    bail!("architecture check failed:\n{}", failures.join("\n"));
}

pub fn check_forbidden_domain_folders(root: &Path) -> anyhow::Result<()> {
    let mut violations = Vec::new();
    let domains_root = root.join("domains");
    for entry in fs::read_dir(&domains_root)
        .with_context(|| format!("failed to read {}", domains_root.display()))?
    {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let domain = entry.file_name().to_string_lossy().into_owned();
        for forbidden in FORBIDDEN_DOMAIN_DIRS {
            for candidate in [
                entry.path().join(forbidden),
                entry.path().join("src").join(forbidden),
            ] {
                if candidate.is_dir() {
                    violations.push(format!("{domain}: {}", relative(root, &candidate)));
                }
            }
        }
    }

    ensure_empty(
        violations,
        "domains must not contain api/application/domain/infrastructure folders",
    )
}

pub fn check_forbidden_cross_domain_imports(root: &Path) -> anyhow::Result<()> {
    let mut violations = Vec::new();
    let domain_names = domain_names(root)?;
    let domains_root = root.join("domains");

    for domain in &domain_names {
        let src = domains_root.join(domain).join("src");
        for file in rust_files(&src)? {
            let source = fs::read_to_string(&file)
                .with_context(|| format!("failed to read {}", file.display()))?;
            for other_domain in domain_names.iter().filter(|name| *name != domain) {
                for pattern in [
                    format!("use {other_domain}::"),
                    format!("{other_domain}::"),
                    format!("extern crate {other_domain}"),
                ] {
                    if source.contains(&pattern) {
                        violations.push(format!("{} imports `{pattern}`", relative(root, &file)));
                    }
                }
            }
        }
    }

    ensure_empty(
        violations,
        "domains must call other domains through public interfaces or events",
    )
}

pub fn check_required_openapi_artifacts(root: &Path) -> anyhow::Result<()> {
    let mut missing = Vec::new();
    for artifact in REQUIRED_OPENAPI_ARTIFACTS {
        if !root.join(artifact).is_file() {
            missing.push((*artifact).to_owned());
        }
    }

    ensure_empty(missing, "required OpenAPI artifacts are missing")
}

pub fn check_contract_artifacts_fresh(root: &Path) -> anyhow::Result<()> {
    let openapi = read_yaml(root.join("contracts/openapi/app-api.v1.yaml"))?;
    let generated_openapi = serde_json::to_value(app_api::openapi_document())
        .context("OpenAPI document should serialize")?;
    let error_schema = read_json(root.join("contracts/errors/error-response.v1.schema.json"))?;
    let event_schema =
        read_json(root.join("contracts/events/identity/identity.user_registered.v1.schema.json"))?;

    let mut violations = Vec::new();
    if openapi != generated_openapi {
        violations.push("contracts/openapi/app-api.v1.yaml is stale".to_owned());
    }
    if error_schema != generate_contracts::generated_error_response_schema() {
        violations.push("contracts/errors/error-response.v1.schema.json is stale".to_owned());
    }
    if event_schema != generate_contracts::generated_identity_user_registered_schema() {
        violations.push(
            "contracts/events/identity/identity.user_registered.v1.schema.json is stale".to_owned(),
        );
    }

    ensure_empty(violations, "contract artifacts must match Rust sources")
}

pub fn check_generated_sdk_fresh(root: &Path) -> anyhow::Result<()> {
    let mut violations = Vec::new();
    let types = fs::read_to_string(root.join("packages/ts-sdk/src/generated/types.ts"))
        .context("failed to read generated TypeScript types")?;
    let client = fs::read_to_string(root.join("packages/ts-sdk/src/generated/client.ts"))
        .context("failed to read generated TypeScript client")?;

    if types != generate_ts_sdk::generated_types_source()? {
        violations.push("packages/ts-sdk/src/generated/types.ts is stale".to_owned());
    }
    if client != generate_ts_sdk::generated_client_source()? {
        violations.push("packages/ts-sdk/src/generated/client.ts is stale".to_owned());
    }

    ensure_empty(violations, "generated SDK files must match OpenAPI")
}

pub fn check_events_are_documented(root: &Path) -> anyhow::Result<()> {
    let mut missing = Vec::new();
    for artifact in REQUIRED_EVENT_ARTIFACTS {
        if !root.join(artifact).is_file() {
            missing.push((*artifact).to_owned());
        }
    }

    ensure_empty(missing, "domain events must have JSON Schema contracts")
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("arch-check crate should be inside tools/arch-check")
        .to_path_buf()
}

fn collect_result(result: anyhow::Result<()>, label: &str, failures: &mut Vec<String>) {
    if let Err(error) = result {
        failures.push(format!("- {label}: {error}"));
    }
}

fn domain_names(root: &Path) -> anyhow::Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    let domains_root = root.join("domains");
    for entry in fs::read_dir(&domains_root)
        .with_context(|| format!("failed to read {}", domains_root.display()))?
    {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            names.insert(entry.file_name().to_string_lossy().into_owned());
        }
    }
    Ok(names)
}

fn rust_files(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_rust_files(root, &mut files)?;
    Ok(files)
}

fn collect_rust_files(root: &Path, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_rust_files(&path, files)?;
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            files.push(path);
        }
    }
    Ok(())
}

fn read_yaml(path: PathBuf) -> anyhow::Result<Value> {
    let source =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_yaml::from_str(&source).with_context(|| format!("failed to parse {}", path.display()))
}

fn read_json(path: PathBuf) -> anyhow::Result<Value> {
    let source =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&source).with_context(|| format!("failed to parse {}", path.display()))
}

fn ensure_empty(violations: Vec<String>, message: &str) -> anyhow::Result<()> {
    if violations.is_empty() {
        return Ok(());
    }

    bail!("{message}:\n{}", violations.join("\n"))
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}
