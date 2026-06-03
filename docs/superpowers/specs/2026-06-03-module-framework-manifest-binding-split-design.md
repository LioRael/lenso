# Module Framework Step 1 — Manifest / Binding Split

**Date:** 2026-06-03
**Status:** Approved design, ready for implementation planning
**Scope:** Step 1 of a 4-step evolution toward an installable module framework. This spec covers ONLY the contract split refactor.

---

## Context & Vision

Lenso is evolving from a fixed modular monolith into a **module framework** where third-party business modules (e.g. a user module) can be installed via a market and managed in the console as a business backend.

The full vision is a 4-step evolution, **each its own spec**:

1. **Split `DomainDescriptor` into `ModuleManifest` (pure data) + `ModuleBinding` (behavior)**, with future seams reserved but unimplemented. Only the `Linked` (compile-time) source implemented. ← **THIS SPEC**
2. `AdminSurface::Schema` + unified data protocol + capabilities → schema-driven CRUD business backend in console.
3. `Remote` (out-of-process) module source.
4. `AdminSurface::Custom` (plugin self-rendering) + `Wasm` module source.

Two orthogonal axes are expressed as seams from day one, then filled in risk order:

- **Loading source** — how a manifest is produced: Linked (now) / Remote / Wasm (later).
- **Admin rendering** — `AdminSurface::Schema` (console generic renderer) vs `AdminSurface::Custom` (module ships its own UI bundle).

This spec delivers **no runtime-visible feature**. Nothing changes in the console; API behavior is identical. The deliverable is an **architectural foundation**: a codebase that is structurally ready for three loading sources and two rendering modes, with only the compile-time cell filled.

### Non-goals (explicitly deferred to later specs)

- Any `AdminSurface` variant implementation (Schema / Custom).
- Capability/permission/tenancy semantics.
- Remote or Wasm loading sources.
- Cross-source HTTP routing abstraction.
- Cross-source runtime-config wire form (config stays internal `&'static` this step).
- Unified CRUD data protocol.

---

## Key Decisions (and why)

| Decision | Choice | Why |
|----------|--------|-----|
| Scope | Step 1 only (split + reserved seams) | Large vision packed into one spec becomes unexecutable; brainstorming discipline decomposes first. |
| Terminology | Unify on `Module` | Repo already uses "module" (`RuntimeDescriptor.module`, `ModuleConfig`); removes existing domain/module ambiguity. |
| Data type | Manifest owned + `Serialize` + `Deserialize`; config stays internal | Round-trippable manifest fields (name, story_display, seams) reuse across all sources with zero re-migration. |
| Runtime config representation | **Deferred** — not in the serializable manifest in Step 1 | The registry needs the real `RuntimeConfigType` enum (the `Enum(&'static [&str])` tuple variant is deliberately non-serde) to validate writes; it cannot be rebuilt from owned wire data. Config's cross-source wire form is the Remote/schema-admin spec's problem — same YAGNI deferral as HTTP and `AdminSurface`. For now `Module` carries `runtime_config: &'static [RuntimeConfigDescriptor]` as a plain non-serialized field, fed to the registry exactly as today. |
| Behavior abstraction | **Narrow trait** `ModuleBinding` | Loading sources are an **open extension point** — the user intends modules to eventually provide their own new loading mechanisms. Openness rules out a closed-set enum. |
| HTTP routing | **Excluded from Binding** this step | HTTP carries utoipa `OpenApiRouter` types that out-of-process/Wasm cannot produce; cross-source HTTP shape is the Remote spec's core problem. YAGNI. |
| Crate | New `platform-module`, delete `platform-domain` | Clean terminology from day one; blast radius is small (4 dependents). |
| Reserved seams | Placeholder only | Seams exist + serialize + take empty values. Concrete shapes belong to the specs that actually research them. |
| Migration strategy | Contract-first, per-module | Each step independently compiles; clear breakpoints, easy rollback (TDD-style small steps). |
| Builder style | Layered (manifest builder + binding builder) | Reinforces data/behavior separation; manifest builder is reusable by future sources, binding builder is source-specific — previews the multi-source architecture. |

---

## Architecture

### Crate structure

New crate `crates/platform-module/`, replacing `crates/platform-domain/`:

```
crates/platform-module/src/
├── lib.rs        // re-exports + crate-level docs
├── manifest.rs   // ModuleManifest (pure data, owned, Serialize + Deserialize)
├── binding.rs    // ModuleBinding trait (behavior, narrow seam)
├── linked.rs     // LinkedBinding (the only impl: compile-time source)
├── module.rs     // Module = { manifest, binding } + Module::linked()
└── admin.rs      // AdminSurface reserved seam (future specs fill it)
```

### Top-level concept

```rust
// module.rs
pub struct Module {
    pub manifest: ModuleManifest,        // serializable data
    pub binding: Arc<dyn ModuleBinding>, // behavior
    /// Internal `&'static` config descriptors — NOT in the serializable
    /// manifest (the registry needs the real RuntimeConfigType enum to
    /// validate). Cross-source config wire form deferred to a later spec.
    pub runtime_config: &'static [RuntimeConfigDescriptor],
}

impl Module {
    /// Build a compile-time (Linked) module from a manifest + linked behavior.
    /// Config defaults to empty; set it with `.with_runtime_config(...)`.
    pub fn linked(manifest: ModuleManifest, binding: LinkedBinding) -> Self {
        Self { manifest, binding: Arc::new(binding), runtime_config: &[] }
    }

    #[must_use]
    pub fn with_runtime_config(mut self, cfg: &'static [RuntimeConfigDescriptor]) -> Self {
        self.runtime_config = cfg;
        self
    }
}
```

### Responsibility split (the design spine)

| Concern | Home | Cross-source |
|---------|------|--------------|
| name, story_display, admin (placeholder), capabilities (placeholder) | `ModuleManifest` (serializable data) | serializable / transportable |
| runtime_config (`&'static`) | `Module` field, NOT in manifest | deferred to Remote/schema-admin spec |
| register runtime functions, register event handlers | `ModuleBinding` (behavior) | per-source impl |
| HTTP routes | NOT in Binding — stays on `app-bootstrap` Linked-only path | deferred to Remote spec |

---

## Contracts

### `ModuleManifest` (data half)

```rust
// manifest.rs
use serde::{Deserialize, Serialize};

/// A module's pure-data contract: the serializable metadata describable
/// without behavior. Owned + serializable so all loading sources (Linked now;
/// Remote/Wasm later) produce the same shape with zero re-migration.
///
/// Note: runtime_config is deliberately NOT here — it lives on `Module` as an
/// internal `&'static` field (see decisions). Only round-trippable fields belong.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ModuleManifest {
    /// Stable module name, e.g. "identity".
    pub name: String,

    /// Console story-display metadata (owned; was &'static).
    #[serde(default)]
    pub story_display: Vec<StoryDisplayDescriptor>,

    /// RESERVED SEAM — admin surface (Schema vs Custom). Shape is a future
    /// spec's core output; here it only needs to exist, serialize, be None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admin: Option<AdminSurface>,

    /// RESERVED SEAM — capabilities the module declares (perms/tenancy).
    /// Filled by the schema-admin spec; empty for now.
    #[serde(default)]
    pub capabilities: Vec<String>,
}
```

Built via a layered builder; config is attached on the `Module`, not the manifest:

```rust
let manifest = ModuleManifest::builder("identity")
    .story_display(story_display())
    .build();
// ... then: Module::linked(manifest, binding).with_runtime_config(crate::config::RUNTIME_CONFIG)
```

### `AdminSurface` (reserved seam)

```rust
// admin.rs

/// RESERVED SEAM. Variants (Schema { ... } / Custom { ... }) are defined by
/// future specs. Non-exhaustive so adding variants is not a breaking change.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AdminSurface {}
```

### `ModuleBinding` (behavior half)

```rust
// binding.rs

/// A module's behavior contract: only what varies across loading sources.
/// Narrow by design — pure data lives in ModuleManifest, read directly by
/// upper layers, never through this trait.
///
/// Two clean seams now. HTTP routing is deliberately EXCLUDED: it carries
/// utoipa OpenApiRouter types that out-of-process/Wasm sources cannot produce,
/// so its cross-source shape is the Remote spec's problem, not this one.
pub trait ModuleBinding: Send + Sync {
    /// Register this module's runtime functions into the shared registry.
    fn register_functions(&self, registry: &mut FunctionRegistry);

    /// Register this module's in-process event handlers.
    fn register_event_handlers(&self, registry: &mut EventHandlerRegistry);
}
```

### `LinkedBinding` (the only impl)

```rust
// linked.rs

/// The compile-time loading source: behavior is linked Rust code.
/// The only ModuleBinding impl in Step 1; Remote/Wasm impls arrive in their
/// own specs without touching this one (open extension point).
pub struct LinkedBinding {
    pub runtime: RuntimeDescriptor,
    pub event_handlers: Vec<Arc<dyn EventHandler>>,
}

impl ModuleBinding for LinkedBinding {
    fn register_functions(&self, registry: &mut FunctionRegistry) {
        self.runtime.register_into(registry);          // reuse existing logic
    }
    fn register_event_handlers(&self, registry: &mut EventHandlerRegistry) {
        registry.register_all(self.event_handlers.clone());  // reuse existing logic
    }
}
```

Built via a source-specific builder:

```rust
let binding = LinkedBinding::builder()
    .runtime(crate::runtime::descriptor())
    .event_handlers(vec![...])   // omitted when empty
    .build();
```

**Intentional asymmetry:** the registration logic stays in `RuntimeDescriptor::register_into` and `EventHandlerRegistry::register_all`. `LinkedBinding` only forwards to them. No logic moves — only structural wrapping — so every migration step is a pure mechanical transform.

---

## Required type changes (honest blast radius)

1. **`StoryDisplayDescriptor` / `StoryDisplaySource`**: currently `Copy` + `&'static str` fields. To live in an owned, serializable manifest, fields change `&'static str` → `String` (drop `Copy`), add `Serialize`/`Deserialize`.
2. **`RuntimeConfigDescriptor`**: **unchanged.** It keeps its `&'static str` fields and non-serde `RuntimeConfigType` (the `Enum(&'static [&str])` tuple variant is deliberately hand-serialized via `to_json`). It is carried on `Module` as `&'static [RuntimeConfigDescriptor]` and fed to `RuntimeConfigRegistry::try_new` (which already takes an owned `Vec`) exactly as today. No serde added; config is not in the wire manifest this step.
3. **`platform_admin::install_story_display`**: signature `Vec<&'static StoryDisplayDescriptor>` → `Vec<StoryDisplayDescriptor>` (owned).

`#[non_exhaustive]` on `ModuleManifest` and `AdminSurface` is the type-system mechanism that makes "reserved seams" real: adding fields/variants later is not a breaking change.

---

## Domain migration (identity / notifications)

### Before (identity)

```rust
// domains/identity/src/module.rs
pub const STORY_DISPLAY: &[StoryDisplayDescriptor] = &[ /* &'static constants */ ];

pub fn domain(_ctx: &AppContext) -> DomainDescriptor {
    DomainDescriptor::new("identity", crate::runtime::descriptor())
        .with_story_display(STORY_DISPLAY)
        .with_runtime_config(crate::config::RUNTIME_CONFIG.as_slice())
}
```

### After (identity)

```rust
// domains/identity/src/module.rs
pub fn story_display() -> Vec<StoryDisplayDescriptor> {
    vec![ /* same data, String fields */ ]
}

pub fn module(_ctx: &AppContext) -> Module {
    let manifest = ModuleManifest::builder("identity")
        .story_display(story_display())
        .build();
    let binding = LinkedBinding::builder()
        .runtime(crate::runtime::descriptor())
        .build();
    Module::linked(manifest, binding)
        .with_runtime_config(crate::config::RUNTIME_CONFIG)  // still &'static, unchanged
}
```

### Connected changes

1. **Function rename `domain` → `module`**: `identity::domain` → `identity::module()`, `notifications::module::domain` → `notifications::module::module()`. The `pub use module::domain` re-exports change too.
2. **`STORY_DISPLAY` const → owned function**: `STORY_DISPLAY` becomes `fn story_display() -> Vec<...>` (owned, for the manifest). `RUNTIME_CONFIG` **stays a `&'static` const** — it is passed through `.with_runtime_config(...)` unchanged. Data content unchanged in both.
3. **notifications** has only an event handler (no runtime/story): its `module()` fills only the binding's `event_handlers`; manifest is essentially empty. Validates the builder on a behavior-only, near-dataless module.

---

## Consumer side (`app-bootstrap` and apps)

`app-bootstrap` shifts from enumerating `DomainDescriptor` to enumerating `Module`; aggregation functions read **manifest (data)** or **binding (behavior)** from the module list.

```rust
// crates/app-bootstrap/src/lib.rs

/// The only place that enumerates concrete modules (semantics unchanged;
/// type DomainDescriptor → Module).
pub fn modules(ctx: &AppContext) -> Vec<Module> {
    vec![identity::module(ctx), notifications::module::module(ctx)]
}

// Behavior aggregation: from each module's binding
pub fn function_registry(modules: &[Module]) -> FunctionRegistry {
    let mut registry = FunctionRegistry::default();
    for m in modules { m.binding.register_functions(&mut registry); }
    registry
}

pub fn event_handlers(modules: &[Module]) -> EventHandlerRegistry {
    let mut registry = EventHandlerRegistry::new();
    for m in modules { m.binding.register_event_handlers(&mut registry); }
    registry
}

// Data aggregation: from each module's manifest (owned; returns Vec)
pub fn story_display_descriptors(modules: &[Module]) -> Vec<StoryDisplayDescriptor> {
    modules.iter().flat_map(|m| m.manifest.story_display.iter().cloned()).collect()
}

pub fn runtime_config_descriptors(modules: &[Module]) -> Vec<RuntimeConfigDescriptor> {
    // From each module's `&'static` config field (NOT the manifest); still &'static, cloned.
    let module_cfg = modules.iter().flat_map(|m| m.runtime_config.iter().cloned());
    platform_core::worker_runtime_config::RUNTIME_CONFIG.iter().cloned()
        .chain(module_cfg).collect()
}

// HTTP: stays Linked-only; explicitly annotated as awaiting the Remote spec's
// cross-source HTTP seam.
pub fn merge_domain_http(base: ApiOpenApiRouter) -> ApiOpenApiRouter {
    base.merge(identity::routes::router())   // unchanged
}
```

### Connected app changes

1. **`story_display_descriptors`** changes from `Iterator<&'static>` (no args) to `Vec<owned>` taking `&[Module]`.
   - `apps/api/src/openapi.rs:38`: build `let modules = app_bootstrap::modules(&ctx);` then pass `story_display_descriptors(&modules)`.
   - `platform_admin::install_story_display` signature change (above).
2. **`runtime_config_descriptors` / `function_registry` / `event_handlers`** param `&[DomainDescriptor]` → `&[Module]` — pure type substitution at call sites in `apps/api/main.rs`, `apps/worker/main.rs`, `apps/api/tests/config_console.rs`; logic unchanged.

### Semantic improvement (natural consequence, not scope creep)

Previously `story_display_descriptors()` and `merge_domain_http()` each hard-coded a domain list ("kept in sync with domains by listing the same domains" — a fragile manual sync point). After migration, story_display derives from `modules()`, **removing one manual sync point**. `merge_domain_http` still enumerates manually (HTTP not yet in binding) but is annotated as a Linked-only refactor point.

---

## Testing & acceptance

This is a **structure refactor with unchanged behavior**, so the testing philosophy is: **prove behavior is unchanged, not add behavior tests.**

### Regression baseline (run green BEFORE changes, confirm green AFTER)

- `domains/identity/tests/` — `postgres_user_repository`, `create_user_outbox`, `identity_application`
- `apps/api/tests/config_console.rs` — runtime_config exposed through console

These must not change semantically. Allowed edits are mechanical only: type name `DomainDescriptor` → `Module`, call `domain()` → `module()`. Their staying green proves function registration, event dispatch, and config exposure behavior is preserved across the refactor.

### New targeted tests (in `platform-module`)

1. **Manifest serialization round-trip**: `ModuleManifest` → JSON → `ModuleManifest`, assert equal. Directly validates the owned + Serialize/Deserialize decision and the contract guarantee for future Remote/Wasm sources.
2. **Empty `AdminSurface` + `#[non_exhaustive]` compiles and serializes**: build a manifest with `admin: None`, serialize, assert `admin` is skipped. Proves the reserved seam exists, serializes, and accepts empty values.
3. **`LinkedBinding` registration equivalence**: build a `LinkedBinding` with one function, call `register_functions`, assert the registry contains it. Proves the wrapper layer loses no logic.

### Acceptance criteria

- [ ] `cargo check` passes across the workspace (the project quality gate is `cargo check`, not clippy); `platform-domain` deleted with no residual references.
- [ ] Each migration step (contract-first → identity → notifications → app-bootstrap → delete old) independently `cargo check`-passes.
- [ ] Regression tests above all green, zero semantic changes.
- [ ] The 3 new `platform-module` tests green.
- [ ] `tools/arch-check` passes (confirm `platform-module` is covered if it has crate-dependency rules).
- [ ] Upper layers (`app-bootstrap`) are source-agnostic — no `match source_kind`-style branches anywhere.
- [ ] OpenAPI single-source rule preserved: `merge_domain_http` behavior unchanged.

**Acceptance is proven by green tests + clean `cargo check` + seams in place — not by a new-feature demo.** This spec ships foundation, not visible behavior.
