# Configuration System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a layered, console-editable configuration system: a typed registry of dynamic settings stored in Postgres, overlaid on the existing static `AppConfig`, read through a live in-memory snapshot on `AppContext`, propagated across instances via LISTEN/NOTIFY, and edited through `/admin/config/*` endpoints and a Runtime Console screen.

**Architecture:** A new `platform-core::settings` module defines `SettingDescriptor` (typed, per-service, with editable/restart-only flags), a `SettingsRegistry` aggregating descriptors, a `SettingsSnapshot` of effective values, and a `SettingsProvider` trait with `StaticSettingsProvider` (defaults only) and `PostgresSettingsProvider` (DB-backed + LISTEN/NOTIFY) implementations. Domains attach descriptors via `DomainDescriptor::with_settings`. The composition root aggregates them. `platform-admin` exposes read/write/audit HTTP handlers. The Runtime Console adds a settings screen consuming the generated SDK.

**Tech Stack:** Rust 2024, Axum 0.8, sqlx 0.9 (Postgres), `arc-swap`, utoipa 5.5 / utoipa-axum 0.2, serde_json; Vite/React + TanStack Query + generated TS SDK.

**Reference spec:** `docs/superpowers/specs/2026-06-02-config-system-design.md`

---

## File Structure

**New files:**
- `crates/platform-core/src/settings/mod.rs` — module root, re-exports.
- `crates/platform-core/src/settings/descriptor.rs` — `SettingScope`, `SettingType`, `SettingDescriptor`, `SettingsRegistry`, validation.
- `crates/platform-core/src/settings/snapshot.rs` — `SettingsSnapshot`, `SettingSource`, typed `get`/`get_value`.
- `crates/platform-core/src/settings/provider.rs` — `SettingsProvider` trait, `StaticSettingsProvider`.
- `crates/platform-core/src/settings/postgres.rs` — `PostgresSettingsProvider` + LISTEN/NOTIFY task.
- `crates/platform-core/migrations/0007_create_config_schema.sql` — `config.setting_values`, `config.setting_audit`.
- `crates/platform-admin/src/config_dto.rs` — config DTOs (descriptors, values, audit, write body).
- `crates/platform-admin/src/config_handlers.rs` — `/admin/config/*` handlers.
- `apps/runtime-console/src/routes/settings.tsx` — settings screen (exact path confirmed in Task 12).

**Modified files:**
- `crates/platform-core/src/lib.rs` — add `pub mod settings;` and re-exports.
- `crates/platform-core/src/migrations.rs` — register migration `0007`.
- `crates/platform-core/src/context.rs` — add `settings` field to `AppContext`.
- `crates/platform-core/Cargo.toml` — add `arc-swap`.
- `crates/platform-domain/src/lib.rs` — add `settings` field + `with_settings`.
- `crates/app-bootstrap/src/lib.rs` — add `setting_descriptors()`.
- `crates/platform-admin/src/lib.rs` — declare config modules, add config routes to `router()`.
- `apps/api/src/main.rs`, `apps/worker/src/main.rs` — build the provider and wire it into `AppContext`.
- `apps/api/src/openapi.rs` — add `admin-config` tag.
- `domains/identity/src/module.rs`, `domains/identity/src/config.rs` — migrate `IdentityConfig` onto the registry (worked example).

---

## Task 1: Descriptor types, registry, and validation

The pure core: typed descriptors, an aggregating registry, and value validation. No DB, no async — fully unit-testable.

**Files:**
- Create: `crates/platform-core/src/settings/mod.rs`
- Create: `crates/platform-core/src/settings/descriptor.rs`
- Modify: `crates/platform-core/src/lib.rs`
- Modify: `crates/platform-core/Cargo.toml`, root `Cargo.toml`

- [ ] **Step 1: Add `arc-swap` to the workspace and platform-core**

In root `Cargo.toml`, under `[workspace.dependencies]`, after `anyhow`:

```toml
arc-swap = "1"
```

In `crates/platform-core/Cargo.toml`, under `[dependencies]`, before `async-trait`:

```toml
arc-swap.workspace = true
```

- [ ] **Step 2: Verify the error variant**

Run: `grep -n "Validation" crates/platform-core/src/error.rs`
Expected: `ErrorCode::Validation` exists. Use it in the code below. (Confirmed in this repo.)

- [ ] **Step 3: Write `descriptor.rs` with types, registry, and tests**

Create `crates/platform-core/src/settings/descriptor.rs`:

```rust
use crate::error::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Which running service a setting applies to. `Shared` is stored under the
/// reserved service key `*` and used as a fallback for every service.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum SettingScope {
    Shared,
    Service(&'static str),
}

impl SettingScope {
    /// The string stored in the `service` column: `*` for shared.
    #[must_use]
    pub fn as_service_key(&self) -> &str {
        match self {
            Self::Shared => "*",
            Self::Service(name) => name,
        }
    }
}

/// The type and constraints of a setting value. Drives write validation and the
/// console edit control.
///
/// Serialized to JSON via the explicit [`SettingType::to_json`] rather than a
/// derive: an internally-tagged serde enum cannot represent the tuple variant
/// `Enum(&[&str])` (sequences can't carry a tag), so the shape is built by hand.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingType {
    Bool,
    Int { min: Option<i64>, max: Option<i64> },
    Float { min: Option<f64>, max: Option<f64> },
    String,
    Enum(&'static [&'static str]),
    Json,
}

impl SettingType {
    /// A stable JSON description for the console: `{ "kind": "...", ... }`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        match self {
            Self::Bool => serde_json::json!({ "kind": "bool" }),
            Self::Int { min, max } => serde_json::json!({ "kind": "int", "min": min, "max": max }),
            Self::Float { min, max } => {
                serde_json::json!({ "kind": "float", "min": min, "max": max })
            }
            Self::String => serde_json::json!({ "kind": "string" }),
            Self::Enum(allowed) => serde_json::json!({ "kind": "enum", "values": allowed }),
            Self::Json => serde_json::json!({ "kind": "json" }),
        }
    }
}

/// A single declared, typed, editable configuration key.
#[derive(Debug, Clone)]
pub struct SettingDescriptor {
    pub key: &'static str,
    pub scope: SettingScope,
    pub value_type: SettingType,
    pub default: Value,
    pub editable: bool,
    pub restart_only: bool,
    pub description: &'static str,
}

impl SettingDescriptor {
    /// Validate a candidate value against this descriptor's type and constraints.
    pub fn validate(&self, value: &Value) -> AppResult<()> {
        let ok = match &self.value_type {
            SettingType::Bool => value.is_boolean(),
            SettingType::Int { min, max } => value
                .as_i64()
                .is_some_and(|n| min.is_none_or(|lo| n >= lo) && max.is_none_or(|hi| n <= hi)),
            SettingType::Float { min, max } => value
                .as_f64()
                .is_some_and(|n| min.is_none_or(|lo| n >= lo) && max.is_none_or(|hi| n <= hi)),
            SettingType::String => value.is_string(),
            SettingType::Enum(allowed) => value.as_str().is_some_and(|s| allowed.contains(&s)),
            SettingType::Json => true,
        };
        if ok {
            Ok(())
        } else {
            Err(AppError::new(
                ErrorCode::Validation,
                format!("value for `{}` failed validation", self.key),
            ))
        }
    }
}

/// An immutable, validated set of descriptors indexed by `(service_key, key)`.
#[derive(Debug, Clone, Default)]
pub struct SettingsRegistry {
    by_scope_key: BTreeMap<(String, String), SettingDescriptor>,
}

impl SettingsRegistry {
    /// Build a registry, rejecting duplicate `(scope, key)` pairs.
    pub fn try_new(descriptors: Vec<SettingDescriptor>) -> AppResult<Self> {
        let mut by_scope_key = BTreeMap::new();
        for descriptor in descriptors {
            let index = (
                descriptor.scope.as_service_key().to_owned(),
                descriptor.key.to_owned(),
            );
            if by_scope_key.contains_key(&index) {
                return Err(AppError::new(
                    ErrorCode::Internal,
                    format!("duplicate setting descriptor for {index:?}"),
                ));
            }
            by_scope_key.insert(index, descriptor);
        }
        Ok(Self { by_scope_key })
    }

    /// Look up a descriptor by exact scope and key.
    #[must_use]
    pub fn get(&self, scope: &SettingScope, key: &str) -> Option<&SettingDescriptor> {
        self.by_scope_key
            .get(&(scope.as_service_key().to_owned(), key.to_owned()))
    }

    /// Look up a descriptor by raw service-key string and key.
    #[must_use]
    pub fn get_raw(&self, service_key: &str, key: &str) -> Option<&SettingDescriptor> {
        self.by_scope_key.get(&(service_key.to_owned(), key.to_owned()))
    }

    /// All descriptors, ordered by `(service_key, key)`.
    pub fn iter(&self) -> impl Iterator<Item = &SettingDescriptor> {
        self.by_scope_key.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bool_descriptor() -> SettingDescriptor {
        SettingDescriptor {
            key: "demo.enabled",
            scope: SettingScope::Shared,
            value_type: SettingType::Bool,
            default: json!(true),
            editable: true,
            restart_only: false,
            description: "demo flag",
        }
    }

    #[test]
    fn validates_bool() {
        let d = bool_descriptor();
        assert!(d.validate(&json!(false)).is_ok());
        assert!(d.validate(&json!("nope")).is_err());
    }

    #[test]
    fn validates_int_range() {
        let d = SettingDescriptor {
            key: "demo.count",
            scope: SettingScope::Service("api"),
            value_type: SettingType::Int { min: Some(1), max: Some(10) },
            default: json!(5),
            editable: true,
            restart_only: false,
            description: "count",
        };
        assert!(d.validate(&json!(5)).is_ok());
        assert!(d.validate(&json!(0)).is_err());
        assert!(d.validate(&json!(11)).is_err());
        assert!(d.validate(&json!("x")).is_err());
    }

    #[test]
    fn validates_enum() {
        let d = SettingDescriptor {
            key: "demo.mode",
            scope: SettingScope::Shared,
            value_type: SettingType::Enum(&["a", "b"]),
            default: json!("a"),
            editable: true,
            restart_only: false,
            description: "mode",
        };
        assert!(d.validate(&json!("a")).is_ok());
        assert!(d.validate(&json!("c")).is_err());
    }

    #[test]
    fn registry_rejects_duplicate_scope_key() {
        let result = SettingsRegistry::try_new(vec![bool_descriptor(), bool_descriptor()]);
        assert!(result.is_err());
    }

    #[test]
    fn registry_looks_up_by_scope_and_key() {
        let registry = SettingsRegistry::try_new(vec![bool_descriptor()]).unwrap();
        assert!(registry.get(&SettingScope::Shared, "demo.enabled").is_some());
        assert!(registry.get(&SettingScope::Service("api"), "demo.enabled").is_none());
    }
}
```

- [ ] **Step 4: Create the module root**

Create `crates/platform-core/src/settings/mod.rs`:

```rust
//! Layered, console-editable configuration overlaid on the static `AppConfig`.
//!
//! Domains and platform crates declare typed [`SettingDescriptor`]s; a
//! [`SettingsRegistry`] aggregates them; later tasks add the snapshot and
//! provider that resolve effective values from defaults plus stored overrides.

mod descriptor;

pub use descriptor::{SettingDescriptor, SettingScope, SettingType, SettingsRegistry};
```

- [ ] **Step 5: Register the module and re-exports**

In `crates/platform-core/src/lib.rs`, add `pub mod settings;` after `pub mod outbox;`. Add after the `outbox` re-export block:

```rust
pub use settings::{SettingDescriptor, SettingScope, SettingType, SettingsRegistry};
```

- [ ] **Step 6: Run the tests**

Run: `cargo test --locked -p platform-core settings::`
Expected: PASS (five tests).

- [ ] **Step 7: Commit**

```bash
git add crates/platform-core/src/settings crates/platform-core/src/lib.rs \
  crates/platform-core/Cargo.toml Cargo.toml Cargo.lock
git commit -m "feat(platform-core): add setting descriptor registry and validation"
```

---

## Task 2: Settings snapshot and typed reads

An immutable snapshot of effective values for one running service, with resolution order (service row → shared row → default) and typed accessors. Pure and unit-testable.

**Files:**
- Create: `crates/platform-core/src/settings/snapshot.rs`
- Modify: `crates/platform-core/src/settings/mod.rs`
- Modify: `crates/platform-core/src/lib.rs`

- [ ] **Step 1: Write `snapshot.rs` with the snapshot, source, and tests**

Create `crates/platform-core/src/settings/snapshot.rs`:

```rust
use crate::error::{AppError, AppResult, ErrorCode};
use crate::settings::descriptor::SettingsRegistry;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Where an effective value came from, for display in the console.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingSource {
    /// A stored row scoped to this service.
    Override,
    /// A stored row scoped to `*` (shared).
    Shared,
    /// No stored row; the descriptor default.
    Default,
}

/// Effective configuration for a single running service: every registered key
/// resolved to a concrete value plus the source it came from.
#[derive(Debug, Clone, Default)]
pub struct SettingsSnapshot {
    /// key -> (value, source)
    values: BTreeMap<String, (Value, SettingSource)>,
}

impl SettingsSnapshot {
    /// Resolve every descriptor for `service_key` against the stored rows.
    ///
    /// `stored` maps `(service_key, key)` to the stored JSON value. Resolution
    /// order per key: a row for this service, else a `*` row, else the default.
    /// Stored values failing validation fall back to the default.
    #[must_use]
    pub fn resolve(
        registry: &SettingsRegistry,
        service_key: &str,
        stored: &BTreeMap<(String, String), Value>,
    ) -> Self {
        let mut values = BTreeMap::new();
        for descriptor in registry.iter() {
            // Only descriptors applicable to this service or shared.
            let applies =
                descriptor.scope.as_service_key() == service_key || descriptor.scope.as_service_key() == "*";
            if !applies {
                continue;
            }
            let key = descriptor.key.to_owned();
            let service_row = stored.get(&(service_key.to_owned(), key.clone()));
            let shared_row = stored.get(&("*".to_owned(), key.clone()));

            let (value, source) = match (service_row, shared_row) {
                (Some(v), _) if descriptor.validate(v).is_ok() => (v.clone(), SettingSource::Override),
                (_, Some(v)) if descriptor.validate(v).is_ok() => (v.clone(), SettingSource::Shared),
                _ => (descriptor.default.clone(), SettingSource::Default),
            };
            values.insert(key, (value, source));
        }
        Self { values }
    }

    /// The raw effective value for a key, if registered.
    #[must_use]
    pub fn raw(&self, key: &str) -> Option<&Value> {
        self.values.get(key).map(|(value, _)| value)
    }

    /// The source of a key's effective value, if registered.
    #[must_use]
    pub fn source(&self, key: &str) -> Option<SettingSource> {
        self.values.get(key).map(|(_, source)| *source)
    }

    /// Deserialize a single key into a typed value.
    pub fn get_value<T: DeserializeOwned>(&self, key: &str) -> AppResult<T> {
        let value = self.raw(key).ok_or_else(|| {
            AppError::new(ErrorCode::Internal, format!("unknown setting key `{key}`"))
        })?;
        serde_json::from_value(value.clone()).map_err(|source| {
            AppError::new(ErrorCode::Internal, format!("setting `{key}` deserialize failed"))
                .with_source(source)
        })
    }

    /// Build a typed struct whose fields are keys sharing `prefix` + `.`.
    ///
    /// Example: prefix `"identity"` with key `"identity.password_reset_ttl_minutes"`
    /// produces an object field `password_reset_ttl_minutes`.
    pub fn get<T: DeserializeOwned>(&self, prefix: &str) -> AppResult<T> {
        let dotted = format!("{prefix}.");
        let mut object = Map::new();
        for (key, (value, _)) in &self.values {
            if let Some(field) = key.strip_prefix(&dotted) {
                object.insert(field.to_owned(), value.clone());
            }
        }
        serde_json::from_value(Value::Object(object)).map_err(|source| {
            AppError::new(ErrorCode::Internal, format!("settings `{prefix}` deserialize failed"))
                .with_source(source)
        })
    }

    /// All resolved keys with their value and source, for the console values API.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &Value, SettingSource)> {
        self.values.iter().map(|(k, (v, s))| (k.as_str(), v, *s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::descriptor::{SettingDescriptor, SettingScope, SettingType};
    use serde::Deserialize;
    use serde_json::json;

    fn registry() -> SettingsRegistry {
        SettingsRegistry::try_new(vec![
            SettingDescriptor {
                key: "identity.password_reset_ttl_minutes",
                scope: SettingScope::Shared,
                value_type: SettingType::Int { min: Some(1), max: Some(1440) },
                default: json!(30),
                editable: true,
                restart_only: false,
                description: "ttl",
            },
            SettingDescriptor {
                key: "api.feature.enabled",
                scope: SettingScope::Service("api"),
                value_type: SettingType::Bool,
                default: json!(false),
                editable: true,
                restart_only: false,
                description: "flag",
            },
        ])
        .unwrap()
    }

    #[test]
    fn falls_back_to_default() {
        let snapshot = SettingsSnapshot::resolve(&registry(), "api", &BTreeMap::new());
        assert_eq!(snapshot.raw("identity.password_reset_ttl_minutes"), Some(&json!(30)));
        assert_eq!(snapshot.source("api.feature.enabled"), Some(SettingSource::Default));
    }

    #[test]
    fn service_row_overrides_shared_and_default() {
        let mut stored = BTreeMap::new();
        stored.insert(("api".to_owned(), "api.feature.enabled".to_owned()), json!(true));
        let snapshot = SettingsSnapshot::resolve(&registry(), "api", &stored);
        assert_eq!(snapshot.raw("api.feature.enabled"), Some(&json!(true)));
        assert_eq!(snapshot.source("api.feature.enabled"), Some(SettingSource::Override));
    }

    #[test]
    fn invalid_stored_value_falls_back_to_default() {
        let mut stored = BTreeMap::new();
        stored.insert(
            ("*".to_owned(), "identity.password_reset_ttl_minutes".to_owned()),
            json!(99999),
        );
        let snapshot = SettingsSnapshot::resolve(&registry(), "api", &stored);
        assert_eq!(snapshot.raw("identity.password_reset_ttl_minutes"), Some(&json!(30)));
        assert_eq!(
            snapshot.source("identity.password_reset_ttl_minutes"),
            Some(SettingSource::Default)
        );
    }

    #[test]
    fn typed_struct_get_by_prefix() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct IdentityConfig {
            password_reset_ttl_minutes: u64,
        }
        let snapshot = SettingsSnapshot::resolve(&registry(), "api", &BTreeMap::new());
        let cfg: IdentityConfig = snapshot.get("identity").unwrap();
        assert_eq!(cfg, IdentityConfig { password_reset_ttl_minutes: 30 });
    }

    #[test]
    fn service_scoped_key_excluded_for_other_service() {
        let snapshot = SettingsSnapshot::resolve(&registry(), "worker", &BTreeMap::new());
        assert!(snapshot.raw("api.feature.enabled").is_none());
    }
}
```

- [ ] **Step 2: Export from the module and crate**

In `crates/platform-core/src/settings/mod.rs`, add `mod snapshot;` after `mod descriptor;` and extend the re-export:

```rust
mod descriptor;
mod snapshot;

pub use descriptor::{SettingDescriptor, SettingScope, SettingType, SettingsRegistry};
pub use snapshot::{SettingSource, SettingsSnapshot};
```

In `crates/platform-core/src/lib.rs`, extend the settings re-export:

```rust
pub use settings::{
    SettingDescriptor, SettingScope, SettingSource, SettingType, SettingsRegistry, SettingsSnapshot,
};
```

- [ ] **Step 3: Run the tests**

Run: `cargo test --locked -p platform-core settings::`
Expected: PASS (Task 1 + Task 2 tests).

- [ ] **Step 4: Commit**

```bash
git add crates/platform-core/src/settings crates/platform-core/src/lib.rs
git commit -m "feat(platform-core): resolve settings snapshot with typed reads"
```

---

## Task 3: Provider trait, static provider, and `AppContext` wiring

The `SettingsProvider` trait, an in-memory `StaticSettingsProvider` (defaults only, no DB), and the new `settings` field on `AppContext`. After this task every existing call site still compiles, defaulting to the static provider.

**Files:**
- Create: `crates/platform-core/src/settings/provider.rs`
- Modify: `crates/platform-core/src/settings/mod.rs`, `crates/platform-core/src/lib.rs`
- Modify: `crates/platform-core/src/context.rs`

- [ ] **Step 1: Write `provider.rs` with the trait, static provider, and tests**

Create `crates/platform-core/src/settings/provider.rs`:

```rust
use crate::settings::descriptor::SettingsRegistry;
use crate::settings::snapshot::SettingsSnapshot;
use arc_swap::ArcSwap;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::Arc;

/// Read access to the current effective configuration for this service.
///
/// Reads are cheap and never touch the database; they read the in-memory
/// snapshot maintained by the implementation.
pub trait SettingsProvider: Debug + Send + Sync {
    /// The current effective snapshot.
    fn snapshot(&self) -> Arc<SettingsSnapshot>;
}

/// Defaults-only provider for tests, the migrate app, and any context without a
/// database-backed configuration source.
#[derive(Debug)]
pub struct StaticSettingsProvider {
    snapshot: Arc<SettingsSnapshot>,
}

impl StaticSettingsProvider {
    /// Resolve a snapshot for `service_key` from registry defaults (no overrides).
    #[must_use]
    pub fn new(registry: &SettingsRegistry, service_key: &str) -> Self {
        let snapshot = SettingsSnapshot::resolve(registry, service_key, &BTreeMap::new());
        Self { snapshot: Arc::new(snapshot) }
    }

    /// An empty provider (no registered settings) for minimal test contexts.
    #[must_use]
    pub fn empty() -> Self {
        Self { snapshot: Arc::new(SettingsSnapshot::default()) }
    }
}

impl SettingsProvider for StaticSettingsProvider {
    fn snapshot(&self) -> Arc<SettingsSnapshot> {
        Arc::clone(&self.snapshot)
    }
}

/// Shared, atomically swappable snapshot cell used by the Postgres provider and
/// its background refresh task (Task 5).
#[derive(Debug)]
pub struct SnapshotCell {
    inner: ArcSwap<SettingsSnapshot>,
}

impl SnapshotCell {
    #[must_use]
    pub fn new(initial: SettingsSnapshot) -> Self {
        Self { inner: ArcSwap::from_pointee(initial) }
    }

    #[must_use]
    pub fn load(&self) -> Arc<SettingsSnapshot> {
        self.inner.load_full()
    }

    pub fn store(&self, snapshot: SettingsSnapshot) {
        self.inner.store(Arc::new(snapshot));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::descriptor::{SettingDescriptor, SettingScope, SettingType};
    use serde_json::json;

    #[test]
    fn static_provider_serves_defaults() {
        let registry = SettingsRegistry::try_new(vec![SettingDescriptor {
            key: "demo.enabled",
            scope: SettingScope::Shared,
            value_type: SettingType::Bool,
            default: json!(true),
            editable: true,
            restart_only: false,
            description: "flag",
        }])
        .unwrap();
        let provider = StaticSettingsProvider::new(&registry, "api");
        assert_eq!(provider.snapshot().raw("demo.enabled"), Some(&json!(true)));
    }

    #[test]
    fn snapshot_cell_swaps() {
        let cell = SnapshotCell::new(SettingsSnapshot::default());
        assert!(cell.load().raw("x").is_none());
        let registry = SettingsRegistry::try_new(vec![SettingDescriptor {
            key: "x",
            scope: SettingScope::Shared,
            value_type: SettingType::Bool,
            default: json!(false),
            editable: true,
            restart_only: false,
            description: "x",
        }])
        .unwrap();
        cell.store(SettingsSnapshot::resolve(&registry, "api", &BTreeMap::new()));
        assert_eq!(cell.load().raw("x"), Some(&json!(false)));
    }
}
```

- [ ] **Step 2: Export from module and crate**

In `crates/platform-core/src/settings/mod.rs`:

```rust
mod descriptor;
mod provider;
mod snapshot;

pub use descriptor::{SettingDescriptor, SettingScope, SettingType, SettingsRegistry};
pub use provider::{SettingsProvider, SnapshotCell, StaticSettingsProvider};
pub use snapshot::{SettingSource, SettingsSnapshot};
```

In `crates/platform-core/src/lib.rs`, extend the re-export:

```rust
pub use settings::{
    SettingDescriptor, SettingScope, SettingSource, SettingType, SettingsProvider,
    SettingsRegistry, SettingsSnapshot, SnapshotCell, StaticSettingsProvider,
};
```

- [ ] **Step 3: Add the `settings` field to `AppContext`**

In `crates/platform-core/src/context.rs`, add the import near the other settings-free imports:

```rust
use crate::settings::{SettingsProvider, StaticSettingsProvider};
```

Add the field to the struct (after `execution_logs`):

```rust
    pub settings: Arc<dyn SettingsProvider>,
```

In `AppContext::new`, initialize it to an empty static provider so existing
callers keep working without changes:

```rust
            settings: Arc::new(StaticSettingsProvider::empty()),
```

Add a builder, after `with_execution_log_provider`:

```rust
    pub fn with_settings_provider(mut self, settings: Arc<dyn SettingsProvider>) -> Self {
        self.settings = settings;
        self
    }
```

In the `Debug` impl for `AppContext`, add a field line so it stays informative:

```rust
            .field("settings", &self.settings)
```

- [ ] **Step 4: Run the tests and a workspace check**

Run: `cargo test --locked -p platform-core settings::`
Expected: PASS.

Run: `cargo check --locked --workspace`
Expected: PASS — `AppContext::new` callers in apps/tests still compile because `settings` has a default.

- [ ] **Step 5: Commit**

```bash
git add crates/platform-core/src/settings crates/platform-core/src/lib.rs \
  crates/platform-core/src/context.rs
git commit -m "feat(platform-core): add settings provider and AppContext field"
```

---

## Task 4: `config` schema migration and data-access helpers

The Postgres `config` schema plus reusable async helpers to read all stored values and to upsert a value with an audit row in one transaction. These helpers are used by both the Postgres provider (Task 5) and the console handlers (Task 8).

**Files:**
- Create: `crates/platform-core/migrations/0007_create_config_schema.sql`
- Create: `crates/platform-core/src/settings/store.rs`
- Modify: `crates/platform-core/src/migrations.rs`
- Modify: `crates/platform-core/src/settings/mod.rs`, `crates/platform-core/src/lib.rs`

- [ ] **Step 1: Write the migration**

Create `crates/platform-core/migrations/0007_create_config_schema.sql`:

```sql
create schema if not exists config;

create table if not exists config.setting_values (
    service text not null,
    key text not null,
    value jsonb not null,
    updated_at timestamptz not null default now(),
    updated_by text,
    primary key (service, key)
);

create table if not exists config.setting_audit (
    id uuid primary key,
    service text not null,
    key text not null,
    old_value jsonb,
    new_value jsonb not null,
    actor text,
    changed_at timestamptz not null default now()
);

create index if not exists setting_audit_key_idx
    on config.setting_audit (service, key, changed_at desc);
```

- [ ] **Step 2: Register the migration**

In `crates/platform-core/src/migrations.rs`, add to the end of the `PLATFORM_MIGRATIONS` array:

```rust
    Migration {
        name: "platform/0007_create_config_schema",
        sql: include_str!("../migrations/0007_create_config_schema.sql"),
    },
```

- [ ] **Step 3: Write `store.rs` data-access helpers**

Create `crates/platform-core/src/settings/store.rs`:

```rust
use crate::db::DbPool;
use crate::error::{AppError, AppResult, ErrorCode};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::BTreeMap;
use uuid::Uuid;

/// One stored setting row plus metadata, for the audit/values console views.
#[derive(Debug, Clone)]
pub struct StoredSetting {
    pub service: String,
    pub key: String,
    pub value: Value,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<String>,
}

/// One audit-log row.
#[derive(Debug, Clone)]
pub struct SettingAuditEntry {
    pub service: String,
    pub key: String,
    pub old_value: Option<Value>,
    pub new_value: Value,
    pub actor: Option<String>,
    pub changed_at: DateTime<Utc>,
}

/// Load every stored value into a `(service, key) -> value` map for snapshot
/// resolution.
pub async fn load_all_values(pool: &DbPool) -> AppResult<BTreeMap<(String, String), Value>> {
    let rows = sqlx::query_as::<_, (String, String, Value)>(
        "select service, key, value from config.setting_values",
    )
    .fetch_all(pool)
    .await
    .map_err(store_error)?;

    Ok(rows
        .into_iter()
        .map(|(service, key, value)| ((service, key), value))
        .collect())
}

/// Upsert a value and insert an audit row in one transaction. Returns the new
/// stored row. `id` is generated by the caller via the platform id generator is
/// not required here; a v7 uuid is used directly for the audit primary key.
pub async fn upsert_value(
    pool: &DbPool,
    service: &str,
    key: &str,
    value: &Value,
    actor: Option<&str>,
) -> AppResult<StoredSetting> {
    let mut tx = pool.begin().await.map_err(store_error)?;

    let old_value = sqlx::query_scalar::<_, Value>(
        "select value from config.setting_values where service = $1 and key = $2",
    )
    .bind(service)
    .bind(key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(store_error)?;

    let row = sqlx::query_as::<_, (String, String, Value, DateTime<Utc>, Option<String>)>(
        r#"
        insert into config.setting_values (service, key, value, updated_at, updated_by)
        values ($1, $2, $3, now(), $4)
        on conflict (service, key)
        do update set value = excluded.value, updated_at = now(), updated_by = excluded.updated_by
        returning service, key, value, updated_at, updated_by
        "#,
    )
    .bind(service)
    .bind(key)
    .bind(value)
    .bind(actor)
    .fetch_one(&mut *tx)
    .await
    .map_err(store_error)?;

    sqlx::query(
        r#"
        insert into config.setting_audit (id, service, key, old_value, new_value, actor, changed_at)
        values ($1, $2, $3, $4, $5, $6, now())
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(service)
    .bind(key)
    .bind(&old_value)
    .bind(value)
    .bind(actor)
    .execute(&mut *tx)
    .await
    .map_err(store_error)?;

    tx.commit().await.map_err(store_error)?;

    Ok(StoredSetting {
        service: row.0,
        key: row.1,
        value: row.2,
        updated_at: row.3,
        updated_by: row.4,
    })
}

/// Delete a stored row (reset to shared/default), recording an audit entry if a
/// row existed. Returns true if a row was deleted.
pub async fn delete_value(
    pool: &DbPool,
    service: &str,
    key: &str,
    actor: Option<&str>,
) -> AppResult<bool> {
    let mut tx = pool.begin().await.map_err(store_error)?;
    let old_value = sqlx::query_scalar::<_, Value>(
        "delete from config.setting_values where service = $1 and key = $2 returning value",
    )
    .bind(service)
    .bind(key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(store_error)?;

    let deleted = old_value.is_some();
    if let Some(old) = old_value {
        sqlx::query(
            r#"
            insert into config.setting_audit (id, service, key, old_value, new_value, actor, changed_at)
            values ($1, $2, $3, $4, 'null'::jsonb, $5, now())
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(service)
        .bind(key)
        .bind(&old)
        .bind(actor)
        .execute(&mut *tx)
        .await
        .map_err(store_error)?;
    }
    tx.commit().await.map_err(store_error)?;
    Ok(deleted)
}

/// Audit history for one `(service, key)`, newest first.
pub async fn load_audit(
    pool: &DbPool,
    service: &str,
    key: &str,
    limit: i64,
) -> AppResult<Vec<SettingAuditEntry>> {
    let rows = sqlx::query_as::<
        _,
        (String, String, Option<Value>, Value, Option<String>, DateTime<Utc>),
    >(
        r#"
        select service, key, old_value, new_value, actor, changed_at
        from config.setting_audit
        where service = $1 and key = $2
        order by changed_at desc
        limit $3
        "#,
    )
    .bind(service)
    .bind(key)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(store_error)?;

    Ok(rows
        .into_iter()
        .map(|(service, key, old_value, new_value, actor, changed_at)| SettingAuditEntry {
            service,
            key,
            old_value,
            new_value,
            actor,
            changed_at,
        })
        .collect())
}

fn store_error(source: sqlx::Error) -> AppError {
    AppError::new(ErrorCode::Internal, "settings store query failed").with_source(source)
}
```

- [ ] **Step 4: Export the store from the module and crate**

In `crates/platform-core/src/settings/mod.rs`, add `pub mod store;` after `mod snapshot;` (public so `platform-admin` can call the helpers), and re-export the row types:

```rust
pub mod store;

pub use store::{SettingAuditEntry, StoredSetting};
```

In `crates/platform-core/src/lib.rs`, extend the settings re-export to include `SettingAuditEntry, StoredSetting`. Keep the list alphabetized.

- [ ] **Step 5: Compile check (DB helpers are integration-tested in Task 6)**

Run: `cargo check --locked -p platform-core`
Expected: PASS. Note: `sqlx` here uses the runtime query API (`query_as`, not the compile-time `query!` macro), so no live DB is needed to compile.

- [ ] **Step 6: Commit**

```bash
git add crates/platform-core/migrations/0007_create_config_schema.sql \
  crates/platform-core/src/migrations.rs crates/platform-core/src/settings \
  crates/platform-core/src/lib.rs
git commit -m "feat(platform-core): add config schema and settings store helpers"
```

---

## Task 5: Postgres provider with LISTEN/NOTIFY refresh

The DB-backed provider: loads an initial snapshot, holds it in a `SnapshotCell`, exposes a `refresh()` that reloads from the store, and spawns a background task that `LISTEN`s on `config_changed` and refreshes on every notification (reconnecting + full reload on connection loss, so a missed notification self-heals).

**Files:**
- Create: `crates/platform-core/src/settings/postgres.rs`
- Modify: `crates/platform-core/src/settings/mod.rs`, `crates/platform-core/src/lib.rs`
- Modify: `crates/platform-core/Cargo.toml` (ensure `tokio` is a dependency)

- [ ] **Step 1: Ensure tokio is available to platform-core**

Run: `grep -n "tokio" crates/platform-core/Cargo.toml`
If absent, add under `[dependencies]`:

```toml
tokio.workspace = true
```

- [ ] **Step 2: Write `postgres.rs`**

Create `crates/platform-core/src/settings/postgres.rs`:

```rust
use crate::db::DbPool;
use crate::error::AppResult;
use crate::settings::descriptor::SettingsRegistry;
use crate::settings::provider::{SettingsProvider, SnapshotCell};
use crate::settings::snapshot::SettingsSnapshot;
use crate::settings::store::load_all_values;
use sqlx::postgres::PgListener;
use std::sync::Arc;

/// The channel name used for cross-instance config-change notifications.
pub const CONFIG_NOTIFY_CHANNEL: &str = "config_changed";

/// Database-backed settings provider. Holds an atomically swappable snapshot
/// resolved from the registry plus stored overrides for one running service.
#[derive(Debug)]
pub struct PostgresSettingsProvider {
    pool: DbPool,
    registry: Arc<SettingsRegistry>,
    service_key: String,
    cell: Arc<SnapshotCell>,
}

impl PostgresSettingsProvider {
    /// Construct the provider and load the initial snapshot from the store.
    pub async fn connect(
        pool: DbPool,
        registry: Arc<SettingsRegistry>,
        service_key: impl Into<String>,
    ) -> AppResult<Arc<Self>> {
        let service_key = service_key.into();
        let stored = load_all_values(&pool).await?;
        let snapshot = SettingsSnapshot::resolve(&registry, &service_key, &stored);
        let cell = Arc::new(SnapshotCell::new(snapshot));
        Ok(Arc::new(Self { pool, registry, service_key, cell }))
    }

    /// Reload all stored values and swap in a fresh snapshot.
    pub async fn refresh(&self) -> AppResult<()> {
        let stored = load_all_values(&self.pool).await?;
        let snapshot = SettingsSnapshot::resolve(&self.registry, &self.service_key, &stored);
        self.cell.store(snapshot);
        Ok(())
    }

    /// Spawn the background LISTEN task. Refreshes on every notification and
    /// fully reloads on (re)connect, so missed notifications self-heal.
    pub fn spawn_listener(self: &Arc<Self>) {
        let provider = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                match PgListener::connect_with(&provider.pool).await {
                    Ok(mut listener) => {
                        if let Err(error) = listener.listen(CONFIG_NOTIFY_CHANNEL).await {
                            tracing::warn!(error = ?error, "config listener failed to subscribe");
                            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                            continue;
                        }
                        // Reconcile after (re)subscribing in case we missed events.
                        if let Err(error) = provider.refresh().await {
                            tracing::warn!(error = ?error, "config refresh after subscribe failed");
                        }
                        loop {
                            match listener.recv().await {
                                Ok(notification) => {
                                    tracing::debug!(
                                        payload = %notification.payload(),
                                        "config change notification received"
                                    );
                                    if let Err(error) = provider.refresh().await {
                                        tracing::warn!(error = ?error, "config refresh failed");
                                    }
                                }
                                Err(error) => {
                                    tracing::warn!(error = ?error, "config listener disconnected");
                                    break;
                                }
                            }
                        }
                    }
                    Err(error) => {
                        tracing::warn!(error = ?error, "config listener connect failed");
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        });
    }
}

impl SettingsProvider for PostgresSettingsProvider {
    fn snapshot(&self) -> Arc<SettingsSnapshot> {
        self.cell.load()
    }
}
```

- [ ] **Step 3: Export from module and crate**

In `crates/platform-core/src/settings/mod.rs`:

```rust
mod postgres;

pub use postgres::{CONFIG_NOTIFY_CHANNEL, PostgresSettingsProvider};
```

In `crates/platform-core/src/lib.rs`, extend the settings re-export to include `CONFIG_NOTIFY_CHANNEL, PostgresSettingsProvider`.

- [ ] **Step 4: Compile check**

Run: `cargo check --locked -p platform-core`
Expected: PASS. (The LISTEN behavior is exercised by the integration test in Task 11.)

- [ ] **Step 5: Commit**

```bash
git add crates/platform-core/src/settings crates/platform-core/src/lib.rs \
  crates/platform-core/Cargo.toml
git commit -m "feat(platform-core): add postgres settings provider with listen/notify"
```

---

## Task 6: Domain descriptor `with_settings` and composition-root aggregation

Let domains contribute setting descriptors and aggregate them at the composition root, mirroring `story_display_descriptors()`.

**Files:**
- Modify: `crates/platform-domain/src/lib.rs`
- Modify: `crates/app-bootstrap/src/lib.rs`
- Modify: `crates/app-bootstrap/Cargo.toml` (if it lacks `platform-core`; confirm first)

- [ ] **Step 1: Add the `settings` field and builder to `DomainDescriptor`**

In `crates/platform-domain/src/lib.rs`, add to the imports:

```rust
use platform_core::{EventHandler, SettingDescriptor, StoryDisplayDescriptor};
```

Add the field to the struct (after `story_display`):

```rust
    /// Editable configuration keys owned by the domain.
    pub settings: &'static [SettingDescriptor],
```

Initialize it in `DomainDescriptor::new` (after `story_display: &[]`):

```rust
            settings: &[],
```

Add the builder, after `with_story_display`:

```rust
    /// Attach editable configuration descriptors for the domain.
    #[must_use]
    pub fn with_settings(mut self, settings: &'static [SettingDescriptor]) -> Self {
        self.settings = settings;
        self
    }
```

In the `Debug` impl, add:

```rust
            .field("settings", &self.settings.len())
```

- [ ] **Step 2: Aggregate descriptors at the composition root**

In `crates/app-bootstrap/src/lib.rs`, add to the imports:

```rust
use platform_core::{AppContext, EventHandlerRegistry, SettingDescriptor, StoryDisplayDescriptor};
```

Add a function after `story_display_descriptors`:

```rust
/// Every domain's setting descriptors, plus any platform-owned descriptors.
///
/// The single source for the editable configuration registry. Apps build a
/// `SettingsRegistry` from this list at startup.
#[must_use]
pub fn setting_descriptors(ctx: &AppContext) -> Vec<SettingDescriptor> {
    domains(ctx)
        .iter()
        .flat_map(|domain| domain.settings.iter().cloned())
        .collect()
}
```

- [ ] **Step 3: Compile check**

Run: `cargo check --locked -p platform-domain -p app-bootstrap`
Expected: PASS. Existing domains compile unchanged because `settings` defaults to `&[]`.

- [ ] **Step 4: Commit**

```bash
git add crates/platform-domain/src/lib.rs crates/app-bootstrap/src/lib.rs
git commit -m "feat(platform): aggregate domain setting descriptors at composition root"
```

---

## Task 7: Worked example — migrate `IdentityConfig` onto the registry

Prove the read path end to end by moving the identity domain's one config value (`password_reset_ttl_minutes`) onto the registry and reading it via `ctx.settings`.

**Files:**
- Modify: `domains/identity/src/config.rs`
- Modify: `domains/identity/src/module.rs`

- [ ] **Step 1: Declare the descriptor and a deserializable config struct**

`SettingDescriptor.default` is a `serde_json::Value`, which is not const-constructible, so the descriptor list is a `LazyLock<Vec<_>>` rather than a `const`. A `static LazyLock<Vec<SettingDescriptor>>` derefs to `&'static [SettingDescriptor]` via `SETTINGS.as_slice()`, which is exactly what `with_settings` expects. Use this form in every domain.

Replace `domains/identity/src/config.rs` with:

```rust
use platform_core::{SettingDescriptor, SettingScope, SettingType};
use serde::Deserialize;
use serde_json::json;
use std::sync::LazyLock;

/// Identity domain configuration, resolved from the settings snapshot under the
/// `identity.` key prefix.
#[derive(Debug, Clone, Deserialize)]
pub struct IdentityConfig {
    pub password_reset_ttl_minutes: u64,
}

impl Default for IdentityConfig {
    fn default() -> Self {
        Self { password_reset_ttl_minutes: 30 }
    }
}

/// Editable settings owned by the identity domain.
pub static SETTINGS: LazyLock<Vec<SettingDescriptor>> = LazyLock::new(|| {
    vec![SettingDescriptor {
        key: "identity.password_reset_ttl_minutes",
        scope: SettingScope::Shared,
        value_type: SettingType::Int { min: Some(5), max: Some(1440) },
        default: json!(30),
        editable: true,
        restart_only: false,
        description: "Minutes a password reset token remains valid.",
    }]
});
```

- [ ] **Step 2: Attach settings to the identity descriptor**

In `domains/identity/src/module.rs`, change the `domain` function:

```rust
pub fn domain(_ctx: &AppContext) -> DomainDescriptor {
    DomainDescriptor::new("identity", crate::runtime::descriptor())
        .with_story_display(STORY_DISPLAY)
        .with_settings(crate::config::SETTINGS.as_slice())
}
```

`with_settings` takes `&'static [SettingDescriptor]`; `SETTINGS.as_slice()` on a `static LazyLock<Vec<_>>` yields that `'static` slice.

- [ ] **Step 3: Read the value through the snapshot where identity uses it**

Run: `grep -rn "IdentityConfig\|password_reset_ttl" domains/identity/src`
For each read site currently using `IdentityConfig::default()` or a hard-coded TTL, replace with:

```rust
let identity_config: crate::config::IdentityConfig =
    ctx.settings.snapshot().get("identity").unwrap_or_default();
```

If there are no current read sites, add a unit test in `domains/identity/src/config.rs` asserting the descriptor default round-trips:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use platform_core::{SettingsRegistry, SettingsSnapshot};
    use std::collections::BTreeMap;

    #[test]
    fn reads_default_ttl_from_snapshot() {
        let registry = SettingsRegistry::try_new(SETTINGS.clone()).unwrap();
        let snapshot = SettingsSnapshot::resolve(&registry, "api", &BTreeMap::new());
        let config: IdentityConfig = snapshot.get("identity").unwrap();
        assert_eq!(config.password_reset_ttl_minutes, 30);
    }
}
```

- [ ] **Step 4: Run identity tests**

Run: `cargo test --locked -p identity`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add domains/identity/src/config.rs domains/identity/src/module.rs
git commit -m "feat(identity): source password reset ttl from settings registry"
```

---

## Task 8: Console config DTOs

Response/request shapes for the `/admin/config/*` endpoints, OpenAPI-annotated so they flow into the generated SDK.

**Files:**
- Create: `crates/platform-admin/src/config_dto.rs`
- Modify: `crates/platform-admin/Cargo.toml` (add `uuid`, `serde_json` already present)

- [ ] **Step 1: Confirm crate deps**

Run: `grep -nE "serde_json|uuid|platform-domain|app-bootstrap" crates/platform-admin/Cargo.toml`
`serde_json` is present. The config handlers need the aggregated registry; rather than depend on `app-bootstrap` (which would create a cycle: app-bootstrap already merges admin routes via the API app, but admin does not depend on bootstrap — verify with `cargo tree`), the registry is injected into `platform-admin` the same way story-display is (a `OnceLock`), set by the API composition root. So **no new dependency on `app-bootstrap` or `platform-domain`** is added here. Add `uuid` only if a handler constructs ids (it does not; ids are created in the store). No Cargo change expected.

- [ ] **Step 2: Write `config_dto.rs`**

Create `crates/platform-admin/src/config_dto.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};

/// A registered setting's static metadata.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConfigDescriptorDto {
    pub key: String,
    pub service: String,
    pub value_type: Value,
    pub default: Value,
    pub editable: bool,
    pub restart_only: bool,
    pub description: String,
}

/// The list of all registered descriptors.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConfigDescriptorListResponse {
    pub data: Vec<ConfigDescriptorDto>,
}

/// A resolved effective value plus where it came from.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConfigValueDto {
    pub key: String,
    pub value: Value,
    pub source: String,
}

/// The list of effective values for the running service.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConfigValueListResponse {
    pub data: Vec<ConfigValueDto>,
}

/// Request body for writing a value.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ConfigWriteRequest {
    pub value: Value,
}

/// Response after a successful write.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConfigWriteResponse {
    pub key: String,
    pub service: String,
    pub value: Value,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<String>,
    /// True when the key is restart-only: the value is persisted but not applied
    /// to running instances until restart.
    pub applies_on_restart: bool,
}

/// One audit entry.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConfigAuditDto {
    pub service: String,
    pub key: String,
    pub old_value: Option<Value>,
    pub new_value: Value,
    pub actor: Option<String>,
    pub changed_at: DateTime<Utc>,
}

/// Audit history response.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConfigAuditListResponse {
    pub data: Vec<ConfigAuditDto>,
}

/// Query params for the audit endpoint.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ConfigAuditQuery {
    pub limit: Option<i64>,
}
```

- [ ] **Step 3: Compile check (wired in Task 9)**

Run: `cargo check --locked -p platform-admin` after Step 1 of Task 9 declares the module; until then this file is unreferenced. Skip standalone check; commit with Task 9.

---

## Task 9: Registry injection and console config handlers

Inject the aggregated `SettingsRegistry` into `platform-admin` (mirroring `install_story_display`), then add the handlers. The write handler validates against the registry, calls the store, and emits `NOTIFY config_changed`.

**Files:**
- Create: `crates/platform-admin/src/config_handlers.rs`
- Modify: `crates/platform-admin/src/lib.rs`
- Modify: `apps/api/src/openapi.rs` (tag), `apps/api/src/main.rs` (install registry — also covered in Task 10)

- [ ] **Step 1: Add a registry `OnceLock` and module declarations in `lib.rs`**

In `crates/platform-admin/src/lib.rs`, add imports and a registry slot near `STORY_DISPLAY`:

```rust
use platform_core::SettingsRegistry;
```

Add module declarations alongside the existing `mod` lines:

```rust
mod config_dto;
mod config_handlers;
```

Re-export the DTOs (after `pub use dto::*;`):

```rust
pub use config_dto::*;
#[allow(clippy::wildcard_imports)]
use config_handlers::*;
```

Add the registry slot and installer near `install_story_display`:

```rust
static SETTINGS_REGISTRY: OnceLock<SettingsRegistry> = OnceLock::new();

/// Install the aggregated settings registry from the composition root. Idempotent.
pub fn install_settings_registry(registry: SettingsRegistry) {
    let _ = SETTINGS_REGISTRY.set(registry);
}

/// The installed registry, or an empty one if none was installed.
fn settings_registry() -> &'static SettingsRegistry {
    static EMPTY: OnceLock<SettingsRegistry> = OnceLock::new();
    SETTINGS_REGISTRY
        .get()
        .unwrap_or_else(|| EMPTY.get_or_init(SettingsRegistry::default))
}
```

Add the config routes to `router()`:

```rust
        .routes(routes!(list_config_descriptors))
        .routes(routes!(list_config_values))
        .routes(routes!(put_config_value))
        .routes(routes!(delete_config_value))
        .routes(routes!(get_config_audit))
```

- [ ] **Step 2: Write `config_handlers.rs`**

Create `crates/platform-admin/src/config_handlers.rs`:

```rust
#[allow(clippy::wildcard_imports)]
use super::*;
use crate::config_dto::*;
use axum::Json;
use axum::extract::{Path, Query, State};
use platform_core::settings::store::{delete_value, load_audit, upsert_value};
use platform_core::{AppContext, AppError, ErrorCode, SettingScope};
use platform_http::{AdminActor, ApiErrorResponse, ErrorResponse, HttpRequestContext};

const AUDIT_DEFAULT_LIMIT: i64 = 50;
const AUDIT_MAX_LIMIT: i64 = 200;

fn actor_label(actor: &AdminActor) -> String {
    match actor {
        AdminActor::Service { service_id, .. } => format!("service:{service_id}"),
        AdminActor::System => "system".to_owned(),
    }
}

fn type_to_json(value_type: &platform_core::SettingType) -> serde_json::Value {
    value_type.to_json()
}

#[utoipa::path(
    get,
    path = "/admin/config/descriptors",
    operation_id = "admin_config_list_descriptors",
    tag = "admin-config",
    params(
        ("authorization" = String, Header, description = "Development service bearer token"),
    ),
    responses(
        (status = 200, description = "Registered setting descriptors", body = ConfigDescriptorListResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service or system authentication is required", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn list_config_descriptors(
    _admin: AdminActor,
    State(_ctx): State<AppContext>,
    HttpRequestContext(_request_ctx): HttpRequestContext,
) -> Result<Json<ConfigDescriptorListResponse>, ApiErrorResponse> {
    let data = settings_registry()
        .iter()
        .map(|d| ConfigDescriptorDto {
            key: d.key.to_owned(),
            service: d.scope.as_service_key().to_owned(),
            value_type: type_to_json(&d.value_type),
            default: d.default.clone(),
            editable: d.editable,
            restart_only: d.restart_only,
            description: d.description.to_owned(),
        })
        .collect();
    Ok(Json(ConfigDescriptorListResponse { data }))
}

#[utoipa::path(
    get,
    path = "/admin/config/values",
    operation_id = "admin_config_list_values",
    tag = "admin-config",
    params(
        ("authorization" = String, Header, description = "Development service bearer token"),
    ),
    responses(
        (status = 200, description = "Effective config values", body = ConfigValueListResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service or system authentication is required", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn list_config_values(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(_request_ctx): HttpRequestContext,
) -> Result<Json<ConfigValueListResponse>, ApiErrorResponse> {
    let snapshot = ctx.settings.snapshot();
    let data = snapshot
        .entries()
        .map(|(key, value, source)| ConfigValueDto {
            key: key.to_owned(),
            value: value.clone(),
            source: serde_json::to_value(source)
                .ok()
                .and_then(|v| v.as_str().map(ToOwned::to_owned))
                .unwrap_or_else(|| "default".to_owned()),
        })
        .collect();
    Ok(Json(ConfigValueListResponse { data }))
}

#[utoipa::path(
    put,
    path = "/admin/config/{service}/{key}",
    operation_id = "admin_config_put_value",
    tag = "admin-config",
    params(
        ("service" = String, Path, description = "Service key: a service name or `*` for shared"),
        ("key" = String, Path, description = "Setting key"),
        ("authorization" = String, Header, description = "Development service bearer token"),
    ),
    request_body = ConfigWriteRequest,
    responses(
        (status = 200, description = "Value written", body = ConfigWriteResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Setting is not editable", body = ErrorResponse, content_type = "application/json"),
        (status = 404, description = "Unknown setting key", body = ErrorResponse, content_type = "application/json"),
        (status = 400, description = "Value failed validation", body = ErrorResponse, content_type = "application/json"),
        (status = 500, description = "Internal server error", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn put_config_value(
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path((service, key)): Path<(String, String)>,
    Json(body): Json<ConfigWriteRequest>,
) -> Result<Json<ConfigWriteResponse>, ApiErrorResponse> {
    let descriptor = settings_registry().get_raw(&service, &key).ok_or_else(|| {
        ApiErrorResponse::with_context(
            AppError::new(ErrorCode::NotFound, format!("unknown setting `{service}:{key}`")),
            &request_ctx,
        )
    })?;

    if !descriptor.editable {
        return Err(ApiErrorResponse::with_context(
            AppError::new(ErrorCode::Forbidden, format!("setting `{key}` is not editable")),
            &request_ctx,
        ));
    }

    descriptor
        .validate(&body.value)
        .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?;

    let actor = actor_label(&admin);
    let stored = upsert_value(&ctx.db, &service, &key, &body.value, Some(&actor))
        .await
        .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?;

    notify_config_changed(&ctx, &service, &key, &request_ctx).await?;

    tracing::info!(
        actor = %actor,
        service = %service,
        key = %key,
        "config value updated"
    );

    Ok(Json(ConfigWriteResponse {
        key: stored.key,
        service: stored.service,
        value: stored.value,
        updated_at: stored.updated_at,
        updated_by: stored.updated_by,
        applies_on_restart: descriptor.restart_only,
    }))
}

#[utoipa::path(
    delete,
    path = "/admin/config/{service}/{key}",
    operation_id = "admin_config_delete_value",
    tag = "admin-config",
    params(
        ("service" = String, Path, description = "Service key: a service name or `*` for shared"),
        ("key" = String, Path, description = "Setting key"),
        ("authorization" = String, Header, description = "Development service bearer token"),
    ),
    responses(
        (status = 200, description = "Value reset to default", body = ConfigWriteResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 404, description = "Unknown setting key", body = ErrorResponse, content_type = "application/json"),
        (status = 500, description = "Internal server error", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn delete_config_value(
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path((service, key)): Path<(String, String)>,
) -> Result<Json<ConfigWriteResponse>, ApiErrorResponse> {
    let descriptor = settings_registry().get_raw(&service, &key).ok_or_else(|| {
        ApiErrorResponse::with_context(
            AppError::new(ErrorCode::NotFound, format!("unknown setting `{service}:{key}`")),
            &request_ctx,
        )
    })?;
    let actor = actor_label(&admin);
    delete_value(&ctx.db, &service, &key, Some(&actor))
        .await
        .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?;
    notify_config_changed(&ctx, &service, &key, &request_ctx).await?;

    Ok(Json(ConfigWriteResponse {
        key: key.clone(),
        service: service.clone(),
        value: descriptor.default.clone(),
        updated_at: chrono::Utc::now(),
        updated_by: Some(actor),
        applies_on_restart: descriptor.restart_only,
    }))
}

#[utoipa::path(
    get,
    path = "/admin/config/{service}/{key}/audit",
    operation_id = "admin_config_get_audit",
    tag = "admin-config",
    params(
        ("service" = String, Path, description = "Service key"),
        ("key" = String, Path, description = "Setting key"),
        ("authorization" = String, Header, description = "Development service bearer token"),
        ConfigAuditQuery
    ),
    responses(
        (status = 200, description = "Audit history", body = ConfigAuditListResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 500, description = "Internal server error", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn get_config_audit(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path((service, key)): Path<(String, String)>,
    Query(query): Query<ConfigAuditQuery>,
) -> Result<Json<ConfigAuditListResponse>, ApiErrorResponse> {
    let limit = query.limit.unwrap_or(AUDIT_DEFAULT_LIMIT).clamp(1, AUDIT_MAX_LIMIT);
    let entries = load_audit(&ctx.db, &service, &key, limit)
        .await
        .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?;
    let data = entries
        .into_iter()
        .map(|e| ConfigAuditDto {
            service: e.service,
            key: e.key,
            old_value: e.old_value,
            new_value: e.new_value,
            actor: e.actor,
            changed_at: e.changed_at,
        })
        .collect();
    Ok(Json(ConfigAuditListResponse { data }))
}

/// Emit a `config_changed` notification so every instance refreshes.
async fn notify_config_changed(
    ctx: &AppContext,
    service: &str,
    key: &str,
    request_ctx: &platform_core::RequestContext,
) -> Result<(), ApiErrorResponse> {
    let payload = format!("{service}:{key}");
    sqlx::query("select pg_notify('config_changed', $1)")
        .bind(payload)
        .execute(&ctx.db)
        .await
        .map_err(|source| {
            ApiErrorResponse::with_context(
                AppError::new(ErrorCode::Internal, "config notify failed").with_source(source),
                request_ctx,
            )
        })?;
    Ok(())
}
```

Note (verified in this repo): `ErrorCode::Forbidden` exists and maps to HTTP 403; `ErrorCode::Validation` maps to HTTP 400; `ErrorCode::NotFound` maps to 404. Task 4 declares `pub mod store;` inside `settings`, so `platform_core::settings::store::{upsert_value, delete_value, load_audit}` is reachable.

- [ ] **Step 3: Add the OpenAPI tag**

In `apps/api/src/openapi.rs`, add to the `tags(...)` list:

```rust
        (name = "admin-config", description = "Editable configuration console APIs")
```

- [ ] **Step 4: Compile check**

Run: `cargo check --locked -p platform-admin -p app-api`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/platform-admin/src apps/api/src/openapi.rs
git commit -m "feat(platform-admin): add console config descriptor/value/audit endpoints"
```

---

## Task 10: Wire providers into API and worker startup

Build the registry from the composition root, install it into `platform-admin`, construct a `PostgresSettingsProvider` for each app's service key, spawn its listener, and attach it to `AppContext`.

**Files:**
- Modify: `apps/api/src/main.rs`
- Modify: `apps/worker/src/main.rs`

- [ ] **Step 1: API startup**

In `apps/api/src/main.rs`, replace the body up to `build_router` with:

```rust
use anyhow::Context as _;
use app_api::build_router;
use platform_core::{
    AppConfig, AppContext, LoggingEventPublisher, PostgresSettingsProvider, SettingsRegistry,
    connect_pool, telemetry,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::from_env();
    telemetry::init(&config.telemetry)?;

    let db = connect_pool(&config.database).await?;
    let mut ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));

    // Build the editable-settings registry from every domain and install it for
    // the console handlers and the API's own reads.
    let descriptors = app_bootstrap::setting_descriptors(&ctx);
    let registry = SettingsRegistry::try_new(descriptors)
        .context("duplicate setting descriptor registered")?;
    platform_admin::install_settings_registry(registry.clone());

    let settings = PostgresSettingsProvider::connect(ctx.db.clone(), Arc::new(registry), "api")
        .await
        .context("failed to load settings snapshot")?;
    settings.spawn_listener();
    ctx = ctx.with_settings_provider(settings);

    let app = build_router(ctx.clone());
    let address: SocketAddr = format!("{}:{}", ctx.config.http.host, ctx.config.http.port)
        .parse()
        .context("invalid HTTP bind address")?;

    info!(%address, "starting API server");
    let listener = tokio::net::TcpListener::bind(address).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            platform_core::Shutdown::wait_for_signal().await;
        })
        .await?;

    Ok(())
}
```

Note: `SettingsRegistry` must derive `Clone` (Task 1 derived it). `install_settings_registry` takes ownership; `registry.clone()` keeps a copy for the provider.

- [ ] **Step 2: Worker startup**

In `apps/worker/src/main.rs`, after constructing `ctx` and before building domains, insert:

```rust
    let descriptors = app_bootstrap::setting_descriptors(&ctx);
    let registry = platform_core::SettingsRegistry::try_new(descriptors)
        .expect("duplicate setting descriptor registered");
    let settings =
        platform_core::PostgresSettingsProvider::connect(ctx.db.clone(), std::sync::Arc::new(registry), "worker")
            .await
            .expect("failed to load settings snapshot");
    settings.spawn_listener();
    let ctx = ctx.with_settings_provider(settings);
```

The worker does not serve the console, so it does not install the registry into `platform-admin`; it only needs its own provider for reads.

- [ ] **Step 3: Compile and check**

Run: `cargo check --locked --workspace --all-targets`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/api/src/main.rs apps/worker/src/main.rs
git commit -m "feat(apps): wire postgres settings provider into api and worker"
```

---

## Task 11: Integration test — write → notify → refresh round trip

A Postgres-backed test proving an edit through the store, followed by a provider refresh, changes the resolved value.

**Files:**
- Create: `crates/platform-core/tests/settings_provider.rs`

- [ ] **Step 1: Write the test**

Create `crates/platform-core/tests/settings_provider.rs`:

```rust
use platform_core::settings::store::upsert_value;
use platform_core::{
    PLATFORM_MIGRATIONS, PostgresSettingsProvider, SettingDescriptor, SettingScope, SettingType,
    SettingsRegistry, apply_migrations,
};
use platform_testing::TestDatabase;
use serde_json::json;
use std::sync::Arc;

fn registry() -> SettingsRegistry {
    SettingsRegistry::try_new(vec![SettingDescriptor {
        key: "demo.ttl_minutes",
        scope: SettingScope::Shared,
        value_type: SettingType::Int { min: Some(1), max: Some(1000) },
        default: json!(30),
        editable: true,
        restart_only: false,
        description: "ttl",
    }])
    .unwrap()
}

#[tokio::test]
async fn refresh_picks_up_written_value() {
    let Some(test_db) = TestDatabase::create().await else {
        return; // DATABASE_URL not set; skip.
    };
    apply_migrations(&test_db.pool, PLATFORM_MIGRATIONS)
        .await
        .expect("migrations apply");

    let provider =
        PostgresSettingsProvider::connect(test_db.pool.clone(), Arc::new(registry()), "api")
            .await
            .expect("connect provider");

    // Default before any write.
    assert_eq!(provider.snapshot().raw("demo.ttl_minutes"), Some(&json!(30)));

    upsert_value(&test_db.pool, "*", "demo.ttl_minutes", &json!(90), Some("test"))
        .await
        .expect("upsert");

    provider.refresh().await.expect("refresh");
    assert_eq!(provider.snapshot().raw("demo.ttl_minutes"), Some(&json!(90)));

    test_db.cleanup().await;
}
```

- [ ] **Step 2: Run the test**

Run: `just db-up` (if not already running), then
`DATABASE_URL=postgres://lenso:lenso@localhost:5432/lenso cargo test --locked -p platform-core --test settings_provider -- --nocapture`
Expected: PASS (or a clean skip if `DATABASE_URL` is unset).

- [ ] **Step 3: Commit**

```bash
git add crates/platform-core/tests/settings_provider.rs
git commit -m "test(platform-core): cover settings provider refresh round trip"
```

---

## Task 12: Console HTTP integration test for config endpoints

Mirror the existing `apps/api/tests/runtime_console.rs` style: spin up the router with a registry installed, write a value, and assert the descriptor/value/audit endpoints behave.

**Files:**
- Create: `apps/api/tests/config_console.rs`

- [ ] **Step 1: Inspect an existing admin test for the exact harness**

Run: `sed -n '1,80p' apps/api/tests/runtime_console.rs`
Note how it builds the router, sets the `AppContext` (including any test settings provider), authenticates with the dev service bearer token, and issues requests. Reuse that harness verbatim, substituting the config endpoints.

- [ ] **Step 2: Write the test**

Create `apps/api/tests/config_console.rs` following that harness. It must:

1. Create a `TestDatabase`, apply `PLATFORM_MIGRATIONS`.
2. Build a `SettingsRegistry` with one editable shared key `demo.flag` (Bool, default `false`).
3. Call `platform_admin::install_settings_registry(registry.clone())` before building the router.
4. Build `AppContext` with a `PostgresSettingsProvider` for `"api"` attached via `with_settings_provider`.
5. Assert `GET /admin/config/descriptors` returns the key.
6. Assert `PUT /admin/config/*/demo.flag` with `{"value": true}` returns 200 and `applies_on_restart: false`.
7. Assert `PUT` with `{"value": "nope"}` returns 400 (the platform error model maps `ErrorCode::Validation` to HTTP 400).
8. Assert `PUT /admin/config/*/unknown.key` returns 404.
9. Assert `GET /admin/config/*/demo.flag/audit` returns one entry with `new_value: true`.

Use this skeleton, filling the request helper from the existing test harness:

```rust
use platform_core::{
    AppConfig, AppContext, LoggingEventPublisher, PLATFORM_MIGRATIONS, PostgresSettingsProvider,
    SettingDescriptor, SettingScope, SettingType, SettingsRegistry, apply_migrations,
};
use platform_testing::TestDatabase;
use serde_json::json;
use std::sync::Arc;

fn registry() -> SettingsRegistry {
    SettingsRegistry::try_new(vec![SettingDescriptor {
        key: "demo.flag",
        scope: SettingScope::Shared,
        value_type: SettingType::Bool,
        default: json!(false),
        editable: true,
        restart_only: false,
        description: "demo flag",
    }])
    .unwrap()
}

#[tokio::test]
async fn config_console_round_trip() {
    let Some(test_db) = TestDatabase::create().await else {
        return;
    };
    apply_migrations(&test_db.pool, PLATFORM_MIGRATIONS)
        .await
        .expect("migrations apply");

    let registry = registry();
    platform_admin::install_settings_registry(registry.clone());

    let mut config = AppConfig::from_env();
    config.database.url = test_db.url.clone();
    let mut ctx = AppContext::new(config, test_db.pool.clone(), Arc::new(LoggingEventPublisher));
    let settings = PostgresSettingsProvider::connect(test_db.pool.clone(), Arc::new(registry), "api")
        .await
        .expect("connect provider");
    ctx = ctx.with_settings_provider(settings);

    let app = app_api::build_router(ctx.clone());

    // --- Use the request/auth helpers from apps/api/tests/runtime_console.rs ---
    // Example assertions (replace `send` with the harness helper):
    //
    // let descriptors = send(&app, "GET", "/admin/config/descriptors", None).await;
    // assert!(descriptors.contains("demo.flag"));
    //
    // let ok = put(&app, "/admin/config/*/demo.flag", json!({"value": true})).await;
    // assert_eq!(ok.status, 200);
    //
    // let bad = put(&app, "/admin/config/*/demo.flag", json!({"value": "nope"})).await;
    // assert_eq!(bad.status, 400);
    //
    // let missing = put(&app, "/admin/config/*/unknown.key", json!({"value": true})).await;
    // assert_eq!(missing.status, 404);
    //
    // let audit = send(&app, "GET", "/admin/config/*/demo.flag/audit", None).await;
    // assert!(audit.contains("\"new_value\":true"));

    test_db.cleanup().await;
}
```

Implementer: copy the concrete `send`/`put`/auth-header helpers from `apps/api/tests/runtime_console.rs` (they already handle the dev bearer token `Bearer dev-service:admin` and `x-request-id`). Replace the commented assertions with real calls.

- [ ] **Step 3: Run the test**

Run: `DATABASE_URL=postgres://lenso:lenso@localhost:5432/lenso cargo test --locked -p app-api --test config_console -- --nocapture`
Expected: PASS (or clean skip without `DATABASE_URL`).

- [ ] **Step 4: Commit**

```bash
git add apps/api/tests/config_console.rs
git commit -m "test(api): cover console config endpoints end to end"
```

---

## Task 13: Regenerate contracts/SDK and add the Runtime Console screen

Surface the new endpoints in the generated OpenAPI + TS SDK, then add an operator settings screen.

**Files:**
- Modify (generated): `contracts/openapi/app-api.v1.yaml`, `packages/ts-sdk/src/generated/*`
- Create: `apps/runtime-console/src/routes/settings.tsx` (confirm router convention first)

- [ ] **Step 1: Regenerate committed artifacts**

Run: `just generate`
Then: `just generated-check`
Expected: the OpenAPI doc and SDK now include `admin_config_*` operations; `generated-check` passes (artifacts match what was just generated).

- [ ] **Step 2: Inspect the console routing + an existing operator screen**

Run: `ls apps/runtime-console/src/routes` and open one existing screen (e.g. the outbox or functions list) to copy its data-loading pattern (TanStack Query + generated SDK client), table primitives from `src/components/ui`, and route registration.

- [ ] **Step 3: Build the settings screen**

Create the settings route following that pattern. Requirements:

- Fetch `GET /admin/config/descriptors` and `GET /admin/config/values` via the generated SDK; join by `key`.
- Render rows grouped by `service` then `key`, each showing: key, description, current value, a source badge (`override` / `shared` / `default`), and edit affordance.
- Edit control typed by `value_type.kind`: a toggle for `bool`, a number input with `min`/`max` for `int`/`float`, a `<select>` for `enum` (options from `value_type` payload), a textarea/JSON editor for `json` and `string`.
- Disable editing when `editable` is false. When `restart_only` is true, show an "applies on restart" note next to the control.
- On save, call `PUT /admin/config/{service}/{key}` with `{ value }`; on success, invalidate the values query so the screen reflects the new value (the backend snapshot updates via NOTIFY). Surface 400 validation errors inline.
- Add a "Reset to default" action calling `DELETE /admin/config/{service}/{key}`.
- Add an audit drawer/expander calling `GET /admin/config/{service}/{key}/audit`, listing actor/changed_at/old→new.

- [ ] **Step 4: Validate the console**

Run: `just console-check`
Expected: format-check, lint, typecheck, and build all pass.

- [ ] **Step 5: Commit**

```bash
git add contracts packages/ts-sdk apps/runtime-console/src
git commit -m "feat(runtime-console): add editable configuration settings screen"
```

---

## Final Verification

- [ ] **Run the full quality gate**

Run: `just check`
Expected: PASS (Rust check, tests, generated-check, arch-check, sdk-check, console-check, formatting).

- [ ] **Confirm the migration applies cleanly**

Run: `just migrate`
Expected: `platform/0007_create_config_schema` applied without error.

- [ ] **Document the new env/ops note (optional)**

If `.env.example` or `docs/architecture/*` enumerate platform tables or config, add a one-line note that editable settings live in the `config` schema and are managed via the Runtime Console. Keep it minimal.

