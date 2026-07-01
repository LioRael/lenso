# Lenso App Composer V26 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build V26 App Composer so users can compose blueprint apps with multiple addons, see service-aware next actions, and hand agents scoped app/module/service context.

**Architecture:** Keep generation and apply logic in `lenso-cli`; Host and Runtime Console stay read-only evidence surfaces. Reuse `.lenso/app-change-plan.json` and add an optional `composition` block instead of creating a new recipe registry or DSL. Keep module install and service install separate in commands and copy.

**Tech Stack:** Rust CLI, Rust admin-data DTOs, OpenAPI YAML, React Runtime Console, JSON Launchpad fixtures, MDX docs, local Lenso skills.

---

## Scope Check

The spec combines three product directions, but they are one implementation path:

```text
compose app shape -> write/apply app change plan -> summarize service ops -> generate agent handoff
```

Do not split this into three unrelated products. Keep `lenso app compose`,
`lenso app next`, Console App Lifecycle, and `lenso agent task --from-app-plan`
using the same state files and command vocabulary.

## File Structure

- `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`: add CLI parser args for `app plan`, `app upgrade`, `app apply`, `app compose`, `app next`, `app explain`, and agent handoff flags.
- `/Users/leosouthey/Projects/framework/lenso-cli/src/launchpad.rs`: add App Change Plan parity, composition data model, next-action selection, explain output, and agent handoff rendering. Keep this in `launchpad.rs` for V26 because this file already owns blueprint/addon recipes and generated-state repair helpers.
- `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/dto.rs`: add optional composition DTO fields to the existing Launchpad change-plan response.
- `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/handlers.rs`: pass through composition data from `.lenso/app-change-plan.json`.
- `/Users/leosouthey/Projects/framework/lenso/contracts/openapi/app-api.v1.yaml`: regenerate after DTO changes.
- `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/available-modules-model.ts`: add composition/service action response types and sample data.
- `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/data/available-modules.ts`: fetch the richer change-plan response and keep query keys stable.
- `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/launchpad-model.ts`: summarize composition, service ops, and next action.
- `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/launchpad-page.tsx`: reposition Launchpad as App Lifecycle and show Composer/Service Ops/Agent panels.
- `/Users/leosouthey/Projects/framework/lenso-examples/fixtures/launchpad/support-desk-composer/`: add the V26 fixture.
- `/Users/leosouthey/Projects/framework/lenso-examples/scripts/check-launchpad-fixtures.mjs`: validate the V26 fixture.
- `/Users/leosouthey/Projects/framework/lenso-examples/README.md`: document the Composer fixture.
- `/Users/leosouthey/Projects/framework/lenso/skills/lenso-start/SKILL.md`: prefer Composer for new app work.
- `/Users/leosouthey/Projects/framework/lenso/skills/lenso-business-planning/SKILL.md`: map business prompts to blueprint/addon/service decisions.
- `/Users/leosouthey/Projects/framework/lenso/skills/lenso-module-authoring/SKILL.md`: consume Composer context for module-scoped work.
- `/Users/leosouthey/Projects/framework/lenso/skills/lenso-remote-module-authoring/SKILL.md`: use service install/readiness wording for service-provided modules.
- `/Users/leosouthey/Projects/framework/lenso/skills/lenso-starter-host/SKILL.md`: route existing app work through `app next`, `app explain`, and App Proof.
- `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/product-blueprints.mdx`: add Composer flow.
- `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/quickstart.mdx`: make Composer the first app path.
- `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/cli-reference.mdx`: add command references.
- `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/runtime-console.mdx`: describe App Lifecycle.
- `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(agent)/agent-development.mdx`: describe `--from-app-plan`.
- `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/troubleshooting.mdx`: add Composer blocked-plan and service next-action guidance.

## Task 0: Branch Prep

**Files:**
- No source edits.

- [ ] **Step 1: Start implementation branches from current main**

Run:

```sh
git -C /Users/leosouthey/Projects/framework/lenso-cli switch main
git -C /Users/leosouthey/Projects/framework/lenso-cli pull --ff-only
git -C /Users/leosouthey/Projects/framework/lenso-cli switch -c feat/app-composer-v26

git -C /Users/leosouthey/Projects/framework/lenso switch main
git -C /Users/leosouthey/Projects/framework/lenso pull --ff-only
git -C /Users/leosouthey/Projects/framework/lenso switch -c feat/app-composer-v26

git -C /Users/leosouthey/Projects/framework/lenso-runtime-console switch main
git -C /Users/leosouthey/Projects/framework/lenso-runtime-console pull --ff-only
git -C /Users/leosouthey/Projects/framework/lenso-runtime-console switch -c feat/app-composer-v26

git -C /Users/leosouthey/Projects/framework/lenso-examples switch main
git -C /Users/leosouthey/Projects/framework/lenso-examples pull --ff-only
git -C /Users/leosouthey/Projects/framework/lenso-examples switch -c feat/app-composer-v26

git -C /Users/leosouthey/Projects/framework/lenso-site switch main
git -C /Users/leosouthey/Projects/framework/lenso-site pull --ff-only
git -C /Users/leosouthey/Projects/framework/lenso-site switch -c feat/app-composer-v26
```

Expected: every repo is on `feat/app-composer-v26` with a clean worktree.

- [ ] **Step 2: Confirm the V25 CLI gap**

Run:

```sh
rg -n "AppCommand::Plan|app_change_plan|AppChangePlan" /Users/leosouthey/Projects/framework/lenso-cli/src
```

Expected before Task 1: no App Change Plan CLI implementation is present on
`lenso-cli/main`. Task 1 fills this dependency.

## Task 1: CLI App Change Plan Parity

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/launchpad.rs`

- [ ] **Step 1: Add parser tests for V25 lifecycle commands**

Add tests beside the existing `parses_app_*` tests in `main.rs`:

```rust
#[test]
fn parses_app_plan_write_plan() {
    let cli = Cli::parse_from(["lenso", "app", "plan", "--write-plan"]);
    let Command::App {
        command: AppCommand::Plan(args),
    } = cli.command
    else {
        panic!("expected app plan");
    };
    assert!(args.write_plan);
    assert_eq!(args.addons, Vec::<String>::new());
}

#[test]
fn parses_app_apply_dry_run() {
    let cli = Cli::parse_from([
        "lenso",
        "app",
        "apply",
        ".lenso/app-change-plan.json",
        "--dry-run",
    ]);
    let Command::App {
        command: AppCommand::Apply(args),
    } = cli.command
    else {
        panic!("expected app apply");
    };
    assert!(args.dry_run);
    assert_eq!(
        args.plan,
        std::path::PathBuf::from(".lenso/app-change-plan.json")
    );
}

#[test]
fn parses_app_upgrade_check() {
    let cli = Cli::parse_from(["lenso", "app", "upgrade", "--check"]);
    let Command::App {
        command: AppCommand::Upgrade(args),
    } = cli.command
    else {
        panic!("expected app upgrade");
    };
    assert!(args.check);
}
```

- [ ] **Step 2: Run parser tests and confirm failure**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-cli
cargo test --locked parses_app_plan_write_plan parses_app_apply_dry_run parses_app_upgrade_check
```

Expected: FAIL because the command variants do not exist yet.

- [ ] **Step 3: Add command variants and args**

In `AppCommand`, add:

```rust
/// Plan safe generated app lifecycle changes.
Plan(AppPlanArgs),
/// Check whether a generated app needs safe upgrades.
Upgrade(AppUpgradeArgs),
/// Apply a generated app change plan.
Apply(AppApplyArgs),
```

Add args near the other `App*Args` structs:

```rust
#[derive(Debug, Args, Clone)]
struct AppPlanArgs {
    /// Lenso host repository root.
    #[arg(long)]
    repo_root: Option<std::path::PathBuf>,

    /// Addon to include in the requested plan. Can be repeated.
    #[arg(long = "addon")]
    addons: Vec<String>,

    /// Write .lenso/app-change-plan.json.
    #[arg(long)]
    write_plan: bool,
}

#[derive(Debug, Args, Clone)]
struct AppUpgradeArgs {
    /// Lenso host repository root.
    #[arg(long)]
    repo_root: Option<std::path::PathBuf>,

    /// Exit non-zero when the app has pending or blocked changes.
    #[arg(long)]
    check: bool,

    /// Write .lenso/app-change-plan.json.
    #[arg(long)]
    write_plan: bool,
}

#[derive(Debug, Args, Clone)]
struct AppApplyArgs {
    /// App change plan path.
    plan: std::path::PathBuf,

    /// Lenso host repository root.
    #[arg(long)]
    repo_root: Option<std::path::PathBuf>,

    /// Print what would be applied without writing files.
    #[arg(long)]
    dry_run: bool,
}
```

- [ ] **Step 4: Wire command dispatch**

In the `Command::App` dispatch block, add:

```rust
AppCommand::Plan(args) => {
    launchpad::app_plan(launchpad::AppPlanOptions {
        addons: args.addons,
        repo_root: args.repo_root,
        write_plan: args.write_plan,
    })?;
}
AppCommand::Upgrade(args) => {
    launchpad::app_upgrade(launchpad::AppUpgradeOptions {
        check: args.check,
        repo_root: args.repo_root,
        write_plan: args.write_plan,
    })?;
}
AppCommand::Apply(args) => {
    launchpad::app_apply(launchpad::AppApplyOptions {
        dry_run: args.dry_run,
        plan: args.plan,
        repo_root: args.repo_root,
    })?;
}
```

- [ ] **Step 5: Add option structs and plan model**

In `launchpad.rs`, add constants:

```rust
const APP_CHANGE_PLAN_PROTOCOL: &str = "lenso.app-change-plan.v1";
const APP_CHANGE_PLAN_FILE: &str = ".lenso/app-change-plan.json";
```

Add public option structs:

```rust
#[derive(Debug, Clone)]
pub(crate) struct AppPlanOptions {
    pub(crate) addons: Vec<String>,
    pub(crate) repo_root: Option<PathBuf>,
    pub(crate) write_plan: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct AppUpgradeOptions {
    pub(crate) check: bool,
    pub(crate) repo_root: Option<PathBuf>,
    pub(crate) write_plan: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct AppApplyOptions {
    pub(crate) dry_run: bool,
    pub(crate) plan: PathBuf,
    pub(crate) repo_root: Option<PathBuf>,
}
```

Add model types:

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppChangePlanState {
    protocol: String,
    status: String,
    generated_at_unix_ms: u64,
    project_name: Option<String>,
    blueprint: Option<String>,
    addons: Vec<String>,
    proof_status: Option<String>,
    changes: Vec<AppChangePlanItem>,
    blocked: Vec<AppChangePlanItem>,
    next_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    composition: Option<AppCompositionState>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppChangePlanItem {
    id: String,
    kind: String,
    name: String,
    action: String,
    safe: bool,
    message: String,
    command: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppCompositionState {
    protocol: String,
    intent: Option<String>,
    requested_addons: Vec<String>,
    applied_addons: Vec<String>,
    pending_addons: Vec<String>,
    service_actions: Vec<AppCompositionAction>,
    agent_actions: Vec<AppCompositionAction>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppCompositionAction {
    id: String,
    kind: String,
    label: String,
    command: Option<String>,
    status: String,
}
```

- [ ] **Step 6: Implement minimal plan/apply behavior**

Implement these functions:

```rust
pub(crate) fn app_plan(options: AppPlanOptions) -> Result<()> {
    let repo_root = options.repo_root.unwrap_or_else(|| PathBuf::from("."));
    let plan = app_change_plan_state(&repo_root, &options.addons, None)?;
    print_app_change_plan(&plan);
    if options.write_plan {
        write_json(&repo_root.join(APP_CHANGE_PLAN_FILE), &plan)?;
        println!("Wrote {}.", repo_root.join(APP_CHANGE_PLAN_FILE).display());
    }
    if plan.status == "blocked" || plan.status == "failed" {
        bail!("app change plan status is {}", plan.status);
    }
    Ok(())
}

pub(crate) fn app_upgrade(options: AppUpgradeOptions) -> Result<()> {
    let repo_root = options.repo_root.unwrap_or_else(|| PathBuf::from("."));
    let plan = app_change_plan_state(&repo_root, &[], None)?;
    print_app_change_plan(&plan);
    if options.write_plan {
        write_json(&repo_root.join(APP_CHANGE_PLAN_FILE), &plan)?;
        println!("Wrote {}.", repo_root.join(APP_CHANGE_PLAN_FILE).display());
    }
    if options.check && matches!(plan.status.as_str(), "changes" | "blocked" | "failed") {
        bail!("app upgrade check found {}", plan.status);
    }
    Ok(())
}

pub(crate) fn app_apply(options: AppApplyOptions) -> Result<()> {
    let repo_root = options.repo_root.unwrap_or_else(|| PathBuf::from("."));
    let plan_path = absolutize_from(&repo_root, &options.plan);
    let plan: AppChangePlanState = read_json_required(&plan_path)?;
    validate_app_change_plan(&repo_root, &plan)?;
    if options.dry_run {
        print_app_change_plan(&plan);
        println!("dry run: no files changed");
        return Ok(());
    }
    repair_generated_state(&repo_root)?;
    println!("Applied generated app changes.");
    println!("Next: lenso app verify --write-proof");
    Ok(())
}
```

`validate_app_change_plan` rejects unknown protocols, blocked items, failed
status, different project names, and different blueprint names.

- [ ] **Step 7: Add plan unit tests**

Add tests in `launchpad.rs`:

```rust
#[test]
fn app_change_plan_is_ready_when_proof_is_ready() {
    let launchpad = support_desk_launchpad_state("acme-support");
    let proof = app_proof_state_from_parts(&launchpad, None, Vec::new(), Vec::new());
    let plan = app_change_plan_from_parts(&launchpad, Some(&proof), Vec::new(), Vec::new(), None);
    assert_eq!(plan.status, "ready");
    assert_eq!(plan.next_command, None);
}

#[test]
fn app_change_plan_blocks_unsupported_addon() {
    let launchpad = support_desk_launchpad_state("acme-support");
    let plan = app_change_plan_for_requested_addons(&launchpad, &["unknown-addon".to_owned()])
        .expect("plan");
    assert_eq!(plan.status, "blocked");
    assert!(plan.blocked.iter().any(|item| item.name == "unknown-addon"));
}

#[test]
fn app_change_plan_marks_requested_addon_change() {
    let launchpad = support_desk_launchpad_state("acme-support");
    let plan = app_change_plan_for_requested_addons(&launchpad, &["support-sla".to_owned()])
        .expect("plan");
    assert_eq!(plan.status, "changes");
    assert!(plan.changes.iter().any(|item| item.name == "support-sla"));
}
```

- [ ] **Step 8: Run tests**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-cli
cargo fmt
cargo test --locked parses_app_plan_write_plan parses_app_apply_dry_run parses_app_upgrade_check
cargo test --locked app_change_plan
```

Expected: parser and App Change Plan unit tests pass.

- [ ] **Step 9: Commit**

Run:

```sh
git add src/main.rs src/launchpad.rs
git commit -m "feat: add app change plan lifecycle"
```

## Task 2: CLI App Composer

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/launchpad.rs`

- [ ] **Step 1: Add parser tests for Composer**

Add tests:

```rust
#[test]
fn parses_app_compose_new_app_with_two_addons() {
    let cli = Cli::parse_from([
        "lenso",
        "app",
        "compose",
        "./acme-support",
        "--blueprint",
        "support-desk",
        "--addon",
        "support-sla",
        "--addon",
        "customer-profile",
        "--apply",
    ]);
    let Command::App {
        command: AppCommand::Compose(args),
    } = cli.command
    else {
        panic!("expected app compose");
    };
    assert_eq!(args.dir, Some(std::path::PathBuf::from("./acme-support")));
    assert_eq!(args.addons, vec!["support-sla", "customer-profile"]);
    assert!(args.apply);
}

#[test]
fn parses_app_compose_existing_app_write_plan() {
    let cli = Cli::parse_from([
        "lenso",
        "app",
        "compose",
        "--repo-root",
        "./acme-support",
        "--addon",
        "notifications",
        "--write-plan",
    ]);
    let Command::App {
        command: AppCommand::Compose(args),
    } = cli.command
    else {
        panic!("expected app compose");
    };
    assert_eq!(args.repo_root, Some(std::path::PathBuf::from("./acme-support")));
    assert_eq!(args.addons, vec!["notifications"]);
    assert!(args.write_plan);
}
```

- [ ] **Step 2: Add command args**

Add to `AppCommand`:

```rust
/// Compose a Launchpad app from a blueprint and addons.
Compose(AppComposeArgs),
```

Add args:

```rust
#[derive(Debug, Args, Clone)]
struct AppComposeArgs {
    /// New app directory. Omit when composing an existing app with --repo-root.
    dir: Option<std::path::PathBuf>,

    /// Existing Lenso host repository root.
    #[arg(long)]
    repo_root: Option<std::path::PathBuf>,

    /// Launchpad blueprint name for new apps.
    #[arg(long, default_value = "support-desk")]
    blueprint: String,

    /// Addon to compose into the app. Can be repeated.
    #[arg(long = "addon")]
    addons: Vec<String>,

    /// Apply safe generated app changes.
    #[arg(long)]
    apply: bool,

    /// Write .lenso/app-change-plan.json.
    #[arg(long)]
    write_plan: bool,

    /// Print explanation without writing files.
    #[arg(long)]
    explain: bool,
}
```

- [ ] **Step 3: Add options and dispatch**

Add:

```rust
#[derive(Debug, Clone)]
pub(crate) struct AppComposeOptions {
    pub(crate) addons: Vec<String>,
    pub(crate) apply: bool,
    pub(crate) blueprint: String,
    pub(crate) dir: Option<PathBuf>,
    pub(crate) explain: bool,
    pub(crate) repo_root: Option<PathBuf>,
    pub(crate) write_plan: bool,
}
```

Dispatch:

```rust
AppCommand::Compose(args) => {
    launchpad::app_compose(launchpad::AppComposeOptions {
        addons: args.addons,
        apply: args.apply,
        blueprint: args.blueprint,
        dir: args.dir,
        explain: args.explain,
        repo_root: args.repo_root,
        write_plan: args.write_plan,
    })?;
}
```

- [ ] **Step 4: Implement Composer validation**

Add `validate_app_compose_options`:

```rust
fn validate_app_compose_options(options: &AppComposeOptions) -> Result<()> {
    if options.dir.is_some() && options.repo_root.is_some() {
        bail!("use either a new app directory or --repo-root, not both");
    }
    if options.dir.is_none() && options.repo_root.is_none() {
        bail!("app compose needs a new app directory or --repo-root");
    }
    if options.apply && options.explain {
        bail!("--apply and --explain cannot be combined");
    }
    Ok(())
}
```

- [ ] **Step 5: Implement `app_compose`**

Behavior:

```rust
pub(crate) fn app_compose(options: AppComposeOptions) -> Result<()> {
    validate_app_compose_options(&options)?;
    if let Some(dir) = options.dir.clone() {
        return compose_new_app(options, dir);
    }
    compose_existing_app(options)
}
```

`compose_new_app`:

```rust
fn compose_new_app(options: AppComposeOptions, dir: PathBuf) -> Result<()> {
    if !options.apply && !options.write_plan && !options.explain {
        println!("Compose preview for new app.");
        println!("Next: rerun with --apply to create the app");
        return Ok(());
    }
    if options.apply {
        create_app(AppCreateOptions {
            blueprint: options.blueprint.clone(),
            dir: dir.clone(),
            force: false,
        })?;
        with_current_dir(&dir, || {
            for addon in &options.addons {
                add_app_addon(AppAddOptions {
                    addon: addon.clone(),
                })?;
            }
            let plan = app_change_plan_state(Path::new("."), &[], Some(composition_from_request(&options)?))?;
            write_json(Path::new(APP_CHANGE_PLAN_FILE), &plan)
        })?;
        println!("Composed app {}.", dir.display());
        println!("Next: cd {} && lenso dev doctor --live --write-state", dir.display());
        return Ok(());
    }
    let composition = composition_preview_for_new_app(&options)?;
    print_app_composition(&composition);
    Ok(())
}
```

`compose_existing_app` reads existing state, builds an App Change Plan with a
composition block, writes when `--write-plan`, applies only when `--apply`, and
prints the primary next command.

- [ ] **Step 6: Add Composer unit tests**

Add tests:

```rust
#[test]
fn compose_options_reject_dir_and_repo_root_together() {
    let err = validate_app_compose_options(&AppComposeOptions {
        addons: Vec::new(),
        apply: false,
        blueprint: "support-desk".to_owned(),
        dir: Some(PathBuf::from("app")),
        explain: false,
        repo_root: Some(PathBuf::from(".")),
        write_plan: false,
    })
    .unwrap_err();
    assert!(err.to_string().contains("either a new app directory or --repo-root"));
}

#[test]
fn composition_tracks_pending_and_applied_addons() {
    let mut launchpad = support_desk_launchpad_state("acme-support");
    launchpad.addons.push(launchpad_addon_from_addon(&support_sla_addon()));
    let composition = composition_for_existing_app(
        &launchpad,
        &["support-sla".to_owned(), "customer-profile".to_owned()],
        None,
    )
    .expect("composition");
    assert_eq!(composition.applied_addons, vec!["support-sla"]);
    assert_eq!(composition.pending_addons, vec!["customer-profile"]);
}
```

- [ ] **Step 7: Run tests**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-cli
cargo fmt
cargo test --locked parses_app_compose_new_app_with_two_addons parses_app_compose_existing_app_write_plan
cargo test --locked compose_options_reject_dir_and_repo_root_together composition_tracks_pending_and_applied_addons
```

Expected: Composer parser and planner tests pass.

- [ ] **Step 8: Commit**

Run:

```sh
git add src/main.rs src/launchpad.rs
git commit -m "feat: compose launchpad apps"
```

## Task 3: CLI App Next And Explain

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/launchpad.rs`

- [ ] **Step 1: Add parser tests**

Add:

```rust
#[test]
fn parses_app_next_live() {
    let cli = Cli::parse_from(["lenso", "app", "next", "--live"]);
    let Command::App {
        command: AppCommand::Next(args),
    } = cli.command
    else {
        panic!("expected app next");
    };
    assert!(args.live);
}

#[test]
fn parses_app_explain_repo_root() {
    let cli = Cli::parse_from(["lenso", "app", "explain", "--repo-root", "./acme-support"]);
    let Command::App {
        command: AppCommand::Explain(args),
    } = cli.command
    else {
        panic!("expected app explain");
    };
    assert_eq!(args.repo_root, Some(std::path::PathBuf::from("./acme-support")));
}
```

- [ ] **Step 2: Add command variants and options**

Add:

```rust
Next(AppNextArgs),
Explain(AppExplainArgs),
```

Args:

```rust
#[derive(Debug, Args, Clone)]
struct AppNextArgs {
    #[arg(long)]
    repo_root: Option<std::path::PathBuf>,
    #[arg(long)]
    live: bool,
}

#[derive(Debug, Args, Clone)]
struct AppExplainArgs {
    #[arg(long)]
    repo_root: Option<std::path::PathBuf>,
}
```

Options:

```rust
#[derive(Debug, Clone)]
pub(crate) struct AppNextOptions {
    pub(crate) live: bool,
    pub(crate) repo_root: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) struct AppExplainOptions {
    pub(crate) repo_root: Option<PathBuf>,
}
```

- [ ] **Step 3: Add next-action model**

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
struct AppNextAction {
    command: String,
    reason: String,
    severity: AppNextSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum AppNextSeverity {
    Info,
    Recommended,
    Required,
}
```

Selection order:

```rust
fn choose_app_next_action(state: &AppLifecycleSnapshot) -> AppNextAction {
    if !state.has_launchpad {
        return required("lenso app create ./my-lenso-app --blueprint support-desk", "no Launchpad app state found");
    }
    if state.change_plan_status == Some("blocked") {
        return required("lenso app explain", "app change plan is blocked");
    }
    if state.change_plan_status == Some("changes") {
        return required("lenso app apply .lenso/app-change-plan.json", "safe generated app changes are pending");
    }
    if state.proof_status != Some("ready") {
        return required("lenso app verify --write-proof", "app proof is missing or stale");
    }
    if state.dev_doctor_status != Some("ready") {
        return recommended("lenso dev doctor --live --write-state", "dev readiness has not been confirmed");
    }
    if let Some(service_command) = state.first_service_command.clone() {
        return recommended(&service_command, "a service needs operator attention");
    }
    recommended("lenso dev up", "app lifecycle is ready for local development")
}
```

- [ ] **Step 4: Read lifecycle snapshot**

`AppLifecycleSnapshot` reads these files:

```rust
struct AppLifecycleSnapshot {
    has_launchpad: bool,
    launchpad_status: Option<String>,
    proof_status: Option<String>,
    dev_doctor_status: Option<String>,
    change_plan_status: Option<String>,
    first_service_command: Option<String>,
}
```

Implement snapshot helpers with existing readers:

- `read_launchpad_state_optional`
- `read_app_proof_state_optional`
- `read_json_value_optional` for dev doctor
- `read_json_value_optional` for change plan
- `read_json_value_optional` for `.lenso/module-services.json`
- `read_json_value_optional` for `.lenso/service-deployments.json`

- [ ] **Step 5: Implement command output**

`app_next` prints:

```text
Next: lenso app apply .lenso/app-change-plan.json
Reason: safe generated app changes are pending

Evidence:
- launchpad: configured
- app proof: ready
- change plan: changes
- dev doctor: ready
- services: 1 action recommended
```

`app_explain` prints the same next command plus:

```text
Composer will change generated app state only:
- .lenso/launchpad.json
- lenso.workspace.json
- lenso.system.json
- missing generated service scaffold directories

Composer will not overwrite service source files.
Modules and services remain separate install actions.
```

- [ ] **Step 6: Add tests**

Add:

```rust
#[test]
fn next_action_prefers_blocked_change_plan() {
    let snapshot = AppLifecycleSnapshot {
        has_launchpad: true,
        launchpad_status: Some("configured".to_owned()),
        proof_status: Some("ready".to_owned()),
        dev_doctor_status: Some("ready".to_owned()),
        change_plan_status: Some("blocked".to_owned()),
        first_service_command: Some("lenso service status support-sla api".to_owned()),
    };
    let action = choose_app_next_action(&snapshot);
    assert_eq!(action.command, "lenso app explain");
    assert_eq!(action.severity, AppNextSeverity::Required);
}

#[test]
fn next_action_recommends_service_after_clean_app_state() {
    let snapshot = AppLifecycleSnapshot {
        has_launchpad: true,
        launchpad_status: Some("configured".to_owned()),
        proof_status: Some("ready".to_owned()),
        dev_doctor_status: Some("ready".to_owned()),
        change_plan_status: Some("ready".to_owned()),
        first_service_command: Some("lenso service status support-sla api".to_owned()),
    };
    let action = choose_app_next_action(&snapshot);
    assert_eq!(action.command, "lenso service status support-sla api");
}
```

- [ ] **Step 7: Run tests and commit**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-cli
cargo fmt
cargo test --locked parses_app_next_live parses_app_explain_repo_root
cargo test --locked next_action
git add src/main.rs src/launchpad.rs
git commit -m "feat: explain app lifecycle next actions"
```

## Task 4: Agent Module Studio Handoff

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/launchpad.rs`

- [ ] **Step 1: Extend agent args**

Modify `AgentContextArgs` and `AgentTaskArgs`:

```rust
/// Include .lenso/app-change-plan.json composition context.
#[arg(long)]
from_app_plan: bool,

/// Scope handoff to one module when known.
#[arg(long = "for-module")]
for_module: Option<String>,
```

Add fields to `AgentContextOptions`:

```rust
pub(crate) from_app_plan: bool,
pub(crate) for_module: Option<String>,
```

- [ ] **Step 2: Add parser test**

Add:

```rust
#[test]
fn parses_agent_task_from_app_plan_for_module() {
    let cli = Cli::parse_from([
        "lenso",
        "agent",
        "task",
        "--from-app-plan",
        "--for-module",
        "support-ticket",
        "add private notes",
    ]);
    let Command::Agent {
        command: AgentCommand::Task(args),
    } = cli.command
    else {
        panic!("expected agent task");
    };
    assert!(args.from_app_plan);
    assert_eq!(args.for_module, Some("support-ticket".to_owned()));
}
```

- [ ] **Step 3: Read optional App Change Plan**

In `agent_context`, read:

```rust
let change_plan = if options.from_app_plan {
    read_app_change_plan_state_optional(&repo_root)?
} else {
    None
};
```

Extend `agent_context_markdown` to accept:

```rust
change_plan: Option<&AppChangePlanState>,
for_module: Option<&str>,
```

- [ ] **Step 4: Add markdown sections**

When change plan exists, include:

```text
## App Change Plan

- Status: changes
- Next command: lenso app apply .lenso/app-change-plan.json
- Requested addons: support-sla, customer-profile
- Pending addons: customer-profile

## Service And Module Boundaries

- Modules are installable business capabilities.
- Services are out-of-process providers.
- Do not move Host-owned auth, queues, retries, outbox, Runtime Story, or Technical Operations into a service.
```

When `--for-module support-ticket` is provided, include only matching module
details and still keep the boundary section.

- [ ] **Step 5: Add tests**

Add:

```rust
#[test]
fn agent_context_mentions_app_composition_when_requested() {
    let launchpad = support_desk_launchpad_state("acme-support");
    let plan = app_change_plan_from_parts(
        &launchpad,
        None,
        Vec::new(),
        Vec::new(),
        Some(AppCompositionState {
            protocol: "lenso.app-composition.v1".to_owned(),
            intent: Some("support desk with SLA".to_owned()),
            requested_addons: vec!["support-sla".to_owned()],
            applied_addons: Vec::new(),
            pending_addons: vec!["support-sla".to_owned()],
            service_actions: Vec::new(),
            agent_actions: Vec::new(),
        }),
    );
    let markdown = agent_context_markdown(
        Some(&launchpad),
        None,
        None,
        None,
        None,
        Some(&plan),
        None,
        Some("add SLA escalation"),
    )
    .expect("markdown");
    assert!(markdown.contains("## App Change Plan"));
    assert!(markdown.contains("Requested addons: support-sla"));
    assert!(markdown.contains("Services are out-of-process providers"));
}
```

- [ ] **Step 6: Run tests and commit**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-cli
cargo fmt
cargo test --locked parses_agent_task_from_app_plan_for_module
cargo test --locked agent_context_mentions_app_composition_when_requested
git add src/main.rs src/launchpad.rs
git commit -m "feat: add app plan agent handoff"
```

## Task 5: Host Admin Data Composition Pass-Through

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/dto.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/handlers.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso/contracts/openapi/app-api.v1.yaml`

- [ ] **Step 1: Add DTO fields**

In the existing `AdminLaunchpadChangePlanResponse`, add:

```rust
#[serde(skip_serializing_if = "Option::is_none")]
pub composition: Option<AdminLaunchpadCompositionDto>,
```

Add DTOs:

```rust
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminLaunchpadCompositionDto {
    pub protocol: String,
    pub intent: Option<String>,
    #[serde(default)]
    pub requested_addons: Vec<String>,
    #[serde(default)]
    pub applied_addons: Vec<String>,
    #[serde(default)]
    pub pending_addons: Vec<String>,
    #[serde(default)]
    pub service_actions: Vec<AdminLaunchpadCompositionActionDto>,
    #[serde(default)]
    pub agent_actions: Vec<AdminLaunchpadCompositionActionDto>,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminLaunchpadCompositionActionDto {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub command: Option<String>,
    pub status: String,
}
```

- [ ] **Step 2: Parse optional composition**

In `handlers.rs`, when reading `.lenso/app-change-plan.json`, map
`value.get("composition")` into `AdminLaunchpadCompositionDto`. Missing
composition returns `None`, not `failed`.

- [ ] **Step 3: Add tests**

Add tests beside existing change-plan tests:

```rust
#[test]
fn launchpad_change_plan_reads_composition() {
    let root = tempdir().expect("tempdir");
    let lenso_dir = root.path().join(".lenso");
    std::fs::create_dir_all(&lenso_dir).expect("mkdir");
    std::fs::write(
        lenso_dir.join("app-change-plan.json"),
        r#"{
          "protocol": "lenso.app-change-plan.v1",
          "status": "changes",
          "generatedAtUnixMs": 1,
          "projectName": "acme-support",
          "blueprint": "support-desk",
          "addons": ["support-sla"],
          "proofStatus": "ready",
          "changes": [],
          "blocked": [],
          "nextCommand": "lenso app apply .lenso/app-change-plan.json",
          "composition": {
            "protocol": "lenso.app-composition.v1",
            "intent": "support desk with SLA",
            "requestedAddons": ["support-sla"],
            "appliedAddons": [],
            "pendingAddons": ["support-sla"],
            "serviceActions": [],
            "agentActions": []
          }
        }"#,
    )
    .expect("write plan");

    let response = launchpad_change_plan_response(root.path());
    assert_eq!(
        response.composition.expect("composition").pending_addons,
        vec!["support-sla"]
    );
}
```

- [ ] **Step 4: Regenerate contracts and run focused checks**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso
cargo fmt --all
cargo test -p lenso-platform-admin-data launchpad_change_plan
just generate-contracts
just generated-check
just arch-check
```

Expected: admin-data tests and generated contract checks pass.

- [ ] **Step 5: Commit**

Run:

```sh
git add crates/platform-admin-data/src/dto.rs crates/platform-admin-data/src/handlers.rs contracts/openapi/app-api.v1.yaml
git commit -m "feat: expose app composition state"
```

## Task 6: Runtime Console App Lifecycle

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/available-modules-model.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/data/available-modules.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/data/available-modules.test.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/launchpad-model.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/launchpad-model.test.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/launchpad-page.tsx`

- [ ] **Step 1: Extend model types**

Add:

```ts
export type LaunchpadCompositionAction = {
  id: string;
  kind: string;
  label: string;
  command?: string | null;
  status: string;
};

export type LaunchpadComposition = {
  protocol: string;
  intent?: string | null;
  requestedAddons: string[];
  appliedAddons: string[];
  pendingAddons: string[];
  serviceActions: LaunchpadCompositionAction[];
  agentActions: LaunchpadCompositionAction[];
};
```

Add optional `composition?: LaunchpadComposition | null` to
`LaunchpadChangePlanResponse`.

- [ ] **Step 2: Update sample response**

Use sample data:

```ts
composition: {
  protocol: "lenso.app-composition.v1",
  intent: "support desk with SLA and customer profile",
  requestedAddons: ["support-sla", "customer-profile"],
  appliedAddons: ["support-sla"],
  pendingAddons: ["customer-profile"],
  serviceActions: [
    {
      id: "service:start:customer-profile",
      kind: "service_start",
      label: "Start customer-profile service",
      command: "lenso service start customer-profile api",
      status: "recommended",
    },
  ],
  agentActions: [
    {
      id: "agent:task:customer-profile",
      kind: "agent_task",
      label: "Generate customer profile task pack",
      command:
        'lenso agent task --from-app-plan "add customer profile lookup"',
      status: "recommended",
    },
  ],
},
```

- [ ] **Step 3: Summarize App Lifecycle**

In `launchpad-model.ts`, extend `LaunchpadChangePlanSummary`:

```ts
export type LaunchpadChangePlanSummary = {
  status: string;
  proofStatus: string;
  safeChanges: number;
  blockedChanges: number;
  nextCommand: string;
  requestedAddons: string[];
  pendingAddons: string[];
  serviceAction?: LaunchpadCompositionAction;
  agentAction?: LaunchpadCompositionAction;
};
```

Add model tests:

```ts
test("summarizes launchpad composition actions", () => {
  const summary = launchpadChangePlanSummary(sampleLaunchpadChangePlanResponse);
  expect(summary.requestedAddons).toEqual(["support-sla", "customer-profile"]);
  expect(summary.pendingAddons).toEqual(["customer-profile"]);
  expect(summary.serviceAction?.command).toBe(
    "lenso service start customer-profile api"
  );
});
```

- [ ] **Step 4: Update page copy and panels**

In `launchpad-page.tsx`:

- Keep the route and component name.
- Change visible section copy from Launchpad-only wording to App Lifecycle where
  it describes the whole app.
- Add a compact Composer panel:

```tsx
<DetailSection title="composer">
  <CommandLine value={changePlan.nextCommand} />
  {changePlan.pendingAddons.length ? (
    <p>{changePlan.pendingAddons.join(", ")}</p>
  ) : (
    <p>No pending addons.</p>
  )}
</DetailSection>
```

- Add a Service Ops panel that shows the first service action and links to
  `/services` when present.
- Add an Agent panel that shows the first agent action command.

- [ ] **Step 5: Run focused Console checks**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-runtime-console
pnpm exec oxfmt --write src/pages/available-modules-model.ts src/data/available-modules.ts src/data/available-modules.test.ts src/pages/launchpad-model.ts src/pages/launchpad-model.test.ts src/pages/launchpad-page.tsx
pnpm exec vitest run src/data/available-modules.test.ts src/pages/launchpad-model.test.ts
pnpm typecheck:local
```

Expected: model tests and TypeScript pass.

- [ ] **Step 6: Commit**

Run:

```sh
git add src/pages/available-modules-model.ts src/data/available-modules.ts src/data/available-modules.test.ts src/pages/launchpad-model.ts src/pages/launchpad-model.test.ts src/pages/launchpad-page.tsx
git commit -m "feat: surface app composer lifecycle"
```

## Task 7: Examples Fixture

**Files:**
- Create: `/Users/leosouthey/Projects/framework/lenso-examples/fixtures/launchpad/support-desk-composer/launchpad.json`
- Create: `/Users/leosouthey/Projects/framework/lenso-examples/fixtures/launchpad/support-desk-composer/dev-doctor.json`
- Create: `/Users/leosouthey/Projects/framework/lenso-examples/fixtures/launchpad/support-desk-composer/app-proof.json`
- Create: `/Users/leosouthey/Projects/framework/lenso-examples/fixtures/launchpad/support-desk-composer/app-change-plan.json`
- Create: `/Users/leosouthey/Projects/framework/lenso-examples/fixtures/launchpad/support-desk-composer/agent-task.md`
- Create: `/Users/leosouthey/Projects/framework/lenso-examples/fixtures/launchpad/support-desk-composer/README.md`
- Modify: `/Users/leosouthey/Projects/framework/lenso-examples/scripts/check-launchpad-fixtures.mjs`
- Modify: `/Users/leosouthey/Projects/framework/lenso-examples/README.md`

- [ ] **Step 1: Generate fixture from local CLI**

Run:

```sh
tmp="$(mktemp -d)"
/Users/leosouthey/Projects/framework/lenso-cli/target/debug/lenso app compose "$tmp/acme-support" --blueprint support-desk --addon support-sla --addon customer-profile --apply
cd "$tmp/acme-support"
/Users/leosouthey/Projects/framework/lenso-cli/target/debug/lenso dev doctor --write-state
/Users/leosouthey/Projects/framework/lenso-cli/target/debug/lenso app verify --write-proof
/Users/leosouthey/Projects/framework/lenso-cli/target/debug/lenso app compose --repo-root . --addon support-sla --addon customer-profile --write-plan
/Users/leosouthey/Projects/framework/lenso-cli/target/debug/lenso agent task --from-app-plan "add enterprise SLA escalation" > agent-task.md
```

Expected: the generated app contains `.lenso/launchpad.json`,
`.lenso/dev-doctor.json`, `.lenso/app-proof.json`,
`.lenso/app-change-plan.json`, and `agent-task.md`.

- [ ] **Step 2: Copy fixture files**

Run:

```sh
fixture=/Users/leosouthey/Projects/framework/lenso-examples/fixtures/launchpad/support-desk-composer
mkdir -p "$fixture"
cp "$tmp/acme-support/.lenso/launchpad.json" "$fixture/launchpad.json"
cp "$tmp/acme-support/.lenso/dev-doctor.json" "$fixture/dev-doctor.json"
cp "$tmp/acme-support/.lenso/app-proof.json" "$fixture/app-proof.json"
cp "$tmp/acme-support/.lenso/app-change-plan.json" "$fixture/app-change-plan.json"
cp "$tmp/acme-support/agent-task.md" "$fixture/agent-task.md"
```

Create `README.md` with:

````md
# Support Desk Composer Fixture

Generated with:

```sh
lenso app compose ./acme-support --blueprint support-desk --addon support-sla --addon customer-profile --apply
lenso dev doctor --write-state
lenso app verify --write-proof
lenso app compose --repo-root . --addon support-sla --addon customer-profile --write-plan
lenso agent task --from-app-plan "add enterprise SLA escalation"
```
````

- [ ] **Step 3: Extend fixture checker**

In `check-launchpad-fixtures.mjs`, assert:

```js
const composerRoot = path.join(root, "support-desk-composer");
const composerPlan = readJson(path.join(composerRoot, "app-change-plan.json"));
assert(
  composerPlan.protocol === "lenso.app-change-plan.v1",
  "composer fixture uses app change plan protocol"
);
assert(
  composerPlan.composition?.protocol === "lenso.app-composition.v1",
  "composer fixture includes app composition"
);
assert(
  composerPlan.composition.requestedAddons.includes("support-sla"),
  "composer fixture requests support-sla"
);
assert(
  composerPlan.composition.requestedAddons.includes("customer-profile"),
  "composer fixture requests customer-profile"
);
const composerTask = fs.readFileSync(path.join(composerRoot, "agent-task.md"), "utf8");
assert(
  composerTask.includes("## App Change Plan"),
  "composer agent task includes app change plan"
);
assert(
  composerTask.includes("Services are out-of-process providers"),
  "composer agent task includes service boundary"
);
```

- [ ] **Step 4: Run fixture check and commit**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-examples
pnpm check:launchpad-fixtures
git diff --check
git add README.md scripts/check-launchpad-fixtures.mjs fixtures/launchpad/support-desk-composer
git commit -m "docs: add app composer launchpad fixture"
```

## Task 8: Docs And Skills

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso/skills/lenso-start/SKILL.md`
- Modify: `/Users/leosouthey/Projects/framework/lenso/skills/lenso-business-planning/SKILL.md`
- Modify: `/Users/leosouthey/Projects/framework/lenso/skills/lenso-module-authoring/SKILL.md`
- Modify: `/Users/leosouthey/Projects/framework/lenso/skills/lenso-remote-module-authoring/SKILL.md`
- Modify: `/Users/leosouthey/Projects/framework/lenso/skills/lenso-starter-host/SKILL.md`
- Modify: `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/product-blueprints.mdx`
- Modify: `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/quickstart.mdx`
- Modify: `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/cli-reference.mdx`
- Modify: `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/runtime-console.mdx`
- Modify: `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(agent)/agent-development.mdx`
- Modify: `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/troubleshooting.mdx`

- [ ] **Step 1: Update skills**

Use these exact command references:

```text
lenso app compose ./acme-support --blueprint support-desk --addon support-sla --apply
lenso app next
lenso app explain
lenso agent task --from-app-plan "add the requested business behavior"
```

Skill wording must preserve:

- Module install is for installable business capabilities.
- Service install is for out-of-process service providers.
- Kubernetes is optional.
- Do not ask agents to move Host-owned auth, runtime, queues, retries, outbox,
  Runtime Story, or Technical Operations into services.

- [ ] **Step 2: Update site docs**

Add the same user path to docs:

```sh
lenso app compose ./acme-support \
  --blueprint support-desk \
  --addon support-sla \
  --addon customer-profile \
  --apply

cd ./acme-support
lenso app next
lenso agent task --from-app-plan "add enterprise SLA escalation"
```

In CLI reference, add rows:

```md
| `lenso app compose` | Compose a Launchpad app from a blueprint and addons. |
| `lenso app next` | Print the next useful app lifecycle command. |
| `lenso app explain` | Explain generated-state, module, and service actions. |
| `lenso agent task --from-app-plan` | Include App Change Plan and composition context in agent handoff. |
```

- [ ] **Step 3: Run docs checks**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-site
pnpm types:check

cd /Users/leosouthey/Projects/framework/lenso
rg -n "lenso app compose|lenso app next|lenso agent task --from-app-plan" skills docs/superpowers/specs/2026-07-02-lenso-app-composer-v26-design.md
git diff --check
```

Expected: site type generation passes, command references exist in skills and
docs, and diff whitespace is clean.

- [ ] **Step 4: Commit**

Run:

```sh
git -C /Users/leosouthey/Projects/framework/lenso add skills
git -C /Users/leosouthey/Projects/framework/lenso commit -m "docs: teach skills app composer flow"

git -C /Users/leosouthey/Projects/framework/lenso-site add content/docs
git -C /Users/leosouthey/Projects/framework/lenso-site commit -m "docs: document app composer workflow"
```

## Task 9: Final Verification And PR Prep

**Files:**
- No source edits unless a focused check fails.

- [ ] **Step 1: Run focused repo checks**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-cli
cargo test --locked app_change_plan
cargo test --locked compose_
cargo test --locked next_action
cargo test --locked agent_context_mentions_app_composition_when_requested

cd /Users/leosouthey/Projects/framework/lenso
cargo test -p lenso-platform-admin-data launchpad_change_plan
just generated-check
just arch-check

cd /Users/leosouthey/Projects/framework/lenso-runtime-console
pnpm exec vitest run src/data/available-modules.test.ts src/pages/launchpad-model.test.ts
pnpm typecheck:local

cd /Users/leosouthey/Projects/framework/lenso-examples
pnpm check:launchpad-fixtures

cd /Users/leosouthey/Projects/framework/lenso-site
pnpm types:check
```

Expected: all focused checks pass.

- [ ] **Step 2: Run full checks only where the touched surface justifies it**

Run full checks for repos with TypeScript/UI/doc generation changes:

```sh
cd /Users/leosouthey/Projects/framework/lenso-runtime-console
pnpm check

cd /Users/leosouthey/Projects/framework/lenso-site
pnpm build
```

Do not run broad example smoke unless fixture generation or service process
startup behavior changed beyond static fixture files.

- [ ] **Step 3: Push branches**

Run:

```sh
git -C /Users/leosouthey/Projects/framework/lenso-cli push -u origin feat/app-composer-v26
git -C /Users/leosouthey/Projects/framework/lenso push -u origin feat/app-composer-v26
git -C /Users/leosouthey/Projects/framework/lenso-runtime-console push -u origin feat/app-composer-v26
git -C /Users/leosouthey/Projects/framework/lenso-examples push -u origin feat/app-composer-v26
git -C /Users/leosouthey/Projects/framework/lenso-site push -u origin feat/app-composer-v26
```

- [ ] **Step 4: Open PRs**

Use these PR titles:

```text
lenso-cli: feat: add V26 app composer
lenso: feat: expose V26 app composition state
lenso-runtime-console: feat: surface V26 app lifecycle
lenso-examples: docs: add V26 app composer fixture
lenso-site: docs: document V26 app composer
```

PR body summary:

```md
## Summary

- adds V26 App Composer flow across blueprint, addons, service ops, and agent handoff
- keeps generation/apply in the CLI and Console read-only
- keeps module install and service install as separate actions
- keeps Kubernetes optional

## Checks

- [ ] lenso-cli focused cargo tests
- [ ] lenso admin-data/generated/arch checks
- [ ] runtime-console model tests and typecheck
- [ ] examples launchpad fixture check
- [ ] site typecheck/build
```

## Self-Review

- Spec coverage: App Composer, Service Ops in app lifecycle, Agent Module Studio,
  module/service separation, Console read-only behavior, examples, skills, docs,
  and focused verification are all mapped to tasks.
- Placeholder scan: no task uses placeholder markers or unnamed future
  components.
- Type consistency: CLI uses `AppCompositionState` and Host/Console use
  matching `composition`, `requestedAddons`, `appliedAddons`, `pendingAddons`,
  `serviceActions`, and `agentActions` fields.
- Scope guard: no remote blueprint registry, marketplace, service mesh, required
  Kubernetes path, or built-in AI runtime is included.
