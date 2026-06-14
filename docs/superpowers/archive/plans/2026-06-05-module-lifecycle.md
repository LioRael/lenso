# Module Lifecycle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add host-owned module lifecycle declarations and worker startup activation jobs without introducing arbitrary module startup hooks.

**Architecture:** `platform-module` owns lifecycle declaration data and manifest lints. `app-bootstrap` validates declarations and enqueues activation runtime functions. `apps/worker` calls the executor before its normal relay/runtime loop so lifecycle work is durable, retried, and visible through existing runtime surfaces.

**Tech Stack:** Rust, serde, utoipa, Axum/OpenAPI generation, SQLx Postgres tests, generated TypeScript SDK.

---

## File Structure

- Create `crates/platform-module/src/lifecycle.rs`: lifecycle declaration structs and enums.
- Modify `crates/platform-module/src/lib.rs`: re-export lifecycle types.
- Modify `crates/platform-module/src/manifest.rs`: add `ModuleManifest.lifecycle`, builder support, and lifecycle lint rules.
- Modify `docs/architecture/module-manifest-lints.md`: document lifecycle lint subjects.
- Modify `crates/platform-admin-data/src/lib.rs`: include lifecycle declarations in `AdminModuleMetadata`.
- Modify `crates/platform-admin-data/src/dto.rs`: include lifecycle declarations in `/admin/data/modules`.
- Modify `crates/platform-admin-data/src/handlers.rs`: pass lifecycle data into manifest linting.
- Modify `crates/app-bootstrap/Cargo.toml`: add `serde_json` runtime dependency and test dependencies.
- Modify `crates/app-bootstrap/src/lib.rs`: carry lifecycle metadata and add `enqueue_lifecycle_activation_jobs`.
- Modify `apps/worker/src/main.rs`: enqueue activation jobs after module loading and function registry construction.
- Modify `tools/arch-check/src/lib.rs`: add lifecycle lint messages to the backend-owned lint message catalog.
- Modify `apps/runtime-console/src/pages/data-render-model.ts`: group lifecycle lint subjects.
- Modify `apps/runtime-console/src/pages/data-render-model.test.ts`: cover lifecycle lint categorization.
- Modify `crates/platform-module-remote/tests/remote_source.rs`: prove remote manifests deserialize lifecycle declarations.
- Modify `examples/remote-module/src/lib.rs`: expose lifecycle declarations from the remote module example fixture.
- Modify `apps/api/tests/remote_module_smoke.rs`: prove module metadata exposes lifecycle declarations.
- Run `just generate` after OpenAPI-visible DTO changes; generated artifacts under `contracts/` and `packages/ts-sdk/src/generated/` must be committed with the source changes.

---

### Task 1: Add Lifecycle Contracts And Manifest Lints

**Files:**
- Create: `crates/platform-module/src/lifecycle.rs`
- Modify: `crates/platform-module/src/lib.rs`
- Modify: `crates/platform-module/src/manifest.rs`
- Test: `crates/platform-module/src/manifest.rs`

- [ ] **Step 1: Write failing lifecycle manifest tests**

Add these imports inside `#[cfg(test)] mod tests` in `crates/platform-module/src/manifest.rs`:

```rust
use crate::{
    LifecycleActivationJobDeclaration, LifecycleActivationRunPolicy,
    LifecycleStartupCheckDeclaration, LifecycleStartupCheckKind, LifecycleSurface,
};
```

Add these tests before `manifest_lint_catalog_covers_current_subjects`:

```rust
#[test]
fn manifest_with_lifecycle_round_trips_through_json() {
    let manifest = ModuleManifest::builder("remote-crm")
        .runtime(RuntimeSurface {
            functions: vec![RuntimeFunctionDeclaration {
                name: "remote_crm.warm_contact_cache.v1".to_owned(),
                version: 1,
                queue: "remote-crm".to_owned(),
                input_schema: Some("remote_crm.warm_contact_cache.v1".to_owned()),
                retry_policy: Some(RuntimeRetryPolicyDeclaration {
                    max_attempts: 2,
                    initial_delay_ms: 500,
                }),
            }],
        })
        .lifecycle(LifecycleSurface {
            startup_checks: vec![LifecycleStartupCheckDeclaration {
                name: "warm cache function is registered".to_owned(),
                required: true,
                check: LifecycleStartupCheckKind::FunctionRegistered {
                    function_name: "remote_crm.warm_contact_cache.v1".to_owned(),
                },
            }],
            activation_jobs: vec![LifecycleActivationJobDeclaration {
                name: "warm contact cache".to_owned(),
                function_name: "remote_crm.warm_contact_cache.v1".to_owned(),
                run_policy: LifecycleActivationRunPolicy::EveryStartup,
                input: serde_json::json!({ "reason": "worker_startup" }),
                required: true,
            }],
        })
        .build();

    let json = serde_json::to_string(&manifest).expect("serialize");

    assert!(json.contains(r#""lifecycle""#), "got {json}");
    assert!(
        json.contains(r#""kind":"function_registered""#),
        "got {json}"
    );
    assert!(
        json.contains(r#""run_policy":"every_startup""#),
        "got {json}"
    );
    let back: ModuleManifest = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(manifest, back);
}

#[test]
fn manifest_lint_flags_lifecycle_declarations_that_cannot_run() {
    let manifest = ModuleManifest::builder("remote-crm")
        .runtime(RuntimeSurface { functions: vec![] })
        .lifecycle(LifecycleSurface {
            startup_checks: vec![
                LifecycleStartupCheckDeclaration {
                    name: "".to_owned(),
                    required: true,
                    check: LifecycleStartupCheckKind::FunctionRegistered {
                        function_name: "remote_crm.missing.v1".to_owned(),
                    },
                },
                LifecycleStartupCheckDeclaration {
                    name: "missing capability".to_owned(),
                    required: true,
                    check: LifecycleStartupCheckKind::CapabilityDeclared {
                        capability: "remote_crm.contacts.read".to_owned(),
                    },
                },
            ],
            activation_jobs: vec![LifecycleActivationJobDeclaration {
                name: "warm contact cache".to_owned(),
                function_name: "remote_crm.warm_contact_cache.v1".to_owned(),
                run_policy: LifecycleActivationRunPolicy::EveryStartup,
                input: serde_json::json!({}),
                required: true,
            }],
        })
        .build();

    let lints = lint_module_manifest(ModuleSource::Remote, &manifest);

    assert!(lints.iter().any(|lint| {
        lint.subject == "lifecycle.startup_check"
            && lint.severity == ModuleManifestLintSeverity::Warning
            && lint.message == "Lifecycle startup check is missing a name."
    }));
    assert!(lints.iter().any(|lint| {
        lint.subject == "lifecycle.startup_check.function_registered.remote_crm.missing.v1"
            && lint.severity == ModuleManifestLintSeverity::Error
    }));
    assert!(lints.iter().any(|lint| {
        lint.subject == "lifecycle.startup_check.capability.remote_crm.contacts.read"
            && lint.severity == ModuleManifestLintSeverity::Warning
    }));
    assert!(lints.iter().any(|lint| {
        lint.subject == "lifecycle.activation_job.warm contact cache"
            && lint.severity == ModuleManifestLintSeverity::Error
    }));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test --locked -p platform-module manifest_with_lifecycle_round_trips_through_json -- --exact
```

Expected: FAIL because lifecycle types and `ModuleManifestBuilder::lifecycle` do not exist.

- [ ] **Step 3: Add lifecycle declaration types**

Create `crates/platform-module/src/lifecycle.rs`:

```rust
//! Host-owned lifecycle declarations for module manifests.
//!
//! Lifecycle entries are data, not startup callbacks. The host validates these
//! declarations and schedules runtime-owned work during worker startup.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LifecycleSurface {
    #[serde(default)]
    pub startup_checks: Vec<LifecycleStartupCheckDeclaration>,
    #[serde(default)]
    pub activation_jobs: Vec<LifecycleActivationJobDeclaration>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LifecycleStartupCheckDeclaration {
    pub name: String,
    #[serde(default)]
    pub required: bool,
    #[serde(flatten)]
    pub check: LifecycleStartupCheckKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum LifecycleStartupCheckKind {
    FunctionRegistered { function_name: String },
    CapabilityDeclared { capability: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LifecycleActivationJobDeclaration {
    pub name: String,
    pub function_name: String,
    #[serde(default = "default_every_startup")]
    pub run_policy: LifecycleActivationRunPolicy,
    #[serde(default)]
    pub input: Value,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum LifecycleActivationRunPolicy {
    EveryStartup,
}

fn default_every_startup() -> LifecycleActivationRunPolicy {
    LifecycleActivationRunPolicy::EveryStartup
}
```

Modify `crates/platform-module/src/lib.rs`:

```rust
mod lifecycle;
```

and add this re-export near the other `pub use` lines:

```rust
pub use lifecycle::{
    LifecycleActivationJobDeclaration, LifecycleActivationRunPolicy,
    LifecycleStartupCheckDeclaration, LifecycleStartupCheckKind, LifecycleSurface,
};
```

- [ ] **Step 4: Wire lifecycle into `ModuleManifest` and linting**

Modify imports in `crates/platform-module/src/manifest.rs`:

```rust
use crate::lifecycle::{
    LifecycleActivationJobDeclaration, LifecycleStartupCheckDeclaration,
    LifecycleStartupCheckKind, LifecycleSurface,
};
```

Add this field after `runtime` in `ModuleManifest`:

```rust
/// Declared lifecycle work. The host validates and schedules these entries;
/// modules do not receive arbitrary startup callbacks.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub lifecycle: Option<LifecycleSurface>,
```

Initialize it in `ModuleManifest::builder`:

```rust
lifecycle: None,
```

Pass it through `lint_module_manifest` and `lint_module_manifest_parts`:

```rust
manifest.lifecycle.as_ref(),
```

Change the `lint_module_manifest_parts` signature to include lifecycle after runtime:

```rust
pub fn lint_module_manifest_parts(
    source: ModuleSource,
    name: &str,
    admin: Option<&AdminSurface>,
    http_routes: &[ModuleHttpRoute],
    runtime: Option<&RuntimeSurface>,
    lifecycle: Option<&LifecycleSurface>,
    capabilities: &[String],
) -> Vec<ModuleManifestLint> {
```

Call lifecycle linting after runtime linting:

```rust
if let Some(lifecycle) = lifecycle {
    lint_lifecycle_surface(lifecycle, runtime, capabilities, &mut lints);
}
```

Add this builder method before `build`:

```rust
/// Attach lifecycle declarations.
#[must_use]
pub fn lifecycle(mut self, lifecycle: LifecycleSurface) -> Self {
    self.manifest.lifecycle = Some(lifecycle);
    self
}
```

Add these helper functions near `lint_runtime_surface`:

```rust
fn lint_lifecycle_surface(
    lifecycle: &LifecycleSurface,
    runtime: Option<&RuntimeSurface>,
    capabilities: &[String],
    lints: &mut Vec<ModuleManifestLint>,
) {
    if lifecycle.startup_checks.is_empty() && lifecycle.activation_jobs.is_empty() {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: "lifecycle".to_owned(),
            message: "Lifecycle surface declares no startup checks or activation jobs.".to_owned(),
            suggestion: "Add lifecycle entries or omit the lifecycle surface.".to_owned(),
        });
        return;
    }

    let runtime_functions = runtime_function_names(runtime);
    let capability_names = capabilities.iter().cloned().collect::<HashSet<_>>();

    for check in &lifecycle.startup_checks {
        lint_lifecycle_startup_check(check, &runtime_functions, &capability_names, lints);
    }

    for job in &lifecycle.activation_jobs {
        lint_lifecycle_activation_job(job, &runtime_functions, lints);
    }
}

fn lint_lifecycle_startup_check(
    check: &LifecycleStartupCheckDeclaration,
    runtime_functions: &HashSet<String>,
    capabilities: &HashSet<String>,
    lints: &mut Vec<ModuleManifestLint>,
) {
    if !present(&check.name) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: "lifecycle.startup_check".to_owned(),
            message: "Lifecycle startup check is missing a name.".to_owned(),
            suggestion: "Set a short operator-facing check name.".to_owned(),
        });
    }

    match &check.check {
        LifecycleStartupCheckKind::FunctionRegistered { function_name } => {
            if !runtime_functions.contains(function_name) {
                lints.push(ModuleManifestLint {
                    severity: ModuleManifestLintSeverity::Error,
                    subject: format!("lifecycle.startup_check.function_registered.{function_name}"),
                    message: "Lifecycle startup check references an unknown runtime function."
                        .to_owned(),
                    suggestion:
                        "Declare the function in ModuleManifest.runtime.functions or remove the check."
                            .to_owned(),
                });
            }
        }
        LifecycleStartupCheckKind::CapabilityDeclared { capability } => {
            if !capabilities.contains(capability) {
                lints.push(ModuleManifestLint {
                    severity: ModuleManifestLintSeverity::Warning,
                    subject: format!("lifecycle.startup_check.capability.{capability}"),
                    message: "Lifecycle startup check references an undeclared capability."
                        .to_owned(),
                    suggestion:
                        "Add the capability to ModuleManifest.capabilities or update the check."
                            .to_owned(),
                });
            }
        }
    }
}

fn lint_lifecycle_activation_job(
    job: &LifecycleActivationJobDeclaration,
    runtime_functions: &HashSet<String>,
    lints: &mut Vec<ModuleManifestLint>,
) {
    let subject = if present(&job.name) {
        format!("lifecycle.activation_job.{}", job.name)
    } else {
        "lifecycle.activation_job".to_owned()
    };

    if !present(&job.name) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: subject.clone(),
            message: "Lifecycle activation job is missing a name.".to_owned(),
            suggestion: "Set a short operator-facing activation job name.".to_owned(),
        });
    }

    if !present(&job.function_name) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject,
            message: "Lifecycle activation job is missing a function name.".to_owned(),
            suggestion: "Set function_name to a declared runtime function.".to_owned(),
        });
    } else if !runtime_functions.contains(&job.function_name) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject,
            message: "Lifecycle activation job references an unknown runtime function."
                .to_owned(),
            suggestion:
                "Declare the function in ModuleManifest.runtime.functions or remove the activation job."
                    .to_owned(),
        });
    }
}

fn runtime_function_names(runtime: Option<&RuntimeSurface>) -> HashSet<String> {
    runtime
        .into_iter()
        .flat_map(|surface| surface.functions.iter())
        .map(|function| function.name.clone())
        .collect()
}
```

Update every destructuring of `ModuleManifest` in this file by adding `lifecycle` where all fields are listed, or use `..` where the value is intentionally ignored.

- [ ] **Step 5: Update the manifest lint catalog test**

In `manifest_lint_catalog_covers_current_subjects`, add lifecycle declarations to the manifest builder before `.build()`:

```rust
.lifecycle(LifecycleSurface {
    startup_checks: vec![LifecycleStartupCheckDeclaration {
        name: "missing function".to_owned(),
        required: true,
        check: LifecycleStartupCheckKind::FunctionRegistered {
            function_name: "remote_crm.missing.v1".to_owned(),
        },
    }],
    activation_jobs: vec![LifecycleActivationJobDeclaration {
        name: "missing activation".to_owned(),
        function_name: "remote_crm.missing.v1".to_owned(),
        run_policy: LifecycleActivationRunPolicy::EveryStartup,
        input: serde_json::json!({}),
        required: true,
    }],
})
```

Add these expected entries before the final `runtime...` entries in the expected catalog:

```rust
(
    ModuleManifestLintSeverity::Error,
    "lifecycle.startup_check.function_registered.remote_crm.missing.v1".to_owned(),
),
(
    ModuleManifestLintSeverity::Error,
    "lifecycle.activation_job.missing activation".to_owned(),
),
```

- [ ] **Step 6: Run platform-module tests**

Run:

```bash
cargo test --locked -p platform-module
```

Expected: PASS.

- [ ] **Step 7: Commit Task 1**

```bash
git add crates/platform-module/src/lifecycle.rs crates/platform-module/src/lib.rs crates/platform-module/src/manifest.rs
git commit -m "feat(module): add lifecycle manifest declarations"
```

---

### Task 2: Expose Lifecycle Metadata And Lints Through Module Metadata

**Files:**
- Modify: `crates/platform-admin-data/src/lib.rs`
- Modify: `crates/platform-admin-data/src/dto.rs`
- Modify: `crates/platform-admin-data/src/handlers.rs`
- Modify: `crates/app-bootstrap/src/lib.rs`
- Modify: `docs/architecture/module-manifest-lints.md`
- Modify: `tools/arch-check/src/lib.rs`
- Modify: `apps/runtime-console/src/pages/data-render-model.ts`
- Modify: `apps/runtime-console/src/pages/data-render-model.test.ts`
- Test: `crates/platform-admin-data/src/handlers.rs`
- Test: `apps/runtime-console/src/pages/data-render-model.test.ts`

- [ ] **Step 1: Write failing metadata lint test**

Update imports in the `#[cfg(test)] mod tests` block in `crates/platform-admin-data/src/handlers.rs`:

```rust
use platform_module::{
    LifecycleActivationJobDeclaration, LifecycleActivationRunPolicy, LifecycleSurface,
    ModuleHttpMethod, ModuleHttpRoute, ModuleSource,
};
```

In `metadata_response_includes_manifest_lints`, add this field to `AdminModuleMetadata`:

```rust
lifecycle: Some(LifecycleSurface {
    startup_checks: Vec::new(),
    activation_jobs: vec![LifecycleActivationJobDeclaration {
        name: "warm contact cache".to_owned(),
        function_name: "remote_crm.warm_contact_cache.v1".to_owned(),
        run_policy: LifecycleActivationRunPolicy::EveryStartup,
        input: serde_json::json!({}),
        required: true,
    }],
}),
```

Change the assertions to:

```rust
assert!(modules[0].lifecycle.is_some());
assert!(modules[0].manifest_lints.iter().any(|lint| {
    lint.subject == "lifecycle.activation_job.warm contact cache"
        && lint.message == "Lifecycle activation job references an unknown runtime function."
}));
assert!(modules[0].manifest_lints.iter().any(|lint| {
    lint.message == "Missing capability declaration for host proxy authorization."
}));
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test --locked -p platform-admin-data metadata_response_includes_manifest_lints -- --exact
```

Expected: FAIL because `AdminModuleMetadata.lifecycle` and `AdminModuleMetadataDto.lifecycle` do not exist.

- [ ] **Step 3: Add lifecycle to admin metadata types**

Modify imports in `crates/platform-admin-data/src/lib.rs`:

```rust
use platform_module::{
    AdminDataSource, AdminSchema, AdminSurface, LifecycleSurface, ModuleHttpRoute,
    ModuleLoadStatus, ModuleSource, RuntimeSurface,
};
```

Add this field to `AdminModuleMetadata` after `runtime`:

```rust
/// Declared lifecycle checks and activation jobs. Metadata only; worker startup
/// owns validation and enqueueing.
pub lifecycle: Option<LifecycleSurface>,
```

Modify imports in `crates/platform-admin-data/src/dto.rs`:

```rust
use platform_module::{
    AdminSchema, AdminSurface, LifecycleSurface, ModuleHttpRoute, ModuleManifestLint,
    ModuleSource, RuntimeSurface,
};
```

Add this field to `AdminModuleMetadataDto` after `runtime`:

```rust
pub lifecycle: Option<LifecycleSurface>,
```

- [ ] **Step 4: Pass lifecycle through handlers and app-bootstrap metadata**

In `crates/platform-admin-data/src/handlers.rs`, set the DTO field:

```rust
lifecycle: m.lifecycle.clone(),
```

Pass lifecycle into `lint_module_manifest_parts`:

```rust
m.lifecycle.as_ref(),
```

In `crates/app-bootstrap/src/lib.rs`, update the `ModuleManifest` destructuring in `admin_metadata_from_modules`:

```rust
let ModuleManifest {
    name,
    admin,
    http_routes,
    runtime,
    lifecycle,
    story_display,
    capabilities,
    ..
} = module.manifest;
```

Set the metadata field:

```rust
lifecycle,
```

In `failed_remote_admin_metadata`, add:

```rust
lifecycle: None,
```

- [ ] **Step 5: Update `lint_module_manifest_parts` call sites**

Update `crates/platform-admin-data/src/handlers.rs` to match the new signature:

```rust
manifest_lints: lint_module_manifest_parts(
    m.source,
    &m.module_name,
    m.admin.as_ref(),
    &m.http_routes,
    m.runtime.as_ref(),
    m.lifecycle.as_ref(),
    &m.capabilities,
),
```

Search for remaining call sites:

```bash
rg -n "lint_module_manifest_parts" crates apps domains tools -g '*.rs'
```

Expected: only the definition and the updated handler call remain.

- [ ] **Step 6: Document lifecycle lint subjects**

Add `lifecycle...` to the subject category table in `docs/architecture/module-manifest-lints.md`:

```markdown
| `lifecycle...` | `lifecycle` |
```

Add these rows to the current catalog before the final `manifest` row:

```markdown
| `warning` | `lifecycle` | Lifecycle surface declares no startup checks or activation jobs. |
| `warning` | `lifecycle.startup_check` | Lifecycle startup check is missing a name. |
| `error` | `lifecycle.startup_check.function_registered.{function}` | Lifecycle startup check references an unknown runtime function. |
| `warning` | `lifecycle.startup_check.capability.{capability}` | Lifecycle startup check references an undeclared capability. |
| `warning` | `lifecycle.activation_job` | Lifecycle activation job is missing a name. |
| `error` | `lifecycle.activation_job` | Lifecycle activation job is missing a function name. |
| `error` | `lifecycle.activation_job.{job}` | Lifecycle activation job references an unknown runtime function. |
```

- [ ] **Step 7: Add lifecycle lint messages to arch-check ownership guard**

In `tools/arch-check/src/lib.rs`, append these strings to `MANIFEST_LINT_MESSAGES`:

```rust
"Lifecycle surface declares no startup checks or activation jobs.",
"Lifecycle startup check is missing a name.",
"Lifecycle startup check references an unknown runtime function.",
"Lifecycle startup check references an undeclared capability.",
"Lifecycle activation job is missing a name.",
"Lifecycle activation job is missing a function name.",
"Lifecycle activation job references an unknown runtime function.",
```

- [ ] **Step 8: Add lifecycle metadata to the Runtime Console model**

In `apps/runtime-console/src/pages/data-render-model.ts`, add these types after `RuntimeFunctionDeclaration`:

```ts
export type LifecycleStartupCheck =
  | {
      kind: "function_registered";
      name: string;
      required?: boolean;
      function_name: string;
    }
  | {
      kind: "capability_declared";
      name: string;
      required?: boolean;
      capability: string;
    };

export type LifecycleActivationJob = {
  name: string;
  function_name: string;
  run_policy?: "every_startup";
  input?: unknown;
  required?: boolean;
};

export type LifecycleSurface = {
  startup_checks?: LifecycleStartupCheck[];
  activation_jobs?: LifecycleActivationJob[];
};
```

Add this field to `AdminModuleMetadata` after `runtime`:

```ts
lifecycle: LifecycleSurface | null;
```

In `apps/runtime-console/src/pages/data-render-model.test.ts`, update the `moduleMetadata` helper omitted/default field lists from:

```ts
"capabilities" | "manifest_lints" | "runtime" | "story_display"
```

to:

```ts
"capabilities" | "lifecycle" | "manifest_lints" | "runtime" | "story_display"
```

and add the default value:

```ts
lifecycle: null,
```

- [ ] **Step 9: Group lifecycle lints in the Runtime Console model**

In `apps/runtime-console/src/pages/data-render-model.ts`, add this branch to `manifestLintCategory` after the runtime branch and before the module branch:

```ts
if (subject === "lifecycle" || subject.startsWith("lifecycle.")) {
  return "lifecycle";
}
```

In `apps/runtime-console/src/pages/data-render-model.test.ts`, add this assertion to `classifies manifest lint subjects`:

```ts
expect(manifestLintCategory("lifecycle")).toBe("lifecycle");
expect(
  manifestLintCategory("lifecycle.activation_job.warm contact cache")
).toBe("lifecycle");
```

- [ ] **Step 10: Run metadata and console model tests**

Run:

```bash
cargo test --locked -p platform-admin-data metadata_response_includes_manifest_lints -- --exact
cargo test --locked -p app-bootstrap linked_module_entry_names_match_manifests -- --exact
pnpm --dir apps/runtime-console test src/pages/data-render-model.test.ts
```

Expected: PASS.

- [ ] **Step 11: Commit Task 2**

```bash
git add crates/platform-admin-data/src/lib.rs crates/platform-admin-data/src/dto.rs crates/platform-admin-data/src/handlers.rs crates/app-bootstrap/src/lib.rs docs/architecture/module-manifest-lints.md tools/arch-check/src/lib.rs apps/runtime-console/src/pages/data-render-model.ts apps/runtime-console/src/pages/data-render-model.test.ts
git commit -m "feat(module): expose lifecycle metadata lints"
```

---

### Task 3: Add Host-Owned Lifecycle Activation Executor

**Files:**
- Modify: `crates/app-bootstrap/Cargo.toml`
- Modify: `crates/app-bootstrap/src/lib.rs`
- Test: `crates/app-bootstrap/src/lib.rs`

- [ ] **Step 1: Add test dependencies**

Modify `crates/app-bootstrap/Cargo.toml`:

```toml
[dependencies]
platform-core.workspace = true
platform-module.workspace = true
platform-module-remote.workspace = true
platform-admin-data.workspace = true
platform-http.workspace = true
platform-runtime.workspace = true
identity.workspace = true
notifications.workspace = true
serde_json.workspace = true

[dev-dependencies]
async-trait.workspace = true
platform-testing.workspace = true
```

- [ ] **Step 2: Write failing executor tests**

Add these imports inside the `#[cfg(test)] mod tests` block in `crates/app-bootstrap/src/lib.rs`:

```rust
use platform_core::{
    AppConfig, AppContext, AppResult, AuthConfig, DatabaseConfig, ErrorCode, ExecutionContext,
    HttpConfig, LoggingEventPublisher, ModuleSourcesConfig, PLATFORM_MIGRATIONS, ServiceConfig,
    TelemetryConfig, apply_migrations,
};
use platform_module::{
    LifecycleActivationJobDeclaration, LifecycleActivationRunPolicy,
    LifecycleStartupCheckDeclaration, LifecycleStartupCheckKind, LifecycleSurface,
};
use platform_runtime::{
    FunctionDefinition, FunctionHandler, RUNTIME_MIGRATIONS, RetryPolicy, RuntimeDescriptor,
};
use platform_testing::TestDatabase;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
```

Add these tests at the end of the test module:

```rust
#[tokio::test]
async fn lifecycle_activation_enqueue_creates_function_run() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_runtime_stack_migrations(&db).await;

    let modules = vec![lifecycle_test_module(
        "remote_crm.warm_contact_cache.v1",
        true,
    )];
    let registry = function_registry(&modules);
    let ctx = AppContext::new(
        test_config(&db),
        db.pool.clone(),
        Arc::new(LoggingEventPublisher),
    );

    let run_ids = enqueue_lifecycle_activation_jobs(&ctx, &modules, &registry)
        .await
        .expect("activation jobs should enqueue");

    assert_eq!(run_ids.len(), 1);
    let row = sqlx::query_as::<_, (String, Value, String, i32, Value)>(
        r#"
        select function_name, input_json, correlation_id, max_attempts, actor
        from runtime.function_runs
        where id = $1
        "#,
    )
    .bind(&run_ids[0])
    .fetch_one(&db.pool)
    .await
    .expect("function run should exist");

    assert_eq!(row.0, "remote_crm.warm_contact_cache.v1");
    assert_eq!(row.1["reason"], "worker_startup");
    assert_eq!(row.1["_lenso_lifecycle"]["module"], "remote-crm");
    assert_eq!(row.1["_lenso_lifecycle"]["activation_job"], "warm contact cache");
    assert_eq!(row.1["_lenso_runtime"]["correlation_id"], row.2);
    assert_eq!(row.3, 2);
    assert_eq!(row.4["kind"], "service");
    assert_eq!(row.4["service_id"], "worker");

    db.cleanup().await;
}

#[test]
fn lifecycle_activation_validation_rejects_required_missing_function() {
    let modules = vec![lifecycle_test_module(
        "remote_crm.warm_contact_cache.v1",
        true,
    )];
    let registry = FunctionRegistry::default();

    let error = validate_lifecycle_activation_jobs(&modules, &registry)
        .expect_err("missing required function should fail validation");

    assert_eq!(error.code, ErrorCode::Validation);
    assert_eq!(
        error.public_message,
        "module lifecycle declarations are invalid"
    );
    assert_eq!(
        error.details[0].field.as_deref(),
        Some("remote-crm.lifecycle.activation_jobs.warm contact cache.function_name")
    );
}

#[tokio::test]
async fn optional_missing_lifecycle_activation_is_skipped() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_runtime_stack_migrations(&db).await;

    let modules = vec![lifecycle_test_module(
        "remote_crm.warm_contact_cache.v1",
        false,
    )];
    let registry = FunctionRegistry::default();
    let ctx = AppContext::new(
        test_config(&db),
        db.pool.clone(),
        Arc::new(LoggingEventPublisher),
    );

    let run_ids = enqueue_lifecycle_activation_jobs(&ctx, &modules, &registry)
        .await
        .expect("optional missing activation should not block startup");

    assert!(run_ids.is_empty());

    db.cleanup().await;
}
```

Add these test helpers inside the same test module:

```rust
#[derive(Debug)]
struct NoopFunction;

#[async_trait::async_trait]
impl FunctionHandler for NoopFunction {
    async fn call(&self, _ctx: ExecutionContext, _input: Value) -> AppResult<Value> {
        Ok(json!({ "ok": true }))
    }
}

fn lifecycle_test_module(function_name: &str, required: bool) -> Module {
    Module::linked(
        ModuleManifest::builder("remote-crm")
            .capabilities(vec!["remote_crm.contacts.read".to_owned()])
            .runtime(RuntimeSurface {
                functions: vec![RuntimeFunctionDeclaration {
                    name: function_name.to_owned(),
                    version: 1,
                    queue: "remote-crm".to_owned(),
                    input_schema: Some(function_name.to_owned()),
                    retry_policy: Some(RuntimeRetryPolicyDeclaration {
                        max_attempts: 2,
                        initial_delay_ms: 1000,
                    }),
                }],
            })
            .lifecycle(LifecycleSurface {
                startup_checks: vec![
                    LifecycleStartupCheckDeclaration {
                        name: "warm function registered".to_owned(),
                        required,
                        check: LifecycleStartupCheckKind::FunctionRegistered {
                            function_name: function_name.to_owned(),
                        },
                    },
                    LifecycleStartupCheckDeclaration {
                        name: "read capability declared".to_owned(),
                        required,
                        check: LifecycleStartupCheckKind::CapabilityDeclared {
                            capability: "remote_crm.contacts.read".to_owned(),
                        },
                    },
                ],
                activation_jobs: vec![LifecycleActivationJobDeclaration {
                    name: "warm contact cache".to_owned(),
                    function_name: function_name.to_owned(),
                    run_policy: LifecycleActivationRunPolicy::EveryStartup,
                    input: json!({ "reason": "worker_startup" }),
                    required,
                }],
            })
            .build(),
        LinkedBinding::builder()
            .runtime(RuntimeDescriptor {
                module: "remote-crm",
                functions: vec![FunctionDefinition {
                    name: function_name.to_owned(),
                    version: 1,
                    queue: "remote-crm".to_owned(),
                    retry_policy: RetryPolicy::fixed(2, Duration::from_secs(1)),
                    handler: Arc::new(NoopFunction),
                }],
                ..RuntimeDescriptor::default()
            })
            .build(),
    )
}

async fn apply_runtime_stack_migrations(db: &TestDatabase) {
    let migrations = PLATFORM_MIGRATIONS
        .iter()
        .chain(RUNTIME_MIGRATIONS)
        .copied()
        .collect::<Vec<_>>();
    apply_migrations(&db.pool, &migrations)
        .await
        .expect("runtime migrations should apply");
}

fn test_config(db: &TestDatabase) -> AppConfig {
    AppConfig {
        service: ServiceConfig::default(),
        database: DatabaseConfig {
            url: db.url.clone(),
            max_connections: 1,
        },
        http: HttpConfig::default(),
        telemetry: TelemetryConfig::default(),
        auth: AuthConfig::default(),
        module_sources: ModuleSourcesConfig::default(),
        modules: Default::default(),
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run:

```bash
cargo test --locked -p app-bootstrap lifecycle_activation -- --nocapture
```

Expected: FAIL because `enqueue_lifecycle_activation_jobs` and `validate_lifecycle_activation_jobs` do not exist.

- [ ] **Step 4: Implement lifecycle validation and enqueueing**

Add these imports near the top of `crates/app-bootstrap/src/lib.rs`:

```rust
use platform_core::{
    ActorContext, AppContext, AppResult, CorrelationId, ErrorDetail, EventHandlerRegistry,
    RuntimeConfigDescriptor, StoryDisplayDescriptor, TraceContext,
};
use platform_module::{LifecycleStartupCheckKind, LifecycleSurface};
use platform_runtime::{EnqueueFunctionRequest, FunctionRegistry, RuntimeClient};
use serde_json::{Value, json};
use std::collections::HashSet;
```

Keep the existing imported names from those crates and remove duplicate imports introduced by this replacement.

Add this public function after `function_registry`:

```rust
/// Validate and enqueue host-owned module lifecycle activation jobs.
///
/// The host schedules runtime functions; modules do not receive arbitrary
/// startup callbacks. Optional invalid entries are logged and skipped.
pub async fn enqueue_lifecycle_activation_jobs(
    ctx: &AppContext,
    modules: &[Module],
    registry: &FunctionRegistry,
) -> AppResult<Vec<String>> {
    validate_lifecycle_activation_jobs(modules, registry)?;

    let client = RuntimeClient::new(ctx.db.clone());
    let mut run_ids = Vec::new();

    for module in modules {
        let Some(lifecycle) = module.manifest.lifecycle.as_ref() else {
            continue;
        };

        for job in &lifecycle.activation_jobs {
            let Some(definition) = registry.get(&job.function_name) else {
                tracing::warn!(
                    module = %module.manifest.name,
                    activation_job = %job.name,
                    function_name = %job.function_name,
                    "skipping optional lifecycle activation job with missing runtime function"
                );
                continue;
            };

            let request = EnqueueFunctionRequest {
                function_name: job.function_name.clone(),
                input_json: lifecycle_activation_input(&module.manifest.name, &job.name, &job.input),
                correlation_id: CorrelationId::new(ctx.ids.new_id("corr_lifecycle")),
                actor: ActorContext::Service {
                    service_id: "worker".to_owned(),
                    scopes: vec!["runtime.functions.enqueue".to_owned()],
                },
                trace: TraceContext::default(),
                causation_id: Some(format!(
                    "module_lifecycle:{}:{}",
                    module.manifest.name, job.name
                )),
                max_attempts: Some(max_attempts_for_enqueue(definition.retry_policy.max_attempts)),
            };

            match client.enqueue_function(request).await {
                Ok(run_id) => run_ids.push(run_id),
                Err(error) if !job.required => {
                    tracing::warn!(
                        error = ?error,
                        module = %module.manifest.name,
                        activation_job = %job.name,
                        function_name = %job.function_name,
                        "optional lifecycle activation job failed to enqueue"
                    );
                }
                Err(error) => return Err(error),
            }
        }
    }

    Ok(run_ids)
}
```

Add this validation function after the enqueue function:

```rust
fn validate_lifecycle_activation_jobs(
    modules: &[Module],
    registry: &FunctionRegistry,
) -> AppResult<()> {
    let mut blocking_details = Vec::new();

    for module in modules {
        let Some(lifecycle) = module.manifest.lifecycle.as_ref() else {
            continue;
        };

        let declared_functions = declared_runtime_functions(lifecycle, &module.manifest);
        validate_lifecycle_startup_checks(
            module,
            lifecycle,
            &declared_functions,
            registry,
            &mut blocking_details,
        );
        validate_lifecycle_jobs(module, lifecycle, &declared_functions, registry, &mut blocking_details);
    }

    if blocking_details.is_empty() {
        Ok(())
    } else {
        Err(platform_core::AppError::validation(
            "module lifecycle declarations are invalid",
            blocking_details,
        ))
    }
}
```

Add these helper functions below validation:

```rust
fn declared_runtime_functions(
    _lifecycle: &LifecycleSurface,
    manifest: &ModuleManifest,
) -> HashSet<String> {
    manifest
        .runtime
        .as_ref()
        .into_iter()
        .flat_map(|runtime| runtime.functions.iter())
        .map(|function| function.name.clone())
        .collect()
}

fn validate_lifecycle_startup_checks(
    module: &Module,
    lifecycle: &LifecycleSurface,
    declared_functions: &HashSet<String>,
    registry: &FunctionRegistry,
    blocking_details: &mut Vec<ErrorDetail>,
) {
    for check in &lifecycle.startup_checks {
        match &check.check {
            LifecycleStartupCheckKind::FunctionRegistered { function_name } => {
                let passes =
                    declared_functions.contains(function_name) && registry.get(function_name).is_some();
                if !passes && check.required {
                    blocking_details.push(ErrorDetail {
                        field: Some(format!(
                            "{}.lifecycle.startup_checks.{}.function_name",
                            module.manifest.name, check.name
                        )),
                        reason: format!(
                            "required lifecycle startup check references unregistered function `{function_name}`"
                        ),
                    });
                } else if !passes {
                    tracing::warn!(
                        module = %module.manifest.name,
                        startup_check = %check.name,
                        function_name = %function_name,
                        "optional lifecycle startup check failed"
                    );
                }
            }
            LifecycleStartupCheckKind::CapabilityDeclared { capability } => {
                let passes = module
                    .manifest
                    .capabilities
                    .iter()
                    .any(|declared| declared == capability);
                if !passes && check.required {
                    blocking_details.push(ErrorDetail {
                        field: Some(format!(
                            "{}.lifecycle.startup_checks.{}.capability",
                            module.manifest.name, check.name
                        )),
                        reason: format!(
                            "required lifecycle startup check references undeclared capability `{capability}`"
                        ),
                    });
                } else if !passes {
                    tracing::warn!(
                        module = %module.manifest.name,
                        startup_check = %check.name,
                        capability = %capability,
                        "optional lifecycle startup check failed"
                    );
                }
            }
        }
    }
}

fn validate_lifecycle_jobs(
    module: &Module,
    lifecycle: &LifecycleSurface,
    declared_functions: &HashSet<String>,
    registry: &FunctionRegistry,
    blocking_details: &mut Vec<ErrorDetail>,
) {
    for job in &lifecycle.activation_jobs {
        let registered =
            declared_functions.contains(&job.function_name) && registry.get(&job.function_name).is_some();
        if !registered && job.required {
            blocking_details.push(ErrorDetail {
                field: Some(format!(
                    "{}.lifecycle.activation_jobs.{}.function_name",
                    module.manifest.name, job.name
                )),
                reason: format!(
                    "required lifecycle activation job references unregistered function `{}`",
                    job.function_name
                ),
            });
        } else if !registered {
            tracing::warn!(
                module = %module.manifest.name,
                activation_job = %job.name,
                function_name = %job.function_name,
                "optional lifecycle activation job references unregistered function"
            );
        }
    }
}

fn lifecycle_activation_input(module_name: &str, job_name: &str, input: &Value) -> Value {
    let mut input = input.clone();
    let lifecycle = json!({
        "module": module_name,
        "activation_job": job_name,
    });

    match &mut input {
        Value::Object(map) => {
            map.insert("_lenso_lifecycle".to_owned(), lifecycle);
            input
        }
        _ => json!({
            "value": input,
            "_lenso_lifecycle": lifecycle,
        }),
    }
}

fn max_attempts_for_enqueue(max_attempts: u32) -> i32 {
    i32::try_from(max_attempts).unwrap_or(i32::MAX)
}
```

The helper `validate_lifecycle_activation_jobs` is private, but tests in the same file can call it.

- [ ] **Step 5: Run executor tests**

Run:

```bash
cargo test --locked -p app-bootstrap lifecycle_activation -- --nocapture
```

Expected: PASS. If `TestDatabase::create()` skips because `DATABASE_URL` is not available, note the skip and run the non-DB validation test separately:

```bash
cargo test --locked -p app-bootstrap lifecycle_activation_validation_rejects_required_missing_function -- --exact
```

- [ ] **Step 6: Commit Task 3**

```bash
git add crates/app-bootstrap/Cargo.toml crates/app-bootstrap/src/lib.rs
git commit -m "feat(module): enqueue lifecycle activation jobs"
```

---

### Task 4: Call Lifecycle Activation From Worker Startup

**Files:**
- Modify: `apps/worker/src/main.rs`

- [ ] **Step 1: Wire executor into worker startup**

In `apps/worker/src/main.rs`, after building the registry and event handlers:

```rust
let registry = app_bootstrap::function_registry(&modules);
let event_handlers = app_bootstrap::event_handlers(&modules);
let activation_run_ids = app_bootstrap::enqueue_lifecycle_activation_jobs(&ctx, &modules, &registry)
    .await
    .context("failed to enqueue module lifecycle activation jobs")?;
```

Update the startup log:

```rust
info!(
    functions = registry.all().count(),
    lifecycle_activation_jobs = activation_run_ids.len(),
    user_registered_handlers = event_handlers.handler_count("identity.user_registered.v1"),
    "starting worker"
);
```

Leave `run_worker_loop(ctx.clone(), event_handlers, Arc::new(registry)).await;` unchanged.

- [ ] **Step 2: Run worker check**

Run:

```bash
cargo check --locked -p app-worker --all-targets
```

Expected: PASS.

- [ ] **Step 3: Commit Task 4**

```bash
git add apps/worker/src/main.rs
git commit -m "feat(worker): schedule module lifecycle activation"
```

---

### Task 5: Cover Remote Metadata And Regenerate Contracts

**Files:**
- Modify: `crates/platform-module-remote/tests/remote_source.rs`
- Modify: `examples/remote-module/src/lib.rs`
- Modify: `apps/api/tests/remote_module_smoke.rs`
- Generated: `contracts/openapi/app-api.v1.yaml`
- Generated: `packages/ts-sdk/src/generated/*`

- [ ] **Step 1: Add lifecycle to the remote source fixture**

In `crates/platform-module-remote/tests/remote_source.rs`, add lifecycle to `manifest()` after `runtime`:

```rust
"lifecycle": {
    "startup_checks": [{
        "name": "sync contact function registered",
        "required": true,
        "kind": "function_registered",
        "function_name": "remote_crm.sync_contact.v1"
    }],
    "activation_jobs": [{
        "name": "sync contacts on startup",
        "function_name": "remote_crm.sync_contact.v1",
        "run_policy": "every_startup",
        "input": { "reason": "worker_startup" },
        "required": false
    }]
},
```

In `loads_manifest_and_attaches_admin_data_source`, add:

```rust
let lifecycle = module
    .manifest
    .lifecycle
    .as_ref()
    .expect("lifecycle surface");
assert_eq!(lifecycle.startup_checks.len(), 1);
assert_eq!(lifecycle.activation_jobs.len(), 1);
assert_eq!(
    lifecycle.activation_jobs[0].function_name,
    "remote_crm.sync_contact.v1"
);
```

- [ ] **Step 2: Add lifecycle to the example remote module fixture**

In `examples/remote-module/src/lib.rs`, add lifecycle imports to the `platform_module` import list:

```rust
LifecycleActivationJobDeclaration, LifecycleActivationRunPolicy,
LifecycleStartupCheckDeclaration, LifecycleStartupCheckKind, LifecycleSurface,
```

Add `.lifecycle(lifecycle_surface())` to the `manifest()` builder after `.runtime(runtime_surface())`:

```rust
.runtime(runtime_surface())
.lifecycle(lifecycle_surface())
.capabilities(vec!["remote_crm.contacts.read".to_owned()])
```

Add this helper near `runtime_surface()`:

```rust
fn lifecycle_surface() -> LifecycleSurface {
    LifecycleSurface {
        startup_checks: vec![LifecycleStartupCheckDeclaration {
            name: "sync contact function registered".to_owned(),
            required: true,
            check: LifecycleStartupCheckKind::FunctionRegistered {
                function_name: "remote_crm.sync_contact.v1".to_owned(),
            },
        }],
        activation_jobs: vec![LifecycleActivationJobDeclaration {
            name: "sync contacts on startup".to_owned(),
            function_name: "remote_crm.sync_contact.v1".to_owned(),
            run_policy: LifecycleActivationRunPolicy::EveryStartup,
            input: json!({ "reason": "worker_startup" }),
            required: false,
        }],
    }
}
```

- [ ] **Step 3: Add API metadata smoke assertion**

In `apps/api/tests/remote_module_smoke.rs`, find the test that fetches `/admin/data/modules` for the remote module metadata. Add these assertions after the module JSON is selected:

```rust
let lifecycle = remote_module["lifecycle"]
    .as_object()
    .expect("remote module lifecycle should be present");
assert_eq!(
    lifecycle["activation_jobs"][0]["function_name"],
    "remote_crm.sync_contact.v1"
);
assert_eq!(
    lifecycle["startup_checks"][0]["kind"],
    "function_registered"
);
```

- [ ] **Step 4: Run focused remote tests**

Run:

```bash
cargo test --locked -p platform-module-remote loads_manifest_and_attaches_admin_data_source -- --exact
cargo test --locked -p remote-module-example
cargo test --locked -p app-api --test remote_module_smoke remote_module_fixture_is_visible_through_admin_data_api -- --exact
```

Expected: PASS.

- [ ] **Step 5: Regenerate contracts and SDK**

Run:

```bash
just generate
```

Expected: OpenAPI and generated TS SDK update to include `LifecycleSurface` and related lifecycle types.

Then run:

```bash
just generated-check
just sdk-check
```

Expected: PASS.

- [ ] **Step 6: Commit Task 5**

```bash
git add crates/platform-module-remote/tests/remote_source.rs examples/remote-module/src/lib.rs apps/api/tests/remote_module_smoke.rs contracts packages/ts-sdk
git commit -m "feat(module): expose lifecycle contract metadata"
```

---

### Task 6: Final Verification

**Files:**
- No source changes expected unless a verification command reveals a real issue.

- [ ] **Step 1: Run formatting**

Run:

```bash
just fmt
```

Expected: source formatting completes.

- [ ] **Step 2: Run narrow Rust and generated checks**

Run:

```bash
cargo test --locked -p platform-module
cargo test --locked -p platform-admin-data
cargo test --locked -p app-bootstrap
cargo test --locked -p platform-module-remote
cargo check --locked -p app-worker --all-targets
just generated-check
just sdk-check
just arch-check
```

Expected: PASS. For DB-backed tests that skip because `DATABASE_URL` is not available, record the skip explicitly in the final handoff.

- [ ] **Step 3: Run full CI gate**

Run:

```bash
just ci
```

Expected: PASS. If dependency installation fails because of network or registry availability, do not change project files to bypass it; rerun when the environment can reach the registry and report the exact failure.

- [ ] **Step 4: Commit any formatting or generated follow-up**

If Step 1 or generated checks changed files, commit them:

```bash
git status --short
git add <changed-files-from-this-lifecycle-slice>
git commit -m "chore: finalize module lifecycle checks"
```

If there are no changes, do not create an empty commit.

- [ ] **Step 5: Final status check**

Run:

```bash
git status --short
git log --oneline -5
```

Expected: worktree contains no lifecycle-related uncommitted changes. Unrelated user files, such as `.codex/config.toml`, may remain untracked and must not be staged.
