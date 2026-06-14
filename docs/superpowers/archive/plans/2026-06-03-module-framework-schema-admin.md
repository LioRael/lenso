# Module Framework Step 2 — Schema Admin — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver a read-only, single-entity (identity User) schema-driven admin vertical slice: modules declare manageable entities as data, a narrow `AdminDataSource` trait provides read access returning `serde_json::Value`, a new `platform-admin-data` crate exposes generic endpoints, and the console renders a generic schema-driven "Data" page.

**Architecture:** `AdminSurface::Schema(AdminSchema)` is pure data in the manifest; `AdminDataSource` (list/get) is behavior on `Module` as `Option<Arc<dyn ...>>`. identity declares a User schema and converts `User` → `Value` only at the seam exit. `platform-admin-data` depends on no domain — it works through the trait + schema via a composition-root-injected registry. Frontend is one generic component driven by runtime schema.

**Tech Stack:** Rust 2024, `serde`, `serde_json`, `axum` 0.8, `utoipa`/`utoipa-axum`, `sqlx`, `async-trait`; React 19 + TanStack Router/Query + Base UI + Tailwind + `ky`. Quality gate is `cargo check` (not clippy).

**Migration order (each step compiles/tests independently):** contracts in `platform-module` → identity implementation → `platform-admin-data` crate + endpoints → app-bootstrap + api wiring → arch-check boundary rule → frontend Data page.

**Spec:** `docs/superpowers/specs/2026-06-03-module-framework-schema-admin-design.md`

---

## File Structure

**Created:**
- `crates/platform-module/src/admin_schema.rs` — `AdminSchema`, `EntitySchema`, `FieldSchema`, `FieldType` (serde + `ToSchema`).
- `crates/platform-module/src/admin_data.rs` — `AdminDataSource` trait, `AdminListQuery`, `AdminPage`.
- `domains/identity/src/admin.rs` — `IdentityAdminData` (impl `AdminDataSource`), `user_to_value`.
- `crates/platform-admin-data/` — new crate: `Cargo.toml`, `src/lib.rs` (registry + router + install), `src/handlers.rs` (3 endpoints), `src/dto.rs` (generic container DTOs).
- `apps/runtime-console/src/pages/data-page.tsx` — generic schema-driven page.
- `apps/runtime-console/src/pages/data-render-model.ts` — pure render-model function (schema + records → columns/cells).
- `apps/runtime-console/src/pages/data-render-model.test.ts` — render-model unit tests.

**Modified:**
- `crates/platform-module/src/admin.rs` — `AdminSurface::Schema(AdminSchema)` variant.
- `crates/platform-module/src/manifest.rs` — builder `.admin(schema)`.
- `crates/platform-module/src/module.rs` — `admin_data` field + `.with_admin_data(..)`.
- `crates/platform-module/src/lib.rs` — re-exports + new modules.
- `crates/platform-module/Cargo.toml` — add `serde_json`, `async-trait`, `utoipa` deps.
- `domains/identity/src/repositories/mod.rs` — `UserRepository::list`.
- `domains/identity/src/module.rs` — `user_schema()`, `.admin(..)`, `.with_admin_data(..)`.
- `domains/identity/src/lib.rs` — `pub mod admin;`.
- `domains/identity/Cargo.toml` — add `serde_json` (for `user_to_value`) if absent.
- `crates/app-bootstrap/src/lib.rs` — `admin_modules(ctx)`.
- `crates/app-bootstrap/Cargo.toml` — add `platform-admin-data`.
- `apps/api/src/main.rs` — `install_admin_modules`.
- `apps/api/src/openapi.rs` — `.merge(platform_admin_data::router())`.
- `apps/api/Cargo.toml` — add `platform-admin-data`.
- `Cargo.toml` (workspace) — add `platform-admin-data` member + dep.
- `tools/arch-check/src/lib.rs` — `check_admin_data_no_domain_deps`.
- `apps/runtime-console/src/app/router.tsx` — register `/data` route.

---
## Task 1: Schema-admin contracts in `platform-module`

Add the data contracts (`AdminSchema` + field vocabulary), the behavior trait (`AdminDataSource`), fill the `AdminSurface::Schema` variant, and extend `Module`/manifest builder. Nothing consumes it yet — develops in isolation. TDD via serde round-trip tests.

**Files:**
- Modify: `crates/platform-module/Cargo.toml`
- Create: `crates/platform-module/src/admin_schema.rs`
- Create: `crates/platform-module/src/admin_data.rs`
- Modify: `crates/platform-module/src/admin.rs`
- Modify: `crates/platform-module/src/manifest.rs`
- Modify: `crates/platform-module/src/module.rs`
- Modify: `crates/platform-module/src/lib.rs`

- [ ] **Step 1: Promote deps in `crates/platform-module/Cargo.toml`**

`serde_json` and `async-trait` are currently dev-deps; the contracts need them as real deps, plus `utoipa` for `ToSchema`. Change `[dependencies]` to:

```toml
[dependencies]
platform-core.workspace = true
platform-runtime.workspace = true
serde.workspace = true
serde_json.workspace = true
async-trait.workspace = true
utoipa.workspace = true

[dev-dependencies]
serde_json.workspace = true
```

(Keep `serde_json` in dev-deps too — harmless; existing tests use it. `async-trait` moves up to deps.)

- [ ] **Step 2: Write `crates/platform-module/src/admin_schema.rs`**

```rust
//! Schema-admin data contracts: a module's declared manageable entities.

use serde::{Deserialize, Serialize};

/// A module's declared admin surface: which entities it exposes for management.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[non_exhaustive]
pub struct AdminSchema {
    pub entities: Vec<EntitySchema>,
}

/// One manageable entity (e.g. identity's "users").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[non_exhaustive]
pub struct EntitySchema {
    /// Stable entity key, unique within the module, e.g. "users".
    pub name: String,
    /// Human label for the console, e.g. "Users".
    pub label: String,
    /// Ordered field descriptors driving list columns / detail rows.
    pub fields: Vec<FieldSchema>,
    /// Capability required to read this entity. Declared now; gated only
    /// coarsely (AdminActor) this step. Fine-grained RBAC is a later spec.
    pub read_capability: String,
}

/// One field of an entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[non_exhaustive]
pub struct FieldSchema {
    /// Key in the record's JSON object, e.g. "email".
    pub name: String,
    /// Human label, e.g. "Email".
    pub label: String,
    pub field_type: FieldType,
    /// Whether the value may be null/absent.
    #[serde(default)]
    pub nullable: bool,
}

/// Minimal field-type vocabulary. `Json` is the catch-all so any field renders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum FieldType {
    String,
    Integer,
    Boolean,
    Timestamp,
    Json,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> AdminSchema {
        AdminSchema {
            entities: vec![EntitySchema {
                name: "users".to_owned(),
                label: "Users".to_owned(),
                read_capability: "identity.users.read".to_owned(),
                fields: vec![
                    FieldSchema { name: "email".into(), label: "Email".into(), field_type: FieldType::String, nullable: false },
                    FieldSchema { name: "created_at".into(), label: "Created".into(), field_type: FieldType::Timestamp, nullable: false },
                ],
            }],
        }
    }

    #[test]
    fn admin_schema_round_trips_through_json() {
        let schema = sample();
        let json = serde_json::to_string(&schema).expect("serialize");
        let back: AdminSchema = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(schema, back);
    }

    #[test]
    fn field_type_serializes_with_kind_tag() {
        let json = serde_json::to_string(&FieldType::Timestamp).expect("serialize");
        assert_eq!(json, r#"{"kind":"timestamp"}"#);
    }
}
```

- [ ] **Step 3: Write `crates/platform-module/src/admin_data.rs`**

```rust
//! Schema-admin behavior seam: a module's read access to its admin entities.

use platform_core::AppResult;
use serde_json::Value;

/// A module's read access to its admin entities. Optional capability — only
/// modules with an admin surface implement it. Records cross as `Value` (the
/// only shape a generic renderer handles); strong types stay inside the impl.
#[async_trait::async_trait]
pub trait AdminDataSource: std::fmt::Debug + Send + Sync {
    /// List records for `entity`, paginated. Returns a page of JSON objects.
    async fn list(&self, entity: &str, query: &AdminListQuery) -> AppResult<AdminPage>;

    /// Fetch one record by id. `Ok(None)` if not found.
    async fn get(&self, entity: &str, id: &str) -> AppResult<Option<Value>>;
}

/// Structured query — fields reserved for future filter/sort without changing
/// the method signature.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct AdminListQuery {
    pub limit: i64,
    /// Opaque pagination cursor (NOT a timestamp — no entity-shape assumption).
    pub cursor: Option<String>,
}

/// One page of records.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AdminPage {
    pub records: Vec<Value>,
    /// Opaque cursor for the next page; `None` at the end.
    pub next_cursor: Option<String>,
}

impl AdminListQuery {
    /// Convenience constructor.
    #[must_use]
    pub fn new(limit: i64, cursor: Option<String>) -> Self {
        Self { limit, cursor }
    }
}
```

NOTE: `AdminListQuery`/`AdminPage` are `#[non_exhaustive]`; within this crate you can still use struct literals, but external crates must construct `AdminListQuery` via `::new` or `..Default::default()`. The `new` constructor is provided for the endpoint crate.

- [ ] **Step 4: Fill `AdminSurface::Schema` in `crates/platform-module/src/admin.rs`**

Replace the empty enum body. The file currently is `pub enum AdminSurface {}`. Change to:

```rust
//! Reserved seam for a module's admin surface.

use crate::admin_schema::AdminSchema;
use serde::{Deserialize, Serialize};

/// A module's admin surface. `Schema` is the generic schema-driven CRUD lane.
///
/// `#[non_exhaustive]` so adding variants later (e.g. `Custom` for plugin
/// self-rendering, Step 4) is not a breaking change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AdminSurface {
    /// Schema-driven CRUD: console renders a generic UI from this declaration.
    Schema(AdminSchema),
}
```

NOTE: `AdminSurface` does NOT derive `ToSchema` (only `AdminSchema` does — that's the type that appears in API responses). The existing manifest test `empty_admin_is_skipped_in_json` still passes (admin defaults to `None`).

- [ ] **Step 5: Add `.admin(schema)` builder to `crates/platform-module/src/manifest.rs`**

The `ModuleManifestBuilder` impl currently has `story_display`, `capabilities`, `build`. Add (import `AdminSchema` — `use crate::admin::AdminSurface;` is already present; add `use crate::admin_schema::AdminSchema;`):

```rust
    /// Attach a schema-driven admin surface.
    #[must_use]
    pub fn admin(mut self, schema: AdminSchema) -> Self {
        self.manifest.admin = Some(AdminSurface::Schema(schema));
        self
    }
```

- [ ] **Step 6: Add `admin_data` to `crates/platform-module/src/module.rs`**

Add the field and builder. Add `use crate::admin_data::AdminDataSource;` to imports. Change the struct + impl:

```rust
#[derive(Debug)]
pub struct Module {
    pub manifest: ModuleManifest,
    pub binding: Arc<dyn ModuleBinding>,
    pub runtime_config: &'static [RuntimeConfigDescriptor],
    /// Optional schema-admin data source. `None` for modules without an admin
    /// surface (e.g. notifications). Set via [`Module::with_admin_data`].
    pub admin_data: Option<Arc<dyn AdminDataSource>>,
}
```

In `Module::linked`, add `admin_data: None,` to the constructed `Self`. Add a new builder method after `with_runtime_config`:

```rust
    /// Attach a schema-admin data source (read access to admin entities).
    #[must_use]
    pub fn with_admin_data(mut self, data: Arc<dyn AdminDataSource>) -> Self {
        self.admin_data = Some(data);
        self
    }
```

NOTE: `Module` derives `Debug`; `AdminDataSource: Debug` (supertrait), so `Option<Arc<dyn AdminDataSource>>` is `Debug` — the derive still compiles.

- [ ] **Step 7: Wire modules + re-exports in `crates/platform-module/src/lib.rs`**

Add the two new private modules and re-export the public types:

```rust
mod admin;
mod admin_data;
mod admin_schema;
mod binding;
mod linked;
mod manifest;
mod module;

pub use admin::AdminSurface;
pub use admin_data::{AdminDataSource, AdminListQuery, AdminPage};
pub use admin_schema::{AdminSchema, EntitySchema, FieldSchema, FieldType};
pub use binding::ModuleBinding;
pub use linked::{LinkedBinding, LinkedBindingBuilder};
pub use manifest::{ModuleManifest, ModuleManifestBuilder};
pub use module::Module;
```

- [ ] **Step 8: Add a manifest-with-admin test in `crates/platform-module/src/manifest.rs`**

In the existing `#[cfg(test)] mod tests`, add (the module already imports `super::*`):

```rust
    #[test]
    fn manifest_with_admin_serializes_schema_kind() {
        use crate::admin_schema::{AdminSchema, EntitySchema, FieldSchema, FieldType};
        let schema = AdminSchema {
            entities: vec![EntitySchema {
                name: "users".to_owned(),
                label: "Users".to_owned(),
                read_capability: "identity.users.read".to_owned(),
                fields: vec![FieldSchema {
                    name: "email".into(), label: "Email".into(),
                    field_type: FieldType::String, nullable: false,
                }],
            }],
        };
        let manifest = ModuleManifest::builder("identity").admin(schema).build();
        let json = serde_json::to_string(&manifest).expect("serialize");
        assert!(json.contains(r#""kind":"schema""#), "got {json}");
    }
```

- [ ] **Step 9: Test and check**

Run: `cargo test -p platform-module`
Expected: PASS — existing tests + `admin_schema_round_trips_through_json`, `field_type_serializes_with_kind_tag`, `manifest_with_admin_serializes_schema_kind`.

Run: `cargo check --workspace`
Expected: PASS — promoting `async-trait`/`utoipa`/`serde_json` to deps doesn't break existing consumers (identity/notifications/app-bootstrap still build; they don't touch the new types yet).

- [ ] **Step 10: Commit**

```bash
git add crates/platform-module/
git commit -m "feat(platform-module): schema-admin contracts

Fill AdminSurface::Schema(AdminSchema); add AdminSchema/EntitySchema/FieldSchema/
FieldType data vocabulary (serde + ToSchema) and the AdminDataSource read trait
(list/get, Value records, opaque cursor). Module gains optional admin_data; the
manifest builder gains .admin(schema). Nothing consumes it yet."
```

---
## Task 2: identity `AdminDataSource` implementation

Add a `list` query to `UserRepository`, implement `AdminDataSource` for identity, declare the User schema, and wire it into `module()`. TDD: the cursor-pagination DB test is written first (skips without a DB), plus a pure `user_to_value`/schema-alignment unit test.

**Files:**
- Modify: `domains/identity/src/repositories/mod.rs`
- Create: `domains/identity/src/admin.rs`
- Modify: `domains/identity/src/module.rs`
- Modify: `domains/identity/src/lib.rs`
- Test: `domains/identity/tests/admin_data.rs` (new)

- [ ] **Step 1: Add `list` to the `UserRepository` trait + impl**

In `domains/identity/src/repositories/mod.rs`, add to the trait (after `find_by_email`):

```rust
    async fn list(&self, limit: i64, cursor: Option<&str>) -> AppResult<Vec<User>>;
```

Add to the `impl UserRepository for PostgresUserRepository` block (after `find_by_email`, before the closing `}`):

```rust
    async fn list(&self, limit: i64, cursor: Option<&str>) -> AppResult<Vec<User>> {
        let rows = match cursor {
            Some(after) => {
                sqlx::query_as::<_, UserRow>(
                    r#"
                    select id, email, display_name, created_at, updated_at
                    from identity.users
                    where id > $1
                    order by id asc
                    limit $2
                    "#,
                )
                .bind(after)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
            None => {
                sqlx::query_as::<_, UserRow>(
                    r#"
                    select id, email, display_name, created_at, updated_at
                    from identity.users
                    order by id asc
                    limit $1
                    "#,
                )
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
        }
        .map_err(map_sql_error)?;

        Ok(rows.into_iter().map(user_from_row).collect())
    }
```

NOTE: `UserRow`, `user_from_row`, `map_sql_error` already exist in this file. Pagination keys on `id` (UUID v7, monotonic — no `created_at` assumption).

- [ ] **Step 2: Write the failing admin-data DB test `domains/identity/tests/admin_data.rs`**

```rust
use chrono::Utc;
use identity::admin::IdentityAdminData;
use identity::models::user::{User, UserId};
use identity::repositories::{PostgresUserRepository, UserRepository};
use platform_core::{PLATFORM_MIGRATIONS, apply_migrations};
use platform_module::{AdminDataSource, AdminListQuery};
use platform_runtime::RUNTIME_MIGRATIONS;
use platform_testing::TestDatabase;
use std::sync::Arc;

async fn seed(repo: &PostgresUserRepository, id: &str, email: &str) {
    let now = Utc::now();
    repo.insert(&User {
        id: UserId(id.to_owned()),
        email: email.to_owned(),
        display_name: Some("Test".to_owned()),
        created_at: now,
        updated_at: now,
    })
    .await
    .expect("insert should succeed");
}

#[tokio::test]
async fn admin_data_lists_users_with_cursor_pagination() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let migrations = PLATFORM_MIGRATIONS
        .iter()
        .chain(RUNTIME_MIGRATIONS)
        .chain(identity::migrations::IDENTITY_MIGRATIONS)
        .copied()
        .collect::<Vec<_>>();
    apply_migrations(&db.pool, &migrations).await.expect("migrations apply");

    let repo = PostgresUserRepository::new(db.pool.clone());
    // ids must be ascending so cursor order is deterministic
    seed(&repo, "usr_a", "a@example.com").await;
    seed(&repo, "usr_b", "b@example.com").await;
    seed(&repo, "usr_c", "c@example.com").await;

    let admin = IdentityAdminData::new(Arc::new(repo));

    // First page: limit 2 → 2 records + a next_cursor
    let page1 = admin
        .list("users", &AdminListQuery::new(2, None))
        .await
        .expect("list page 1");
    assert_eq!(page1.records.len(), 2);
    assert_eq!(page1.records[0]["id"], "usr_a");
    assert_eq!(page1.records[1]["id"], "usr_b");
    let cursor = page1.next_cursor.clone().expect("should have next cursor");
    assert_eq!(cursor, "usr_b");

    // Second page: pass cursor → last record, no further cursor
    let page2 = admin
        .list("users", &AdminListQuery::new(2, Some(cursor)))
        .await
        .expect("list page 2");
    assert_eq!(page2.records.len(), 1);
    assert_eq!(page2.records[0]["id"], "usr_c");
    assert!(page2.next_cursor.is_none());

    // get by id returns the record; unknown id → None
    let one = admin.get("users", "usr_a").await.expect("get");
    assert_eq!(one.expect("some")["email"], "a@example.com");
    assert!(admin.get("users", "nope").await.expect("get none").is_none());

    // unknown entity → error
    assert!(admin.list("widgets", &AdminListQuery::new(10, None)).await.is_err());

    db.cleanup().await;
}
```

- [ ] **Step 3: Run the test to verify it fails (no `identity::admin` yet)**

Run: `cargo test -p identity --test admin_data`
Expected: FAIL to COMPILE (`unresolved import identity::admin`). That's the expected red state.

- [ ] **Step 4: Write `domains/identity/src/admin.rs`**

```rust
//! Schema-admin data source: read access to identity's admin entities.

use crate::models::user::{User, UserId};
use crate::repositories::UserRepository;
use platform_core::{AppError, AppResult, ErrorCode};
use platform_module::{AdminDataSource, AdminListQuery, AdminPage};
use serde_json::Value;
use std::sync::Arc;

/// identity's read-only admin data source. Holds a `UserRepository` and exposes
/// the "users" entity. Strong `User` types are converted to `Value` only here,
/// at the seam exit.
#[derive(Debug)]
pub struct IdentityAdminData {
    repository: Arc<dyn UserRepository>,
}

impl IdentityAdminData {
    #[must_use]
    pub fn new(repository: Arc<dyn UserRepository>) -> Self {
        Self { repository }
    }
}

#[async_trait::async_trait]
impl AdminDataSource for IdentityAdminData {
    async fn list(&self, entity: &str, query: &AdminListQuery) -> AppResult<AdminPage> {
        match entity {
            "users" => {
                let rows = self
                    .repository
                    .list(query.limit + 1, query.cursor.as_deref())
                    .await?;
                let has_more = rows.len() as i64 > query.limit;
                let take = rows.len().min(query.limit.max(0) as usize);
                let page_rows = &rows[..take];
                let next_cursor = if has_more {
                    page_rows.last().map(|u| u.id.0.clone())
                } else {
                    None
                };
                let records = page_rows.iter().map(user_to_value).collect();
                Ok(AdminPage { records, next_cursor })
            }
            other => Err(unknown_entity(other)),
        }
    }

    async fn get(&self, entity: &str, id: &str) -> AppResult<Option<Value>> {
        match entity {
            "users" => Ok(self
                .repository
                .find_by_id(&UserId(id.to_owned()))
                .await?
                .as_ref()
                .map(user_to_value)),
            other => Err(unknown_entity(other)),
        }
    }
}

fn unknown_entity(entity: &str) -> AppError {
    AppError::new(ErrorCode::NotFound, format!("unknown admin entity: {entity}"))
}

/// Strong type → `Value`, ONLY at the boundary. Keys MUST match `user_schema()`.
fn user_to_value(user: &User) -> Value {
    serde_json::json!({
        "id": user.id.0,
        "email": user.email,
        "display_name": user.display_name,
        "created_at": user.created_at,
        "updated_at": user.updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn user_to_value_keys_match_schema_fields() {
        let now = Utc::now();
        let user = User {
            id: UserId("usr_1".to_owned()),
            email: "a@example.com".to_owned(),
            display_name: None,
            created_at: now,
            updated_at: now,
        };
        let value = user_to_value(&user);
        let object = value.as_object().expect("object");
        let mut keys: Vec<&String> = object.keys().collect();
        keys.sort();
        // MUST stay in sync with crate::module::user_schema() field names.
        assert_eq!(keys, vec!["created_at", "display_name", "email", "id", "updated_at"]);
    }
}
```

- [ ] **Step 5: Declare `pub mod admin;` in `domains/identity/src/lib.rs`**

Add `pub mod admin;` to the module list (alphabetical-ish; place after the opening, e.g. right after `pub mod commands;`):

```rust
pub mod admin;
```

- [ ] **Step 6: Add `user_schema()` + wire into `module()` in `domains/identity/src/module.rs`**

Change the import line 2 from:
```rust
use platform_module::{LinkedBinding, Module, ModuleManifest};
```
to:
```rust
use platform_module::{
    AdminSchema, EntitySchema, FieldSchema, FieldType, LinkedBinding, Module, ModuleManifest,
};
```

Add `use crate::admin::IdentityAdminData;`, `use crate::repositories::PostgresUserRepository;`, and `use std::sync::Arc;` to the imports.

Add the schema function (after `story_display()`):

```rust
pub fn user_schema() -> AdminSchema {
    AdminSchema {
        entities: vec![EntitySchema {
            name: "users".to_owned(),
            label: "Users".to_owned(),
            read_capability: "identity.users.read".to_owned(),
            fields: vec![
                FieldSchema { name: "id".into(), label: "ID".into(), field_type: FieldType::String, nullable: false },
                FieldSchema { name: "email".into(), label: "Email".into(), field_type: FieldType::String, nullable: false },
                FieldSchema { name: "display_name".into(), label: "Display Name".into(), field_type: FieldType::String, nullable: true },
                FieldSchema { name: "created_at".into(), label: "Created".into(), field_type: FieldType::Timestamp, nullable: false },
                FieldSchema { name: "updated_at".into(), label: "Updated".into(), field_type: FieldType::Timestamp, nullable: false },
            ],
        }],
    }
}
```

Change `manifest()` to attach the schema:

```rust
pub fn manifest() -> ModuleManifest {
    ModuleManifest::builder("identity")
        .story_display(story_display())
        .admin(user_schema())
        .build()
}
```

Change `module()` to take a USED `ctx` (it was `_ctx`) and attach the data source:

```rust
pub fn module(ctx: &AppContext) -> Module {
    let repository = Arc::new(PostgresUserRepository::new(ctx.db.clone()));
    let binding = LinkedBinding::builder()
        .runtime(crate::runtime::descriptor())
        .build();
    Module::linked(manifest(), binding)
        .with_runtime_config(crate::config::RUNTIME_CONFIG.as_slice())
        .with_admin_data(Arc::new(IdentityAdminData::new(repository)))
}
```

NOTE: `manifest()` stays context-free (no ctx) — preserves OpenAPI purity. Only `module(ctx)` touches the DB.

- [ ] **Step 7: Run the tests**

Run: `cargo test -p identity --test admin_data` (needs a DB; without one it early-returns and reports ok with 0 work)
Expected: PASS (or skip if no DB — the `let Some(db) = ... else { return }` guard).

Run: `cargo test -p identity --lib`
Expected: PASS — includes `user_to_value_keys_match_schema_fields`.

Run: `cargo check --workspace`
Expected: PASS — `app-bootstrap`'s `modules(ctx)` still compiles (identity's `module(ctx)` signature unchanged; only its body grew).

- [ ] **Step 8: Commit**

```bash
git add domains/identity/
git commit -m "feat(identity): schema-admin data source for users

Add UserRepository::list (UUID v7 cursor pagination), IdentityAdminData
implementing AdminDataSource (list/get → Value at the boundary), and a User
AdminSchema declared in the manifest + data source wired into module(ctx).
Tests: cursor pagination (DB) and user_to_value/schema key alignment (unit)."
```

---
## Task 3: `platform-admin-data` crate (generic endpoints)

New crate exposing three generic endpoints over an injected registry of admin-capable modules. Depends on NO domain. Mirrors `platform-admin`'s router/OnceLock-install pattern.

**Files:**
- Modify: `Cargo.toml` (workspace)
- Create: `crates/platform-admin-data/Cargo.toml`
- Create: `crates/platform-admin-data/src/lib.rs`
- Create: `crates/platform-admin-data/src/dto.rs`
- Create: `crates/platform-admin-data/src/handlers.rs`

- [ ] **Step 1: Add the crate to the workspace `Cargo.toml`**

In `members`, add (after `crates/platform-admin`):
```toml
    "crates/platform-admin-data",
```
In `[workspace.dependencies]`, after the `platform-admin = ...` line:
```toml
platform-admin-data = { path = "crates/platform-admin-data" }
```

- [ ] **Step 2: Write `crates/platform-admin-data/Cargo.toml`**

```toml
[package]
name = "platform-admin-data"
version = "0.1.0"
edition.workspace = true
license.workspace = true
publish.workspace = true
rust-version.workspace = true

[dependencies]
axum.workspace = true
platform-core.workspace = true
platform-http.workspace = true
platform-module.workspace = true
serde.workspace = true
serde_json.workspace = true
utoipa.workspace = true

[lints]
workspace = true
```

NOTE: NO domain dependency. NO `sqlx` (data access is behind the trait). `utoipa-axum` types come via `platform-http`'s re-exports (`OpenApiRouter`, `routes`).

- [ ] **Step 3: Write `crates/platform-admin-data/src/dto.rs`**

```rust
//! Generic container DTOs for schema-admin endpoints. The record shape is
//! `serde_json::Value` because the renderer is generic across arbitrary modules.

use platform_module::AdminSchema;
use serde::Serialize;
use utoipa::ToSchema;

/// Response for `GET /admin/data/schema`: every admin-capable module's schema.
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminSchemaListResponse {
    pub modules: Vec<AdminModuleSchema>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminModuleSchema {
    pub module_name: String,
    pub schema: AdminSchema,
}

/// Response for `GET /admin/data/{module}/{entity}`: a page of records.
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminDataListResponse {
    /// Each record is an arbitrary JSON object whose keys match the entity schema.
    pub data: Vec<serde_json::Value>,
    pub page: AdminDataPageInfo,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminDataPageInfo {
    pub limit: i64,
    pub next_cursor: Option<String>,
}

/// Response for `GET /admin/data/{module}/{entity}/{id}`.
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminDataDetailResponse {
    pub data: serde_json::Value,
}
```

- [ ] **Step 4: Write `crates/platform-admin-data/src/handlers.rs`**

```rust
use crate::dto::{
    AdminDataDetailResponse, AdminDataListResponse, AdminDataPageInfo, AdminModuleSchema,
    AdminSchemaListResponse,
};
use crate::{admin_modules, find_module};
use axum::Json;
use axum::extract::{Path, Query};
use platform_core::{AppError, ErrorCode};
use platform_http::{AdminActor, ApiErrorResponse, ErrorResponse, HttpRequestContext};
use platform_module::AdminListQuery;
use serde::Deserialize;

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

#[derive(Debug, Deserialize)]
pub(crate) struct DataListQuery {
    pub limit: Option<i64>,
    pub cursor: Option<String>,
}

#[utoipa::path(
    get,
    path = "/admin/data/schema",
    operation_id = "admin_data_list_schemas",
    tag = "admin-data",
    params(("authorization" = String, Header, description = "Development service bearer token")),
    responses(
        (status = 200, description = "All admin-capable modules' schemas", body = AdminSchemaListResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service or system authentication is required", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn list_schemas(
    _admin: AdminActor,
    HttpRequestContext(_ctx): HttpRequestContext,
) -> Result<Json<AdminSchemaListResponse>, ApiErrorResponse> {
    let modules = admin_modules()
        .iter()
        .map(|m| AdminModuleSchema {
            module_name: m.module_name.clone(),
            schema: m.schema.clone(),
        })
        .collect();
    Ok(Json(AdminSchemaListResponse { modules }))
}

#[utoipa::path(
    get,
    path = "/admin/data/{module}/{entity}",
    operation_id = "admin_data_list_records",
    tag = "admin-data",
    params(
        ("module" = String, Path, description = "Module name, e.g. identity"),
        ("entity" = String, Path, description = "Entity name, e.g. users"),
        ("limit" = Option<i64>, Query, description = "Max records (default 50, max 200)"),
        ("cursor" = Option<String>, Query, description = "Opaque pagination cursor"),
        ("authorization" = String, Header, description = "Development service bearer token"),
    ),
    responses(
        (status = 200, description = "A page of records", body = AdminDataListResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 404, description = "Unknown module or entity", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn list_records(
    _admin: AdminActor,
    Path((module, entity)): Path<(String, String)>,
    Query(query): Query<DataListQuery>,
    HttpRequestContext(request_ctx): HttpRequestContext,
) -> Result<Json<AdminDataListResponse>, ApiErrorResponse> {
    let admin_module = find_module(&module, &request_ctx)?;
    ensure_entity(admin_module, &entity, &request_ctx)?;

    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let page = admin_module
        .data_source
        .list(&entity, &AdminListQuery::new(limit, query.cursor))
        .await
        .map_err(|e| ApiErrorResponse::with_context(e, &request_ctx))?;

    Ok(Json(AdminDataListResponse {
        data: page.records,
        page: AdminDataPageInfo { limit, next_cursor: page.next_cursor },
    }))
}

#[utoipa::path(
    get,
    path = "/admin/data/{module}/{entity}/{id}",
    operation_id = "admin_data_get_record",
    tag = "admin-data",
    params(
        ("module" = String, Path, description = "Module name"),
        ("entity" = String, Path, description = "Entity name"),
        ("id" = String, Path, description = "Record id"),
        ("authorization" = String, Header, description = "Development service bearer token"),
    ),
    responses(
        (status = 200, description = "One record", body = AdminDataDetailResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 404, description = "Unknown module/entity or record not found", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn get_record(
    _admin: AdminActor,
    Path((module, entity, id)): Path<(String, String, String)>,
    HttpRequestContext(request_ctx): HttpRequestContext,
) -> Result<Json<AdminDataDetailResponse>, ApiErrorResponse> {
    let admin_module = find_module(&module, &request_ctx)?;
    ensure_entity(admin_module, &entity, &request_ctx)?;

    match admin_module
        .data_source
        .get(&entity, &id)
        .await
        .map_err(|e| ApiErrorResponse::with_context(e, &request_ctx))?
    {
        Some(data) => Ok(Json(AdminDataDetailResponse { data })),
        None => Err(ApiErrorResponse::with_context(
            AppError::new(ErrorCode::NotFound, "record not found"),
            &request_ctx,
        )),
    }
}

fn ensure_entity(
    module: &crate::AdminModule,
    entity: &str,
    ctx: &platform_core::RequestContext,
) -> Result<(), ApiErrorResponse> {
    if module.schema.entities.iter().any(|e| e.name == entity) {
        Ok(())
    } else {
        Err(ApiErrorResponse::with_context(
            AppError::new(ErrorCode::NotFound, format!("unknown entity: {entity}")),
            ctx,
        ))
    }
}
```

NOTE: verify `ApiErrorResponse::with_context(AppError, &RequestContext)` signature against `crates/platform-http/src/errors.rs` and the `RequestContext` type name from `HttpRequestContext(ctx)` (it is `platform_core`'s request context — confirm the exact path/type used by `platform-admin`'s handlers, e.g. how `config_handlers.rs` constructs `ApiErrorResponse`). Adjust the `ensure_entity`/`find_module` ctx parameter type to match. If `platform-admin` uses a different error-construction helper (e.g. `ApiErrorResponse::from(err)` without ctx), mirror that exact form instead.

- [ ] **Step 5: Write `crates/platform-admin-data/src/lib.rs`**

```rust
//! Schema-admin data API: generic endpoints that render any module's declared
//! admin entities. Depends on NO business domain — it works only through the
//! injected [`AdminDataSource`] registry and the manifest schema, mirroring
//! `platform-admin`'s seam-only discipline.

use platform_core::{AppError, ErrorCode, RequestContext};
use platform_http::{ApiErrorResponse, ApiOpenApiRouter, OpenApiRouter, routes};
use platform_module::{AdminDataSource, AdminSchema};
use std::sync::{Arc, OnceLock};

mod dto;
mod handlers;

pub use dto::*;
#[allow(clippy::wildcard_imports)]
use handlers::*;

/// One module's admin capability: its declared schema + its live data source.
#[derive(Clone)]
pub struct AdminModule {
    pub module_name: String,
    pub schema: AdminSchema,
    pub data_source: Arc<dyn AdminDataSource>,
}

static ADMIN_REGISTRY: OnceLock<Vec<AdminModule>> = OnceLock::new();

/// Install the admin-capable module registry. Called once by the composition
/// root before the router serves traffic. Idempotent: later calls are ignored.
pub fn install_admin_modules(modules: Vec<AdminModule>) {
    let _ = ADMIN_REGISTRY.set(modules);
}

fn admin_modules() -> &'static [AdminModule] {
    ADMIN_REGISTRY.get().map(Vec::as_slice).unwrap_or_default()
}

fn find_module<'a>(
    module: &str,
    ctx: &RequestContext,
) -> Result<&'a AdminModule, ApiErrorResponse> {
    admin_modules()
        .iter()
        .find(|m| m.module_name == module)
        .ok_or_else(|| {
            ApiErrorResponse::with_context(
                AppError::new(ErrorCode::NotFound, format!("unknown module: {module}")),
                ctx,
            )
        })
}

/// The schema-admin router, mounted by the API app.
pub fn router() -> ApiOpenApiRouter {
    OpenApiRouter::new()
        .routes(routes!(list_schemas))
        .routes(routes!(list_records))
        .routes(routes!(get_record))
}
```

NOTE: `find_module` returns `&'a AdminModule` borrowed from the `'static` registry — `admin_modules()` returns `&'static [AdminModule]`, so the lifetime works (tie `'a` to `'static` implicitly; if the borrow checker complains, change the signature to `-> Result<&'static AdminModule, ApiErrorResponse>`). Confirm `RequestContext` is the correct type name exported by `platform_core` for what `HttpRequestContext(ctx)` yields — adjust imports in both files to match (Task verifies against `platform-admin`'s usage).

- [ ] **Step 6: Check the crate compiles in isolation**

Run: `cargo check -p platform-admin-data`
Expected: PASS. Fix any error-construction signature mismatch (the most likely failure point — `ApiErrorResponse::with_context` exact shape, or `RequestContext` type name). Match whatever `crates/platform-admin/src/config_handlers.rs` does for error responses.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/platform-admin-data/
git commit -m "feat(platform-admin-data): generic schema-admin endpoints

New crate with three generic endpoints (GET /admin/data/schema,
/{module}/{entity}, /{module}/{entity}/{id}) over an injected AdminDataSource
registry. Records cross as serde_json::Value; AdminActor-gated. Zero domain
dependency — works only through the trait + manifest schema."
```

---
## Task 4: Wire into `app-bootstrap` and the api app

Aggregate admin-capable modules in the composition root and inject + mount in the api app. After this, the endpoints serve real data end to end.

**Files:**
- Modify: `crates/app-bootstrap/Cargo.toml`
- Modify: `crates/app-bootstrap/src/lib.rs`
- Modify: `apps/api/Cargo.toml`
- Modify: `apps/api/src/main.rs`
- Modify: `apps/api/src/openapi.rs`

- [ ] **Step 1: Add `platform-admin-data` dep to `app-bootstrap`**

In `crates/app-bootstrap/Cargo.toml`, under `[dependencies]`:
```toml
platform-admin-data.workspace = true
```

- [ ] **Step 2: Add `admin_modules(ctx)` to `crates/app-bootstrap/src/lib.rs`**

Add imports: change `use platform_module::{Module, ModuleManifest};` to:
```rust
use platform_module::{AdminSurface, Module, ModuleManifest};
```
Add `use platform_admin_data::AdminModule;`.

Add the function (after `modules` / `module_manifests`):

```rust
/// Aggregate admin-capable modules: those declaring an `AdminSurface::Schema`
/// AND providing an `AdminDataSource`. Modules without an admin surface (e.g.
/// notifications) are filtered out — "optional capability" semantics.
#[must_use]
pub fn admin_modules(ctx: &AppContext) -> Vec<AdminModule> {
    modules(ctx)
        .into_iter()
        .filter_map(|m| {
            // `modules(ctx)` yields owned Modules — destructure to move fields out.
            let data_source = m.admin_data?;
            let ModuleManifest { name, admin, .. } = m.manifest;
            let AdminSurface::Schema(schema) = admin? else {
                return None;
            };
            Some(AdminModule { module_name: name, schema, data_source })
        })
        .collect()
}
```

NOTE: `ModuleManifest` is `#[non_exhaustive]` within `platform-module` but destructuring with `..` works from outside the defining crate for the fields that ARE public (`name`, `admin`). If the borrow checker / non_exhaustive rules reject the `{ name, admin, .. }` destructure across crates, fall back to field access: `let name = m.manifest.name; let admin = m.manifest.admin;` (taking `m.admin_data` FIRST so `m` isn't partially moved before that). The `else { return None }` arm is required because `AdminSurface` is `#[non_exhaustive]`.

- [ ] **Step 3: Verify app-bootstrap compiles**

Run: `cargo check -p app-bootstrap`
Expected: PASS. If the destructure form fails, apply the field-access fallback from the NOTE.

- [ ] **Step 4: Add `platform-admin-data` dep to the api app**

In `apps/api/Cargo.toml`, under `[dependencies]` (next to `platform-admin.workspace = true`):
```toml
platform-admin-data.workspace = true
```

- [ ] **Step 5: Install the registry in `apps/api/src/main.rs`**

After the existing `platform_admin::install_runtime_config_registry(registry.clone());` line (~line 24), add:

```rust
    platform_admin_data::install_admin_modules(app_bootstrap::admin_modules(&ctx));
```

NOTE: this runs after `ctx` is built and before `build_router`. The registry must be installed before any request hits the endpoints (same ordering guarantee as `install_runtime_config_registry`).

- [ ] **Step 6: Mount the router in `apps/api/src/openapi.rs`**

The last line of `api_router()` is:
```rust
    app_bootstrap::merge_domain_http(base).merge(platform_admin::router())
```
Change it to also merge the new router:
```rust
    app_bootstrap::merge_domain_http(base)
        .merge(platform_admin::router())
        .merge(platform_admin_data::router())
```

NOTE: the three new endpoints' `#[utoipa::path]` annotations now enter `openapi_document()` automatically. `api_router()` stays context-free — the registry is injected in `main.rs`, not read here.

- [ ] **Step 7: Verify the workspace + regenerate contracts if required**

Run: `cargo check --workspace`
Expected: PASS.

Run: `cargo test -p platform-admin-data` (if it has no integration tests yet, this is a no-op; the endpoint integration test is added in Task 5).

Run: `cargo run --locked -p arch-check`
Expected: This may FAIL on "fresh contract artifacts" / "fresh generated SDK" because three new endpoints changed the OpenAPI document. If so, regenerate with the project recipe `just generate` (which runs `generate-contracts` then `generate-ts-sdk`; equivalently `cargo run --locked -p generate-contracts && cargo run --locked -p generate-ts-sdk`), then re-run arch-check. Commit the regenerated artifacts under `contracts/` and `packages/ts-sdk/` together with this task.

- [ ] **Step 8: Commit**

```bash
git add crates/app-bootstrap/ apps/api/ contracts/ packages/
git commit -m "feat(api): wire schema-admin endpoints

app-bootstrap::admin_modules(ctx) aggregates admin-capable modules (identity);
api installs the registry and merges platform_admin_data::router(). The three
generic endpoints enter the OpenAPI document; contracts/SDK regenerated."
```

---

## Task 5: arch-check boundary rule + endpoint integration test

Lock the `platform-admin-data` zero-domain-dependency boundary with an arch-check rule, and add an endpoint-level integration test.

**Files:**
- Modify: `tools/arch-check/src/lib.rs`
- Create: `crates/platform-admin-data/tests/endpoints.rs`
- Modify: `crates/platform-admin-data/Cargo.toml` (dev-deps for the test)

- [ ] **Step 1: Add the boundary rule to `tools/arch-check/src/lib.rs`**

In `run()`, after the `check_forbidden_cross_domain_imports` block, add:
```rust
    collect_result(
        check_admin_data_no_domain_deps(&root),
        "platform-admin-data domain dependency",
        &mut failures,
    );
```

Add the function (mirror the existing `fs`/path helpers in the file):
```rust
/// `platform-admin-data` must not depend on any business domain — it works only
/// through the `AdminDataSource` seam and manifest schema.
pub fn check_admin_data_no_domain_deps(root: &Path) -> anyhow::Result<()> {
    let manifest = root.join("crates/platform-admin-data/Cargo.toml");
    let source = fs::read_to_string(&manifest)
        .with_context(|| format!("failed to read {}", manifest.display()))?;
    let domain_names = domain_names(root)?;
    let mut violations = Vec::new();
    for domain in &domain_names {
        // a path/workspace dep on a domain crate would name it in Cargo.toml
        if source.contains(&format!("{domain}.workspace"))
            || source.contains(&format!("\"{domain}\""))
            || source.contains(&format!("{domain} ="))
        {
            violations.push(format!("platform-admin-data depends on domain `{domain}`"));
        }
    }
    ensure_empty(
        violations,
        "platform-admin-data must not depend on any domain crate (use the AdminDataSource seam)",
    )
}
```

NOTE: `domain_names`, `ensure_empty`, `relative` already exist in this file (used by `check_forbidden_cross_domain_imports`). Reuse them. Verify `domain_names(root)` returns the domain crate names (e.g. `["identity", "notifications"]`).

- [ ] **Step 2: Run arch-check to confirm the rule passes (negative control)**

Run: `cargo run --locked -p arch-check`
Expected: PASS — `platform-admin-data/Cargo.toml` names no domain, so the rule is green. (If you temporarily add `identity.workspace = true` to that Cargo.toml, the rule should FAIL — optional manual sanity check, revert after.)

- [ ] **Step 3: Add an endpoint integration test**

This test builds the router with a stub `AdminDataSource` (no DB needed) and asserts the schema + list flow. Add to `crates/platform-admin-data/Cargo.toml`:
```toml
[dev-dependencies]
axum.workspace = true
tokio = { workspace = true }
serde_json.workspace = true
async-trait.workspace = true
tower = { workspace = true }
http.workspace = true
```

NOTE: check the workspace `[workspace.dependencies]` for `tower`, `http`, `tokio` availability (the API crate uses them). If `tower`/`http` aren't workspace deps, test via `axum`'s own test surface or add them to the workspace. Confirm before writing.

Create `crates/platform-admin-data/tests/endpoints.rs`:
```rust
use axum::body::Body;
use axum::http::{Request, StatusCode};
use platform_admin_data::{AdminModule, install_admin_modules, router};
use platform_module::{
    AdminDataSource, AdminListQuery, AdminPage, AdminSchema, EntitySchema, FieldSchema, FieldType,
};
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt; // for `oneshot`

#[derive(Debug)]
struct StubData;

#[async_trait::async_trait]
impl AdminDataSource for StubData {
    async fn list(&self, _entity: &str, _q: &AdminListQuery) -> platform_core::AppResult<AdminPage> {
        Ok(AdminPage {
            records: vec![serde_json::json!({"id": "usr_1", "email": "a@example.com"})],
            next_cursor: None,
        })
    }
    async fn get(&self, _entity: &str, id: &str) -> platform_core::AppResult<Option<Value>> {
        Ok((id == "usr_1").then(|| serde_json::json!({"id": "usr_1", "email": "a@example.com"})))
    }
}

fn schema() -> AdminSchema {
    AdminSchema {
        entities: vec![EntitySchema {
            name: "users".into(),
            label: "Users".into(),
            read_capability: "identity.users.read".into(),
            fields: vec![FieldSchema {
                name: "email".into(), label: "Email".into(),
                field_type: FieldType::String, nullable: false,
            }],
        }],
    }
}

#[tokio::test]
async fn schema_endpoint_lists_installed_modules() {
    install_admin_modules(vec![AdminModule {
        module_name: "identity".into(),
        schema: schema(),
        data_source: Arc::new(StubData),
    }]);

    let app = router().into_make_service(); // see NOTE if this API differs
    // Use a oneshot request against the router. Construct the router into an
    // axum::Router first via `router().split_for_parts()` or the project's
    // standard way of turning an ApiOpenApiRouter into a servable Router.
    let _ = app;
}
```

NOTE: `router()` returns an `ApiOpenApiRouter` (utoipa-axum), not a plain `axum::Router`. To exercise it with `oneshot`, split it: `let (axum_router, _doc) = router().split_for_parts();` then `axum_router.oneshot(request)`. Check how `apps/api/src/lib.rs` (`build_router`) turns the router into a servable one and mirror that. The endpoints require an `AdminActor` (a `Bearer dev-service:admin` header) — set `.header("authorization", "Bearer dev-service:admin")` on the request, matching the frontend's `http-client.ts`. Finalize this test against the real `split_for_parts` API; assert `GET /admin/data/schema` returns 200 with the installed module, and `GET /admin/data/identity/users` returns the stubbed record. If wiring a full request proves heavy, at minimum assert `router()` builds and `admin_modules()` returns the installed entry via a thin pub accessor — but prefer the real HTTP-level test.

- [ ] **Step 4: Run the test**

Run: `cargo test -p platform-admin-data`
Expected: PASS.

- [ ] **Step 5: Verify whole workspace**

Run: `cargo test --workspace`
Expected: PASS (DB-dependent identity test skips without a DB).

Run: `cargo run --locked -p arch-check`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add tools/arch-check/ crates/platform-admin-data/
git commit -m "test(platform-admin-data): boundary rule + endpoint test

arch-check now asserts platform-admin-data depends on no domain crate. Add an
endpoint integration test exercising the schema + list flow with a stub
AdminDataSource (no DB)."
```

---
## Task 6: Frontend generic "Data" page

One generic, schema-driven page: fetch schema, pick module/entity, render a generic table + detail driven by `FieldType`. The render model is a pure, unit-tested function. Adding entities needs zero frontend change.

**Files:**
- Create: `apps/runtime-console/src/pages/data-render-model.ts`
- Create: `apps/runtime-console/src/pages/data-render-model.test.ts`
- Create: `apps/runtime-console/src/pages/data-page.tsx`
- Modify: `apps/runtime-console/src/app/router.tsx`
- Modify: `apps/runtime-console/src/components/runtime/runtime-console-shell.tsx`

- [ ] **Step 1: Write the render-model types + pure function `data-render-model.ts`**

```ts
// Mirrors platform-module's AdminSchema/FieldType JSON shapes. Hand-typed
// because the records are generic (serde_json::Value) — there is no per-entity
// generated SDK type.

export type FieldType =
  | { kind: "string" }
  | { kind: "integer" }
  | { kind: "boolean" }
  | { kind: "timestamp" }
  | { kind: "json" };

export type FieldSchema = {
  name: string;
  label: string;
  field_type: FieldType;
  nullable: boolean;
};

export type EntitySchema = {
  name: string;
  label: string;
  fields: FieldSchema[];
  read_capability: string;
};

export type AdminSchema = { entities: EntitySchema[] };

export type ModuleSchema = { module_name: string; schema: AdminSchema };

export type AdminRecord = Record<string, unknown>;

export type RenderedCell = {
  field: string;
  kind: FieldType["kind"];
  /** Display string for the value, already formatted per field type. */
  display: string;
};

/** Format one raw value per its field type into a display string. */
export function renderCell(field: FieldSchema, value: unknown): RenderedCell {
  const kind = field.field_type.kind;
  let display: string;
  if (value === null || value === undefined) {
    display = "—";
  } else {
    switch (kind) {
      case "timestamp":
        display = formatTimestamp(value);
        break;
      case "boolean":
        display = value ? "✓" : "✗";
        break;
      case "json":
        display = JSON.stringify(value);
        break;
      default:
        display = String(value);
    }
  }
  return { field: field.name, kind, display };
}

/** Build the ordered cells for one record, driven by the entity's field schema. */
export function renderRow(entity: EntitySchema, record: AdminRecord): RenderedCell[] {
  return entity.fields.map((field) => renderCell(field, record[field.name]));
}

function formatTimestamp(value: unknown): string {
  const date = new Date(String(value));
  return Number.isNaN(date.getTime()) ? String(value) : date.toISOString();
}
```

- [ ] **Step 2: Write `data-render-model.test.ts`**

```ts
import { describe, expect, it } from "vitest";

import {
  type EntitySchema,
  renderCell,
  renderRow,
} from "./data-render-model";

const entity: EntitySchema = {
  name: "users",
  label: "Users",
  read_capability: "identity.users.read",
  fields: [
    { name: "email", label: "Email", field_type: { kind: "string" }, nullable: false },
    { name: "active", label: "Active", field_type: { kind: "boolean" }, nullable: false },
    { name: "created_at", label: "Created", field_type: { kind: "timestamp" }, nullable: false },
    { name: "meta", label: "Meta", field_type: { kind: "json" }, nullable: true },
  ],
};

describe("renderCell", () => {
  it("renders strings verbatim", () => {
    expect(renderCell(entity.fields[0], "a@example.com").display).toBe("a@example.com");
  });
  it("renders booleans as check/cross", () => {
    expect(renderCell(entity.fields[1], true).display).toBe("✓");
    expect(renderCell(entity.fields[1], false).display).toBe("✗");
  });
  it("renders timestamps as ISO", () => {
    expect(renderCell(entity.fields[2], "2026-06-03T00:00:00Z").display).toBe(
      "2026-06-03T00:00:00.000Z",
    );
  });
  it("stringifies json", () => {
    expect(renderCell(entity.fields[3], { a: 1 }).display).toBe('{"a":1}');
  });
  it("renders null/absent as em dash", () => {
    expect(renderCell(entity.fields[0], null).display).toBe("—");
    expect(renderCell(entity.fields[3], undefined).display).toBe("—");
  });
});

describe("renderRow", () => {
  it("produces one cell per schema field, in order", () => {
    const cells = renderRow(entity, {
      email: "a@example.com",
      active: true,
      created_at: "2026-06-03T00:00:00Z",
      meta: { x: 1 },
    });
    expect(cells.map((c) => c.field)).toEqual(["email", "active", "created_at", "meta"]);
  });
});
```

- [ ] **Step 3: Run the render-model test (verify it passes)**

Run (from `apps/runtime-console/`): `pnpm test data-render-model` (or the project's vitest invocation — check `package.json` scripts; likely `pnpm test` or `pnpm vitest run`).
Expected: PASS — the pure function has no dependencies.

- [ ] **Step 4: Write `data-page.tsx`**

```tsx
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";

import { httpClient, isApiMode } from "../lib/http-client";
import {
  type AdminRecord,
  type EntitySchema,
  type ModuleSchema,
  renderRow,
} from "./data-render-model";

type SchemaResponse = { modules: ModuleSchema[] };
type ListResponse = {
  data: AdminRecord[];
  page: { limit: number; next_cursor: string | null };
};

const dataKeys = {
  schema: ["admin-data", "schema"] as const,
  list: (m: string, e: string) => ["admin-data", "list", m, e] as const,
};

export function DataPage() {
  const [selected, setSelected] = useState<{ module: string; entity: EntitySchema } | null>(null);

  const schemaQuery = useQuery({
    queryKey: dataKeys.schema,
    queryFn: () => httpClient.get("admin/data/schema").json<SchemaResponse>(),
    enabled: isApiMode(),
  });

  const listQuery = useQuery({
    queryKey: selected ? dataKeys.list(selected.module, selected.entity.name) : ["admin-data", "list", "none"],
    queryFn: () =>
      httpClient
        .get(`admin/data/${selected!.module}/${selected!.entity.name}?limit=50`)
        .json<ListResponse>(),
    enabled: isApiMode() && selected !== null,
  });

  if (!isApiMode()) {
    return <DataPlaceholder reason="schema-admin requires API mode" />;
  }

  return (
    <section className="grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden bg-(--background) text-(--foreground)">
      <header className="border-b border-(--border-subtle) bg-(--surface) px-3 py-2">
        <h1 className="font-mono text-[13px] font-semibold">Data</h1>
      </header>
      <div className="grid grid-cols-[220px_minmax(0,1fr)] min-h-0">
        {/* entity picker */}
        <nav className="border-r border-(--border-subtle) overflow-auto p-2 font-mono text-[12px]">
          {schemaQuery.data?.modules.flatMap((m) =>
            m.schema.entities.map((entity) => (
              <button
                key={`${m.module_name}.${entity.name}`}
                type="button"
                onClick={() => setSelected({ module: m.module_name, entity })}
                className="block w-full text-left px-2 py-1 hover:bg-(--sidebar)"
              >
                {m.module_name} / {entity.label}
              </button>
            )),
          )}
        </nav>
        {/* generic table */}
        <div className="overflow-auto p-3 font-mono text-[12px]">
          {selected && listQuery.data ? (
            <table className="w-full">
              <thead>
                <tr>
                  {selected.entity.fields.map((f) => (
                    <th key={f.name} className="text-left px-2 py-1 text-(--muted)">{f.label}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {listQuery.data.data.map((record, i) => (
                  <tr key={i} className="border-t border-(--border-subtle)">
                    {renderRow(selected.entity, record).map((cell) => (
                      <td key={cell.field} className="px-2 py-1">{cell.display}</td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          ) : (
            <p className="text-(--muted)">Select an entity.</p>
          )}
        </div>
      </div>
    </section>
  );
}

function DataPlaceholder({ reason }: { reason: string }) {
  return (
    <section className="grid h-full place-items-center bg-(--background) text-(--muted) font-mono text-[12px]">
      {reason}
    </section>
  );
}
```

NOTE: this mirrors `config-page.tsx`'s use of `httpClient`, `isApiMode`, TanStack Query, and the existing Tailwind CSS-variable classes. The detail view (`GET /.../{id}`) is intentionally minimal here (list is the core proof); a row click opening a detail drawer can be added, but the slice's acceptance is "list renders from schema". If you add the detail drawer, reuse `renderCell` per field. Match the exact UI-component imports available in `components/ui/` (e.g. there may be no `<table>`-based component — raw table with the project's CSS-var classes is fine, as shown).

- [ ] **Step 5: Register the `/data` route in `apps/runtime-console/src/app/router.tsx`**

Add the import near the other page imports:
```tsx
import { DataPage } from "../pages/data-page";
```
Add the route (near `configRoute`):
```tsx
const dataRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/data",
  component: DataPage,
});
```
Add `dataRoute` to the route tree. Find where routes are assembled into the tree (e.g. `rootRoute.addChildren([... ])` near the bottom of the file) and include `dataRoute` in that array.

- [ ] **Step 6: Add the nav item in `runtime-console-shell.tsx`**

In `primaryNavItems` (around line 33), add an entry (import a suitable `lucide-react` icon, e.g. `Database`, alongside the existing icon imports):
```tsx
  { to: "/data", label: "Data", icon: Database },
```

- [ ] **Step 7: Run frontend checks**

Run (from `apps/runtime-console/`): `pnpm test` and the lint recipe (check `package.json` — likely `pnpm lint` using oxlint, `pnpm fmt` using oxfmt).
Expected: PASS — render-model tests green; lint clean. Fix any type/lint issues.

Run (from repo root): `pnpm -C apps/runtime-console build` (or the project's typecheck) to confirm TypeScript compiles.
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add apps/runtime-console/
git commit -m "feat(console): generic schema-driven Data page

One generic page renders any module's admin entities: fetch /admin/data/schema,
pick an entity, render a table driven by FieldType via a pure (unit-tested)
render model. Adding entities needs zero frontend change."
```

---

## Final Verification (acceptance criteria from spec)

- [ ] `cargo check --workspace` passes; `platform-admin-data` is a workspace member (Tasks 1–5).
- [ ] Each contract-first step independently compiled/tested (Tasks 1→6 each ran checks).
- [ ] Back-end tests green; identity DB integration test runs with a DB, skips without (Task 2).
- [ ] Frontend render-model tests green; `pnpm` lint/test pass (Task 6).
- [ ] `tools/arch-check` passes, INCLUDING `check_admin_data_no_domain_deps` (Task 5).
- [ ] OpenAPI single-source preserved: three new endpoints in `openapi_document()`; `manifest()` stays context-free; contracts/SDK regenerated (Task 4).
- [ ] End-to-end manual check: start api + console (`just api`, `just console` in API mode), the "Data" page lists identity users.

```bash
cargo test --workspace
cargo run --locked -p arch-check
( cd apps/runtime-console && pnpm test )
```
