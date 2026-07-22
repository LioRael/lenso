use anyhow::{Context as _, bail};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN_MODULE_DIRS: &[&str] = &["api", "application", "domain", "infrastructure"];
const REQUIRED_OPENAPI_ARTIFACTS: &[&str] = &["contracts/openapi/app-api.v1.yaml"];
const FORBIDDEN_OPENAPI_PATHS: &[&str] = &["/admin/runtime/timeline/{correlation_id}"];

pub fn run() -> anyhow::Result<()> {
    let root = repo_root();
    let mut failures = Vec::new();

    collect_result(
        check_forbidden_module_folders(&root),
        "forbidden module folders",
        &mut failures,
    );
    collect_result(
        check_forbidden_cross_module_imports(&root),
        "forbidden cross-module imports",
        &mut failures,
    );
    collect_result(
        check_crates_no_module_deps(
            &root,
            &[
                "platform-admin",
                "platform-admin-data",
                "platform-module-remote",
            ],
        ),
        "platform admin/remote module dependency",
        &mut failures,
    );
    collect_result(
        check_required_openapi_artifacts(&root),
        "required OpenAPI artifacts",
        &mut failures,
    );
    collect_result(
        check_openapi_omits_removed_paths(&root),
        "removed OpenAPI paths",
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

pub fn check_forbidden_module_folders(root: &Path) -> anyhow::Result<()> {
    let mut violations = Vec::new();
    let modules_root = root.join("modules");
    for entry in fs::read_dir(&modules_root)
        .with_context(|| format!("failed to read {}", modules_root.display()))?
    {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let module = entry.file_name().to_string_lossy().into_owned();
        for forbidden in FORBIDDEN_MODULE_DIRS {
            for candidate in [
                entry.path().join(forbidden),
                entry.path().join("src").join(forbidden),
            ] {
                if candidate.is_dir() {
                    violations.push(format!("{module}: {}", relative(root, &candidate)));
                }
            }
        }
    }

    ensure_empty(
        violations,
        "modules must not contain api/application/domain/infrastructure folders",
    )
}

pub fn check_forbidden_cross_module_imports(root: &Path) -> anyhow::Result<()> {
    let mut violations = Vec::new();
    let module_names = module_names(root)?;
    let modules_root = root.join("modules");

    for module in &module_names {
        let src = modules_root.join(module).join("src");
        for file in rust_files(&src)? {
            let source = fs::read_to_string(&file)
                .with_context(|| format!("failed to read {}", file.display()))?;
            for other_module in module_names.iter().filter(|name| *name != module) {
                for line in source.lines() {
                    for pattern in [
                        format!("use {other_module}::"),
                        format!("{other_module}::"),
                        format!("extern crate {other_module}"),
                    ] {
                        if line.contains(&pattern)
                            && !allowed_public_module_import(line, other_module)
                        {
                            violations
                                .push(format!("{} imports `{pattern}`", relative(root, &file)));
                        }
                    }
                }
            }
        }
    }

    ensure_empty(
        violations,
        "modules must call other modules through public interfaces or events",
    )
}

fn allowed_public_module_import(line: &str, module: &str) -> bool {
    line.contains(&format!("use {module}::public")) || line.contains(&format!("{module}::public::"))
}

/// Platform admin/remote crates must not depend on concrete modules; they work
/// through composition-root injection and `platform-module` seams.
pub fn check_crates_no_module_deps(root: &Path, crates: &[&str]) -> anyhow::Result<()> {
    let module_names = module_names(root)?;
    let mut violations = Vec::new();

    for crate_name in crates {
        let manifest = root.join(format!("crates/{crate_name}/Cargo.toml"));
        let source = fs::read_to_string(&manifest)
            .with_context(|| format!("failed to read {}", manifest.display()))?;

        for module in &module_names {
            if source.contains(&format!("{module}.workspace"))
                || source.contains(&format!("\"{module}\""))
                || source.contains(&format!("{module} ="))
            {
                violations.push(format!("{crate_name} depends on module `{module}`"));
            }
        }
    }

    ensure_empty(
        violations,
        "platform admin/remote crates must not depend on any concrete module crate",
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

pub fn check_openapi_omits_removed_paths(root: &Path) -> anyhow::Result<()> {
    let openapi_path = root.join("contracts/openapi/app-api.v1.yaml");
    let openapi = read_yaml(openapi_path.clone())?;
    let paths = openapi
        .get("paths")
        .and_then(Value::as_object)
        .context("OpenAPI document should contain object paths")?;
    let mut violations = Vec::new();

    for path in FORBIDDEN_OPENAPI_PATHS {
        if paths.contains_key(*path) {
            violations.push(format!(
                "{} still declares removed path `{path}`",
                relative(root, &openapi_path),
            ));
        }
    }

    ensure_empty(
        violations,
        "removed API paths must not remain in committed OpenAPI",
    )
}

pub fn check_contract_artifacts_fresh(root: &Path) -> anyhow::Result<()> {
    let openapi = read_yaml(root.join("contracts/openapi/app-api.v1.yaml"))?;
    let generated_openapi = serde_json::to_value(lenso_api::openapi_document())
        .context("OpenAPI document should serialize")?;
    let error_schema = read_json(root.join("contracts/errors/error-response.v1.schema.json"))?;
    let autonomous_service_schema =
        read_json(root.join("contracts/services/lenso-service.v2.schema.json"))?;
    let direct_http_bindings =
        read_json(root.join("contracts/services/support-http.v1.bindings.json"))?;
    let direct_grpc_proto =
        fs::read_to_string(root.join("contracts/services/support-grpc.v1.proto"))
            .context("direct gRPC Protobuf contract should be readable")?;
    let direct_grpc_bindings =
        read_json(root.join("contracts/services/support-grpc.v1.bindings.json"))?;
    let system_v2_schema = read_json(root.join("contracts/services/lenso-system.v2.schema.json"))?;
    let system_v2_fixture =
        read_json(root.join("contracts/services/lenso-system.v2.fixture.json"))?;
    let ga_support_manifest_schema =
        read_json(root.join("contracts/ga/lenso.ga-support-manifest.v1.schema.json"))?;
    let ga_support_manifest =
        read_json(root.join("contracts/ga/lenso.ga-support-manifest.v1.json"))?;
    let manifest_migration_plan_schema =
        read_json(root.join("contracts/ga/lenso.manifest-migration-plan.v1.schema.json"))?;
    let service_upgrade_plan_schema =
        read_json(root.join("contracts/ga/lenso.service-upgrade-plan.v1.schema.json"))?;
    let contract_retirement_plan_schema =
        read_json(root.join("contracts/ga/lenso.contract-retirement-plan.v1.schema.json"))?;
    let failure_scenario_evidence_schema =
        read_json(root.join("contracts/ga/lenso.failure-scenario-evidence.v1.schema.json"))?;
    let ga_support_guidance = fs::read_to_string(root.join("docs/operations/ga-support.md"))
        .context("generated GA support guidance should be readable")?;
    let extraction_readiness_schema = read_json(
        root.join("contracts/extraction/lenso.extraction-readiness-report.v1.schema.json"),
    )?;
    let extraction_readiness_blocked =
        read_json(root.join("contracts/extraction/support-ticket.blocked.json"))?;
    let extraction_readiness_corrected =
        read_json(root.join("contracts/extraction/support-ticket.corrected.json"))?;
    let extraction_readiness_blocked_human =
        fs::read_to_string(root.join("contracts/extraction/support-ticket.blocked.txt"))
            .context("blocked support-ticket Extraction Readiness Report should be readable")?;
    let extraction_readiness_corrected_human =
        fs::read_to_string(root.join("contracts/extraction/support-ticket.corrected.txt"))
            .context("corrected support-ticket Extraction Readiness Report should be readable")?;
    let extraction_plan_schema =
        read_json(root.join("contracts/extraction/lenso.extraction-plan.v1.schema.json"))?;
    let extraction_plan = read_json(root.join("contracts/extraction/support-ticket.plan.json"))?;
    let extraction_plan_human =
        fs::read_to_string(root.join("contracts/extraction/support-ticket.plan.txt"))
            .context("support-ticket Extraction Plan should be readable")?;
    let extraction_scaffold_schema =
        read_json(root.join("contracts/extraction/lenso.extraction-scaffold.v1.schema.json"))?;
    let extraction_scaffold =
        read_json(root.join("contracts/extraction/support-ticket.scaffold.json"))?;
    let extraction_scaffold_patch =
        fs::read_to_string(root.join("contracts/extraction/support-ticket.scaffold.patch"))
            .context("support-ticket Extraction Scaffold patch should be readable")?;
    let extraction_run_schema =
        read_json(root.join("contracts/extraction/lenso.extraction-run.v1.schema.json"))?;
    let extraction_run =
        read_json(root.join("contracts/extraction/support-ticket.expansion-run.json"))?;
    let extraction_run_human =
        fs::read_to_string(root.join("contracts/extraction/support-ticket.expansion-run.txt"))
            .context("support-ticket destination-expansion Run should be readable")?;
    let common_context_schema =
        read_json(root.join("contracts/context/lenso-context.v1.schema.json"))?;
    let common_context_fixture =
        read_json(root.join("contracts/context/lenso-context.v1.fixture.json"))?;
    let event_envelope_schema =
        read_json(root.join("contracts/events/lenso/lenso.event-envelope.v1.schema.json"))?;
    let support_event_contract =
        read_json(root.join("contracts/events/support/support.ticket-opened.v1.artifact.json"))?;
    let support_event_schema =
        read_json(root.join("contracts/events/support/support.ticket-opened.v1.schema.json"))?;
    let support_event_envelope =
        read_json(root.join("contracts/events/support/support.ticket-opened.v1.envelope.json"))?;
    let compatibility_matrix =
        read_json(root.join("contracts/compatibility/contract-compatibility.v1.json"))?;
    let mut violations = Vec::new();
    if openapi != generated_openapi {
        violations.push("contracts/openapi/app-api.v1.yaml is stale".to_owned());
    }
    if error_schema != generate_contracts::generated_error_response_schema() {
        violations.push("contracts/errors/error-response.v1.schema.json is stale".to_owned());
    }
    if autonomous_service_schema != generate_contracts::generated_autonomous_service_schema() {
        violations.push("contracts/services/lenso-service.v2.schema.json is stale".to_owned());
    }
    if direct_http_bindings != generate_contracts::generated_direct_http_bindings() {
        violations.push("contracts/services/support-http.v1.bindings.json is stale".to_owned());
    }
    if direct_grpc_proto != generate_contracts::generated_direct_grpc_proto() {
        violations.push("contracts/services/support-grpc.v1.proto is stale".to_owned());
    }
    if direct_grpc_bindings != generate_contracts::generated_direct_grpc_bindings() {
        violations.push("contracts/services/support-grpc.v1.bindings.json is stale".to_owned());
    }
    if system_v2_schema != generate_contracts::generated_system_v2_schema() {
        violations.push("contracts/services/lenso-system.v2.schema.json is stale".to_owned());
    }
    if system_v2_fixture != generate_contracts::generated_system_v2_fixture() {
        violations.push("contracts/services/lenso-system.v2.fixture.json is stale".to_owned());
    }
    if ga_support_manifest_schema != generate_contracts::generated_ga_support_manifest_schema() {
        violations
            .push("contracts/ga/lenso.ga-support-manifest.v1.schema.json is stale".to_owned());
    }
    if ga_support_manifest != generate_contracts::generated_ga_support_manifest() {
        violations.push("contracts/ga/lenso.ga-support-manifest.v1.json is stale".to_owned());
    }
    if manifest_migration_plan_schema
        != generate_contracts::generated_manifest_migration_plan_schema()
    {
        violations
            .push("contracts/ga/lenso.manifest-migration-plan.v1.schema.json is stale".to_owned());
    }
    if service_upgrade_plan_schema != generate_contracts::generated_service_upgrade_plan_schema() {
        violations
            .push("contracts/ga/lenso.service-upgrade-plan.v1.schema.json is stale".to_owned());
    }
    if contract_retirement_plan_schema
        != generate_contracts::generated_contract_retirement_plan_schema()
    {
        violations
            .push("contracts/ga/lenso.contract-retirement-plan.v1.schema.json is stale".to_owned());
    }
    if failure_scenario_evidence_schema
        != generate_contracts::generated_failure_scenario_evidence_schema()
    {
        violations.push(
            "contracts/ga/lenso.failure-scenario-evidence.v1.schema.json is stale".to_owned(),
        );
    }
    if ga_support_guidance != generate_contracts::generated_ga_support_guidance() {
        violations.push("docs/operations/ga-support.md is stale".to_owned());
    }
    if extraction_readiness_schema != generate_contracts::generated_extraction_readiness_schema() {
        violations.push(
            "contracts/extraction/lenso.extraction-readiness-report.v1.schema.json is stale"
                .to_owned(),
        );
    }
    if extraction_readiness_blocked
        != generate_contracts::generated_support_ticket_extraction_readiness_blocked()
    {
        violations.push("contracts/extraction/support-ticket.blocked.json is stale".to_owned());
    }
    if extraction_readiness_corrected
        != generate_contracts::generated_support_ticket_extraction_readiness_corrected()
    {
        violations.push("contracts/extraction/support-ticket.corrected.json is stale".to_owned());
    }
    if extraction_readiness_blocked_human
        != generate_contracts::generated_support_ticket_extraction_readiness_blocked_human()
    {
        violations.push("contracts/extraction/support-ticket.blocked.txt is stale".to_owned());
    }
    if extraction_readiness_corrected_human
        != generate_contracts::generated_support_ticket_extraction_readiness_corrected_human()
    {
        violations.push("contracts/extraction/support-ticket.corrected.txt is stale".to_owned());
    }
    if extraction_plan_schema != generate_contracts::generated_extraction_plan_schema() {
        violations
            .push("contracts/extraction/lenso.extraction-plan.v1.schema.json is stale".to_owned());
    }
    if extraction_plan != generate_contracts::generated_support_ticket_extraction_plan() {
        violations.push("contracts/extraction/support-ticket.plan.json is stale".to_owned());
    }
    if extraction_plan_human != generate_contracts::generated_support_ticket_extraction_plan_human()
    {
        violations.push("contracts/extraction/support-ticket.plan.txt is stale".to_owned());
    }
    if extraction_scaffold_schema != generate_contracts::generated_extraction_scaffold_schema() {
        violations.push(
            "contracts/extraction/lenso.extraction-scaffold.v1.schema.json is stale".to_owned(),
        );
    }
    if extraction_scaffold != generate_contracts::generated_support_ticket_extraction_scaffold() {
        violations.push("contracts/extraction/support-ticket.scaffold.json is stale".to_owned());
    }
    if extraction_scaffold_patch
        != generate_contracts::generated_support_ticket_extraction_scaffold_patch()
    {
        violations.push("contracts/extraction/support-ticket.scaffold.patch is stale".to_owned());
    }
    if extraction_run_schema != generate_contracts::generated_extraction_run_schema() {
        violations
            .push("contracts/extraction/lenso.extraction-run.v1.schema.json is stale".to_owned());
    }
    if extraction_run != generate_contracts::generated_support_ticket_extraction_run() {
        violations
            .push("contracts/extraction/support-ticket.expansion-run.json is stale".to_owned());
    }
    if extraction_run_human != generate_contracts::generated_support_ticket_extraction_run_human() {
        violations
            .push("contracts/extraction/support-ticket.expansion-run.txt is stale".to_owned());
    }
    if common_context_schema != generate_contracts::generated_common_context_schema() {
        violations.push("contracts/context/lenso-context.v1.schema.json is stale".to_owned());
    }
    if common_context_fixture != generate_contracts::generated_common_context_fixture() {
        violations.push("contracts/context/lenso-context.v1.fixture.json is stale".to_owned());
    }
    if event_envelope_schema != generate_contracts::generated_event_envelope_schema() {
        violations
            .push("contracts/events/lenso/lenso.event-envelope.v1.schema.json is stale".to_owned());
    }
    if support_event_contract != generate_contracts::generated_support_event_contract() {
        violations.push(
            "contracts/events/support/support.ticket-opened.v1.artifact.json is stale".to_owned(),
        );
    }
    if support_event_schema != generate_contracts::generated_support_event_schema() {
        violations.push(
            "contracts/events/support/support.ticket-opened.v1.schema.json is stale".to_owned(),
        );
    }
    if support_event_envelope != generate_contracts::generated_support_event_envelope() {
        violations.push(
            "contracts/events/support/support.ticket-opened.v1.envelope.json is stale".to_owned(),
        );
    }
    if compatibility_matrix != generate_contracts::generated_contract_compatibility_matrix() {
        violations
            .push("contracts/compatibility/contract-compatibility.v1.json is stale".to_owned());
    }
    let glossary = fs::read_to_string(root.join("docs/architecture/common-context-contracts.md"))
        .context("common context glossary should be readable")?;
    if glossary != generate_contracts::generated_common_context_glossary() {
        violations.push("docs/architecture/common-context-contracts.md is stale".to_owned());
    }
    let compatibility =
        fs::read_to_string(root.join("docs/architecture/contract-compatibility.md"))
            .context("contract compatibility documentation should be readable")?;
    if compatibility != generate_contracts::generated_contract_compatibility() {
        violations.push("docs/architecture/contract-compatibility.md is stale".to_owned());
    }

    ensure_empty(violations, "contract artifacts must match Rust sources")
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

    ensure_empty(missing, "module event schema references must exist")
}

pub fn check_event_contract_names_match_paths(root: &Path) -> anyhow::Result<()> {
    let mut violations = Vec::new();
    for file in schema_files(&root.join("contracts/events"))? {
        let expected_name = contract_name_from_schema_path(&file)?;
        let value = read_json(file.clone())?;
        let parent_module = file
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .unwrap_or_default();

        if !expected_name.starts_with(&format!("{parent_module}.")) {
            violations.push(format!(
                "{} contract name should start with `{parent_module}.`",
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

fn module_names(root: &Path) -> anyhow::Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    let modules_root = root.join("modules");
    for entry in fs::read_dir(&modules_root)
        .with_context(|| format!("failed to read {}", modules_root.display()))?
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

fn event_schema_refs(root: &Path) -> anyhow::Result<BTreeSet<String>> {
    let mut references = BTreeSet::new();
    for file in rust_files(&root.join("modules"))? {
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
    let modules_root = root.join("modules");
    for module in module_names(root)? {
        let runtime_root = modules_root.join(module).join("src/runtime");
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
