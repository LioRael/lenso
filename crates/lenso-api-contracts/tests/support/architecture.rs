use anyhow::{Context as _, bail};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN_MODULE_DIRS: &[&str] = &["api", "application", "domain", "infrastructure"];

pub fn run() -> anyhow::Result<()> {
    let root = repo_root();
    let mut failures = Vec::new();

    collect_result(
        check_forbidden_module_folders(&root),
        "forbidden module folders",
        &mut failures,
    );
    collect_result(
        check_root_tooling_boundary(&root),
        "root tooling boundary",
        &mut failures,
    );
    collect_result(
        check_root_justfile_boundary(&root),
        "root justfile boundary",
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
                "lenso-module-management",
                "platform-module-management",
                "platform-provider",
            ],
        ),
        "shared crate concrete-module dependency",
        &mut failures,
    );
    collect_result(
        check_module_contract_reset(&root),
        "Module Ecosystem V1 contract reset",
        &mut failures,
    );
    collect_result(
        check_public_application_lifecycle(&root),
        "public application lifecycle",
        &mut failures,
    );
    collect_result(
        check_service_capability_tiers(&root),
        "Service Capability Tiers",
        &mut failures,
    );
    collect_result(
        check_retired_public_product_vocabulary(&root),
        "retired public product vocabulary",
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

pub fn check_public_application_lifecycle(root: &Path) -> anyhow::Result<()> {
    let readme_path = root.join("README.md");
    let readme = fs::read_to_string(&readme_path)
        .with_context(|| format!("failed to read {}", readme_path.display()))?;
    let required_markers = [
        "## Public lifecycle",
        "1. **Compose.**",
        "2. **Run locally.**",
        "3. **Connect.**",
        "4. **Status.**",
    ];
    let mut violations = Vec::new();
    let mut previous = None;

    for marker in required_markers {
        match readme.find(marker) {
            Some(position) if previous.is_none_or(|previous| position > previous) => {
                previous = Some(position);
            }
            Some(_) => violations.push(format!(
                "README.md public lifecycle marker `{marker}` is out of order"
            )),
            None => violations.push(format!("README.md public lifecycle is missing `{marker}`")),
        }
    }

    for marker in [
        "lenso.app.json",
        "lenso system dev",
        "does not release or deploy",
    ] {
        if !readme.contains(marker) {
            violations.push(format!("README.md public lifecycle must state `{marker}`"));
        }
    }

    let getting_started_path = root.join("docs/getting-started.md");
    let getting_started = fs::read_to_string(&getting_started_path)
        .with_context(|| format!("failed to read {}", getting_started_path.display()))?;
    let getting_started_markers = [
        "## Public lifecycle",
        "### Compose",
        "### Run locally",
        "### Connect",
        "### Status",
    ];
    let mut previous = None;
    for marker in getting_started_markers {
        match getting_started.find(marker) {
            Some(position) if previous.is_none_or(|previous| position > previous) => {
                previous = Some(position);
            }
            Some(_) => violations.push(format!(
                "docs/getting-started.md lifecycle marker `{marker}` is out of order"
            )),
            None => violations.push(format!(
                "docs/getting-started.md lifecycle is missing `{marker}`"
            )),
        }
    }

    for marker in [
        "lenso.app.json",
        "lenso system dev",
        "does not release or deploy",
    ] {
        if !getting_started.contains(marker) {
            violations.push(format!(
                "docs/getting-started.md public lifecycle must state `{marker}`"
            ));
        }
    }

    for retired_command in [
        "lenso host init",
        "lenso service install",
        "lenso service release plan",
        "lenso service release apply",
    ] {
        if getting_started.contains(retired_command) {
            violations.push(format!(
                "docs/getting-started.md must not teach `{retired_command}` as the application lifecycle"
            ));
        }
    }

    ensure_empty(
        violations,
        "public product documentation must teach Compose, Run locally, Connect, and Status",
    )
}

pub fn check_service_capability_tiers(root: &Path) -> anyhow::Result<()> {
    let tiers_path = root.join("docs/architecture/service-capability-tiers.md");
    let tiers = fs::read_to_string(&tiers_path)
        .with_context(|| format!("failed to read {}", tiers_path.display()))?;
    let typescript_readme_path = root.join("sdk/typescript/packages/service-kit/README.md");
    let typescript_readme = fs::read_to_string(&typescript_readme_path)
        .with_context(|| format!("failed to read {}", typescript_readme_path.display()))?;
    let product_readme_path = root.join("README.md");
    let product_readme = fs::read_to_string(&product_readme_path)
        .with_context(|| format!("failed to read {}", product_readme_path.display()))?;
    let mut violations = Vec::new();

    for marker in [
        "# Service Capability Tiers",
        "## Provider",
        "`lenso.service.v1`",
        "Rust and TypeScript",
        "## Autonomous Service",
        "`lenso.service.v2`",
        "Rust only",
        "direct HTTP",
        "direct gRPC",
        "Event Contracts",
        "Durable Workflows",
        "Workload Identity",
        "Delegated Actor Context",
        "Service-owned storage",
    ] {
        if !tiers.contains(marker) {
            violations.push(format!(
                "docs/architecture/service-capability-tiers.md is missing `{marker}`"
            ));
        }
    }

    for marker in [
        "Provider tier only",
        "`lenso.service.v1`",
        "`lenso.service.v2`",
        "does not provide Autonomous Service parity",
    ] {
        if !typescript_readme.contains(marker) {
            violations.push(format!(
                "sdk/typescript/packages/service-kit/README.md is missing `{marker}`"
            ));
        }
    }

    if !product_readme
        .contains("[Service Capability Tiers](docs/architecture/service-capability-tiers.md)")
    {
        violations
            .push("README.md must link the authoritative Service Capability Tiers".to_owned());
    }

    ensure_empty(
        violations,
        "public documentation must distinguish Provider v1 from Rust-only Autonomous Service v2",
    )
}

pub fn check_retired_public_product_vocabulary(root: &Path) -> anyhow::Result<()> {
    const CURATED_PUBLIC_FILES: &[&str] = &[
        "README.md",
        "docs/getting-started.md",
        "docs/agent-ready-module-demo.md",
        "docs/architecture/framework-public-surface.md",
        "docs/architecture/service-capability-tiers.md",
        "crates/lenso/README.md",
        "sdk/typescript/packages/service-kit/README.md",
        "skills/README.md",
        "skills/lenso-start/SKILL.md",
        "skills/lenso-starter-host/SKILL.md",
        "skills/lenso-app-composition/SKILL.md",
        "skills/lenso-app-composition/agents/openai.yaml",
        "skills/lenso-app-composition/references/app-verification.md",
        "skills/lenso-app-composition/references/generated-state.md",
        "skills/lenso-module-authoring/SKILL.md",
        "skills/lenso-module-authoring/references/manifest-and-surfaces.md",
    ];
    const RETIRED_PHRASES: &[&str] = &[
        "proof",
        "evidence",
        "readiness",
        "degradation",
        "change plan",
        "launchpad",
        "environment-owned",
        "admin data",
        "schema-admin",
        "admin action",
        "admin_action",
        "isolated_web",
        "generic query",
        "generic command",
        "data workspace",
    ];
    let mut violations = Vec::new();

    for relative_path in CURATED_PUBLIC_FILES {
        let path = root.join(relative_path);
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) => {
                violations.push(format!("{relative_path} must be readable: {error}"));
                continue;
            }
        };
        let lowercase = source.to_lowercase();
        for retired in RETIRED_PHRASES {
            if lowercase.contains(retired) {
                violations.push(format!(
                    "{relative_path} teaches retired public term `{retired}`"
                ));
            }
        }
    }

    if root
        .join("skills/lenso-app-composition/references/app-proof.md")
        .exists()
    {
        violations.push(
            "skills/lenso-app-composition/references/app-proof.md must stay removed".to_owned(),
        );
    }

    ensure_empty(
        violations,
        "curated product documentation must use the simplified application model",
    )
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

pub fn check_root_tooling_boundary(root: &Path) -> anyhow::Result<()> {
    let mut violations = Vec::new();

    if root.join("tools").exists() {
        violations.push("root tools/ must not exist; crates and packages own tooling".to_owned());
    }
    if root.join("scripts").exists() {
        violations.push("root scripts/ must not exist; commands belong to their owners".to_owned());
    }

    ensure_empty(violations, "root tooling must stay owner-local")
}

pub fn check_root_justfile_boundary(root: &Path) -> anyhow::Result<()> {
    let path = root.join("justfile");
    let violations = if path.exists() {
        vec!["root justfile must not exist; use owner-local commands".to_owned()]
    } else {
        Vec::new()
    };

    ensure_empty(violations, "root task runners must stay owner-local")
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

/// Shared management and platform crates must not depend on concrete modules;
/// they work through contracts, composition-root injection, and narrow seams.
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
        "shared management and platform crates must not depend on concrete module crates",
    )
}

pub fn check_module_contract_reset(root: &Path) -> anyhow::Result<()> {
    let manifest = read_json(root.join("contracts/modules/lenso.module-manifest.v1.schema.json"))?;
    let release = read_json(root.join("contracts/modules/lenso.module-release.v1.schema.json"))?;
    let service =
        read_json(root.join("crates/lenso-service/schemas/lenso-service.v1.schema.json"))?;
    let contracts_facade = fs::read_to_string(root.join("crates/lenso-contracts/src/lib.rs"))
        .context("lenso-contracts facade should be readable")?;
    let host_config = fs::read_to_string(root.join("crates/platform-core/src/config.rs"))
        .context("Host config source should be readable")?;
    let provider_runtime =
        fs::read_to_string(root.join("crates/lenso-module-management/src/provider_runtime.rs"))
            .context("Provider runtime compiler should be readable")?;
    let provider_adapter =
        fs::read_to_string(root.join("crates/platform-provider/src/provider_runtime.rs"))
            .context("Provider runtime transport adapter should be readable")?;
    let provider_config = fs::read_to_string(root.join("crates/platform-provider/src/config.rs"))
        .context("Provider runtime config should be readable")?;
    let provider_proxy = fs::read_to_string(root.join("crates/platform-provider/src/proxy.rs"))
        .context("Provider proxy config should be readable")?;
    let api_startup = fs::read_to_string(root.join("crates/lenso-api/src/lib.rs"))
        .context("API startup source should be readable")?;
    let worker_startup = fs::read_to_string(root.join("crates/lenso-worker/src/lib.rs"))
        .context("worker startup source should be readable")?;
    let bootstrap = fs::read_to_string(root.join("crates/lenso-bootstrap/src/lib.rs"))
        .context("Host bootstrap source should be readable")?;
    let provider_runtime_schema =
        read_json(root.join("contracts/management/lenso.provider-runtime-plan.v1.schema.json"))?;
    let openapi: Value = serde_yaml::from_str(
        &fs::read_to_string(root.join("contracts/openapi/app-api.v1.yaml"))
            .context("committed OpenAPI should be readable")?,
    )
    .context("committed OpenAPI should parse")?;
    let mut violations = Vec::new();

    for (name, schema) in [("Manifest", &manifest), ("Release", &release)] {
        if schema.get("additionalProperties") != Some(&Value::Bool(false)) {
            violations.push(format!("{name} schema must reject unknown root fields"));
        }
        let properties = schema
            .get("properties")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for removed in ["source", "bundled", "env", "secrets", "catalog", "install"] {
            if properties.contains_key(removed) {
                violations.push(format!("{name} schema exposes removed `{removed}` field"));
            }
        }
    }

    let delivery_kinds = release
        .pointer("/$defs/ModuleDelivery/oneOf")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|variant| variant.pointer("/properties/kind/const"))
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    if delivery_kinds != BTreeSet::from(["linked", "service"]) {
        violations.push(format!(
            "Module Release delivery kinds must be exactly linked/service, got {delivery_kinds:?}"
        ));
    }
    if contracts_facade.contains("ModuleSource")
        || root
            .join("crates/lenso-contracts/src/module_source.rs")
            .exists()
    {
        violations.push("public lenso-contracts must not expose ModuleSource".to_owned());
    }
    if host_config.contains("REMOTE_MODULES") || host_config.contains("ProviderSourceConfig") {
        violations
            .push("public Host config must not expose retired remote source aliases".to_owned());
    }
    for (surface, source) in [
        ("Host config", host_config.as_str()),
        ("Host bootstrap", bootstrap.as_str()),
    ] {
        for removed in [
            "LENSO_SERVICE_PROVIDERS",
            "module-services.json",
            "ServiceProviderSourceConfig",
            "start_installed_provider_services",
            "auth_token_env",
        ] {
            if source.contains(removed) {
                violations.push(format!(
                    "{surface} must not retain removed Provider discovery/supervisor surface `{removed}`"
                ));
            }
        }
    }
    if root
        .join("crates/lenso-api/tests/provider_smoke.rs")
        .exists()
    {
        violations
            .push("legacy environment-discovery provider_smoke test must stay removed".to_owned());
    }
    if openapi
        .pointer("/components/schemas/ModuleSource")
        .is_some()
        || openapi
            .get("paths")
            .and_then(Value::as_object)
            .is_some_and(|paths| paths.keys().any(|path| path.starts_with("/admin/")))
    {
        violations.push(
            "application OpenAPI must not retain the retired same-host admin plane".to_owned(),
        );
    }
    for removed_path in [
        "/paths/~1admin~1data~1available-modules~1{module}~1install/post",
        "/paths/~1admin~1data~1available-modules~1{module}~1install/delete",
    ] {
        if openapi.pointer(removed_path).is_some() {
            violations.push(
                "catalog install mutations must stay behind reviewed target-owned System Plane plans"
                    .to_owned(),
            );
        }
    }
    if service
        .pointer("/$defs/compatibility/properties/providerProtocolVersion")
        .is_some()
        || service
            .pointer("/$defs/compatibility/properties/provider_protocol_version")
            .is_some()
        || service
            .pointer("/$defs/compatibility/properties/serviceProtocolVersion")
            .is_none()
    {
        violations.push(
            "Service contract compatibility must expose serviceProtocolVersion without retired remote aliases"
                .to_owned(),
        );
    }
    for removed in [
        "crates/lenso-service/schemas/lenso-module.v1.schema.json",
        "crates/lenso-service/schemas/lenso-module-release.v1.schema.json",
    ] {
        if root.join(removed).exists() {
            violations.push(format!(
                "removed handwritten schema still exists: {removed}"
            ));
        }
    }

    if provider_runtime_schema.pointer("/properties/protocol/const")
        != Some(&Value::String("lenso.provider-runtime-plan.v1".to_owned()))
        || provider_runtime_schema.get("additionalProperties") != Some(&Value::Bool(false))
    {
        violations.push(
            "Provider Runtime Plan must remain a closed lenso.provider-runtime-plan.v1 contract"
                .to_owned(),
        );
    }
    for required_authority in [
        "ApplicationModuleLock",
        "ModulePlanningContext",
        "ServiceInstallationSet",
        "release_digest",
        "manifest_digest",
    ] {
        if !provider_runtime.contains(required_authority) {
            violations.push(format!(
                "Provider runtime compiler must retain `{required_authority}` authority"
            ));
        }
    }
    for forbidden_discovery in ["reqwest", "LENSO_SERVICE_PROVIDERS", "/lenso/provider/v1"] {
        if provider_runtime.contains(forbidden_discovery) {
            violations.push(format!(
                "Provider runtime compiler must not perform live or legacy discovery via `{forbidden_discovery}`"
            ));
        }
    }
    if !provider_adapter.contains("&locked.manifest")
        || !provider_adapter.contains("resolve_identity(&self.adapters, provider).await?")
        || !provider_adapter.contains("resolve_endpoints(&self.adapters, provider).await?")
        || !provider_adapter.contains("trait ProviderEndpointResolver")
        || !provider_adapter.contains("trait ProviderCredentialResolver")
        || provider_adapter.contains(".load_all()")
    {
        violations.push(
            "Provider transport must verify locked Manifests and must not load discovered Modules"
                .to_owned(),
        );
    }
    for (name, startup, required_loader) in [(
        "worker",
        worker_startup.as_str(),
        "load_modules_with_composition_and_provider_plan",
    )] {
        if !startup.contains("provider_runtime_plan_from_workspace")
            || !startup.contains(required_loader)
            || startup.contains("start_installed_provider_services")
        {
            violations.push(format!(
                "{name} startup must consume Provider Runtime Plan without the legacy Service supervisor"
            ));
        }
    }
    if !api_startup.contains("provider_runtime_plan_from_workspace")
        || !api_startup.contains("load_provider_runtime_with_composition")
        || !api_startup.contains("install_provider_http_proxy_registry")
    {
        violations.push(
            "API startup must install the locked Provider HTTP registry without same-host admin APIs"
                .to_owned(),
        );
    }
    if host_config.contains("services: service_provider_sources_from_env()?") {
        violations.push(
            "Host startup must not discover Provider Services from environment variables"
                .to_owned(),
        );
    }
    if !bootstrap.contains("ProviderRuntimeAdapters::production_defaults()")
        || !bootstrap.contains("ProviderRuntimeAdapter::with_adapters")
    {
        violations.push(
            "Host composition must inject Provider endpoint and credential adapters".to_owned(),
        );
    }
    if !provider_config.contains("pub(crate) auth_token: Option<String>")
        || !provider_config.contains(".field(\"auth_configured\"")
        || provider_config.contains(".field(\"auth_token\"")
        || provider_proxy.contains(".field(\"auth_token\"")
    {
        violations.push(
            "Provider transport credentials must stay private and redacted from Debug output"
                .to_owned(),
        );
    }

    ensure_empty(
        violations,
        "public Module contracts must use strict Manifest/Release V1 shapes",
    )
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
        .expect("architecture checks should be inside the lenso-api-contracts crate")
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
    for source_root in [root.join("modules"), root.join("crates/platform-runtime")] {
        for file in rust_files(&source_root)? {
            let source = fs::read_to_string(&file)
                .with_context(|| format!("failed to read {}", file.display()))?;
            references.extend(extract_contract_refs(
                &source,
                "contracts/events/",
                ".schema.json",
            ));
        }
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
