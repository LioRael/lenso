# Module Framework Step 1 — Manifest/Binding Split — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split the compile-time `DomainDescriptor` into a serializable `ModuleManifest` (data) + a narrow `ModuleBinding` trait (behavior), with reserved seams for future loading sources and admin rendering, implementing only the `Linked` (compile-time) source.

**Architecture:** New crate `platform-module` owns the contracts (`ModuleManifest`, `ModuleBinding`, `LinkedBinding`, `Module`, `AdminSurface` placeholder). `platform-core`'s `StoryDisplayDescriptor` becomes owned + serde so it can live in a serializable manifest. Domains expose a context-free `manifest()` and a context-bound `module(ctx)`; `app-bootstrap` enumerates `Module`s, reading data from manifests and behavior from bindings. Runtime config and HTTP routing stay on their current `&'static` / Linked-only paths (deferred to later specs). The old `platform-domain` crate is deleted last.

**Tech Stack:** Rust 2024, `serde`, `axum` 0.8, `utoipa-axum`, `sqlx`; workspace quality gate is `cargo check` (not clippy).

**Migration order (each step `cargo check`-passes):** owned story-display refactor → create `platform-module` → identity adds `manifest()`/`module()` (old `domain()` kept) → notifications same → switch `app-bootstrap` + apps to `Module` → delete `platform-domain` + old `domain()` fns.

**Spec:** `docs/superpowers/specs/2026-06-03-module-framework-manifest-binding-split-design.md`

---

## File Structure

**Created:**
- `crates/platform-module/Cargo.toml` — new crate manifest (deps: `platform-core`, `platform-runtime`, `serde`, `serde_json` for tests).
- `crates/platform-module/src/lib.rs` — re-exports + crate docs.
- `crates/platform-module/src/manifest.rs` — `ModuleManifest` + `ModuleManifestBuilder`.
- `crates/platform-module/src/admin.rs` — `AdminSurface` reserved-seam enum.
- `crates/platform-module/src/binding.rs` — `ModuleBinding` trait.
- `crates/platform-module/src/linked.rs` — `LinkedBinding` + `LinkedBindingBuilder`.
- `crates/platform-module/src/module.rs` — `Module` + `Module::linked` + `with_runtime_config`.

**Modified:**
- `crates/platform-core/src/story_display.rs` — `StoryDisplayDescriptor`/`StoryDisplaySource` → owned `String` + `Serialize`/`Deserialize`, drop `Copy`.
- `crates/platform-admin/src/lib.rs` — `STORY_DISPLAY` OnceLock + `install_story_display` become owned.
- `crates/platform-admin/src/stories.rs` — `matches!` guards use refs; `story_display_descriptor` returns non-`'static` ref.
- `crates/platform-domain/src/lib.rs` — `DomainDescriptor.story_display` field → owned `Vec` (temporary; crate deleted in final task).
- `domains/identity/src/module.rs` — const → fns; add `manifest()` + `module()`.
- `domains/identity/src/lib.rs` — re-exports.
- `domains/notifications/src/module.rs` — const → fns; add `manifest()` + `module()`.
- `crates/app-bootstrap/src/lib.rs` — enumerate `Module`; new `module_manifests()`.
- `apps/api/src/openapi.rs` — story-display install from manifests.
- `apps/api/src/main.rs`, `apps/worker/src/main.rs` — `domains()` → `modules()`.
- `Cargo.toml` (workspace) — swap `platform-domain` member/dep → `platform-module`.

**Deleted (final task):**
- `crates/platform-domain/` — entire crate.
- `domains/*/src/module.rs::domain()` — old fns.

---

## Task 1: Make `StoryDisplayDescriptor` owned + serde (atomic refactor)

This is one atomic change because `StoryDisplayDescriptor` is shared by `platform-core` (def), `platform-admin` (consumer with `matches!` guards), and `platform-domain` (stores it as `&'static`). All edits land in one commit so `cargo check` stays green. No behavior changes — purely turning `&'static str`/`Copy` into owned `String`.

**Files:**
- Modify: `crates/platform-core/src/story_display.rs` (full rewrite, 15 lines)
- Modify: `crates/platform-admin/src/lib.rs:57,65` (OnceLock + installer signature)
- Modify: `crates/platform-admin/src/stories.rs:261,291-313` (ref patterns)
- Modify: `crates/platform-domain/src/lib.rs:27,40,54` (field type + builder)
- Modify: `domains/identity/src/module.rs`, `domains/notifications/src/module.rs` (const→fn returning owned)
- Modify: `crates/app-bootstrap/src/lib.rs:68` (`story_display_descriptors` returns owned `Vec`)
- Modify: `apps/api/src/openapi.rs:38` (collect owned)

- [ ] **Step 1: Rewrite `story_display.rs` as owned + serde**

Replace the entire file `crates/platform-core/src/story_display.rs` with:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StoryDisplaySource {
    ExecutionName { name: String },
    HttpRequest { method: String, path: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryDisplayDescriptor {
    pub source: StoryDisplaySource,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub story_title: Option<String>,
}
```

Note: `ExecutionName(&str)` tuple variant becomes a struct variant `ExecutionName { name }` so it can derive serde (a tuple variant would serialize as a sequence and can't carry the `tag`).

- [ ] **Step 2: Update `platform-admin/src/lib.rs` to owned**

At line 57, change:

```rust
static STORY_DISPLAY: OnceLock<Vec<StoryDisplayDescriptor>> = OnceLock::new();
```

At line 65, change the installer signature:

```rust
pub fn install_story_display(catalog: Vec<StoryDisplayDescriptor>) {
    let _ = STORY_DISPLAY.set(catalog);
}
```

- [ ] **Step 3: Update `platform-admin/src/stories.rs` ref patterns**

The `story_display_descriptors()` iterator now yields `&StoryDisplayDescriptor` (not `&'static`). Update `story_display_descriptor` (lines 291–313) to return a borrowed ref and match on the new struct variants:

```rust
pub(crate) fn story_display_descriptor(
    row: &StoryWorkRow,
) -> Option<&'static StoryDisplayDescriptor> {
    if row.item_type == "http_request" {
        let (method, path) = row.name.split_once(' ')?;
        return story_display_descriptors().find(|descriptor| {
            matches!(
                &descriptor.source,
                StoryDisplaySource::HttpRequest {
                    method: descriptor_method,
                    path: descriptor_path,
                } if descriptor_method == method && descriptor_path == path
            )
        });
    }

    story_display_descriptors().find(|descriptor| {
        matches!(
            &descriptor.source,
            StoryDisplaySource::ExecutionName { name } if name == row.name.as_str()
        )
    })
}
```

The return stays `&'static` because `STORY_DISPLAY` is a `OnceLock` (its contents are effectively `'static` once set); `story_display_descriptors()` keeps `Item = &'static StoryDisplayDescriptor` via the existing `OnceLock` borrow. No change needed at the `story_title`/`display_name_for_node` call sites — `descriptor.story_title` is now `Option<String>`, and the existing `.and_then(|descriptor| descriptor.story_title)` at line 261 must become `.and_then(|descriptor| descriptor.story_title.clone())`.

Apply this one-line change at line 261:

```rust
        .find_map(|row| story_display_descriptor(row).and_then(|descriptor| descriptor.story_title.clone()))
```

- [ ] **Step 4: Update `platform-domain/src/lib.rs` to owned story_display (temporary)**

`platform-domain` is deleted in Task 7, but must compile now. Change the field (line 27) and builder (line 54) and constructor default (line 40) from `&'static [StoryDisplayDescriptor]` to `Vec<StoryDisplayDescriptor>`:

```rust
    pub story_display: Vec<StoryDisplayDescriptor>,
```

```rust
    pub fn new(name: &'static str, runtime: RuntimeDescriptor) -> Self {
        Self {
            name,
            runtime,
            event_handlers: Vec::new(),
            story_display: Vec::new(),
            runtime_config: &[],
        }
    }
```

```rust
    pub fn with_story_display(mut self, story_display: Vec<StoryDisplayDescriptor>) -> Self {
        self.story_display = story_display;
        self
    }
```

The `Debug` impl at line 77 (`self.story_display.len()`) still compiles unchanged.

- [ ] **Step 5: Convert domain story-display consts to owned fns**

In `domains/identity/src/module.rs`, replace the `pub const STORY_DISPLAY: &[StoryDisplayDescriptor] = &[...]` with a function returning owned data, and update the `domain()` call:

```rust
pub fn story_display() -> Vec<StoryDisplayDescriptor> {
    vec![
        StoryDisplayDescriptor {
            source: StoryDisplaySource::HttpRequest {
                method: "POST".to_owned(),
                path: "/v1/identity/users".to_owned(),
            },
            display_name: "Create User Request".to_owned(),
            story_title: Some("User Registration".to_owned()),
        },
        StoryDisplayDescriptor {
            source: StoryDisplaySource::ExecutionName { name: "identity.create_user".to_owned() },
            display_name: "Create User".to_owned(),
            story_title: Some("User Registration".to_owned()),
        },
        StoryDisplayDescriptor {
            source: StoryDisplaySource::ExecutionName { name: "identity.user_registered.v1".to_owned() },
            display_name: "User Registered".to_owned(),
            story_title: Some("User Registration".to_owned()),
        },
    ]
}

pub fn domain(_ctx: &AppContext) -> DomainDescriptor {
    DomainDescriptor::new("identity", crate::runtime::descriptor())
        .with_story_display(story_display())
        .with_runtime_config(crate::config::RUNTIME_CONFIG.as_slice())
}
```

In `domains/notifications/src/module.rs`, do the same:

```rust
pub fn story_display() -> Vec<StoryDisplayDescriptor> {
    vec![
        StoryDisplayDescriptor {
            source: StoryDisplaySource::ExecutionName {
                name: "notifications.handle_user_registered".to_owned(),
            },
            display_name: "Handle User Registered".to_owned(),
            story_title: None,
        },
        StoryDisplayDescriptor {
            source: StoryDisplaySource::ExecutionName {
                name: "notifications.send_welcome_email.v1".to_owned(),
            },
            display_name: "Send Welcome Email".to_owned(),
            story_title: None,
        },
    ]
}

pub fn domain(ctx: &AppContext) -> DomainDescriptor {
    let runtime_client = RuntimeClient::new(ctx.db.clone());
    DomainDescriptor::new("notifications", crate::runtime::descriptor())
        .with_story_display(story_display())
        .with_event_handlers(vec![Arc::new(
            crate::events::WelcomeEmailRequestedHandler::new(runtime_client),
        )])
}
```

- [ ] **Step 6: Update `app-bootstrap` aggregator to owned**

In `crates/app-bootstrap/src/lib.rs`, `story_display_descriptors()` currently chains `&'static` consts. Change it to return owned `Vec` (line ~68):

```rust
pub fn story_display_descriptors() -> Vec<StoryDisplayDescriptor> {
    identity::module::story_display()
        .into_iter()
        .chain(notifications::module::story_display())
        .collect()
}
```

Add `use platform_core::StoryDisplayDescriptor;` to the imports if not already present (it currently imports `StoryDisplayDescriptor` — confirm and keep).

- [ ] **Step 7: Update the openapi.rs install call**

In `apps/api/src/openapi.rs:38`, the function now returns an owned `Vec`, so drop `.collect()`:

```rust
    platform_admin::install_story_display(app_bootstrap::story_display_descriptors());
```

- [ ] **Step 8: Verify the workspace compiles**

Run: `cargo check --workspace`
Expected: PASS (no errors). This proves the owned refactor is internally consistent across core, admin, domain, bootstrap, and apps.

- [ ] **Step 9: Run the regression tests**

Run: `cargo test -p platform-admin --lib`
Expected: PASS — the story-display matching tests (e.g. `display_name` assertions at stories.rs:722–724) still pass, proving behavior is unchanged.

- [ ] **Step 10: Commit**

```bash
git add crates/platform-core/src/story_display.rs crates/platform-admin/src/lib.rs crates/platform-admin/src/stories.rs crates/platform-domain/src/lib.rs domains/identity/src/module.rs domains/notifications/src/module.rs crates/app-bootstrap/src/lib.rs apps/api/src/openapi.rs
git commit -m "refactor(core): make StoryDisplayDescriptor owned + serde

Turn &'static str/Copy fields into owned String with Serialize/Deserialize so
it can live in a serializable module manifest. Behavior unchanged; updates all
consumers (admin guards, domain builders, bootstrap aggregator) in one atomic
step to keep cargo check green."
```

---
## Task 2: Create the `platform-module` crate (contracts)

Build the new crate with all contracts and three unit tests. Nothing consumes it yet, so it can be developed and checked in isolation.

**Files:**
- Create: `crates/platform-module/Cargo.toml`
- Create: `crates/platform-module/src/lib.rs`
- Create: `crates/platform-module/src/manifest.rs`
- Create: `crates/platform-module/src/admin.rs`
- Create: `crates/platform-module/src/binding.rs`
- Create: `crates/platform-module/src/linked.rs`
- Create: `crates/platform-module/src/module.rs`
- Modify: `Cargo.toml` (workspace members + dep alias)

- [ ] **Step 1: Add the crate to the workspace**

In the root `Cargo.toml`, add to `members` (after `crates/platform-domain` line; both coexist until Task 7):

```toml
    "crates/platform-module",
```

And add to `[workspace.dependencies]` (after the `platform-domain` line):

```toml
platform-module = { path = "crates/platform-module" }
```

- [ ] **Step 2: Write `crates/platform-module/Cargo.toml`**

```toml
[package]
name = "platform-module"
version = "0.1.0"
edition.workspace = true
license.workspace = true
publish.workspace = true
rust-version.workspace = true

[dependencies]
platform-core.workspace = true
platform-runtime.workspace = true
serde.workspace = true

[dev-dependencies]
serde_json.workspace = true

[lints]
workspace = true
```

- [ ] **Step 3: Write `src/admin.rs` (reserved seam)**

```rust
//! Reserved seam for a module's admin surface.

use serde::{Deserialize, Serialize};

/// RESERVED SEAM. Variants (`Schema { .. }` / `Custom { .. }`) are defined by
/// future specs that build the schema-driven and self-rendered admin surfaces.
///
/// `#[non_exhaustive]` so adding variants later is not a breaking change. Empty
/// for now: a manifest's `admin` field is always `None` in this step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AdminSurface {}
```

- [ ] **Step 4: Write `src/manifest.rs` (data half + builder)**

```rust
//! A module's pure-data contract: serializable metadata describable without
//! behavior. Owned + serde so every loading source produces the same shape.

use crate::admin::AdminSurface;
use platform_core::StoryDisplayDescriptor;
use serde::{Deserialize, Serialize};

/// The serializable metadata a module exposes. Runtime config is deliberately
/// NOT here — it stays an internal `&'static` field on [`crate::Module`]
/// because the config registry needs the real (non-serde) `RuntimeConfigType`
/// to validate. Only round-trippable fields belong here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ModuleManifest {
    /// Stable module name, e.g. `"identity"`.
    pub name: String,

    /// Console story-display metadata.
    #[serde(default)]
    pub story_display: Vec<StoryDisplayDescriptor>,

    /// RESERVED SEAM — admin surface (Schema vs Custom). Always `None` now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admin: Option<AdminSurface>,

    /// RESERVED SEAM — capabilities the module declares (perms/tenancy).
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl ModuleManifest {
    /// Start building a manifest for `name`.
    #[must_use]
    pub fn builder(name: impl Into<String>) -> ModuleManifestBuilder {
        ModuleManifestBuilder {
            manifest: ModuleManifest {
                name: name.into(),
                story_display: Vec::new(),
                admin: None,
                capabilities: Vec::new(),
            },
        }
    }
}

/// Fluent builder for [`ModuleManifest`]. Reusable by every loading source.
pub struct ModuleManifestBuilder {
    manifest: ModuleManifest,
}

impl ModuleManifestBuilder {
    /// Attach console story-display metadata.
    #[must_use]
    pub fn story_display(mut self, story_display: Vec<StoryDisplayDescriptor>) -> Self {
        self.manifest.story_display = story_display;
        self
    }

    /// Attach declared capabilities.
    #[must_use]
    pub fn capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.manifest.capabilities = capabilities;
        self
    }

    /// Finish building.
    #[must_use]
    pub fn build(self) -> ModuleManifest {
        self.manifest
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use platform_core::{StoryDisplayDescriptor, StoryDisplaySource};

    #[test]
    fn manifest_round_trips_through_json() {
        let manifest = ModuleManifest::builder("identity")
            .story_display(vec![StoryDisplayDescriptor {
                source: StoryDisplaySource::ExecutionName {
                    name: "identity.create_user".to_owned(),
                },
                display_name: "Create User".to_owned(),
                story_title: Some("User Registration".to_owned()),
            }])
            .build();

        let json = serde_json::to_string(&manifest).expect("serialize");
        let back: ModuleManifest = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(manifest, back);
    }

    #[test]
    fn empty_admin_is_skipped_in_json() {
        let manifest = ModuleManifest::builder("notifications").build();
        let json = serde_json::to_string(&manifest).expect("serialize");
        assert!(!json.contains("admin"), "admin: None must be skipped, got {json}");
    }
}
```

- [ ] **Step 5: Write `src/binding.rs` (behavior half)**

```rust
//! A module's behavior contract: only what varies across loading sources.

use platform_core::EventHandlerRegistry;
use platform_runtime::FunctionRegistry;

/// Narrow by design — pure data lives in [`crate::ModuleManifest`], read
/// directly by upper layers, never through this trait.
///
/// HTTP routing is deliberately EXCLUDED: it carries utoipa `OpenApiRouter`
/// types that out-of-process/Wasm sources cannot produce, so its cross-source
/// shape is the Remote spec's problem. Two clean seams now.
pub trait ModuleBinding: Send + Sync {
    /// Register this module's runtime functions into the shared registry.
    fn register_functions(&self, registry: &mut FunctionRegistry);

    /// Register this module's in-process event handlers.
    fn register_event_handlers(&self, registry: &mut EventHandlerRegistry);
}
```

- [ ] **Step 6: Write `src/linked.rs` (the only impl + builder)**

```rust
//! The compile-time loading source: behavior is linked Rust code.

use crate::binding::ModuleBinding;
use platform_core::{EventHandler, EventHandlerRegistry};
use platform_runtime::{FunctionRegistry, RuntimeDescriptor};
use std::sync::Arc;

/// The only [`ModuleBinding`] impl in Step 1. Remote/Wasm impls arrive in their
/// own specs without touching this one (open extension point). Only forwards to
/// the existing registration logic — no logic moves here.
pub struct LinkedBinding {
    pub runtime: RuntimeDescriptor,
    pub event_handlers: Vec<Arc<dyn EventHandler>>,
}

impl LinkedBinding {
    /// Start building a linked binding.
    #[must_use]
    pub fn builder() -> LinkedBindingBuilder {
        LinkedBindingBuilder {
            runtime: RuntimeDescriptor::default(),
            event_handlers: Vec::new(),
        }
    }
}

impl ModuleBinding for LinkedBinding {
    fn register_functions(&self, registry: &mut FunctionRegistry) {
        self.runtime.register_into(registry);
    }

    fn register_event_handlers(&self, registry: &mut EventHandlerRegistry) {
        registry.register_all(self.event_handlers.clone());
    }
}

/// Fluent builder for [`LinkedBinding`]. Source-specific (Linked only).
pub struct LinkedBindingBuilder {
    runtime: RuntimeDescriptor,
    event_handlers: Vec<Arc<dyn EventHandler>>,
}

impl LinkedBindingBuilder {
    /// Set the runtime descriptor (functions, queues, triggers, flows).
    #[must_use]
    pub fn runtime(mut self, runtime: RuntimeDescriptor) -> Self {
        self.runtime = runtime;
        self
    }

    /// Set the in-process event handlers.
    #[must_use]
    pub fn event_handlers(mut self, handlers: Vec<Arc<dyn EventHandler>>) -> Self {
        self.event_handlers = handlers;
        self
    }

    /// Finish building.
    #[must_use]
    pub fn build(self) -> LinkedBinding {
        LinkedBinding {
            runtime: self.runtime,
            event_handlers: self.event_handlers,
        }
    }
}
```

- [ ] **Step 7: Write `src/module.rs` (Module = manifest + binding + config)**

```rust
//! A loaded module: serializable manifest + behavior binding + internal config.

use crate::binding::ModuleBinding;
use crate::linked::LinkedBinding;
use crate::manifest::ModuleManifest;
use platform_core::RuntimeConfigDescriptor;
use std::sync::Arc;

/// One loaded module. `manifest` is serializable data; `binding` is behavior;
/// `runtime_config` is internal `&'static` config NOT in the manifest (the
/// registry needs the real `RuntimeConfigType` enum to validate). Cross-source
/// config wire form is deferred to a later spec.
pub struct Module {
    pub manifest: ModuleManifest,
    pub binding: Arc<dyn ModuleBinding>,
    pub runtime_config: &'static [RuntimeConfigDescriptor],
}

impl Module {
    /// Build a compile-time (Linked) module from a manifest + linked behavior.
    /// Config defaults to empty; attach it with [`Module::with_runtime_config`].
    #[must_use]
    pub fn linked(manifest: ModuleManifest, binding: LinkedBinding) -> Self {
        Self {
            manifest,
            binding: Arc::new(binding),
            runtime_config: &[],
        }
    }

    /// Attach the module's editable configuration descriptors.
    #[must_use]
    pub fn with_runtime_config(
        mut self,
        runtime_config: &'static [RuntimeConfigDescriptor],
    ) -> Self {
        self.runtime_config = runtime_config;
        self
    }
}
```

- [ ] **Step 8: Write `src/lib.rs` (re-exports)**

```rust
//! Module framework contracts: the data/behavior split a module exposes to the
//! composition root.
//!
//! - [`ModuleManifest`]: serializable data (name, story display, reserved
//!   seams). Produced by every loading source.
//! - [`ModuleBinding`]: behavior (register functions/event handlers). One impl
//!   per loading source; [`LinkedBinding`] is the compile-time one.
//! - [`Module`]: a loaded module bundling manifest + binding + internal config.

mod admin;
mod binding;
mod linked;
mod manifest;
mod module;

pub use admin::AdminSurface;
pub use binding::ModuleBinding;
pub use linked::{LinkedBinding, LinkedBindingBuilder};
pub use manifest::{ModuleManifest, ModuleManifestBuilder};
pub use module::Module;
```

- [ ] **Step 9: Write the binding equivalence test**

Append to `crates/platform-module/src/linked.rs` (after the builder):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use platform_core::ExecutionContext;
    use platform_runtime::{FunctionDefinition, FunctionHandler, RetryPolicy};
    use serde_json::Value;

    // Minimal no-op handler to register one function.
    #[derive(Debug)]
    struct NoopHandler;

    #[async_trait]
    impl FunctionHandler for NoopHandler {
        async fn call(
            &self,
            _ctx: ExecutionContext,
            _input: Value,
        ) -> platform_core::AppResult<Value> {
            Ok(Value::Null)
        }
    }

    #[test]
    fn linked_binding_registers_its_functions() {
        let runtime = RuntimeDescriptor {
            module: "test",
            functions: vec![FunctionDefinition {
                name: "test.noop",
                version: 1,
                queue: "test",
                retry_policy: RetryPolicy::default(),
                handler: Arc::new(NoopHandler),
            }],
            ..RuntimeDescriptor::default()
        };
        let binding = LinkedBinding::builder().runtime(runtime).build();

        let mut registry = FunctionRegistry::default();
        binding.register_functions(&mut registry);

        assert!(registry.get("test.noop").is_some());
    }
}
```

NOTE: `FunctionDefinition` is a plain struct literal (no `::new`) with fields `name: &'static str`, `version: u16`, `queue: &'static str`, `retry_policy`, `handler`. `FunctionHandler` requires `Debug` + `#[async_trait]` and its `call` takes `(ctx: ExecutionContext, input: Value) -> AppResult<Value>`. `ExecutionContext` is re-exported from `platform_core` (NOT `platform_runtime`); `FunctionDefinition`, `FunctionHandler`, `RetryPolicy` from `platform_runtime`. `async-trait.workspace = true` and `serde_json.workspace = true` must be in `[dev-dependencies]`.

- [ ] **Step 10: Check and test the new crate in isolation**

Run: `cargo test -p platform-module`
Expected: PASS — 3 tests green (`manifest_round_trips_through_json`, `empty_admin_is_skipped_in_json`, `linked_binding_registers_its_functions`).

- [ ] **Step 11: Commit**

```bash
git add Cargo.toml crates/platform-module/
git commit -m "feat(platform-module): add Manifest/Binding contracts

New crate with ModuleManifest (serializable data), ModuleBinding (narrow
behavior trait), LinkedBinding (compile-time impl), Module (manifest + binding
+ internal config), and AdminSurface reserved seam. Layered builders. Three
unit tests: manifest JSON round-trip, admin-None skip, linked registration."
```

---
## Task 3: Add `manifest()` + `module()` to identity (keep `domain()`)

Add the new entry points alongside the old `domain()` so the workspace still compiles (`app-bootstrap` still calls `domain()` until Task 5). `manifest()` is context-free; `module(ctx)` builds the binding.

**Files:**
- Modify: `domains/identity/Cargo.toml` (add `platform-module` dep)
- Modify: `domains/identity/src/module.rs`
- Modify: `domains/identity/src/lib.rs`

- [ ] **Step 1: Add the dependency**

In `domains/identity/Cargo.toml`, under `[dependencies]`, add (keep `platform-domain` for now):

```toml
platform-module.workspace = true
```

- [ ] **Step 2: Add `manifest()` and `module()` to `domains/identity/src/module.rs`**

Keep `story_display()` (from Task 1) and the existing `domain()`. Add imports and the two new functions:

```rust
use platform_module::{LinkedBinding, Module, ModuleManifest};
```

```rust
/// Context-free manifest: serializable metadata only (no AppContext needed).
pub fn manifest() -> ModuleManifest {
    ModuleManifest::builder("identity")
        .story_display(story_display())
        .build()
}

/// The loaded module: manifest + linked behavior + internal config.
pub fn module(_ctx: &AppContext) -> Module {
    let binding = LinkedBinding::builder()
        .runtime(crate::runtime::descriptor())
        .build();
    Module::linked(manifest(), binding)
        .with_runtime_config(crate::config::RUNTIME_CONFIG.as_slice())
}
```

NOTE: `crate::config::RUNTIME_CONFIG.as_slice()` is the exact form the old `domain()` already used against a `&'static [RuntimeConfigDescriptor]` parameter — it compiles today, so it compiles here unchanged. `RUNTIME_CONFIG` is a `static LazyLock<Vec<..>>`, and `.as_slice()` on it yields the required `&'static` slice.

- [ ] **Step 3: Export `module` and `manifest` from `domains/identity/src/lib.rs`**

The line `pub use module::domain;` stays. The `module` mod is already `pub mod module;`, so `identity::module::manifest()` and `identity::module::module()` are reachable. No change required unless a convenience re-export is desired; leave as-is.

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p identity`
Expected: PASS — both old `domain()` and new `manifest()`/`module()` coexist.

- [ ] **Step 5: Commit**

```bash
git add domains/identity/Cargo.toml domains/identity/src/module.rs
git commit -m "feat(identity): add manifest() + module() entry points

Context-free manifest() (serializable metadata) and module(ctx) (manifest +
LinkedBinding + config). Old domain() kept until app-bootstrap switches."
```

---

## Task 4: Add `manifest()` + `module()` to notifications (keep `domain()`)

Same shape as identity, but notifications has an event handler and no runtime config.

**Files:**
- Modify: `domains/notifications/Cargo.toml` (add `platform-module` dep)
- Modify: `domains/notifications/src/module.rs`

- [ ] **Step 1: Add the dependency**

In `domains/notifications/Cargo.toml`, under `[dependencies]`:

```toml
platform-module.workspace = true
```

- [ ] **Step 2: Add `manifest()` and `module()` to `domains/notifications/src/module.rs`**

Keep `story_display()` (Task 1) and existing `domain()`. Add:

```rust
use platform_module::{LinkedBinding, Module, ModuleManifest};
```

```rust
/// Context-free manifest: serializable metadata only.
pub fn manifest() -> ModuleManifest {
    ModuleManifest::builder("notifications")
        .story_display(story_display())
        .build()
}

/// The loaded module: manifest + linked behavior (event handler, no config).
pub fn module(ctx: &AppContext) -> Module {
    let runtime_client = RuntimeClient::new(ctx.db.clone());
    let binding = LinkedBinding::builder()
        .runtime(crate::runtime::descriptor())
        .event_handlers(vec![Arc::new(
            crate::events::WelcomeEmailRequestedHandler::new(runtime_client),
        )])
        .build();
    Module::linked(manifest(), binding)
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p notifications`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add domains/notifications/Cargo.toml domains/notifications/src/module.rs
git commit -m "feat(notifications): add manifest() + module() entry points

Mirrors identity: context-free manifest() + module(ctx) with the welcome-email
event handler in the binding. Old domain() kept until app-bootstrap switches."
```

---

## Task 5: Switch `app-bootstrap` and apps to `Module`

Replace `DomainDescriptor` aggregation with `Module`. Add a context-free `module_manifests()` for the OpenAPI path (keeps `openapi_document()` pure). Update both apps' call sites.

**Files:**
- Modify: `crates/app-bootstrap/Cargo.toml` (dep `platform-domain` → `platform-module`)
- Modify: `crates/app-bootstrap/src/lib.rs`
- Modify: `apps/api/src/openapi.rs`
- Modify: `apps/worker/src/main.rs`

- [ ] **Step 1: Swap the dependency**

In `crates/app-bootstrap/Cargo.toml`, replace `platform-domain.workspace = true` with:

```toml
platform-module.workspace = true
```

- [ ] **Step 2: Rewrite the aggregators in `crates/app-bootstrap/src/lib.rs`**

Replace the `use platform_domain::DomainDescriptor;` import with `use platform_module::{Module, ModuleManifest};` and update each function. Final shapes:

```rust
/// The authoritative list of loaded modules (context-bound: builds bindings).
#[must_use]
pub fn modules(ctx: &AppContext) -> Vec<Module> {
    vec![identity::module::module(ctx), notifications::module::module(ctx)]
}

/// Context-free manifests for read-only / OpenAPI paths that have no AppContext.
/// Kept in sync with `modules` by listing the same modules.
#[must_use]
pub fn module_manifests() -> Vec<ModuleManifest> {
    vec![identity::module::manifest(), notifications::module::manifest()]
}

#[must_use]
pub fn function_registry(modules: &[Module]) -> FunctionRegistry {
    let mut registry = FunctionRegistry::default();
    for module in modules {
        module.binding.register_functions(&mut registry);
    }
    registry
}

#[must_use]
pub fn event_handlers(modules: &[Module]) -> EventHandlerRegistry {
    let mut registry = EventHandlerRegistry::new();
    for module in modules {
        module.binding.register_event_handlers(&mut registry);
    }
    registry
}

/// Story-display descriptors for every module. Sourced from context-free
/// manifests so the OpenAPI path stays pure (no AppContext).
#[must_use]
pub fn story_display_descriptors() -> Vec<StoryDisplayDescriptor> {
    module_manifests()
        .into_iter()
        .flat_map(|manifest| manifest.story_display)
        .collect()
}

#[must_use]
pub fn runtime_config_descriptors(ctx: &AppContext) -> Vec<RuntimeConfigDescriptor> {
    let module_descriptors = modules(ctx)
        .iter()
        .flat_map(|module| module.runtime_config.iter().cloned())
        .collect::<Vec<_>>();
    platform_core::worker_runtime_config::RUNTIME_CONFIG
        .iter()
        .cloned()
        .chain(module_descriptors)
        .collect()
}
```

`merge_domain_http` stays exactly as-is (Linked-only HTTP, deferred). Update its doc comment to note it is kept in sync manually with `modules`.

NOTE: `story_display_descriptors()` changes from the Task-1 form (which chained `identity::module::story_display()`) to sourcing from `module_manifests()`. Both are context-free; this version routes through the manifest, removing one hand-maintained domain list (story display now derives from the manifest list). The signature is unchanged: `() -> Vec<StoryDisplayDescriptor>`.

- [ ] **Step 3: Verify `openapi.rs` still compiles**

`apps/api/src/openapi.rs:38` already calls `app_bootstrap::story_display_descriptors()` (owned, no `.collect()` after Task 1). No change needed — confirm it reads:

```rust
    platform_admin::install_story_display(app_bootstrap::story_display_descriptors());
```

- [ ] **Step 4: Update `apps/worker/src/main.rs`**

Line 33–35 currently:

```rust
    let domains = app_bootstrap::domains(&ctx);
    let registry = app_bootstrap::function_registry(&domains);
    let event_handlers = app_bootstrap::event_handlers(&domains);
```

Change to:

```rust
    let modules = app_bootstrap::modules(&ctx);
    let registry = app_bootstrap::function_registry(&modules);
    let event_handlers = app_bootstrap::event_handlers(&modules);
```

`apps/api/src/main.rs` calls only `runtime_config_descriptors(&ctx)` (unchanged signature) — no edit needed. Confirm with grep.

- [ ] **Step 5: Verify the workspace compiles**

Run: `cargo check --workspace`
Expected: PASS — `app-bootstrap` now yields `Module`; `domains()` is gone (replaced by `modules()`).

- [ ] **Step 6: Run the worker + api regression tests**

Run: `cargo test -p identity -p notifications --lib`
Expected: PASS.

Run: `cargo test -p app-api --test config_console`
Expected: PASS — runtime-config still exposed through the console (proves config wiring survived).

- [ ] **Step 7: Commit**

```bash
git add crates/app-bootstrap/Cargo.toml crates/app-bootstrap/src/lib.rs apps/worker/src/main.rs
git commit -m "refactor(bootstrap): enumerate Module instead of DomainDescriptor

modules(ctx) builds full Modules (binding + config); context-free
module_manifests() feeds the pure OpenAPI/story-display path. Aggregators read
behavior from bindings, data from manifests. Worker switched to modules()."
```

---

## Task 6: Migrate domain tests and remove old `domain()` calls

Switch any remaining `DomainDescriptor`/`domain()` references in tests to the new API, so nothing depends on the soon-deleted crate.

**Files:**
- Modify: any test referencing `domain()` / `DomainDescriptor` (grep first)

- [ ] **Step 1: Find remaining references**

Run: `grep -rn "DomainDescriptor\|::domain(\|platform_domain\|platform-domain" crates apps domains tools Cargo.toml | grep -v target`
Expected: occurrences only in `domains/*/src/module.rs` (the old `domain()` fns), `crates/platform-domain/` itself, and the workspace `Cargo.toml`. If any test or app still calls `domain()`, note it here.

- [ ] **Step 2: Update any stragglers**

For each straggler found, replace `domain(ctx)` with `module(ctx)` and `DomainDescriptor` with `Module`, adjusting field access (`.story_display` is on `.manifest`, `.runtime_config` is on the `Module` directly, behavior via `.binding`). If there are none (the only callers were `app-bootstrap`, already migrated), skip.

- [ ] **Step 3: Verify**

Run: `cargo check --workspace`
Expected: PASS.

- [ ] **Step 4: Commit (skip if no changes)**

```bash
git add -A
git commit -m "refactor: migrate remaining domain() references to module()"
```

---

## Task 7: Delete `platform-domain` and old `domain()` fns

Final cleanup. The old crate and the per-domain `domain()` functions are now unused.

**Files:**
- Delete: `crates/platform-domain/`
- Modify: `Cargo.toml` (remove member + dep alias)
- Modify: `domains/identity/Cargo.toml`, `domains/notifications/Cargo.toml` (drop `platform-domain` dep)
- Modify: `domains/identity/src/module.rs`, `domains/notifications/src/module.rs` (delete `domain()` + its imports)
- Modify: `domains/identity/src/lib.rs` (drop `pub use module::domain;`)

- [ ] **Step 1: Delete the old `domain()` functions**

In `domains/identity/src/module.rs`, delete the `pub fn domain(...)` function and the now-unused `use platform_domain::DomainDescriptor;` import. Do the same in `domains/notifications/src/module.rs`.

In `domains/identity/src/lib.rs`, delete the line `pub use module::domain;`.

- [ ] **Step 2: Drop the `platform-domain` dependency from domains**

In `domains/identity/Cargo.toml` and `domains/notifications/Cargo.toml`, remove the `platform-domain.workspace = true` line.

- [ ] **Step 3: Remove from the workspace**

In the root `Cargo.toml`, delete the `"crates/platform-domain",` member line and the `platform-domain = { path = "crates/platform-domain" }` dependency line.

- [ ] **Step 4: Delete the crate directory**

Run: `git rm -r crates/platform-domain`

- [ ] **Step 5: Verify the whole workspace compiles with the crate gone**

Run: `cargo check --workspace`
Expected: PASS — no residual references to `platform_domain`.

- [ ] **Step 6: Confirm no dangling references**

Run: `grep -rn "platform_domain\|platform-domain\|DomainDescriptor" crates apps domains tools Cargo.toml | grep -v target`
Expected: no output.

- [ ] **Step 7: Full test + arch-check sweep**

Run: `cargo test --workspace`
Expected: PASS.

Run: `cargo run -p arch-check` (or the project's arch-check invocation — check `justfile`)
Expected: PASS — confirm `platform-module` is not flagged by any crate-dependency rule. If arch-check has an allowlist of crates/dependencies, add `platform-module` with the same rules `platform-domain` had.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor: delete platform-domain crate

DomainDescriptor fully replaced by ModuleManifest + ModuleBinding in
platform-module. Old domain() fns and the crate are removed."
```

---

## Final Verification (acceptance criteria from spec)

- [ ] `cargo check --workspace` passes; `platform-domain` deleted, no residual refs (Task 7 Step 6).
- [ ] Each migration step independently `cargo check`-passed (Tasks 1, 2, 3, 4, 5, 7 each ran it).
- [ ] Regression tests green, zero semantic change (`platform-admin` story tests, `config_console`, identity/notifications).
- [ ] 3 new `platform-module` tests green (Task 2 Step 10).
- [ ] `arch-check` passes with `platform-module` covered (Task 7 Step 7).
- [ ] `app-bootstrap` is source-agnostic — no `match source_kind` branches (review `lib.rs`).
- [ ] OpenAPI single-source rule preserved: `merge_domain_http` unchanged, `openapi_document()` stays context-free via `module_manifests()`.

```bash
just check 2>/dev/null || cargo check --workspace
cargo test --workspace
```
