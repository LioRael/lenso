# Rename Settings → RuntimeConfig Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rename the dynamic configuration system's core symbols from `Setting*` to `RuntimeConfig*` (and the console's "Settings" surface to "Configuration"), eliminating the two-scheme naming inconsistency, with zero behavior change.

**Architecture:** A pure mechanical rename done in dependency order: first free the `RuntimeConfig` name by renaming the existing static struct to `WorkerConfig`; then rename the core `platform-core::settings` module and its symbols to `runtime_config`; then update every consumer (admin handlers, bootstrap, apps, domains, tests); then the console. Each task is guarded by `cargo check`/tests so a half-applied rename never lands.

**Tech Stack:** Rust 2024 (cargo), Vite/React TypeScript (pnpm, oxlint/ultracite), Postgres integration tests.

**Reference spec:** `docs/superpowers/specs/2026-06-03-rename-settings-to-runtime-config.md`

**Critical invariants (must hold after every task):**
- No behavior change. Routes `/admin/config/*`, OpenAPI tag `admin-config`, DTOs `Config*Dto`, DB schema/tables (`config.setting_values`, `config.setting_audit`), and the `config_changed` NOTIFY channel are ALL unchanged.
- `just generated-check` must show no diff (outward contract untouched).
- Integration tests require Postgres: run with `DATABASE_URL=postgres://postgres@localhost:5432/postgres`.

---

## File Structure

**Prerequisite rename (Task 1):**
- `crates/platform-core/src/config.rs` — static `RuntimeConfig` struct → `WorkerConfig`, `AppConfig.runtime` field → `worker`.
- `crates/platform-core/src/lib.rs` — re-export `RuntimeConfig` → `WorkerConfig`.

**Core module rename (Task 2):**
- `crates/platform-core/src/settings/` → `crates/platform-core/src/runtime_config/` (dir + 6 files: mod, descriptor, snapshot, provider, postgres, store).
- `crates/platform-core/src/lib.rs` — `pub mod settings` → `pub mod runtime_config`, re-exports.
- `crates/platform-core/src/context.rs` — `AppContext.settings` → `runtime_config`, builder, imports.
- `crates/platform-core/tests/settings_provider.rs` → `runtime_config_provider.rs`.

**Consumers (Task 3):**
- `crates/platform-domain/src/lib.rs` — `with_settings`/`settings` field.
- `crates/app-bootstrap/src/lib.rs` — `setting_descriptors`.
- `crates/platform-admin/src/lib.rs`, `config_handlers.rs` — registry injection + reads.
- `apps/api/src/main.rs`, `apps/worker/src/main.rs` — startup wiring.
- `domains/identity/src/config.rs`, `module.rs` — `SETTINGS` const + `with_settings`.
- `apps/api/tests/config_console.rs` — symbol references.

**Console (Task 4):**
- `apps/runtime-console/src/pages/settings-page.tsx` → `config-page.tsx` (`SettingsPage` → `ConfigPage`).
- `apps/runtime-console/src/app/router.tsx` — route `/settings` → `/config`, import.
- `apps/runtime-console/src/components/runtime/runtime-console-shell.tsx` — nav label/route.

---

## Task 1: Prerequisite — rename static `RuntimeConfig` → `WorkerConfig`

Free the `RuntimeConfig` name (currently a static struct holding `worker_poll_interval_ms`) for the dynamic system. This struct is part of the `AppConfig` family.

**Files:**
- Modify: `crates/platform-core/src/config.rs`
- Modify: `crates/platform-core/src/lib.rs`

- [ ] **Step 1: Rename the struct and field in config.rs**

In `crates/platform-core/src/config.rs`:
- The struct currently at line ~148:
```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RuntimeConfig {
    pub worker_poll_interval_ms: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            worker_poll_interval_ms: 1_000,
        }
    }
}
```
Rename `RuntimeConfig` → `WorkerConfig` (both the struct and the `impl Default`).

- In the `AppConfig` struct definition, change the field:
```rust
    pub runtime: RuntimeConfig,
```
to:
```rust
    pub worker: WorkerConfig,
```

- In `AppConfig::from_env` (or the struct literal in it), change:
```rust
            runtime: RuntimeConfig::default(),
```
to:
```rust
            worker: WorkerConfig::default(),
```

- [ ] **Step 2: Update the re-export in lib.rs**

In `crates/platform-core/src/lib.rs`, the config re-export line currently reads:
```rust
pub use config::{
    AppConfig, AuthConfig, DatabaseConfig, HttpConfig, LogFormat, ModuleConfig, RuntimeConfig,
    ServiceConfig, TelemetryConfig, parse_cors_allowed_origins,
};
```
Replace `RuntimeConfig` with `WorkerConfig` (keep alphabetical order — `WorkerConfig` goes after `TelemetryConfig`):
```rust
pub use config::{
    AppConfig, AuthConfig, DatabaseConfig, HttpConfig, LogFormat, ModuleConfig, ServiceConfig,
    TelemetryConfig, WorkerConfig, parse_cors_allowed_origins,
};
```

- [ ] **Step 3: Verify no other references**

Run: `grep -rn "RuntimeConfig\b" crates apps domains --include="*.rs" | grep -v "RuntimeConfigDescriptor\|RuntimeDescriptor"`
Expected: NO output (the only `RuntimeConfig` references were the struct, field, and re-export just changed). If anything remains, update it to `WorkerConfig` / `worker`.

Also run: `grep -rn "\.runtime\b" crates apps domains --include="*.rs" | grep -iE "config|worker_poll"`
Expected: NO output referencing config (the remaining `.runtime` hits like `domain.runtime`, `self.runtime` are unrelated runtime descriptors — leave them).

- [ ] **Step 4: Compile check**

Run: `cargo check --locked --workspace --all-targets`
Expected: PASS. (`worker_poll_interval_ms` has no readers yet, so the field rename is internal.)

- [ ] **Step 5: Format and commit**

```bash
cargo fmt -p platform-core
git add crates/platform-core/src/config.rs crates/platform-core/src/lib.rs
git commit -m "refactor(platform-core): rename static RuntimeConfig to WorkerConfig"
```

---

## Task 2: Rename the core `settings` module → `runtime_config`

Rename the module directory, its 6 files' internal symbols, the `AppContext` field, and the test file. This is the bulk of the rename. The module is self-contained, so it compiles after this task even before consumers update — EXCEPT `AppContext` is consumed elsewhere, so this task also fixes `context.rs` but will leave consumer crates broken until Task 3. **Therefore Task 2's compile check is scoped to `-p platform-core`, and full-workspace check happens in Task 3.**

**Files:**
- Rename dir: `crates/platform-core/src/settings/` → `crates/platform-core/src/runtime_config/`
- Modify all 6 files within + `lib.rs` + `context.rs`
- Rename test: `crates/platform-core/tests/settings_provider.rs` → `runtime_config_provider.rs`

- [ ] **Step 1: Move the directory and test file with git**

```bash
cd /Users/leosouthey/Projects/framework/lenso
git mv crates/platform-core/src/settings crates/platform-core/src/runtime_config
git mv crates/platform-core/tests/settings_provider.rs crates/platform-core/tests/runtime_config_provider.rs
```

- [ ] **Step 2: Apply the symbol rename across the moved files**

Apply these exact symbol renames in every file under `crates/platform-core/src/runtime_config/` AND `crates/platform-core/src/context.rs`, `crates/platform-core/src/lib.rs`, `crates/platform-core/tests/runtime_config_provider.rs`. Use find-and-replace per identifier (whole-word). Order longest-first to avoid partial overlaps:

| Find (whole word) | Replace |
| --- | --- |
| `StaticSettingsProvider` | `StaticRuntimeConfigProvider` |
| `PostgresSettingsProvider` | `PostgresRuntimeConfigProvider` |
| `SettingsProvider` | `RuntimeConfigProvider` |
| `SettingsRegistry` | `RuntimeConfigRegistry` |
| `SettingsSnapshot` | `RuntimeConfigSnapshot` |
| `SettingDescriptor` | `RuntimeConfigDescriptor` |
| `SettingAuditEntry` | `RuntimeConfigAuditEntry` |
| `SettingType` | `RuntimeConfigType` |
| `SettingScope` | `RuntimeConfigScope` |
| `SettingSource` | `RuntimeConfigSource` |
| `SnapshotCell` | `RuntimeConfigCell` |
| `StoredSetting` | `StoredRuntimeConfig` |

IMPORTANT ordering note: replace `SettingsProvider` etc. (the longer `Settings`-prefixed names) carefully — `SettingDescriptor` and `SettingType`/`SettingScope`/`SettingSource`/`SettingAuditEntry` start with the singular `Setting`, while `SettingsRegistry`/`SettingsSnapshot`/`SettingsProvider` use plural `Settings`. Do the two `Static`/`Postgres` provider names FIRST, then `SettingsProvider`, then the rest. A whole-word regex like `\bSettingDescriptor\b` avoids mangling `RuntimeConfigDescriptor` if run twice.

Recommended mechanical approach (run from repo root), applied to the precise file set:

```bash
FILES="crates/platform-core/src/runtime_config/mod.rs \
crates/platform-core/src/runtime_config/descriptor.rs \
crates/platform-core/src/runtime_config/snapshot.rs \
crates/platform-core/src/runtime_config/provider.rs \
crates/platform-core/src/runtime_config/postgres.rs \
crates/platform-core/src/runtime_config/store.rs \
crates/platform-core/src/context.rs \
crates/platform-core/src/lib.rs \
crates/platform-core/tests/runtime_config_provider.rs"

for f in $FILES; do
  perl -i -pe '
    s/\bStaticSettingsProvider\b/StaticRuntimeConfigProvider/g;
    s/\bPostgresSettingsProvider\b/PostgresRuntimeConfigProvider/g;
    s/\bSettingsProvider\b/RuntimeConfigProvider/g;
    s/\bSettingsRegistry\b/RuntimeConfigRegistry/g;
    s/\bSettingsSnapshot\b/RuntimeConfigSnapshot/g;
    s/\bSettingAuditEntry\b/RuntimeConfigAuditEntry/g;
    s/\bSettingDescriptor\b/RuntimeConfigDescriptor/g;
    s/\bSettingType\b/RuntimeConfigType/g;
    s/\bSettingScope\b/RuntimeConfigScope/g;
    s/\bSettingSource\b/RuntimeConfigSource/g;
    s/\bSnapshotCell\b/RuntimeConfigCell/g;
    s/\bStoredSetting\b/StoredRuntimeConfig/g;
  ' "$f"
done
```

- [ ] **Step 3: Rename the module path and the `settings` field**

These are NOT covered by the symbol table above — they are module paths / field names. Apply by hand (or extend the perl) across the same file set:

- `crate::settings::` → `crate::runtime_config::` (everywhere it appears, e.g. in `context.rs`, `postgres.rs`, `store.rs` imports, and `config_handlers.rs` later in Task 3).
- `use crate::settings::` → `use crate::runtime_config::`
- In `crates/platform-core/src/lib.rs`: `pub mod settings;` → `pub mod runtime_config;`, and the re-export line `pub use settings::{...}` → `pub use runtime_config::{...}` (with the renamed symbols).
- In `crates/platform-core/src/context.rs`: the struct field `pub settings: Arc<dyn SettingsProvider>` becomes `pub runtime_config: Arc<dyn RuntimeConfigProvider>`; the init in `AppContext::new` `settings: Arc::new(StaticSettingsProvider::empty())` becomes `runtime_config: Arc::new(StaticRuntimeConfigProvider::empty())`; the Debug field `.field("settings", &self.settings)` becomes `.field("runtime_config", &self.runtime_config)`; the builder `with_settings_provider` becomes `with_runtime_config_provider` and assigns `self.runtime_config = ...`.

Concretely for the module path + field (run from repo root over the same `$FILES`):
```bash
for f in $FILES; do
  perl -i -pe '
    s/\bcrate::settings\b/crate::runtime_config/g;
    s/\bpub mod settings\b/pub mod runtime_config/g;
    s/\buse settings::/use runtime_config::/g;
    s/\bpub use settings::/pub use runtime_config::/g;
  ' "$f"
done
```
Then hand-edit `context.rs` for the field/builder/Debug renames (the field name `settings` is too generic to blanket-replace safely):
- `pub settings: Arc<dyn RuntimeConfigProvider>,` → `pub runtime_config: Arc<dyn RuntimeConfigProvider>,`
- `settings: Arc::new(StaticRuntimeConfigProvider::empty()),` → `runtime_config: Arc::new(StaticRuntimeConfigProvider::empty()),`
- `.field("settings", &self.settings)` → `.field("runtime_config", &self.runtime_config)`
- the builder:
```rust
    pub fn with_runtime_config_provider(
        mut self,
        runtime_config: Arc<dyn RuntimeConfigProvider>,
    ) -> Self {
        self.runtime_config = runtime_config;
        self
    }
```
Also update the import in `context.rs`: `use crate::runtime_config::{RuntimeConfigProvider, StaticRuntimeConfigProvider};`

- [ ] **Step 4: Update the lib.rs module doc comment if it names `settings`**

In `crates/platform-core/src/runtime_config/mod.rs`, update the module doc comment's prose that says "settings" to "runtime config" where it refers to the module (cosmetic but keeps docs honest). The doc comment currently starts `//! Layered, console-editable configuration...` — leave the description, just ensure no stale `settings` module references remain.

- [ ] **Step 5: Scoped compile check**

Run: `cargo check --locked -p platform-core --all-targets`
Expected: PASS. The integration test `runtime_config_provider.rs` and all unit tests compile with renamed symbols. (Consumer crates are NOT checked here — they break until Task 3.)

- [ ] **Step 6: Run platform-core tests**

Run: `cargo test --locked -p platform-core runtime_config::`
Expected: PASS (all unit tests under the renamed module).

Run: `DATABASE_URL=postgres://postgres@localhost:5432/postgres cargo test --locked -p platform-core --test runtime_config_provider -- --nocapture`
Expected: PASS (the refresh round-trip test).

- [ ] **Step 7: Format and commit**

```bash
cargo fmt -p platform-core
git add crates/platform-core
git commit -m "refactor(platform-core): rename settings module to runtime_config"
```

---

## Task 3: Update all Rust consumers

Fix every crate that referenced the renamed core symbols or the `AppContext.settings` field. After this task the full workspace compiles and all Rust tests pass.

**Files:**
- `crates/platform-domain/src/lib.rs`
- `crates/app-bootstrap/src/lib.rs`
- `crates/platform-admin/src/lib.rs`, `crates/platform-admin/src/config_handlers.rs`
- `apps/api/src/main.rs`, `apps/worker/src/main.rs`
- `domains/identity/src/config.rs`, `domains/identity/src/module.rs`
- `apps/api/tests/config_console.rs`

- [ ] **Step 1: Apply the same symbol rename to all consumer files**

Run the same symbol-rename perl from Task 2 Step 2 over the consumer file set:

```bash
CONSUMERS="crates/platform-domain/src/lib.rs \
crates/app-bootstrap/src/lib.rs \
crates/platform-admin/src/lib.rs \
crates/platform-admin/src/config_handlers.rs \
apps/api/src/main.rs \
apps/worker/src/main.rs \
domains/identity/src/config.rs \
domains/identity/src/module.rs \
apps/api/tests/config_console.rs"

for f in $CONSUMERS; do
  perl -i -pe '
    s/\bStaticSettingsProvider\b/StaticRuntimeConfigProvider/g;
    s/\bPostgresSettingsProvider\b/PostgresRuntimeConfigProvider/g;
    s/\bSettingsProvider\b/RuntimeConfigProvider/g;
    s/\bSettingsRegistry\b/RuntimeConfigRegistry/g;
    s/\bSettingsSnapshot\b/RuntimeConfigSnapshot/g;
    s/\bSettingAuditEntry\b/RuntimeConfigAuditEntry/g;
    s/\bSettingDescriptor\b/RuntimeConfigDescriptor/g;
    s/\bSettingType\b/RuntimeConfigType/g;
    s/\bSettingScope\b/RuntimeConfigScope/g;
    s/\bSettingSource\b/RuntimeConfigSource/g;
    s/\bSnapshotCell\b/RuntimeConfigCell/g;
    s/\bStoredSetting\b/StoredRuntimeConfig/g;
    s/\bplatform_core::settings\b/platform_core::runtime_config/g;
  ' "$f"
done
```

- [ ] **Step 2: Rename the method/field/fn names not covered by symbols**

These are method/field/function identifiers — apply across the consumer set:

| Find (whole word) | Replace |
| --- | --- |
| `with_settings_provider` | `with_runtime_config_provider` |
| `with_settings` | `with_runtime_config` |
| `setting_descriptors` | `runtime_config_descriptors` |
| `install_settings_registry` | `install_runtime_config_registry` |
| `settings_registry` | `runtime_config_registry` |

```bash
for f in $CONSUMERS; do
  perl -i -pe '
    s/\bwith_settings_provider\b/with_runtime_config_provider/g;
    s/\bwith_settings\b/with_runtime_config/g;
    s/\bsetting_descriptors\b/runtime_config_descriptors/g;
    s/\binstall_settings_registry\b/install_runtime_config_registry/g;
    s/\bsettings_registry\b/runtime_config_registry/g;
  ' "$f"
done
```

- [ ] **Step 3: Update `ctx.settings` field accesses and the identity `SETTINGS` const**

The `AppContext.settings` field is now `runtime_config`. Update every `.settings` access on a context value (in `config_handlers.rs` it's `ctx.settings.snapshot()`). Because `.settings` is generic, do this with a targeted replace on the known call sites:

In `crates/platform-admin/src/config_handlers.rs`: `ctx.settings.snapshot()` → `ctx.runtime_config.snapshot()`.

In `apps/api/src/main.rs` and `apps/worker/src/main.rs`: any `.with_settings_provider(...)` was already renamed in Step 2; confirm no remaining `.settings` field write.

For the identity domain, rename the descriptor const `SETTINGS` → `RUNTIME_CONFIG`:
- In `domains/identity/src/config.rs`: `pub static SETTINGS: LazyLock<Vec<RuntimeConfigDescriptor>>` → `pub static RUNTIME_CONFIG: LazyLock<Vec<RuntimeConfigDescriptor>>`. Update the test inside that file that references `SETTINGS` (e.g. `SettingsRegistry::try_new(SETTINGS.clone())` is now `RuntimeConfigRegistry::try_new(RUNTIME_CONFIG.clone())`, and `SETTINGS[0]` → `RUNTIME_CONFIG[0]`).
- In `domains/identity/src/module.rs`: `.with_runtime_config(crate::config::SETTINGS.as_slice())` → `.with_runtime_config(crate::config::RUNTIME_CONFIG.as_slice())`.

Run a grep to find any stray `SETTINGS` in identity:
```bash
grep -rn "\bSETTINGS\b" domains/identity/src
```
Expected after edits: NO output.

- [ ] **Step 4: Update the `DomainDescriptor.settings` field in platform-domain**

In `crates/platform-domain/src/lib.rs`, the struct field `pub settings: &'static [RuntimeConfigDescriptor]` → `pub runtime_config: &'static [RuntimeConfigDescriptor]`; the `new` initializer `settings: &[]` → `runtime_config: &[]`; the builder body `self.settings = settings` → `self.runtime_config = runtime_config` (the builder is already named `with_runtime_config` from Step 2); the Debug field `.field("settings", &self.settings.len())` → `.field("runtime_config", &self.runtime_config.len())`.

In `crates/app-bootstrap/src/lib.rs`, the `runtime_config_descriptors` fn body iterates `domain.settings` → `domain.runtime_config`:
```rust
pub fn runtime_config_descriptors(ctx: &AppContext) -> Vec<RuntimeConfigDescriptor> {
    domains(ctx)
        .iter()
        .flat_map(|domain| domain.runtime_config.iter().cloned())
        .collect()
}
```

- [ ] **Step 5: Full workspace compile check**

Run: `cargo check --locked --workspace --all-targets`
Expected: PASS. If any `Setting*` / `.settings` / `settings_registry` reference remains, the compiler names the file:line — fix it with the same mapping.

- [ ] **Step 6: Grep for leftover old names**

Run:
```bash
grep -rnE "\b(SettingDescriptor|SettingType|SettingScope|SettingSource|SettingsRegistry|SettingsSnapshot|SettingsProvider|StaticSettingsProvider|PostgresSettingsProvider|SnapshotCell|StoredSetting|SettingAuditEntry|with_settings|setting_descriptors|install_settings_registry|settings_registry)\b" crates apps domains --include="*.rs"
```
Expected: NO output. (The DB table `setting_values`/`setting_audit` and the `config.setting_*` SQL strings are intentionally KEPT — they won't match these patterns.)

Also confirm the intentionally-kept names are still present:
```bash
grep -rn "setting_values\|setting_audit" crates/platform-core --include="*.rs" | head
```
Expected: still present in `store.rs` SQL and migration (unchanged — these are DB identifiers).

- [ ] **Step 7: Run the full Rust test suite (with Postgres)**

Run: `DATABASE_URL=postgres://postgres@localhost:5432/postgres cargo test --locked --workspace`
Expected: PASS. Notably `config_console_round_trip`, `runtime_config_provider`'s `refresh_picks_up_written_value`, and identity's config tests all green.

- [ ] **Step 8: Format and commit**

```bash
cargo fmt --all
git add crates apps domains
git commit -m "refactor: rename settings consumers to runtime_config"
```

---

## Task 4: Rename the console "Settings" surface → "Configuration"

Free the word "Settings" in the console. Rename the page file/component, the route `/settings` → `/config`, and the nav label.

**Files:**
- Rename: `apps/runtime-console/src/pages/settings-page.tsx` → `config-page.tsx`
- Modify: `apps/runtime-console/src/app/router.tsx`
- Modify: `apps/runtime-console/src/components/runtime/runtime-console-shell.tsx`

- [ ] **Step 1: Rename the page file and component**

```bash
cd /Users/leosouthey/Projects/framework/lenso
git mv apps/runtime-console/src/pages/settings-page.tsx apps/runtime-console/src/pages/config-page.tsx
```
In `apps/runtime-console/src/pages/config-page.tsx`, rename the exported component `SettingsPage` → `ConfigPage`. Also rename the inner mock-mode component if it's named `DeferredSettings` → `DeferredConfig` (cosmetic, keeps the file consistent). Update any `configQueryKeys`-style local names only if they say "settings" (the query keys were already `config*` — leave them).

Run to confirm the component rename is complete within the file:
```bash
grep -n "SettingsPage\|DeferredSettings" apps/runtime-console/src/pages/config-page.tsx
```
Expected: NO output.

- [ ] **Step 2: Update the router**

In `apps/runtime-console/src/app/router.tsx`:
- Import: `import { SettingsPage } from "../pages/settings-page";` → `import { ConfigPage } from "../pages/config-page";`
- The route definition:
```tsx
const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/settings",
  component: SettingsPage,
});
```
→
```tsx
const configRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/config",
  component: ConfigPage,
});
```
- In the `routeTree` children array, `settingsRoute` → `configRoute`.

- [ ] **Step 3: Update the nav shell**

In `apps/runtime-console/src/components/runtime/runtime-console-shell.tsx`:
- The nav item:
```tsx
const settingsNavItem = {
  to: "/settings",
  label: "Settings",
  icon: Settings,
};
```
→
```tsx
const configNavItem = {
  to: "/config",
  label: "Configuration",
  icon: Settings,
};
```
(Keep the `Settings` Lucide icon import — it's a fine icon for configuration; swapping it is optional and out of scope.)
- The render site `<NavLink {...settingsNavItem} />` → `<NavLink {...configNavItem} />`.

- [ ] **Step 4: Grep for leftover `/settings` and `SettingsPage`**

Run:
```bash
grep -rnE "/settings\b|SettingsPage|settingsRoute|settingsNavItem|settings-page" apps/runtime-console/src
```
Expected: NO output.

- [ ] **Step 5: Console quality gate**

Run: `just console-check`
Expected: PASS (format-check, oxlint/ultracite lint, typecheck, 78 tests, vite build). If a test referenced the old route/component, update it.

- [ ] **Step 6: Commit**

```bash
git add apps/runtime-console/src
git commit -m "refactor(runtime-console): rename Settings nav to Configuration"
```

---

## Final Verification

- [ ] **Run the full quality gate**

Run: `DATABASE_URL=postgres://postgres@localhost:5432/postgres just check`
Expected: PASS — fmt-check, rust-check, all tests (incl. Postgres integration), generated-check, arch-check, sdk-check, console-check.

- [ ] **Confirm the outward contract did not change**

Run: `just generated-check`
Expected: PASS with NO diff — the OpenAPI YAML and TS SDK are byte-identical because all `Config*Dto` names, routes, and the `admin-config` tag were intentionally preserved.

- [ ] **Final grep sweep**

Run:
```bash
grep -rnE "\bSetting(Descriptor|Type|Scope|Source|sRegistry|sSnapshot|sProvider|AuditEntry)\b|StaticSettingsProvider|PostgresSettingsProvider|\bSnapshotCell\b|\bStoredSetting\b" crates apps domains --include="*.rs"
grep -rnE "SettingsPage|/settings\b" apps/runtime-console/src
```
Expected: NO output from either. The DB identifiers `config.setting_values` / `config.setting_audit`, the route `/admin/config`, and the `config_changed` channel remain (intentionally).
