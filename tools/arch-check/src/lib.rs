use anyhow::{Context as _, bail};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN_DOMAIN_DIRS: &[&str] = &["api", "application", "domain", "infrastructure"];
const MANIFEST_LINT_MESSAGES: &[&str] = &[
    "No HTTP interfaces are declared in this manifest.",
    "routes declare the same method and path.",
    "Missing display_name for compact runtime story nodes.",
    "Missing story_title for direct HTTP entry stories.",
    "Missing capability declaration for host proxy authorization.",
    "Declared routes include display, story, and capability metadata.",
    "Declared routes include display and story metadata.",
];
const REQUIRED_OPENAPI_ARTIFACTS: &[&str] = &["contracts/openapi/app-api.v1.yaml"];

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
        check_crates_no_domain_deps(
            &root,
            &[
                "platform-admin",
                "platform-admin-data",
                "platform-module-remote",
            ],
        ),
        "platform admin/remote domain dependency",
        &mut failures,
    );
    collect_result(
        check_runtime_console_does_not_duplicate_manifest_lints(&root),
        "runtime console manifest lint ownership",
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
        check_contract_files_parse(&root),
        "parseable contract files",
        &mut failures,
    );
    collect_result(
        check_generated_sdk_fresh(&root),
        "fresh generated SDK",
        &mut failures,
    );
    collect_result(
        check_event_schema_refs_exist(&root),
        "event schema references",
        &mut failures,
    );
    collect_result(
        check_event_contract_names_match_paths(&root),
        "event contract names",
        &mut failures,
    );
    collect_result(
        check_runtime_function_contracts(&root),
        "runtime function contracts",
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

/// Platform admin/remote crates must not depend on business domains; they work
/// through composition-root injection and `platform-module` seams.
pub fn check_crates_no_domain_deps(root: &Path, crates: &[&str]) -> anyhow::Result<()> {
    let domain_names = domain_names(root)?;
    let mut violations = Vec::new();

    for crate_name in crates {
        let manifest = root.join(format!("crates/{crate_name}/Cargo.toml"));
        let source = fs::read_to_string(&manifest)
            .with_context(|| format!("failed to read {}", manifest.display()))?;

        for domain in &domain_names {
            if source.contains(&format!("{domain}.workspace"))
                || source.contains(&format!("\"{domain}\""))
                || source.contains(&format!("{domain} ="))
            {
                violations.push(format!("{crate_name} depends on domain `{domain}`"));
            }
        }
    }

    ensure_empty(
        violations,
        "platform admin/remote crates must not depend on any domain crate",
    )
}

pub fn check_runtime_console_does_not_duplicate_manifest_lints(root: &Path) -> anyhow::Result<()> {
    let mut violations = Vec::new();
    let console_src = root.join("apps/runtime-console/src");
    for file in typescript_files(&console_src)? {
        let source = fs::read_to_string(&file)
            .with_context(|| format!("failed to read {}", file.display()))?;
        for message in MANIFEST_LINT_MESSAGES {
            if source.contains(message) {
                violations.push(format!(
                    "{} duplicates backend manifest lint message `{message}`",
                    relative(root, &file),
                ));
            }
        }
    }

    ensure_empty(
        violations,
        "runtime console must render backend manifest lints instead of reimplementing them",
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

pub fn check_contract_files_parse(root: &Path) -> anyhow::Result<()> {
    let mut violations = Vec::new();
    for file in contract_files(root)? {
        let source = fs::read_to_string(&file)
            .with_context(|| format!("failed to read {}", file.display()))?;
        match file.extension().and_then(|extension| extension.to_str()) {
            Some("json") => {
                if let Err(error) = serde_json::from_str::<Value>(&source) {
                    violations.push(format!(
                        "{} failed JSON parse: {error}",
                        relative(root, &file)
                    ));
                }
            }
            Some("yaml" | "yml") => {
                if let Err(error) = serde_yaml::from_str::<Value>(&source) {
                    violations.push(format!(
                        "{} failed YAML parse: {error}",
                        relative(root, &file)
                    ));
                }
            }
            _ => {}
        }
    }

    ensure_empty(violations, "contract JSON/YAML files must parse")
}

pub fn check_event_schema_refs_exist(root: &Path) -> anyhow::Result<()> {
    let mut missing = Vec::new();
    for reference in event_schema_refs(root)? {
        if !root.join(&reference).is_file() {
            missing.push(reference);
        }
    }

    ensure_empty(missing, "domain event schema references must exist")
}

pub fn check_event_contract_names_match_paths(root: &Path) -> anyhow::Result<()> {
    let mut violations = Vec::new();
    for file in schema_files(&root.join("contracts/events"))? {
        let expected_name = contract_name_from_schema_path(&file)?;
        let value = read_json(file.clone())?;
        let parent_domain = file
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .unwrap_or_default();

        if !expected_name.starts_with(&format!("{parent_domain}.")) {
            violations.push(format!(
                "{} contract name should start with `{parent_domain}.`",
                relative(root, &file),
            ));
        }

        collect_contract_name_violations(root, &file, &value, &expected_name, &mut violations);
    }

    ensure_empty(
        violations,
        "event contract title/$id values must match their paths",
    )
}

pub fn check_runtime_function_contracts(root: &Path) -> anyhow::Result<()> {
    let function_names = runtime_function_names(root)?;
    let contract_names = runtime_function_contract_names(root)?;
    let mut violations = Vec::new();

    for function_name in function_names {
        if !contract_names.contains(&function_name) {
            violations.push(format!(
                "{function_name} is missing contracts/runtime/functions/{function_name}.schema.json",
            ));
        }
    }

    for file in schema_files(&root.join("contracts/runtime/functions"))? {
        let expected_name = contract_name_from_schema_path(&file)?;
        let value = read_json(file.clone())?;
        collect_contract_name_violations(root, &file, &value, &expected_name, &mut violations);
    }

    ensure_empty(
        violations,
        "runtime functions must have contracts with matching title/$id values",
    )
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

fn typescript_files(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_typescript_files(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn contract_files(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_contract_files(&root.join("contracts"), &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_contract_files(root: &Path, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_contract_files(&path, files)?;
        } else if matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("json" | "yaml" | "yml")
        ) {
            files.push(path);
        }
    }
    Ok(())
}

fn schema_files(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_schema_files(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_schema_files(root: &Path, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_schema_files(&path, files)?;
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".schema.json"))
        {
            files.push(path);
        }
    }
    Ok(())
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

fn collect_typescript_files(root: &Path, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_typescript_files(&path, files)?;
        } else if matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("ts" | "tsx")
        ) {
            files.push(path);
        }
    }
    Ok(())
}

fn event_schema_refs(root: &Path) -> anyhow::Result<BTreeSet<String>> {
    let mut references = BTreeSet::new();
    for file in rust_files(&root.join("domains"))? {
        let source = fs::read_to_string(&file)
            .with_context(|| format!("failed to read {}", file.display()))?;
        references.extend(extract_contract_refs(
            &source,
            "contracts/events/",
            ".schema.json",
        ));
    }
    Ok(references)
}

fn runtime_function_names(root: &Path) -> anyhow::Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    let domains_root = root.join("domains");
    for domain in domain_names(root)? {
        let runtime_root = domains_root.join(domain).join("src/runtime");
        for file in rust_files(&runtime_root)? {
            let source = fs::read_to_string(&file)
                .with_context(|| format!("failed to read {}", file.display()))?;
            names.extend(runtime_function_names_from_source(&source));
        }
    }
    Ok(names)
}

fn runtime_function_contract_names(root: &Path) -> anyhow::Result<BTreeSet<String>> {
    schema_files(&root.join("contracts/runtime/functions"))?
        .into_iter()
        .map(|file| contract_name_from_schema_path(&file))
        .collect()
}

fn runtime_function_names_from_source(source: &str) -> BTreeSet<String> {
    let constants = string_constants(source);
    let mut names = BTreeSet::new();
    for block in extract_struct_blocks(source, "FunctionDefinition") {
        for line in block.lines() {
            let Some(raw_name) = line.trim().strip_prefix("name:") else {
                continue;
            };
            let raw_name = normalize_runtime_function_name_expr(raw_name);
            let name = first_quoted_string(raw_name).or_else(|| constants.get(raw_name).cloned());
            if let Some(name) = name.filter(|name| is_versioned_name(name)) {
                names.insert(name);
            }
        }
    }
    names
}

fn normalize_runtime_function_name_expr(source: &str) -> &str {
    let source = source.trim().trim_end_matches(',').trim();
    source
        .strip_suffix(".to_owned()")
        .or_else(|| source.strip_suffix(".to_string()"))
        .unwrap_or(source)
        .trim()
}

fn string_constants(source: &str) -> BTreeMap<String, String> {
    let mut constants = BTreeMap::new();
    for line in source.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed
            .strip_prefix("pub const ")
            .or_else(|| trimmed.strip_prefix("const "))
        else {
            continue;
        };
        let Some((name, value)) = rest.split_once('=') else {
            continue;
        };
        if !name.contains("&str") {
            continue;
        }
        if let Some(value) = first_quoted_string(value) {
            constants.insert(
                name.split(':').next().unwrap_or_default().trim().to_owned(),
                value,
            );
        }
    }
    constants
}

fn extract_struct_blocks<'a>(source: &'a str, struct_name: &str) -> Vec<&'a str> {
    let mut blocks = Vec::new();
    let marker = format!("{struct_name} {{");
    let mut offset = 0;
    while let Some(relative_start) = source[offset..].find(&marker) {
        let start = offset + relative_start;
        let Some(relative_brace_start) = source[start..].find('{') else {
            break;
        };
        let brace_start = start + relative_brace_start;
        let mut depth = 0usize;
        let mut block_end = None;
        for (relative_index, character) in source[brace_start..].char_indices() {
            match character {
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        block_end = Some(brace_start + relative_index);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = block_end else {
            break;
        };
        blocks.push(&source[brace_start + 1..end]);
        offset = end + 1;
    }
    blocks
}

fn extract_contract_refs(source: &str, prefix: &str, suffix: &str) -> BTreeSet<String> {
    let mut refs = BTreeSet::new();
    let mut offset = 0;
    while let Some(relative_start) = source[offset..].find(prefix) {
        let start = offset + relative_start;
        let after_start = &source[start..];
        let Some(relative_end) = after_start.find(suffix) else {
            break;
        };
        let end = start + relative_end + suffix.len();
        refs.insert(source[start..end].to_owned());
        offset = end;
    }
    refs
}

fn first_quoted_string(source: &str) -> Option<String> {
    let start = source.find('"')?;
    let rest = &source[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}

fn is_versioned_name(name: &str) -> bool {
    name.rsplit('.').next().is_some_and(|version| {
        version.len() > 1
            && version.starts_with('v')
            && version[1..]
                .chars()
                .all(|character| character.is_ascii_digit())
    })
}

fn contract_name_from_schema_path(path: &Path) -> anyhow::Result<String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .with_context(|| format!("contract path has invalid file name: {}", path.display()))?;
    file_name
        .strip_suffix(".schema.json")
        .map(ToOwned::to_owned)
        .with_context(|| {
            format!(
                "contract path is not a .schema.json file: {}",
                path.display()
            )
        })
}

fn collect_contract_name_violations(
    root: &Path,
    file: &Path,
    value: &Value,
    expected_name: &str,
    violations: &mut Vec<String>,
) {
    let title = value.get("title").and_then(Value::as_str);
    if title != Some(expected_name) {
        violations.push(format!(
            "{} title should be `{expected_name}`",
            relative(root, file),
        ));
    }

    let contract_id = value.get("$id").and_then(Value::as_str);
    if !contract_id.is_some_and(|contract_id| contract_id_matches_name(contract_id, expected_name))
    {
        violations.push(format!(
            "{} $id should identify `{expected_name}`",
            relative(root, file),
        ));
    }
}

fn contract_id_matches_name(contract_id: &str, expected_name: &str) -> bool {
    contract_id == expected_name
        || contract_id == format!("{expected_name}.schema.json")
        || contract_id.ends_with(&format!("/{expected_name}"))
        || contract_id.ends_with(&format!("/{expected_name}.schema.json"))
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
