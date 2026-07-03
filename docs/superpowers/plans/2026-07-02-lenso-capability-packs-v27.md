# Lenso Capability Packs V27 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build V27 Capability Packs so teams can author local reusable business capabilities and compose them into Launchpad apps.

**Architecture:** Keep pack authoring and composition in `lenso-cli`; keep Host and Runtime Console read-only. Reuse existing module manifests, service manifests, App Change Plans, App Proof, and App Lifecycle state. Add only `lenso.capability.json` as the local pack manifest.

**Tech Stack:** Rust CLI, serde JSON, Rust admin-data DTOs, OpenAPI YAML, React Runtime Console, JSON examples, MDX docs, Lenso local skills.

---

## Scope Check

This plan intentionally stops before registry, signing, trust policy, remote
distribution, or Kubernetes-only deployment. The v27 product is local
Capability Packs plus Composer, Console, and Agent integration.

Implementation should start after v26 is merged. If v26 is still open, stack the
implementation branches on `feat/app-composer-v26`; otherwise start from
`origin/main`.

## File Structure

- `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`: add `capability` commands, `app compose --pack`, and `agent task --for-capability`.
- `/Users/leosouthey/Projects/framework/lenso-cli/src/launchpad.rs`: extend Composer and Agent handoff with pack-aware plan state.
- `/Users/leosouthey/Projects/framework/lenso-cli/src/capability.rs`: create the pack manifest model, `init`, `check`, and `inspect`.
- `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/dto.rs`: add optional pack composition DTO fields if Console needs typed pass-through.
- `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/handlers.rs`: parse optional pack fields from `.lenso/app-change-plan.json`.
- `/Users/leosouthey/Projects/framework/lenso/contracts/openapi/app-api.v1.yaml`: regenerate after DTO changes.
- `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/available-modules-model.ts`: add pack composition types.
- `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/data/available-modules.ts`: update sample App Change Plan data.
- `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/launchpad-model.ts`: summarize pack state.
- `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/launchpad-page.tsx`: show pack lifecycle in App Lifecycle.
- `/Users/leosouthey/Projects/framework/lenso-examples/fixtures/capabilities/support-sla-pack/`: add fixture.
- `/Users/leosouthey/Projects/framework/lenso-examples/scripts/check-launchpad-fixtures.mjs`: validate the pack fixture.
- `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/quickstart.mdx`: mention Capability Packs after Composer.
- `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/product-blueprints.mdx`: document `--pack`.
- `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/cli-reference.mdx`: add `lenso capability` rows.
- `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/runtime-console.mdx`: add pack lifecycle copy.
- `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(agent)/agent-development.mdx`: add `--for-capability`.
- `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/troubleshooting.mdx`: add pack check failures.
- `/Users/leosouthey/Projects/framework/lenso/skills/*.md`: update Lenso skills for Composer plus Capability Packs.

## Task 0: Branch Prep

- [ ] **Step 1: Confirm v26 base**

Run in each repo:

```sh
git status --short --branch
git fetch origin
```

Expected: no unrelated dirty source files. If v26 PRs are not merged, create
branches from the v26 branches. If v26 is merged, create branches from
`origin/main`.

- [ ] **Step 2: Create v27 branches**

Use the same branch name across touched repos:

```sh
feat/capability-packs-v27
```

## Task 1: Capability Pack CLI

**Files:**
- Create: `/Users/leosouthey/Projects/framework/lenso-cli/src/capability.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`

- [ ] **Step 1: Add parser tests**

Add tests in `src/main.rs`:

```rust
#[test]
fn parses_capability_init_ts() {
    let cli = Cli::parse_from([
        "lenso",
        "capability",
        "init",
        "support-sla",
        "--dir",
        "./capabilities/support-sla",
        "--lang",
        "ts",
        "--for-blueprint",
        "support-desk",
    ]);
    let Command::Capability { command } = cli.command else {
        panic!("expected capability command");
    };
    let CapabilityCommand::Init(args) = command else {
        panic!("expected capability init");
    };
    assert_eq!(args.name, "support-sla");
    assert_eq!(args.lang, "ts");
    assert_eq!(args.for_blueprint, vec!["support-desk"]);
}

#[test]
fn parses_capability_check_json() {
    let cli = Cli::parse_from([
        "lenso",
        "capability",
        "check",
        "./capabilities/support-sla",
        "--json",
    ]);
    let Command::Capability { command } = cli.command else {
        panic!("expected capability command");
    };
    let CapabilityCommand::Check(args) = command else {
        panic!("expected capability check");
    };
    assert!(args.json);
}
```

- [ ] **Step 2: Add command surface**

Add to `Command`:

```rust
/// Author and inspect local reusable capability packs.
Capability {
    #[command(subcommand)]
    command: CapabilityCommand,
},
```

Add:

```rust
#[derive(Debug, Subcommand)]
enum CapabilityCommand {
    /// Create a local capability pack.
    Init(CapabilityInitArgs),
    /// Validate a local capability pack.
    Check(CapabilityCheckArgs),
    /// Inspect a local capability pack.
    Inspect(CapabilityInspectArgs),
}

#[derive(Debug, Args, Clone)]
struct CapabilityInitArgs {
    name: String,
    #[arg(long)]
    dir: std::path::PathBuf,
    #[arg(long, default_value = "ts")]
    lang: String,
    #[arg(long = "for-blueprint")]
    for_blueprint: Vec<String>,
}

#[derive(Debug, Args, Clone)]
struct CapabilityCheckArgs {
    path: std::path::PathBuf,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args, Clone)]
struct CapabilityInspectArgs {
    path: std::path::PathBuf,
}
```

- [ ] **Step 3: Add minimal pack model**

Create `src/capability.rs` with:

```rust
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const PROTOCOL: &str = "lenso.capability-pack.v1";
const MANIFEST: &str = "lenso.capability.json";

#[derive(Debug, Clone)]
pub(crate) struct InitOptions {
    pub(crate) name: String,
    pub(crate) dir: PathBuf,
    pub(crate) lang: String,
    pub(crate) blueprints: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct CheckOptions {
    pub(crate) path: PathBuf,
    pub(crate) json: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct InspectOptions {
    pub(crate) path: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CapabilityPack {
    pub(crate) protocol: String,
    pub(crate) name: String,
    pub(crate) label: String,
    pub(crate) summary: String,
    pub(crate) supports: CapabilitySupports,
    #[serde(default)]
    pub(crate) modules: Vec<CapabilityModule>,
    #[serde(default)]
    pub(crate) services: Vec<CapabilityService>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) agent: Option<CapabilityAgent>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CapabilitySupports {
    #[serde(default)]
    pub(crate) blueprints: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CapabilityModule {
    pub(crate) name: String,
    pub(crate) manifest: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CapabilityService {
    pub(crate) provider: String,
    pub(crate) service: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) language: Option<String>,
    pub(crate) manifest: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CapabilityAgent {
    pub(crate) default_task: String,
}
```

- [ ] **Step 4: Implement init/check/inspect**

Implement:

```rust
pub(crate) fn init(options: InitOptions) -> Result<()> {
    validate_slug(&options.name)?;
    validate_lang(&options.lang)?;
    fs::create_dir_all(&options.dir)?;
    let manifest_path = options.dir.join(MANIFEST);
    if manifest_path.exists() {
        bail!("{} already exists", manifest_path.display());
    }
    let pack = CapabilityPack {
        protocol: PROTOCOL.to_owned(),
        name: options.name.clone(),
        label: title_label(&options.name),
        summary: format!("Adds {} business behavior.", options.name),
        supports: CapabilitySupports {
            blueprints: options.blueprints,
        },
        modules: vec![CapabilityModule {
            name: options.name.clone(),
            manifest: "module/lenso.module.json".to_owned(),
        }],
        services: vec![CapabilityService {
            provider: format!("{}-provider", options.name),
            service: "api".to_owned(),
            language: Some(options.lang),
            manifest: "service/lenso.service.json".to_owned(),
        }],
        agent: Some(CapabilityAgent {
            default_task: format!("add or change {} behavior", options.name),
        }),
    };
    write_json(&manifest_path, &pack)?;
    fs::write(
        options.dir.join("README.md"),
        format!("# {}\n\nLocal Lenso capability pack.\n", pack.label),
    )?;
    println!("Created capability pack {}.", pack.name);
    println!("Next: lenso capability check {}", options.dir.display());
    Ok(())
}

pub(crate) fn check(options: CheckOptions) -> Result<()> {
    let report = check_pack(&options.path)?;
    if options.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Capability pack: {}", report.name);
        for issue in &report.issues {
            println!("- {}: {}", issue.code, issue.message);
        }
        if report.issues.is_empty() {
            println!("Status: ready");
        }
    }
    if report.issues.iter().any(|issue| issue.severity == "error") {
        bail!("capability pack check failed");
    }
    Ok(())
}

pub(crate) fn inspect(options: InspectOptions) -> Result<()> {
    let pack = read_pack(&options.path)?;
    println!("{} ({})", pack.label, pack.name);
    println!("{}", pack.summary);
    println!("blueprints: {}", pack.supports.blueprints.join(", "));
    println!("modules: {}", pack.modules.iter().map(|m| m.name.as_str()).collect::<Vec<_>>().join(", "));
    println!("services: {}", pack.services.iter().map(|s| s.service.as_str()).collect::<Vec<_>>().join(", "));
    println!("Next: lenso app compose --pack {}", options.path.display());
    Ok(())
}
```

Keep helpers private. `check_pack` should reject protocol mismatch, bad slugs,
path escapes, missing manifests, duplicate module names, duplicate service keys,
and unknown languages.

- [ ] **Step 5: Dispatch commands**

Wire `CapabilityCommand` to `capability::init`, `capability::check`, and
`capability::inspect`.

- [ ] **Step 6: Add focused tests and commit**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-cli
cargo fmt
cargo test --locked parses_capability
cargo test --locked capability_pack
git add src/main.rs src/capability.rs
git commit -m "feat: add capability pack cli"
```

## Task 2: Composer Pack Integration

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/launchpad.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/capability.rs`

- [ ] **Step 1: Add parser coverage**

Add:

```rust
#[test]
fn parses_app_compose_pack() {
    let cli = Cli::parse_from([
        "lenso",
        "app",
        "compose",
        "./acme-support",
        "--blueprint",
        "support-desk",
        "--pack",
        "./capabilities/support-sla",
        "--write-plan",
    ]);
    let Command::App {
        command: AppCommand::Compose(args),
    } = cli.command else {
        panic!("expected app compose");
    };
    assert_eq!(args.packs, vec![std::path::PathBuf::from("./capabilities/support-sla")]);
}
```

- [ ] **Step 2: Add `--pack` args**

Add `packs: Vec<PathBuf>` to `AppComposeArgs`, `AppPlanArgs`, and
`AppComposeOptions`:

```rust
#[arg(long = "pack")]
packs: Vec<std::path::PathBuf>,
```

- [ ] **Step 3: Extend composition state**

Add to `AppCompositionState`:

```rust
#[serde(default)]
requested_packs: Vec<String>,
#[serde(default)]
applied_packs: Vec<String>,
#[serde(default)]
pending_packs: Vec<String>,
#[serde(default)]
capability_packs: Vec<AppCompositionCapabilityPack>,
```

Add:

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppCompositionCapabilityPack {
    name: String,
    path: String,
    status: String,
    modules: Vec<String>,
    services: Vec<String>,
    next_command: Option<String>,
}
```

- [ ] **Step 4: Plan packs as generated-state changes**

Add pack planning beside addon planning:

```rust
fn app_change_plan_for_packs(
    launchpad: &LaunchpadState,
    packs: &[PathBuf],
) -> Result<(Vec<AppChangePlanItem>, Vec<AppChangePlanItem>, Vec<AppCompositionCapabilityPack>)> {
    let mut changes = Vec::new();
    let mut blocked = Vec::new();
    let mut planned = Vec::new();
    for path in packs {
        let pack = capability::read_pack(path)?;
        if !pack.supports.blueprints.is_empty()
            && !pack.supports.blueprints.contains(&launchpad.blueprint)
        {
            blocked.push(AppChangePlanItem {
                id: format!("capability-pack:{}:blueprint", pack.name),
                kind: "capability-pack".to_owned(),
                name: pack.name.clone(),
                action: "block".to_owned(),
                safe: false,
                message: format!("pack does not support blueprint `{}`", launchpad.blueprint),
                command: Some(format!("lenso capability inspect {}", path.display())),
            });
            continue;
        }
        changes.push(AppChangePlanItem {
            id: format!("capability-pack:{}", pack.name),
            kind: "capability-pack".to_owned(),
            name: pack.name.clone(),
            action: "compose".to_owned(),
            safe: true,
            message: format!("compose capability pack `{}`", pack.name),
            command: Some(format!("lenso capability check {}", path.display())),
        });
        planned.push(AppCompositionCapabilityPack {
            name: pack.name.clone(),
            path: path.display().to_string(),
            status: "pending".to_owned(),
            modules: pack.modules.iter().map(|m| m.name.clone()).collect(),
            services: pack
                .services
                .iter()
                .map(|s| format!("{}/{}", s.provider, s.service))
                .collect(),
            next_command: Some(format!("lenso capability check {}", path.display())),
        });
    }
    Ok((changes, blocked, planned))
}
```

Also add duplicate checks against existing Launchpad modules and services.

- [ ] **Step 5: Apply pack changes conservatively**

For `app apply`, only write generated state:

- add pack names to Launchpad addons-like evidence or a new generated
  `capabilityPacks` field
- add service workspace entries only when the pack service manifest is known
- never overwrite pack source files

- [ ] **Step 6: Run tests and commit**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-cli
cargo fmt
cargo test --locked parses_app_compose_pack
cargo test --locked capability_pack_composition
cargo test --locked app_change_plan
git add src/main.rs src/launchpad.rs src/capability.rs
git commit -m "feat: compose capability packs"
```

## Task 3: Agent Pack Handoff

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/launchpad.rs`

- [ ] **Step 1: Add parser test**

Add:

```rust
#[test]
fn parses_agent_task_for_capability() {
    let cli = Cli::parse_from([
        "lenso",
        "agent",
        "task",
        "--for-capability",
        "support-sla",
        "add enterprise escalation",
    ]);
    let Command::Agent {
        command: AgentCommand::Task(args),
    } = cli.command else {
        panic!("expected agent task");
    };
    assert_eq!(args.for_capability.as_deref(), Some("support-sla"));
}
```

- [ ] **Step 2: Add args and context field**

Add `for_capability: Option<String>` to `AgentContextArgs`,
`AgentTaskArgs`, and `AgentContextOptions`.

- [ ] **Step 3: Render pack scope**

In `agent_context_markdown`, add a `## Capability Scope` section when
`for_capability` is set. Include matching pack state from the App Change Plan
composition if present.

- [ ] **Step 4: Test handoff content**

Add a unit test asserting the markdown contains:

```text
## Capability Scope
support-sla
Services are out-of-process providers
Runtime queues, retries, Outbox, Runtime Story, Technical Operations, and auth stay Host-owned.
```

- [ ] **Step 5: Run tests and commit**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-cli
cargo fmt
cargo test --locked parses_agent_task_for_capability
cargo test --locked agent_context_mentions_capability_scope
git add src/main.rs src/launchpad.rs
git commit -m "feat: add capability scoped agent handoff"
```

## Task 4: Host Admin Data Pass-Through

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/dto.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/handlers.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso/contracts/openapi/app-api.v1.yaml`

- [ ] **Step 1: Add optional DTO fields**

Extend `AdminLaunchpadCompositionDto`:

```rust
#[serde(default)]
pub requested_packs: Vec<String>,
#[serde(default)]
pub applied_packs: Vec<String>,
#[serde(default)]
pub pending_packs: Vec<String>,
#[serde(default)]
pub capability_packs: Vec<AdminLaunchpadCapabilityPackDto>,
```

Add:

```rust
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminLaunchpadCapabilityPackDto {
    pub name: String,
    pub path: String,
    pub status: String,
    #[serde(default)]
    pub modules: Vec<String>,
    #[serde(default)]
    pub services: Vec<String>,
    pub next_command: Option<String>,
}
```

- [ ] **Step 2: Parse optional pack fields**

In `handlers.rs`, map `composition.capabilityPacks` into the DTO. Missing fields
must default to empty arrays, not failed state.

- [ ] **Step 3: Add focused test**

Add a change-plan test with `requestedPacks`, `pendingPacks`, and
`capabilityPacks[0].name == "support-sla"`.

- [ ] **Step 4: Regenerate and commit**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso
cargo fmt --all
cargo test -p lenso-platform-admin-data launchpad_change_plan
just generate-contracts
just generated-check
just arch-check
git add crates/platform-admin-data/src/dto.rs crates/platform-admin-data/src/handlers.rs contracts/openapi/app-api.v1.yaml
git commit -m "feat: expose capability pack lifecycle state"
```

## Task 5: Runtime Console Pack Lifecycle

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/available-modules-model.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/data/available-modules.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/launchpad-model.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/launchpad-model.test.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/launchpad-page.tsx`

- [ ] **Step 1: Add model types**

Add:

```ts
export type LaunchpadCapabilityPack = {
  name: string;
  path: string;
  status: string;
  modules: string[];
  services: string[];
  nextCommand?: string | null;
};
```

Extend `LaunchpadComposition` with:

```ts
requestedPacks?: string[];
appliedPacks?: string[];
pendingPacks?: string[];
capabilityPacks?: LaunchpadCapabilityPack[];
```

- [ ] **Step 2: Update sample data**

Add `support-sla` pack state to `sampleLaunchpadChangePlanResponse`.

- [ ] **Step 3: Summarize pack state**

Extend `LaunchpadChangePlanSummary` with:

```ts
requestedPacks: string[];
pendingPacks: string[];
capabilityPacks: LaunchpadCapabilityPack[];
packAction: string | null;
```

Set `packAction` from the first pending pack `nextCommand`.

- [ ] **Step 4: Render a compact panel**

In `launchpad-page.tsx`, add a Capability Packs panel inside App Lifecycle. Show:

- pending pack names
- first next command
- empty state text when no packs are requested

- [ ] **Step 5: Run checks and commit**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-runtime-console
pnpm exec oxfmt --write src/pages/available-modules-model.ts src/data/available-modules.ts src/pages/launchpad-model.ts src/pages/launchpad-model.test.ts src/pages/launchpad-page.tsx
pnpm exec vitest run src/pages/launchpad-model.test.ts
pnpm typecheck:local
git add src/pages/available-modules-model.ts src/data/available-modules.ts src/pages/launchpad-model.ts src/pages/launchpad-model.test.ts src/pages/launchpad-page.tsx
git commit -m "feat: show capability packs in app lifecycle"
```

## Task 6: Examples Fixture

**Files:**
- Create: `/Users/leosouthey/Projects/framework/lenso-examples/fixtures/capabilities/support-sla-pack/lenso.capability.json`
- Create: `/Users/leosouthey/Projects/framework/lenso-examples/fixtures/capabilities/support-sla-pack/README.md`
- Create: `/Users/leosouthey/Projects/framework/lenso-examples/fixtures/capabilities/support-sla-pack/module/lenso.module.json`
- Create: `/Users/leosouthey/Projects/framework/lenso-examples/fixtures/capabilities/support-sla-pack/service/lenso.service.json`
- Create: `/Users/leosouthey/Projects/framework/lenso-examples/fixtures/capabilities/support-sla-pack/app-change-plan.json`
- Create: `/Users/leosouthey/Projects/framework/lenso-examples/fixtures/capabilities/support-sla-pack/agent-task.md`
- Modify: `/Users/leosouthey/Projects/framework/lenso-examples/scripts/check-launchpad-fixtures.mjs`
- Modify: `/Users/leosouthey/Projects/framework/lenso-examples/README.md`

- [ ] **Step 1: Generate fixture**

Use the local CLI:

```sh
tmp="$(mktemp -d)"
lenso capability init support-sla --dir "$tmp/support-sla-pack" --lang ts --for-blueprint support-desk
lenso capability check "$tmp/support-sla-pack"
lenso app compose "$tmp/acme-support" --blueprint support-desk --pack "$tmp/support-sla-pack" --apply
cd "$tmp/acme-support"
lenso app compose --repo-root . --pack "$tmp/support-sla-pack" --write-plan
lenso agent task --for-capability support-sla "add enterprise SLA escalation" > "$tmp/support-sla-pack/agent-task.md"
```

- [ ] **Step 2: Copy fixture files**

Copy the pack manifest, README, sample module/service manifests, generated App
Change Plan, and agent task output into
`fixtures/capabilities/support-sla-pack/`.

- [ ] **Step 3: Extend checker**

Assert:

```js
assert(pack.protocol === "lenso.capability-pack.v1", "pack protocol");
assert(pack.name === "support-sla", "pack name");
assert(pack.supports.blueprints.includes("support-desk"), "pack blueprint");
assert(plan.composition.pendingPacks.includes("support-sla"), "pending pack");
assert(task.includes("## Capability Scope"), "agent capability scope");
```

- [ ] **Step 4: Run check and commit**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-examples
pnpm check:launchpad-fixtures
git diff --check
git add README.md scripts/check-launchpad-fixtures.mjs fixtures/capabilities/support-sla-pack
git commit -m "docs: add capability pack fixture"
```

## Task 7: Docs And Skills

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso/skills/lenso-start/SKILL.md`
- Modify: `/Users/leosouthey/Projects/framework/lenso/skills/lenso-business-planning/SKILL.md`
- Modify: `/Users/leosouthey/Projects/framework/lenso/skills/lenso-module-authoring/SKILL.md`
- Modify: `/Users/leosouthey/Projects/framework/lenso/skills/lenso-remote-module-authoring/SKILL.md`
- Modify: `/Users/leosouthey/Projects/framework/lenso/skills/lenso-starter-host/SKILL.md`
- Modify: `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/quickstart.mdx`
- Modify: `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/product-blueprints.mdx`
- Modify: `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/cli-reference.mdx`
- Modify: `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/runtime-console.mdx`
- Modify: `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(agent)/agent-development.mdx`
- Modify: `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/troubleshooting.mdx`

- [ ] **Step 1: Update command references**

Use this path in docs and skills:

```sh
lenso capability init support-sla --dir ./capabilities/support-sla --lang ts --for-blueprint support-desk
lenso capability check ./capabilities/support-sla
lenso app compose ./acme-support --blueprint support-desk --pack ./capabilities/support-sla --apply
lenso agent task --for-capability support-sla "add enterprise SLA escalation"
```

- [ ] **Step 2: Preserve boundary language**

Docs and skills must say:

- Capability Pack is local authoring metadata.
- Module install is still for business capabilities.
- Service install is still for out-of-process providers.
- Host-owned auth, queues, retries, outbox, Runtime Story, and Technical
  Operations stay in the host.
- Kubernetes is optional.

- [ ] **Step 3: Run docs checks and commit**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-site
pnpm types:check

cd /Users/leosouthey/Projects/framework/lenso
rg -n "lenso capability|--pack|--for-capability" skills
git diff --check

git -C /Users/leosouthey/Projects/framework/lenso add skills
git -C /Users/leosouthey/Projects/framework/lenso commit -m "docs: teach capability pack workflow"

git -C /Users/leosouthey/Projects/framework/lenso-site add content/docs
git -C /Users/leosouthey/Projects/framework/lenso-site commit -m "docs: document capability packs"
```

## Task 8: Final Verification And PRs

- [ ] **Step 1: Run focused checks**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-cli
cargo test --locked capability_pack
cargo test --locked parses_capability
cargo test --locked parses_app_compose_pack
cargo test --locked parses_agent_task_for_capability
cargo test --locked app_change_plan

cd /Users/leosouthey/Projects/framework/lenso
cargo test -p lenso-platform-admin-data launchpad_change_plan
just generated-check
just arch-check

cd /Users/leosouthey/Projects/framework/lenso-runtime-console
pnpm exec vitest run src/pages/launchpad-model.test.ts
pnpm typecheck:local

cd /Users/leosouthey/Projects/framework/lenso-examples
pnpm check:launchpad-fixtures

cd /Users/leosouthey/Projects/framework/lenso-site
pnpm types:check
```

- [ ] **Step 2: Run full checks where touched surface justifies it**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-runtime-console
pnpm check

cd /Users/leosouthey/Projects/framework/lenso-site
pnpm build
```

- [ ] **Step 3: Push and open PRs**

Open PRs with:

```text
lenso-cli: feat: add V27 capability packs
lenso: feat: expose V27 capability pack lifecycle
lenso-runtime-console: feat: show V27 capability packs
lenso-examples: docs: add V27 capability pack fixture
lenso-site: docs: document V27 capability packs
```

## Self-Review

- Spec coverage: local pack authoring, Composer integration, Agent handoff,
  Console visibility, examples, skills, docs, and focused verification are all
  mapped to tasks.
- Placeholder scan: no task depends on an unnamed future registry, trust system,
  or marketplace.
- Scope guard: no signing, remote catalog, service mesh, gateway, required
  Kubernetes path, or browser-side apply is included.
- Boundary check: module install and service install stay separate, and
  Host-owned runtime boundaries stay explicit.
