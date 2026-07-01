# Lenso App Proof V24 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build V24 App Proof so generated blueprint apps can be verified, diffed, safely repaired, shown in Console, and handed to coding agents.

**Architecture:** Keep App Proof as generated control-plane evidence, not a new runtime contract. Reuse V23 Launchpad state, dev doctor state, `lenso.system.json`, and `lenso.workspace.json`; Host and Console only read `.lenso/app-proof.json`. Safe repair writes generated JSON and missing scaffold directories only, never existing service source files.

**Tech Stack:** Rust CLI, Rust admin-data endpoint, committed OpenAPI contract, React Runtime Console, JSON fixtures, MDX docs.

---

## File Structure

- `lenso-cli/src/main.rs`: add parser structs and dispatch for `lenso app verify`, `lenso app diff`, and `lenso app repair`.
- `lenso-cli/src/launchpad.rs`: add App Proof data types, proof checks, drift detection, safe repair, proof writing, and agent context rendering.
- `lenso/crates/platform-admin-data/src/dto.rs`: add App Proof response DTOs.
- `lenso/crates/platform-admin-data/src/handlers.rs`: add read-only `.lenso/app-proof.json` response helper and route handler.
- `lenso/crates/platform-admin-data/src/lib.rs`: register the proof route.
- `lenso/contracts/openapi/app-api.v1.yaml`: regenerate with `just generate-contracts`.
- `lenso-runtime-console/src/pages/available-modules-model.ts`: add Launchpad proof response types.
- `lenso-runtime-console/src/data/available-modules.ts`: add sample proof data and `fetchLaunchpadProof`.
- `lenso-runtime-console/src/pages/launchpad-model.ts`: summarize proof status, drifts, and next command.
- `lenso-runtime-console/src/pages/launchpad-page.tsx`: render App Proof on `/launchpad`.
- `lenso-examples/fixtures/launchpad/support-desk-proof/`: add proof fixture files.
- `lenso-examples/scripts/check-launchpad-fixtures.mjs`: verify proof fixture shape.
- `lenso-site/content/docs/(host)/product-blueprints.mdx`: document verify/diff/repair.
- `lenso-site/content/docs/(host)/cli-reference.mdx`: add command reference.
- `lenso-site/content/docs/(host)/runtime-console.mdx`: mention Launchpad App Proof.
- `lenso-site/content/docs/(host)/troubleshooting.mdx`: add proof/drift repair guidance.

## Task 1: CLI Command Shell

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/launchpad.rs`

- [ ] **Step 1: Add failing parser tests**

Add tests beside existing parser tests in `src/main.rs`:

```rust
#[test]
fn parses_app_verify() {
    let cli = Cli::parse_from(["lenso", "app", "verify", "--write-proof"]);
    let Command::App {
        command: AppCommand::Verify(args),
    } = cli.command
    else {
        panic!("expected app verify");
    };
    assert!(args.write_proof);
}

#[test]
fn parses_app_diff() {
    let cli = Cli::parse_from(["lenso", "app", "diff"]);
    let Command::App {
        command: AppCommand::Diff(args),
    } = cli.command
    else {
        panic!("expected app diff");
    };
    assert!(args.repo_root.is_none());
}

#[test]
fn parses_app_repair_dry_run() {
    let cli = Cli::parse_from(["lenso", "app", "repair", "--dry-run"]);
    let Command::App {
        command: AppCommand::Repair(args),
    } = cli.command
    else {
        panic!("expected app repair");
    };
    assert!(args.dry_run);
}
```

Run:

```sh
cargo test --locked parses_app_verify parses_app_diff parses_app_repair_dry_run
```

Expected: compile failure because the command variants do not exist.

- [ ] **Step 2: Add command args and dispatch**

Extend `AppCommand` in `src/main.rs`:

```rust
#[derive(Debug, Subcommand)]
enum AppCommand {
    Create(AppCreateArgs),
    List,
    Inspect(AppInspectArgs),
    Add(AppAddArgs),
    /// Verify a generated Launchpad app and optionally write App Proof.
    Verify(AppVerifyArgs),
    /// Compare the generated app state with its blueprint and addons.
    Diff(AppDiffArgs),
    /// Repair safe generated app state drift.
    Repair(AppRepairArgs),
}

#[derive(Debug, Args, Clone)]
struct AppVerifyArgs {
    /// Lenso host repository root.
    #[arg(long)]
    repo_root: Option<std::path::PathBuf>,

    /// Write .lenso/app-proof.json.
    #[arg(long)]
    write_proof: bool,
}

#[derive(Debug, Args, Clone)]
struct AppDiffArgs {
    /// Lenso host repository root.
    #[arg(long)]
    repo_root: Option<std::path::PathBuf>,
}

#[derive(Debug, Args, Clone)]
struct AppRepairArgs {
    /// Lenso host repository root.
    #[arg(long)]
    repo_root: Option<std::path::PathBuf>,

    /// Print planned safe repairs without writing files.
    #[arg(long)]
    dry_run: bool,
}
```

Add dispatch near existing `AppCommand` handling:

```rust
AppCommand::Verify(args) => launchpad::app_verify(launchpad::AppVerifyOptions {
    repo_root: args.repo_root,
    write_proof: args.write_proof,
})?,
AppCommand::Diff(args) => launchpad::app_diff(launchpad::AppDiffOptions {
    repo_root: args.repo_root,
})?,
AppCommand::Repair(args) => launchpad::app_repair(launchpad::AppRepairOptions {
    dry_run: args.dry_run,
    repo_root: args.repo_root,
})?,
```

- [ ] **Step 3: Add public option structs and stub functions**

In `src/launchpad.rs`, add:

```rust
const APP_PROOF_PROTOCOL: &str = "lenso.app-proof.v1";
const APP_PROOF_FILE: &str = ".lenso/app-proof.json";

#[derive(Debug, Clone)]
pub(crate) struct AppVerifyOptions {
    pub(crate) repo_root: Option<PathBuf>,
    pub(crate) write_proof: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct AppDiffOptions {
    pub(crate) repo_root: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) struct AppRepairOptions {
    pub(crate) dry_run: bool,
    pub(crate) repo_root: Option<PathBuf>,
}

pub(crate) fn app_verify(_options: AppVerifyOptions) -> Result<()> {
    bail!("app proof is not implemented")
}

pub(crate) fn app_diff(_options: AppDiffOptions) -> Result<()> {
    bail!("app proof is not implemented")
}

pub(crate) fn app_repair(_options: AppRepairOptions) -> Result<()> {
    bail!("app proof is not implemented")
}
```

- [ ] **Step 4: Verify parser tests pass**

Run:

```sh
cargo test --locked parses_app_verify parses_app_diff parses_app_repair_dry_run
```

Expected: parser tests pass.

- [ ] **Step 5: Commit CLI command shell**

```sh
git add src/main.rs src/launchpad.rs
git commit -m "feat: add app proof command shell"
```

## Task 2: App Proof Engine

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/launchpad.rs`

- [ ] **Step 1: Add proof model types**

Add serializable types near `DevDoctorState`:

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppProofState {
    protocol: String,
    status: String,
    checked_at_unix_ms: u64,
    project_name: Option<String>,
    blueprint: Option<String>,
    addons: Vec<String>,
    checks: Vec<AppProofCheck>,
    drifts: Vec<AppProofDrift>,
    next_command: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppProofCheck {
    id: String,
    label: String,
    status: String,
    message: String,
    command: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppProofDrift {
    resource: String,
    name: String,
    message: String,
    command: Option<String>,
}
```

- [ ] **Step 2: Add unit tests for proof status folding**

Add tests in `launchpad.rs`:

```rust
#[test]
fn app_proof_status_ready_when_checks_pass() {
    let checks = vec![AppProofCheck {
        command: None,
        id: "workspace".to_owned(),
        label: "Workspace".to_owned(),
        message: "ok".to_owned(),
        status: "passed".to_owned(),
    }];
    assert_eq!(app_proof_status(&checks, &[]), "ready");
}

#[test]
fn app_proof_status_drifted_when_drift_exists() {
    let checks = vec![AppProofCheck {
        command: Some("lenso app repair".to_owned()),
        id: "workspace-service-support-sla".to_owned(),
        label: "support-sla workspace entry".to_owned(),
        message: "missing".to_owned(),
        status: "drifted".to_owned(),
    }];
    let drifts = vec![AppProofDrift {
        command: Some("lenso app repair".to_owned()),
        message: "support-sla is missing from lenso.workspace.json".to_owned(),
        name: "support-sla".to_owned(),
        resource: "workspace-service".to_owned(),
    }];
    assert_eq!(app_proof_status(&checks, &drifts), "drifted");
}
```

Expected before implementation:

```sh
cargo test --locked app_proof_status
```

fails because `app_proof_status` does not exist.

- [ ] **Step 3: Implement status and next command helpers**

Add:

```rust
fn app_proof_status(checks: &[AppProofCheck], drifts: &[AppProofDrift]) -> &'static str {
    if checks.iter().any(|check| check.status == "failed") {
        "failed"
    } else if checks
        .iter()
        .any(|check| check.status == "needs_attention")
    {
        "needs_attention"
    } else if !drifts.is_empty() || checks.iter().any(|check| check.status == "drifted") {
        "drifted"
    } else if checks.is_empty() {
        "empty"
    } else {
        "ready"
    }
}

fn app_proof_next_command(
    checks: &[AppProofCheck],
    drifts: &[AppProofDrift],
) -> Option<String> {
    drifts
        .iter()
        .find_map(|drift| drift.command.clone())
        .or_else(|| checks.iter().find_map(|check| check.command.clone()))
}
```

- [ ] **Step 4: Add proof generation test**

Add a test that uses existing V23 helpers:

```rust
#[test]
fn app_proof_state_includes_blueprint_addon_and_doctor() {
    let mut launchpad = support_desk_launchpad_state("acme-support");
    launchpad.addons.push(LaunchpadAddon {
        label: "Support SLA".to_owned(),
        modules: vec!["support-sla".to_owned()],
        name: "support-sla".to_owned(),
        services: vec!["support-sla".to_owned()],
        status: "configured".to_owned(),
    });
    let doctor = DevDoctorState {
        checked_at_unix_ms: 1782900000000,
        checks: vec![DevDoctorCheck {
            command: None,
            id: "env".to_owned(),
            label: ".env file".to_owned(),
            message: ".env exists".to_owned(),
            status: "passed".to_owned(),
        }],
        live: false,
        protocol: DEV_DOCTOR_PROTOCOL.to_owned(),
        status: "ready".to_owned(),
    };

    let proof = app_proof_state_from_parts(&launchpad, Some(&doctor), Vec::new(), Vec::new());

    assert_eq!(proof.protocol, APP_PROOF_PROTOCOL);
    assert_eq!(proof.project_name.as_deref(), Some("acme-support"));
    assert_eq!(proof.blueprint.as_deref(), Some("support-desk"));
    assert_eq!(proof.addons, vec!["support-sla"]);
    assert_eq!(proof.status, "ready");
}
```

- [ ] **Step 5: Implement proof generation**

Add:

```rust
fn app_proof_state_from_parts(
    launchpad: &LaunchpadState,
    doctor: Option<&DevDoctorState>,
    mut checks: Vec<AppProofCheck>,
    drifts: Vec<AppProofDrift>,
) -> AppProofState {
    checks.push(AppProofCheck {
        command: doctor
            .filter(|state| state.status != "ready")
            .map(|_| "lenso dev doctor --write-state".to_owned()),
        id: "launchpad-doctor-state".to_owned(),
        label: "Launchpad doctor state".to_owned(),
        message: match doctor {
            Some(state) => format!(".lenso/dev-doctor.json status is {}", state.status),
            None => ".lenso/dev-doctor.json is missing".to_owned(),
        },
        status: match doctor {
            Some(state) if state.status == "ready" => "passed".to_owned(),
            Some(state) => state.status.clone(),
            None => "needs_attention".to_owned(),
        },
    });

    let status = app_proof_status(&checks, &drifts).to_owned();
    let next_command = app_proof_next_command(&checks, &drifts);

    AppProofState {
        addons: launchpad
            .addons
            .iter()
            .map(|addon| addon.name.clone())
            .collect(),
        blueprint: Some(launchpad.blueprint.clone()),
        checked_at_unix_ms: current_unix_ms(),
        checks,
        drifts,
        next_command,
        project_name: Some(launchpad.project_name.clone()),
        protocol: APP_PROOF_PROTOCOL.to_owned(),
        status,
    }
}
```

- [ ] **Step 6: Add file readers**

Reuse existing `read_launchpad_state_required` and add:

```rust
fn read_dev_doctor_state_optional(repo_root: &Path) -> Result<Option<DevDoctorState>> {
    let path = repo_root.join(DEV_DOCTOR_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let source = fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&source)
        .with_context(|| format!("failed to parse {}", path.display()))
        .map(Some)
}
```

- [ ] **Step 7: Verify proof engine tests pass**

Run:

```sh
cargo test --locked app_proof_status app_proof_state_includes_blueprint_addon_and_doctor
```

Expected: all proof model tests pass.

- [ ] **Step 8: Commit proof engine**

```sh
git add src/launchpad.rs
git commit -m "feat: add app proof state model"
```

## Task 3: App Diff And Verify

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/launchpad.rs`

- [ ] **Step 1: Add drift detection test**

Add:

```rust
#[test]
fn app_diff_detects_missing_workspace_service() {
    let launchpad = support_desk_launchpad_state("acme-support");
    let workspace = json!({
        "protocol": "lenso.service-workspace.v1",
        "services": []
    });

    let (checks, drifts) = app_diff_from_values(&launchpad, Some(&workspace), None);

    assert!(drifts.iter().any(|drift| {
        drift.resource == "workspace-service" && drift.name == "support-api"
    }));
    assert!(checks.iter().any(|check| {
        check.id == "workspace-service-support-api" && check.status == "drifted"
    }));
}
```

- [ ] **Step 2: Implement minimal drift checks**

Add:

```rust
fn app_diff_from_values(
    launchpad: &LaunchpadState,
    workspace: Option<&Value>,
    system: Option<&Value>,
) -> (Vec<AppProofCheck>, Vec<AppProofDrift>) {
    let mut checks = Vec::new();
    let mut drifts = Vec::new();
    let workspace_services = workspace_service_names(workspace);
    let system_services = system_service_names(system);

    for service in &launchpad.services {
        push_service_check(
            &mut checks,
            &mut drifts,
            "workspace-service",
            &format!("workspace-service-{}", service.name),
            &service.name,
            workspace_services.contains(&service.name),
            "lenso app repair",
            "lenso.workspace.json",
        );
        push_service_check(
            &mut checks,
            &mut drifts,
            "system-service",
            &format!("system-service-{}", service.name),
            &service.name,
            system_services.contains(&service.name),
            "lenso app repair",
            "lenso.system.json",
        );
    }

    (checks, drifts)
}

fn workspace_service_names(workspace: Option<&Value>) -> Vec<String> {
    workspace
        .and_then(|value| value.get("services"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|service| service.get("name").and_then(Value::as_str))
        .map(str::to_owned)
        .collect()
}

fn system_service_names(system: Option<&Value>) -> Vec<String> {
    system
        .and_then(|value| value.get("services"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|service| service.get("name").and_then(Value::as_str))
        .map(str::to_owned)
        .collect()
}

fn push_service_check(
    checks: &mut Vec<AppProofCheck>,
    drifts: &mut Vec<AppProofDrift>,
    resource: &str,
    id: &str,
    service: &str,
    present: bool,
    command: &str,
    file: &str,
) {
    if present {
        checks.push(AppProofCheck {
            command: None,
            id: id.to_owned(),
            label: format!("{service} in {file}"),
            message: format!("{service} is present in {file}"),
            status: "passed".to_owned(),
        });
    } else {
        checks.push(AppProofCheck {
            command: Some(command.to_owned()),
            id: id.to_owned(),
            label: format!("{service} in {file}"),
            message: format!("{service} is missing from {file}"),
            status: "drifted".to_owned(),
        });
        drifts.push(AppProofDrift {
            command: Some(command.to_owned()),
            message: format!("{service} is missing from {file}"),
            name: service.to_owned(),
            resource: resource.to_owned(),
        });
    }
}
```

- [ ] **Step 3: Implement file-level app proof builder**

Add:

```rust
fn app_proof_state(repo_root: &Path) -> Result<AppProofState> {
    let launchpad = read_launchpad_state_required(repo_root)?;
    let doctor = read_dev_doctor_state_optional(repo_root)?;
    let workspace = read_json_value_optional(&repo_root.join(WORKSPACE_FILE))?;
    let system = read_json_value_optional(&repo_root.join(SYSTEM_FILE))?;
    let (checks, drifts) = app_diff_from_values(&launchpad, workspace.as_ref(), system.as_ref());
    Ok(app_proof_state_from_parts(
        &launchpad,
        doctor.as_ref(),
        checks,
        drifts,
    ))
}

fn read_json_value_optional(path: &Path) -> Result<Option<Value>> {
    if !path.exists() {
        return Ok(None);
    }
    let source = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&source)
        .with_context(|| format!("failed to parse {}", path.display()))
        .map(Some)
}
```

- [ ] **Step 4: Implement `app_verify` and `app_diff` output**

Replace stubs:

```rust
pub(crate) fn app_verify(options: AppVerifyOptions) -> Result<()> {
    let repo_root = repo_root(options.repo_root)?;
    let proof = app_proof_state(&repo_root)?;
    print_app_proof(&proof);
    if options.write_proof {
        write_json(&repo_root.join(APP_PROOF_FILE), &proof)?;
        println!("wrote {}", repo_root.join(APP_PROOF_FILE).display());
    }
    if matches!(proof.status.as_str(), "failed" | "needs_attention") {
        bail!("app proof status is {}", proof.status);
    }
    Ok(())
}

pub(crate) fn app_diff(options: AppDiffOptions) -> Result<()> {
    let repo_root = repo_root(options.repo_root)?;
    let proof = app_proof_state(&repo_root)?;
    if proof.drifts.is_empty() {
        println!("No app drift found.");
        return Ok(());
    }
    for drift in &proof.drifts {
        println!(
            "- {} {}: {}",
            drift.resource,
            drift.name,
            drift.message
        );
        if let Some(command) = &drift.command {
            println!("  command: {command}");
        }
    }
    bail!("app drift found");
}
```

Add:

```rust
fn print_app_proof(proof: &AppProofState) {
    println!("App proof: {}", proof.status);
    if let Some(project_name) = &proof.project_name {
        println!("project: {project_name}");
    }
    if let Some(blueprint) = &proof.blueprint {
        println!("blueprint: {blueprint}");
    }
    println!("addons: {}", proof.addons.join(", "));
    println!("checks: {}", proof.checks.len());
    println!("drifts: {}", proof.drifts.len());
    if let Some(command) = &proof.next_command {
        println!("next: {command}");
    }
}
```

- [ ] **Step 5: Run CLI checks**

Run:

```sh
cargo test --locked app_diff_detects_missing_workspace_service
cargo test --locked app_proof
```

Expected: tests pass.

- [ ] **Step 6: Commit verify/diff**

```sh
git add src/launchpad.rs
git commit -m "feat: verify app proof drift"
```

## Task 4: Safe Repair And Agent Context

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/launchpad.rs`

- [ ] **Step 1: Add repair dry-run test**

Add:

```rust
#[test]
fn app_repair_plan_mentions_missing_workspace_service() {
    let drifts = vec![AppProofDrift {
        command: Some("lenso app repair".to_owned()),
        message: "support-sla is missing from lenso.workspace.json".to_owned(),
        name: "support-sla".to_owned(),
        resource: "workspace-service".to_owned(),
    }];

    let repairs = app_repair_plan(&drifts);

    assert_eq!(repairs, vec!["restore workspace service support-sla"]);
}
```

- [ ] **Step 2: Implement repair plan helper**

Add:

```rust
fn app_repair_plan(drifts: &[AppProofDrift]) -> Vec<String> {
    drifts
        .iter()
        .filter_map(|drift| match drift.resource.as_str() {
            "workspace-service" => Some(format!("restore workspace service {}", drift.name)),
            "system-service" => Some(format!("restore system service {}", drift.name)),
            _ => None,
        })
        .collect()
}
```

- [ ] **Step 3: Implement minimal safe repair command**

Replace `app_repair` stub:

```rust
pub(crate) fn app_repair(options: AppRepairOptions) -> Result<()> {
    let repo_root = repo_root(options.repo_root)?;
    let proof = app_proof_state(&repo_root)?;
    let repairs = app_repair_plan(&proof.drifts);
    if repairs.is_empty() {
        println!("No safe app repairs needed.");
        return Ok(());
    }
    for repair in &repairs {
        println!("- {repair}");
    }
    if options.dry_run {
        println!("dry run: no files changed");
        return Ok(());
    }
    repair_generated_state(&repo_root)?;
    println!("repaired generated app state");
    println!("next: lenso app verify --write-proof");
    Ok(())
}
```

Add one safe implementation that reuses V23 state writers:

```rust
fn repair_generated_state(repo_root: &Path) -> Result<()> {
    let launchpad = read_launchpad_state_required(repo_root)?;
    let blueprint = blueprint_by_name(&launchpad.blueprint)?;
    let addon_recipes = launchpad
        .addons
        .iter()
        .map(|addon| addon_by_name(&addon.name))
        .collect::<Result<Vec<_>>>()?;
    with_current_dir(repo_root, || {
        repair_launchpad_state(&launchpad, &blueprint, &addon_recipes)?;
        repair_workspace_recipes(&blueprint, &addon_recipes)?;
        repair_system_recipes(&launchpad.project_name, &blueprint, &addon_recipes)?;
        repair_missing_service_scaffolds(&blueprint.services)?;
        for addon in &addon_recipes {
            repair_missing_service_scaffolds(&addon.services)?;
        }
        Ok(())
    })
}

fn repair_launchpad_state(
    launchpad: &LaunchpadState,
    blueprint: &Blueprint,
    addons: &[Addon],
) -> Result<()> {
    let mut repaired = launchpad_state_from_blueprint(&launchpad.project_name, blueprint);
    for addon in &launchpad.addons {
        let addon_recipe = addons
            .iter()
            .find(|recipe| recipe.name == addon.name)
            .with_context(|| format!("unknown addon {}", addon.name))?;
        for service in &addon_recipe.services {
            if !repaired.services.iter().any(|item| item.name == service.name) {
                repaired.services.push(launchpad_service_from_blueprint(service));
            }
        }
        for module in &addon_recipe.modules {
            if !repaired.modules.iter().any(|item| item.name == module.name) {
                repaired.modules.push(launchpad_module_from_blueprint(module));
            }
        }
        if !repaired.addons.iter().any(|item| item.name == addon.name) {
            repaired.addons.push(addon.clone());
        }
    }
    write_json(Path::new(LAUNCHPAD_FILE), &repaired)
}

fn repair_workspace_recipes(blueprint: &Blueprint, addons: &[Addon]) -> Result<()> {
    for service in &blueprint.services {
        upsert_workspace_service(service)?;
    }
    for addon in addons {
        for service in &addon.services {
            upsert_workspace_service(service)?;
        }
    }
    Ok(())
}

fn repair_system_recipes(
    project_name: &str,
    blueprint: &Blueprint,
    addons: &[Addon],
) -> Result<()> {
    let path = Path::new(SYSTEM_FILE);
    let mut system = if path.exists() {
        read_json_value_required(path)?
    } else {
        system_from_blueprint(project_name, blueprint)
    };
    for service in &blueprint.services {
        upsert_json_object_by_name(&mut system, "services", system_service_from_blueprint(service))?;
    }
    for module in &blueprint.modules {
        upsert_json_object_by_name(&mut system, "modules", system_module_from_blueprint(module))?;
    }
    for dependency in &blueprint.dependencies {
        upsert_json_dependency(&mut system, system_dependency_from_blueprint(dependency))?;
    }
    for addon in addons {
        for service in &addon.services {
            upsert_json_object_by_name(&mut system, "services", system_service_from_blueprint(service))?;
        }
        for module in &addon.modules {
            upsert_json_object_by_name(&mut system, "modules", system_module_from_blueprint(module))?;
        }
        for dependency in &addon.dependencies {
            upsert_json_dependency(&mut system, system_dependency_from_blueprint(dependency))?;
        }
    }
    write_json(path, &system)
}

fn repair_missing_service_scaffolds(services: &[BlueprintService]) -> Result<()> {
    for service in services {
        if !Path::new(&service_cwd(service)).exists() {
            create_service_scaffold(service)?;
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Add source overwrite regression test**

Add:

```rust
#[test]
fn app_repair_plan_does_not_include_source_overwrite() {
    let drifts = vec![AppProofDrift {
        command: Some("manual review".to_owned()),
        message: "service directory exists with user code".to_owned(),
        name: "support-api".to_owned(),
        resource: "service-source".to_owned(),
    }];

    assert!(app_repair_plan(&drifts).is_empty());
}
```

- [ ] **Step 5: Include App Proof in agent context**

Extend `agent_context_markdown` signature to accept `proof: Option<&AppProofState>`:

```rust
fn agent_context_markdown(
    state: Option<&LaunchpadState>,
    system: Option<&Value>,
    workspace: Option<&Value>,
    doctor: Option<&DevDoctorState>,
    proof: Option<&AppProofState>,
    task: Option<&str>,
) -> String
```

Add this section after Dev Doctor:

```rust
if let Some(proof) = proof {
    output.push_str("\n## App Proof\n\n");
    output.push_str(&format!("- Status: {}\n", proof.status));
    output.push_str(&format!("- Drifts: {}\n", proof.drifts.len()));
    if let Some(command) = &proof.next_command {
        output.push_str(&format!("- Next command: {command}\n"));
    }
    output.push_str("- Generated control-plane files may be repaired.\n");
    output.push_str("- Existing service source files are user code.\n");
}
```

Read the optional proof in `agent_context`:

```rust
let proof = read_app_proof_state_optional(&repo_root)?;
```

Add:

```rust
fn read_app_proof_state_optional(repo_root: &Path) -> Result<Option<AppProofState>> {
    let path = repo_root.join(APP_PROOF_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let source = fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&source)
        .with_context(|| format!("failed to parse {}", path.display()))
        .map(Some)
}
```

- [ ] **Step 6: Add agent context test**

Add:

```rust
#[test]
fn agent_context_mentions_app_proof_when_present() {
    let proof = AppProofState {
        addons: vec!["support-sla".to_owned()],
        blueprint: Some("support-desk".to_owned()),
        checked_at_unix_ms: 1782900000000,
        checks: Vec::new(),
        drifts: Vec::new(),
        next_command: Some("lenso app verify --write-proof".to_owned()),
        project_name: Some("acme-support".to_owned()),
        protocol: APP_PROOF_PROTOCOL.to_owned(),
        status: "ready".to_owned(),
    };
    let markdown = agent_context_markdown(None, None, None, None, Some(&proof), None);
    assert!(markdown.contains("## App Proof"));
    assert!(markdown.contains("Status: ready"));
    assert!(markdown.contains("Existing service source files are user code."));
}
```

- [ ] **Step 7: Run CLI full test/build**

Run:

```sh
cargo fmt
cargo test --locked
cargo build --locked
```

Expected: all pass.

- [ ] **Step 8: Real CLI proof path**

Run:

```sh
tmp=$(mktemp -d)
target/debug/lenso app create "$tmp/acme-support" --blueprint support-desk
cd "$tmp/acme-support"
target/debug/lenso app add support-sla
target/debug/lenso dev doctor --write-state
target/debug/lenso app verify --write-proof
test -f .lenso/app-proof.json
target/debug/lenso agent task "add enterprise SLA escalation" | rg "## App Proof"
```

Expected: proof file exists and agent task includes App Proof.

- [ ] **Step 9: Commit repair and agent context**

```sh
git add src/launchpad.rs
git commit -m "feat: repair app proof drift"
```

## Task 5: Host App Proof Endpoint

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/dto.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/handlers.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/lib.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso/contracts/openapi/app-api.v1.yaml`

- [ ] **Step 1: Add DTOs**

In `dto.rs`, add beside Launchpad doctor DTOs:

```rust
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminLaunchpadProofResponse {
    pub version: u8,
    pub status: AdminLaunchpadProofStatus,
    pub proof_file: String,
    pub checked_at_unix_ms: Option<u64>,
    pub project_name: Option<String>,
    pub blueprint: Option<String>,
    pub addons: Vec<String>,
    pub checks: Vec<AdminLaunchpadProofCheckDto>,
    pub drifts: Vec<AdminLaunchpadProofDriftDto>,
    pub next_command: Option<String>,
}

#[derive(Debug, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdminLaunchpadProofStatus {
    Ready,
    Drifted,
    NeedsAttention,
    Failed,
    Empty,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminLaunchpadProofCheckDto {
    pub id: String,
    pub label: String,
    pub status: String,
    pub message: String,
    pub command: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminLaunchpadProofDriftDto {
    pub resource: String,
    pub name: String,
    pub message: String,
    pub command: Option<String>,
}
```

- [ ] **Step 2: Add handler helper and tests**

In `handlers.rs`, add:

```rust
const APP_PROOF_PATH: &str = ".lenso/app-proof.json";
```

Add helper mirroring `launchpad_doctor_response`:

```rust
fn launchpad_proof_response(path: &FsPath) -> AdminLaunchpadProofResponse {
    let proof_file = path.to_string_lossy().to_string();
    let Ok(source) = fs::read_to_string(path) else {
        return AdminLaunchpadProofResponse {
            addons: Vec::new(),
            blueprint: None,
            checked_at_unix_ms: None,
            checks: Vec::new(),
            drifts: Vec::new(),
            next_command: Some("lenso app verify --write-proof".to_owned()),
            project_name: None,
            proof_file,
            status: AdminLaunchpadProofStatus::Empty,
            version: 1,
        };
    };
    let Ok(file) = serde_json::from_str::<Value>(&source) else {
        return AdminLaunchpadProofResponse {
            addons: Vec::new(),
            blueprint: None,
            checked_at_unix_ms: None,
            checks: Vec::new(),
            drifts: Vec::new(),
            next_command: Some("fix .lenso/app-proof.json".to_owned()),
            project_name: None,
            proof_file,
            status: AdminLaunchpadProofStatus::Failed,
            version: 1,
        };
    };
    let checks = file
        .get("checks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(launchpad_proof_check_from_value)
        .collect::<Vec<_>>();
    let drifts = file
        .get("drifts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(launchpad_proof_drift_from_value)
        .collect::<Vec<_>>();
    AdminLaunchpadProofResponse {
        addons: json_string_list(&file, "addons"),
        blueprint: file.get("blueprint").and_then(Value::as_str).map(str::to_owned),
        checked_at_unix_ms: file.get("checkedAtUnixMs").and_then(Value::as_u64),
        checks,
        drifts,
        next_command: file.get("nextCommand").and_then(Value::as_str).map(str::to_owned),
        project_name: file.get("projectName").and_then(Value::as_str).map(str::to_owned),
        proof_file,
        status: launchpad_proof_status_from_value(file.get("status").and_then(Value::as_str)),
        version: 1,
    }
}
```

Add parser helpers:

```rust
fn launchpad_proof_check_from_value(value: &Value) -> Option<AdminLaunchpadProofCheckDto> {
    Some(AdminLaunchpadProofCheckDto {
        command: value
            .get("command")
            .and_then(Value::as_str)
            .map(str::to_owned),
        id: value.get("id")?.as_str()?.to_owned(),
        label: value.get("label")?.as_str()?.to_owned(),
        message: value
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        status: value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned(),
    })
}

fn launchpad_proof_drift_from_value(value: &Value) -> Option<AdminLaunchpadProofDriftDto> {
    Some(AdminLaunchpadProofDriftDto {
        command: value
            .get("command")
            .and_then(Value::as_str)
            .map(str::to_owned),
        message: value
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        name: value.get("name")?.as_str()?.to_owned(),
        resource: value.get("resource")?.as_str()?.to_owned(),
    })
}

fn launchpad_proof_status_from_value(status: Option<&str>) -> AdminLaunchpadProofStatus {
    match status {
        Some("ready") => AdminLaunchpadProofStatus::Ready,
        Some("drifted") => AdminLaunchpadProofStatus::Drifted,
        Some("needs_attention") => AdminLaunchpadProofStatus::NeedsAttention,
        Some("failed") => AdminLaunchpadProofStatus::Failed,
        _ => AdminLaunchpadProofStatus::Empty,
    }
}
```

Add tests:

```rust
#[test]
fn launchpad_proof_response_reads_state() {
    let root = std::env::temp_dir().join(format!("lenso-launchpad-proof-{}", current_unix_ms()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join(".lenso")).unwrap();
    let path = root.join(".lenso/app-proof.json");
    fs::write(
        &path,
        serde_json::json!({
            "protocol": "lenso.app-proof.v1",
            "status": "ready",
            "checkedAtUnixMs": 1782900000000_u64,
            "projectName": "acme-support",
            "blueprint": "support-desk",
            "addons": ["support-sla"],
            "checks": [],
            "drifts": [],
            "nextCommand": null
        })
        .to_string(),
    )
    .unwrap();

    let response = launchpad_proof_response(&path);

    assert_eq!(response.status, AdminLaunchpadProofStatus::Ready);
    assert_eq!(response.addons, vec!["support-sla"]);
    assert_eq!(response.project_name.as_deref(), Some("acme-support"));
    let _ = fs::remove_dir_all(root);
}
```

- [ ] **Step 3: Add route**

Add handler:

```rust
#[utoipa::path(
    get,
    path = "/admin/data/launchpad/proof",
    operation_id = "admin_data_launchpad_proof",
    tag = "admin-data",
    params(("authorization" = String, Header, description = "Development service bearer token")),
    responses(
        (status = 200, description = "Launchpad app proof state", body = AdminLaunchpadProofResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service or system authentication is required", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn launchpad_proof(
    _admin: AdminActor,
    HttpRequestContext(_request_ctx): HttpRequestContext,
) -> Result<Json<AdminLaunchpadProofResponse>, ApiErrorResponse> {
    Ok(Json(launchpad_proof_response(FsPath::new(APP_PROOF_PATH))))
}
```

Register in `lib.rs`:

```rust
.routes(routes!(launchpad_proof))
```

- [ ] **Step 4: Generate contracts and verify**

Run:

```sh
cargo fmt --all
cargo test -p lenso-platform-admin-data launchpad_proof
just generate-contracts
just generated-check
just arch-check
```

Expected: tests and checks pass, OpenAPI contains `/admin/data/launchpad/proof`.

- [ ] **Step 5: Commit Host endpoint**

```sh
git add contracts/openapi/app-api.v1.yaml crates/platform-admin-data/src/dto.rs crates/platform-admin-data/src/handlers.rs crates/platform-admin-data/src/lib.rs
git commit -m "feat: expose launchpad app proof"
```

## Task 6: Runtime Console App Proof Panel

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/available-modules-model.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/data/available-modules.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/data/available-modules.test.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/launchpad-model.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/launchpad-model.test.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/launchpad-page.tsx`

- [ ] **Step 1: Add proof types**

In `available-modules-model.ts`, add:

```ts
export type LaunchpadProofResponse = {
  version: number;
  status: "ready" | "drifted" | "needs_attention" | "failed" | "empty" | string;
  proofFile: string;
  checkedAtUnixMs?: number | null;
  projectName?: string | null;
  blueprint?: string | null;
  addons: string[];
  checks: LaunchpadProofCheck[];
  drifts: LaunchpadProofDrift[];
  nextCommand?: string | null;
};

export type LaunchpadProofCheck = {
  id: string;
  label: string;
  status: string;
  message: string;
  command?: string | null;
};

export type LaunchpadProofDrift = {
  resource: string;
  name: string;
  message: string;
  command?: string | null;
};
```

- [ ] **Step 2: Add data fetch and tests**

In `available-modules.ts`, add sample:

```ts
export const sampleLaunchpadProofResponse = {
  addons: ["support-sla"],
  blueprint: "support-desk",
  checkedAtUnixMs: 1_782_900_000_000,
  checks: [
    {
      command: null,
      id: "workspace-service-support-sla",
      label: "support-sla in lenso.workspace.json",
      message: "support-sla is present in lenso.workspace.json",
      status: "passed",
    },
  ],
  drifts: [],
  nextCommand: null,
  projectName: "acme-support",
  proofFile: ".lenso/app-proof.json",
  status: "ready",
  version: 1,
} satisfies LaunchpadProofResponse;
```

Add:

```ts
export const launchpadProofQueryKey = ["launchpad", "proof"] as const;
```

Add fetch:

```ts
type LaunchpadProofHttpClient = {
  get: (path: string) => {
    json: () => Promise<LaunchpadProofResponse>;
  };
};

export async function fetchLaunchpadProof({
  apiMode = isApiMode(),
  client = httpClient,
}: {
  apiMode?: boolean;
  client?: LaunchpadProofHttpClient;
} = {}): Promise<LaunchpadProofResponse> {
  if (apiMode) {
    return client.get("admin/data/launchpad/proof").json();
  }
  return sampleLaunchpadProofResponse;
}
```

Update `moduleRefreshInvalidationQueryKeys()` to include `launchpadProofQueryKey`.

Add provider test:

```ts
test("fetches Launchpad app proof state", async () => {
  await expect(fetchLaunchpadProof()).resolves.toBe(sampleLaunchpadProofResponse);
  expect(launchpadProofQueryKey).toEqual(["launchpad", "proof"]);

  const getCalls: string[] = [];
  const client = {
    get(path: string) {
      getCalls.push(path);
      return { json: async () => sampleLaunchpadProofResponse };
    },
  };

  await fetchLaunchpadProof({ apiMode: true, client });
  expect(getCalls).toEqual(["admin/data/launchpad/proof"]);
});
```

- [ ] **Step 3: Add proof summary model**

In `launchpad-model.ts`, add:

```ts
export type LaunchpadProofSummary = {
  addons: string[];
  blueprint: string;
  driftCount: number;
  drifts: Array<{
    command: string | null;
    message: string;
    name: string;
    resource: string;
  }>;
  nextCommand: string;
  proofFile: string;
  status: string;
};

export function launchpadProofSummary(
  response: LaunchpadProofResponse | undefined
): LaunchpadProofSummary {
  return {
    addons: response?.addons ?? [],
    blueprint: response?.blueprint ?? "not configured",
    driftCount: response?.drifts.length ?? 0,
    drifts:
      response?.drifts.map((drift) => ({
        command: drift.command ?? null,
        message: drift.message,
        name: drift.name,
        resource: drift.resource,
      })) ?? [],
    nextCommand: response?.nextCommand ?? "lenso app verify --write-proof",
    proofFile: response?.proofFile ?? ".lenso/app-proof.json",
    status: response?.status ?? "empty",
  };
}
```

Add model test:

```ts
test("summarizes Launchpad app proof", () => {
  expect(launchpadProofSummary(sampleLaunchpadProofResponse)).toEqual(
    expect.objectContaining({
      addons: ["support-sla"],
      blueprint: "support-desk",
      driftCount: 0,
      nextCommand: "lenso app verify --write-proof",
      status: "ready",
    })
  );
});
```

- [ ] **Step 4: Render proof panel**

In `launchpad-page.tsx`, add a third query:

```tsx
const {
  data: proofResponse,
  error: proofError,
  isError: isProofError,
  isLoading: isProofLoading,
} = useQuery({
  queryFn: () => fetchLaunchpadProof(),
  queryKey: launchpadProofQueryKey,
});
const proof = launchpadProofSummary(proofResponse);
```

Add counter:

```tsx
<LaunchpadCounter label="proof" value={proof.driftCount} />
```

Add panel in the right rail:

```tsx
<LaunchpadProofPanel
  error={proofError}
  isError={isProofError}
  isLoading={isProofLoading}
  proof={proof}
/>
```

Implement `LaunchpadProofPanel` like the existing doctor panel:

```tsx
function LaunchpadProofPanel({
  error,
  isError,
  isLoading,
  proof,
}: {
  error: unknown;
  isError: boolean;
  isLoading: boolean;
  proof: LaunchpadProofSummary;
}) {
  return (
    <section className="border-b border-(--line) px-3 py-2">
      <div className="mb-2 flex items-center gap-1.5 font-mono text-[10px] uppercase text-(--fg-tertiary)">
        <ShieldCheck size={12} />
        app proof
      </div>
      {isLoading ? (
        <div className="font-mono text-[11px] text-(--fg-tertiary)">Loading app proof...</div>
      ) : isError ? (
        <div className="font-mono text-[11px] text-(--tone-error-fg)">{errorMessage(error)}</div>
      ) : (
        <div className="grid gap-1.5 font-mono text-[11px]">
          <div className="flex min-w-0 items-center gap-2">
            <StatusDot status={proof.status} />
            <span className="min-w-0 truncate">{launchpadStatusLabel(proof.status)}</span>
            <span className="ml-auto text-[10px] text-(--fg-tertiary)">
              {proof.driftCount} drift
            </span>
          </div>
          <div className="truncate text-[10px] text-(--fg-tertiary)">
            state file: {proof.proofFile}
          </div>
          {proof.drifts.slice(0, 3).map((drift) => (
            <div className="min-w-0" key={`${drift.resource}:${drift.name}`}>
              <div className="truncate">{drift.resource} / {drift.name}</div>
              <div className="truncate text-[10px] text-(--fg-tertiary)">{drift.message}</div>
            </div>
          ))}
          <code className="min-w-0 overflow-hidden text-ellipsis border border-(--line) bg-(--bg-canvas) px-2 py-1 text-[10px] text-(--fg-secondary)">
            {proof.nextCommand}
          </code>
        </div>
      )}
    </section>
  );
}
```

Import `ShieldCheck` from `lucide-react`.

- [ ] **Step 5: Run Console checks**

Run:

```sh
pnpm exec oxfmt --write src/data/available-modules.ts src/data/available-modules.test.ts src/pages/available-modules-model.ts src/pages/launchpad-model.ts src/pages/launchpad-model.test.ts src/pages/launchpad-page.tsx
pnpm exec vitest run src/data/available-modules.test.ts src/pages/launchpad-model.test.ts
npx -y react-doctor@latest . --verbose --scope changed
pnpm check
```

Expected: vitest and `pnpm check` pass. `react-doctor` may report existing warnings, but must have zero errors from changed Launchpad files.

- [ ] **Step 6: Commit Console proof panel**

```sh
git add src/data/available-modules.ts src/data/available-modules.test.ts src/pages/available-modules-model.ts src/pages/launchpad-model.ts src/pages/launchpad-model.test.ts src/pages/launchpad-page.tsx
git commit -m "feat: show launchpad app proof"
```

## Task 7: Examples And Site Docs

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso-examples/README.md`
- Modify: `/Users/leosouthey/Projects/framework/lenso-examples/scripts/check-launchpad-fixtures.mjs`
- Create: `/Users/leosouthey/Projects/framework/lenso-examples/fixtures/launchpad/support-desk-proof/launchpad.json`
- Create: `/Users/leosouthey/Projects/framework/lenso-examples/fixtures/launchpad/support-desk-proof/dev-doctor.json`
- Create: `/Users/leosouthey/Projects/framework/lenso-examples/fixtures/launchpad/support-desk-proof/app-proof.json`
- Create: `/Users/leosouthey/Projects/framework/lenso-examples/fixtures/launchpad/support-desk-proof/agent-task.md`
- Modify: `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/product-blueprints.mdx`
- Modify: `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/cli-reference.mdx`
- Modify: `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/runtime-console.mdx`
- Modify: `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/troubleshooting.mdx`

- [ ] **Step 1: Generate proof fixture from real CLI**

From `/Users/leosouthey/Projects/framework/lenso-examples`:

```sh
tmp=$(mktemp -d)
/Users/leosouthey/Projects/framework/lenso-cli/target/debug/lenso app create "$tmp/acme-support" --blueprint support-desk
cd "$tmp/acme-support"
/Users/leosouthey/Projects/framework/lenso-cli/target/debug/lenso app add support-sla
/Users/leosouthey/Projects/framework/lenso-cli/target/debug/lenso dev doctor --write-state
/Users/leosouthey/Projects/framework/lenso-cli/target/debug/lenso app verify --write-proof
/Users/leosouthey/Projects/framework/lenso-cli/target/debug/lenso agent task "add enterprise SLA escalation" > agent-task.md
mkdir -p /Users/leosouthey/Projects/framework/lenso-examples/fixtures/launchpad/support-desk-proof
cp .lenso/launchpad.json /Users/leosouthey/Projects/framework/lenso-examples/fixtures/launchpad/support-desk-proof/launchpad.json
cp .lenso/dev-doctor.json /Users/leosouthey/Projects/framework/lenso-examples/fixtures/launchpad/support-desk-proof/dev-doctor.json
cp .lenso/app-proof.json /Users/leosouthey/Projects/framework/lenso-examples/fixtures/launchpad/support-desk-proof/app-proof.json
cp agent-task.md /Users/leosouthey/Projects/framework/lenso-examples/fixtures/launchpad/support-desk-proof/agent-task.md
```

- [ ] **Step 2: Extend fixture checker**

In `scripts/check-launchpad-fixtures.mjs`, read the proof fixture:

```js
const proofRoot = path.join(process.cwd(), "fixtures/launchpad/support-desk-proof");
const appProof = JSON.parse(
  fs.readFileSync(path.join(proofRoot, "app-proof.json"), "utf8"),
);
const proofAgentTask = fs.readFileSync(path.join(proofRoot, "agent-task.md"), "utf8");

assert(appProof.protocol === "lenso.app-proof.v1", "app proof protocol");
assert(appProof.status === "ready", "app proof status");
assert(appProof.addons.includes("support-sla"), "app proof includes support-sla");
assert(proofAgentTask.includes("## App Proof"), "agent task includes app proof");
assert(
  proofAgentTask.includes("Existing service source files are user code."),
  "agent task includes app proof source boundary",
);
```

- [ ] **Step 3: Update examples README**

Add to the Launchpad section:

```md
V24 adds App Proof for generated app lifecycle checks:

```sh
lenso app verify --write-proof
lenso app diff
lenso app repair --dry-run
pnpm check:launchpad-fixtures
```

The `support-desk-proof` fixture shows Launchpad, dev doctor, App Proof, and
agent task context for `support-desk + support-sla`.
```

- [ ] **Step 4: Run examples check and commit**

```sh
pnpm check:launchpad-fixtures
git diff --check
git add README.md scripts/check-launchpad-fixtures.mjs fixtures/launchpad/support-desk-proof
git commit -m "docs: add app proof fixtures"
```

- [ ] **Step 5: Update site docs**

In Product Blueprints, add:

```md
## Verify generated state

After adding addons, write App Proof:

```sh
lenso app verify --write-proof
lenso app diff
lenso app repair --dry-run
```

App Proof checks generated control-plane state. It is not a security
attestation and not a deployment gate.
```

In CLI Reference, add rows:

```md
| `lenso app verify --write-proof` | Verify generated blueprint state and write `.lenso/app-proof.json`. |
| `lenso app diff` | Compare Launchpad, workspace, and system graph state with the blueprint and addons. |
| `lenso app repair --dry-run` | Preview safe generated-state repairs. |
| `lenso app repair` | Apply safe generated-state repairs without overwriting service source files. |
```

In Runtime Console, mention `/launchpad` App Proof. In Troubleshooting, add the fix:

```md
- App Proof is empty: run `lenso app verify --write-proof`.
- App Proof is drifted: run `lenso app diff`, then `lenso app repair --dry-run`.
```

- [ ] **Step 6: Run site checks and commit**

```sh
pnpm types:check
pnpm lint
git diff --check
git add 'content/docs/(host)/product-blueprints.mdx' 'content/docs/(host)/cli-reference.mdx' 'content/docs/(host)/runtime-console.mdx' 'content/docs/(host)/troubleshooting.mdx'
git commit -m "docs: document app proof"
```

## Task 8: Final Verification

**Files:**
- No new files unless checks generate committed artifacts already covered above.

- [ ] **Step 1: Verify all touched repos are clean**

```sh
for repo in lenso lenso-cli lenso-runtime-console lenso-examples lenso-site; do
  git -C "/Users/leosouthey/Projects/framework/$repo" status --short --branch
done
```

Expected: each repo shows the V24 branch with no uncommitted changes.

- [ ] **Step 2: Run final CLI happy path**

```sh
tmp=$(mktemp -d)
/Users/leosouthey/Projects/framework/lenso-cli/target/debug/lenso app create "$tmp/acme-support" --blueprint support-desk
cd "$tmp/acme-support"
/Users/leosouthey/Projects/framework/lenso-cli/target/debug/lenso app add support-sla
/Users/leosouthey/Projects/framework/lenso-cli/target/debug/lenso dev doctor --write-state
/Users/leosouthey/Projects/framework/lenso-cli/target/debug/lenso app verify --write-proof
/Users/leosouthey/Projects/framework/lenso-cli/target/debug/lenso app diff
/Users/leosouthey/Projects/framework/lenso-cli/target/debug/lenso app repair --dry-run
test -f .lenso/app-proof.json
```

Expected:

- `app verify --write-proof` succeeds.
- `app diff` prints no drift.
- `app repair --dry-run` prints no safe app repairs needed.
- `.lenso/app-proof.json` exists.

- [ ] **Step 3: Run repo checks**

```sh
cd /Users/leosouthey/Projects/framework/lenso-cli
cargo test --locked
cargo build --locked

cd /Users/leosouthey/Projects/framework/lenso
cargo test -p lenso-platform-admin-data launchpad_proof
just generated-check
just arch-check

cd /Users/leosouthey/Projects/framework/lenso-runtime-console
pnpm check

cd /Users/leosouthey/Projects/framework/lenso-examples
pnpm check:launchpad-fixtures

cd /Users/leosouthey/Projects/framework/lenso-site
pnpm types:check
pnpm lint
```

Expected: all commands exit 0.

- [ ] **Step 4: Final branch summary**

Collect commits:

```sh
for repo in lenso lenso-cli lenso-runtime-console lenso-examples lenso-site; do
  git -C "/Users/leosouthey/Projects/framework/$repo" log --oneline -5
done
```

Report the changed repos, commit hashes, and verification commands.

## Self-Review

- Spec coverage: CLI verify/diff/repair, `.lenso/app-proof.json`, Host proof endpoint, Console Launchpad proof panel, agent context, examples, docs, and regression checks are covered.
- Scope check: no user-authored blueprint DSL, no marketplace, no Kubernetes requirement, no process supervisor.
- Type consistency: `AppProofState`, Host `AdminLaunchpadProofResponse`, and Console `LaunchpadProofResponse` all use `status`, `checkedAtUnixMs`, `projectName`, `blueprint`, `addons`, `checks`, `drifts`, and `nextCommand`.
