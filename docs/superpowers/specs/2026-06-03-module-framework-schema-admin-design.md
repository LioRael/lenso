# Module Framework Step 2 — Schema Admin (Read-Only Vertical Slice)

**Date:** 2026-06-03
**Status:** Approved design, ready for implementation planning
**Scope:** Step 2 of the module-framework evolution. A read-only, single-entity vertical slice proving the schema-driven admin path end to end.

---

## Context & Vision

Lenso is evolving into a module framework where business modules are managed in the console as a business backend. Step 1 (merged) split `DomainDescriptor` into `ModuleManifest` (serializable data) + `ModuleBinding` (behavior) in the `platform-module` crate, and reserved an empty `AdminSurface` enum + `manifest.admin: Option<AdminSurface>` seam.

This spec fills the **`AdminSurface::Schema`** seam: a module declares its manageable entities as data, the console renders a generic CRUD UI from that declaration, and a data-access seam (`AdminDataSource`) lets the console read the module's records without the console knowing anything about the module's tables.

The 4-step roadmap (each its own spec):
1. Manifest/Binding split — **DONE**.
2. `AdminSurface::Schema` + data protocol + capabilities seam → schema-driven business backend. ← **THIS SPEC**
3. `Remote` (out-of-process) module source.
4. `AdminSurface::Custom` (plugin self-rendering) + `Wasm` module source.

### Scope: minimal read-only vertical slice

This spec delivers exactly:
- ONE entity — identity's `User`.
- READ only — list + detail (no create/update/delete).
- The full vertical slice: contract → identity implementation → generic HTTP endpoints → generic console renderer.

It proves the slice works end to end and locks the contract seams. Everything else is **explicitly deferred**.

### Non-goals (deferred to later specs)

- Write operations (create / update / delete) and their validation/concurrency/audit concerns.
- Multiple entities or relations between entities.
- Fine-grained RBAC / multi-tenancy (capabilities are *declared* but gated only coarsely here).
- Remote / Wasm `AdminDataSource` implementations.
- `AdminSurface::Custom` (plugin self-rendering).

### Design reference frame (NOT built here)

The user intends the observability console (`platform-admin`) to eventually become a user-installable module. That module's complex visualizations would use `AdminSurface::Custom` (Step 4), not Schema. Implication for *this* spec: the Schema path is the "plain business-entity CRUD" lane, and `AdminDataSource` must be general enough that a future complex module could also implement it (hence: opaque cursor, no timestamp assumption, `Value` records). We build none of that now — it only constrains the seam shapes.

---

## Key Decisions (and why)

| Decision | Choice | Why |
|----------|--------|-----|
| Scope | Read-only, single entity (User), full vertical slice | Proves the slice + locks seams; write/multi-entity are large concerns of their own (validation, relations). Seams open from day one, filled by risk order. |
| Schema vs execution | Schema is data in the manifest; execution is behavior via a NEW trait | Same data/behavior split as Step 1: a serializable schema can travel independent of behavior. |
| Data-access seam | Separate narrow trait `AdminDataSource`, optional `Option<Arc<dyn ...>>` on `Module` | Not every module has an admin UI (notifications doesn't); folding it into `ModuleBinding` would force all bindings to implement it. Optional capability = accurate semantics. |
| Record shape across boundary | `serde_json::Value` + schema field descriptors | The only shape a GENERIC renderer handles across arbitrary modules. Strong types (`User`) stay inside the impl, converted to `Value` only at the seam exit. Naturally serializable → future Remote/Wasm reuse the renderer. |
| Trait methods | Only `list` + `get` (read), with a structured `AdminListQuery` | Narrow trait (Step 1 lesson). Structured query reserves room for future filter/sort without changing signatures — vs. the rejected "wide trait with unimplemented write stubs". |
| Pagination | `limit` + opaque cursor (`Option<String>`) | More general than `platform-admin`'s `next_created_before` — does not assume entities have a timestamp (satisfies the "future complex module" constraint). |
| Field-type vocabulary | Minimal set `String/Integer/Boolean/Timestamp/Json`, `Json` catch-all | Covers User; `Json` guarantees any field renders; `#[non_exhaustive]` enum so `Enum`/`Relation`/etc. add later without breaking the contract. Mirrors `RuntimeConfigType`. |
| Capabilities | Schema declares `read_capability` per entity; gating uses existing coarse `AdminActor` only | Fills the reserved `capabilities` seam in the contract without dragging in a full RBAC spec. Fine-grained model deferred. |
| Crate placement | New crate `platform-admin-data` (NOT in `platform-admin`) | `platform-admin` is runtime-observability with a zero-domain-dependency boundary (and is itself slated to become a module later). Schema-admin is a new responsibility; give it a clean crate that also depends on no domain — only the `AdminDataSource` seam + manifest schema. |
| Frontend | One generic console page, runtime schema-driven | Adding a new entity/module needs ZERO frontend change — the payoff of "console as business backend, module authors write no UI code". |
| HTTP shape | Generic container endpoints (`data: Vec<Value>` + page) | The only shape that serves arbitrary entities. Endpoints still carry `#[utoipa::path]` and enter `openapi_document()`, so the OpenAPI-single-source rule holds; only the body type is a stable generic container, not a per-entity generated type. |
| Migration strategy | Contract-first, back-end vertical slice, front-end last | Each step compiles/tests independently; frontend builds against stable real endpoints (no mock drift). Same rhythm as Step 1. |

---

## Architecture & Data Flow

```
platform-module (contracts)
  AdminSurface::Schema(AdminSchema)          ← data, in the manifest
  AdminSchema { entities: [EntitySchema{ name, label, fields, read_capability }] }
  FieldType { String|Integer|Boolean|Timestamp|Json }
  trait AdminDataSource { list(), get() }    ← behavior seam
  Module.admin_data: Option<Arc<dyn AdminDataSource>>
        ▲ declare schema             ▲ implement trait
identity (module)
  manifest(): ...admin(user_schema())
  IdentityAdminData: list/get → repository SQL → User → serde_json::Value
        ▲ read via seam (never touches identity tables)
platform-admin-data (new crate, generic endpoints)
  GET /admin/data/schema               → all modules' AdminSchema
  GET /admin/data/{module}/{entity}    → list (Value records)
  GET /admin/data/{module}/{entity}/{id} → detail
  AdminActor coarse gate; reads via AdminDataSource registry
        ▲ HTTP (generic container responses)
runtime-console (frontend)
  "Data" page: fetch schema → pick module/entity → schema-driven render
```

**Data flow (list users):**
1. Frontend `GET /admin/data/schema` → identity's User schema (fields + types).
2. Frontend `GET /admin/data/identity/users?limit=50` → `platform-admin-data` endpoint.
3. Endpoint looks up the `AdminDataSource` registry by module, calls `list("users", query)`.
4. identity's impl runs repository SQL, converts `Vec<User>` → `Vec<Value>`, wraps in `AdminPage`.
5. Endpoint returns `{ data: [...Value], page: { limit, next_cursor } }`.
6. Frontend renders those `Value`s driven by the schema from step 1.

**Boundary:** `platform-admin-data` has ZERO dependency on identity — it works only through the `AdminDataSource` trait and the manifest schema, the same "seam-only" discipline as `platform-admin` reading runtime tables.

---

## Contracts (`platform-module`)

### `AdminSurface` — fill the first variant (`admin.rs`)

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AdminSurface {
    /// Schema-driven CRUD: console renders a generic UI from this declaration.
    Schema(AdminSchema),
    // Custom { .. } — reserved for Step 4 (plugin self-rendering). Not yet.
}
```

### `AdminSchema` + field-type vocabulary (new `admin_schema.rs`)

`AdminSchema` derives `utoipa::ToSchema` in addition to serde, so the same type is both manifest data and an OpenAPI response type (one type, two uses).

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[non_exhaustive]
pub struct AdminSchema {
    pub entities: Vec<EntitySchema>,
}

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
```

### `AdminDataSource` behavior trait (new `admin_data.rs`)

```rust
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
```

`AdminListQuery`/`AdminPage` are runtime behavior I/O, not declared data, so they do NOT derive serde — only the schema needs to serialize. (`AdminPage.records` holds `Value`s, which serialize fine; the container itself is in-process.)

### `Module` + manifest builder additions (`module.rs`, `manifest.rs`)

```rust
// module.rs — Module gains a field:
pub admin_data: Option<Arc<dyn AdminDataSource>>,
// Module::linked(...) defaults it to None.
// New builder method:
pub fn with_admin_data(mut self, data: Arc<dyn AdminDataSource>) -> Self {
    self.admin_data = Some(data);
    self
}

// manifest.rs — ModuleManifestBuilder gains:
pub fn admin(mut self, schema: AdminSchema) -> Self {
    self.manifest.admin = Some(AdminSurface::Schema(schema));
    self
}
```

`#[non_exhaustive]` covers `AdminSchema`/`EntitySchema`/`FieldSchema`/`FieldType`/`AdminListQuery`/`AdminPage` so future fields/types/methods don't break the contract. `lib.rs` re-exports the new public types.

---

## identity Implementation

### `UserRepository` gains a list query

Current methods: `insert`, `insert_with_outbox`, `find_by_id`, `find_by_email`. Add:

```rust
async fn list(&self, limit: i64, cursor: Option<&str>) -> AppResult<Vec<User>>;
```

Cursor pagination keyed on `id` (UUID v7, monotonic — no `created_at` assumption, satisfying the "no timestamp" constraint). `cursor` is the last row's `id`:

```sql
-- first page (no cursor):
select id, email, display_name, created_at, updated_at
from identity.users
order by id asc
limit $1
-- with cursor: add `where id > $cursor`
```

The `AdminDataSource` impl fetches `limit + 1` rows to detect a next page (see below).

### `IdentityAdminData` (new `domains/identity/src/admin.rs`)

```rust
#[derive(Debug)]
pub struct IdentityAdminData {
    repository: Arc<dyn UserRepository>,
}

#[async_trait::async_trait]
impl AdminDataSource for IdentityAdminData {
    async fn list(&self, entity: &str, query: &AdminListQuery) -> AppResult<AdminPage> {
        match entity {
            "users" => {
                let rows = self.repository
                    .list(query.limit + 1, query.cursor.as_deref())
                    .await?;
                let has_more = rows.len() as i64 > query.limit;
                let take = rows.len().min(query.limit as usize);
                let page_rows = &rows[..take];
                let next_cursor = has_more
                    .then(|| page_rows.last().map(|u| u.id.0.clone()))
                    .flatten();
                let records = page_rows.iter().map(user_to_value).collect();
                Ok(AdminPage { records, next_cursor })
            }
            other => Err(AppError::new(
                ErrorCode::NotFound,
                format!("unknown admin entity: {other}"),
            )),
        }
    }

    async fn get(&self, entity: &str, id: &str) -> AppResult<Option<Value>> {
        match entity {
            "users" => Ok(self.repository
                .find_by_id(&UserId(id.to_owned()))
                .await?
                .as_ref()
                .map(user_to_value)),
            other => Err(AppError::new(
                ErrorCode::NotFound,
                format!("unknown admin entity: {other}"),
            )),
        }
    }
}

/// Strong type → Value, ONLY at the boundary. Field keys match the schema.
fn user_to_value(user: &User) -> Value {
    serde_json::json!({
        "id": user.id.0,
        "email": user.email,
        "display_name": user.display_name,
        "created_at": user.created_at,
        "updated_at": user.updated_at,
    })
}
```

### Schema declaration + module wiring (`module.rs`)

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

pub fn manifest() -> ModuleManifest {
    ModuleManifest::builder("identity")
        .story_display(story_display())
        .admin(user_schema())               // schema → manifest (data)
        .build()
}

pub fn module(ctx: &AppContext) -> Module {
    let repository = Arc::new(PostgresUserRepository::new(ctx.db.clone()));
    let binding = LinkedBinding::builder().runtime(crate::runtime::descriptor()).build();
    Module::linked(manifest(), binding)
        .with_runtime_config(crate::config::RUNTIME_CONFIG.as_slice())
        .with_admin_data(Arc::new(IdentityAdminData { repository }))   // behavior
}
```

`manifest()` stays context-free (schema is pure data — preserves the OpenAPI-purity rule). `IdentityAdminData` needs `ctx.db`, so it is built only inside `module(ctx)`. Data/behavior separation holds end to end.

NOTE: `PostgresUserRepository` is shared between the route state (existing `IdentityRouteState`) and the new admin data source; both construct it from `ctx.db`. No change to existing route wiring.

---

## `platform-admin-data` (new crate, generic endpoints)

New workspace crate. Dependencies mirror `platform-admin`: `axum`, `platform-core`, `platform-http`, `platform-module`, `serde`, `serde_json`, `utoipa`, `utoipa-axum`. Depends on NO domain crate.

### Registry + injection (same OnceLock pattern as `platform-admin`)

The composition root aggregates admin-capable modules and injects them. The registry holds both schema (data) and data source (behavior), indexed by module name.

```rust
/// One module's admin capability: its declared schema + its live data source.
#[derive(Clone)]
pub struct AdminModule {
    pub module_name: String,
    pub schema: AdminSchema,
    pub data_source: Arc<dyn AdminDataSource>,
}

static ADMIN_REGISTRY: OnceLock<Vec<AdminModule>> = OnceLock::new();

/// Injected once by the composition root before serving. Idempotent.
pub fn install_admin_modules(modules: Vec<AdminModule>) {
    let _ = ADMIN_REGISTRY.set(modules);
}

fn admin_modules() -> &'static [AdminModule] {
    ADMIN_REGISTRY.get().map(Vec::as_slice).unwrap_or_default()
}
```

### Endpoints (all `#[utoipa::path]`, `AdminActor`-gated, enter `openapi_document()`)

```rust
// GET /admin/data/schema
//   → { modules: [ { module_name, schema } ] }
pub(crate) async fn list_schemas(
    _admin: AdminActor,
    HttpRequestContext(_ctx): HttpRequestContext,
) -> Result<Json<AdminSchemaListResponse>, ApiErrorResponse>;

// GET /admin/data/{module}/{entity}?limit=50&cursor=...
//   → { data: [ <Value record> ], page: { limit, next_cursor } }
pub(crate) async fn list_records(
    _admin: AdminActor,
    Path((module, entity)): Path<(String, String)>,
    Query(query): Query<DataListQuery>,
    HttpRequestContext(ctx): HttpRequestContext,
) -> Result<Json<AdminDataListResponse>, ApiErrorResponse>;

// GET /admin/data/{module}/{entity}/{id}
//   → { data: <Value record> }   (404 if not found)
pub(crate) async fn get_record(
    _admin: AdminActor,
    Path((module, entity, id)): Path<(String, String, String)>,
    HttpRequestContext(ctx): HttpRequestContext,
) -> Result<Json<AdminDataDetailResponse>, ApiErrorResponse>;
```

`DataListQuery { limit: Option<i64>, cursor: Option<String> }` (defaults `limit` to e.g. 50, clamped to a max like 200).

### Generic container DTOs (stable types, not per-entity)

```rust
#[derive(Serialize, ToSchema)]
pub struct AdminSchemaListResponse { pub modules: Vec<AdminModuleSchema> }

#[derive(Serialize, ToSchema)]
pub struct AdminModuleSchema { pub module_name: String, pub schema: AdminSchema }

#[derive(Serialize, ToSchema)]
pub struct AdminDataListResponse {
    pub data: Vec<serde_json::Value>,   // each record is an arbitrary JSON object
    pub page: AdminDataPageInfo,
}

#[derive(Serialize, ToSchema)]
pub struct AdminDataPageInfo { pub limit: i64, pub next_cursor: Option<String> }

#[derive(Serialize, ToSchema)]
pub struct AdminDataDetailResponse { pub data: serde_json::Value }
```

### Endpoint logic

- `list_schemas`: map `admin_modules()` → `{ module_name, schema }`.
- `list_records`: find `AdminModule` by `module` (404 if absent) → verify `entity` is declared in its schema (404 if not) → build `AdminListQuery { limit, cursor }` → call `data_source.list(entity, &query)` → wrap into `AdminDataListResponse`. Internal `AppError` from the data source propagates as `ApiErrorResponse`.
- `get_record`: same lookup → `data_source.get(entity, &id)` → `Some(value)` → 200; `None` → 404.

`router()` returns an `ApiOpenApiRouter` registering the three routes (same shape as `platform_admin::router()`).

---

## Wiring (`app-bootstrap`, api app)

### `app-bootstrap` aggregates admin modules

`AdminModule` needs schema (from manifest, context-free) + data_source (from `Module`, needs ctx). Aggregation happens where ctx exists:

```rust
pub fn admin_modules(ctx: &AppContext) -> Vec<AdminModule> {
    modules(ctx)
        .into_iter()
        .filter_map(|m| {
            // `modules(ctx)` yields owned `Module`s, so move the fields out — no clone.
            let admin = m.manifest.admin?;
            let AdminSurface::Schema(schema) = admin else { return None };
            let data_source = m.admin_data?;
            Some(AdminModule { module_name: m.manifest.name, schema, data_source })
        })
        .collect()
}
```

notifications declares no admin and has no `admin_data`, so it is filtered out — "optional capability" semantics fall out naturally.

NOTE: two reasons the body is written with a `let-else` rather than a single `let ... = ...?` pattern: (1) `?` cannot be applied to a refutable let-binding pattern, and (2) `AdminSurface` is `#[non_exhaustive]`, so matching only the `Schema` variant requires an explicit fallback arm (`else { return None }`). Because `m` is owned, `m.manifest.admin` / `m.admin_data` / `m.manifest.name` are moved out, not cloned — but `m.manifest.admin?` partially moves `manifest`, so bind `name` BEFORE or destructure carefully; if the borrow checker objects to using `m.manifest.name` after moving `m.manifest.admin`, destructure up front: `let ModuleManifest { name, admin, .. } = m.manifest;` then match on `admin` and pull `m.admin_data` separately (note `m.admin_data` is still accessible since only `m.manifest` was destructured). The implementation plan resolves the exact borrow form; the semantics are: keep entities whose manifest is `Schema` AND that provide a data source.

### api app

- `apps/api/src/main.rs`: after ctx is built, alongside `install_runtime_config_registry`:
  ```rust
  platform_admin_data::install_admin_modules(app_bootstrap::admin_modules(&ctx));
  ```
- `apps/api/src/openapi.rs`: alongside `.merge(platform_admin::router())`:
  ```rust
  .merge(platform_admin_data::router())
  ```
  The three endpoints' `#[utoipa::path]` annotations enter `openapi_document()` automatically — OpenAPI single-source rule preserved. `api_router()` stays context-free (schemas come from injected registry, set in `main.rs` before serving).

---

## Frontend (`runtime-console`)

A single generic admin page, runtime schema-driven. Stack: React 19, TanStack Router/Query, Base UI, Tailwind, `ky` http-client (all existing).

- New "Data" route entry (same pattern as existing `pages/*.tsx`).
- On entry: `GET /admin/data/schema` → list `{ module, entity }` choices.
- Select an entity → `GET /admin/data/{module}/{entity}` → take `data: Value[]` + the entity's `fields` → render a **generic table**: columns = schema fields; each cell rendered by `FieldType` (`Timestamp` formats the date, `String` raw, `Boolean` check/cross, `Integer` numeric, `Json` collapsed view).
- Click a row → `GET /.../{id}` → detail view (field label → value, same FieldType rendering).
- ONE generic component driven by runtime schema — NOT one page per entity. Adding a new entity/module requires zero frontend change.

The schema-driven render model (schema + records → column/cell descriptors) is extracted as a pure function for unit testing, mirroring existing `*.test.ts` files.

---

## Testing & Acceptance

This is a new-feature vertical slice, so tests prove the new behavior, layered:

### `platform-module` contracts (unit)
- `AdminSchema` JSON round-trip: `AdminSurface::Schema(...)` → JSON → back, assert equal.
- manifest-with-admin serialization: `manifest().admin` is `Some(Schema(..))`; serialized JSON contains `"kind":"schema"`.
- `FieldType` variants serialize to `{"kind":"..."}` (mirrors `RuntimeConfigType`).

### identity implementation (unit + DB integration)
- `user_to_value`: given a `User`, assert the produced `Value`'s object keys exactly match `user_schema()`'s field `name`s (guards schema/data drift — the crux of generic rendering).
- `IdentityAdminData::list` (DB integration, `TestDatabase`, skip without a DB): insert 3 users; `list(limit=2)` returns 2 records + a `next_cursor`; passing that cursor returns the 3rd + `next_cursor = None`. Verifies cursor pagination.
- `list`/`get` for an unknown entity return `NotFound`.

### `platform-admin-data` endpoints (integration)
- `GET /admin/data/schema` returns identity's User schema.
- `GET /admin/data/identity/users` flows registry → identity data source → `{ data, page }`; record keys align with schema.
- unknown module/entity → 404; missing `AdminActor` auth → 401.

### Frontend (unit, existing `*.test.ts` pattern)
- schema-driven render model: given a schema + records, assert correct column/cell descriptors; cover each `FieldType` → render mapping.

### Acceptance criteria
- [ ] `cargo check --workspace` passes; `platform-admin-data` is a workspace member.
- [ ] Each contract-first step independently `cargo check`/test-passes (contracts → identity → endpoints → wiring → frontend).
- [ ] Back-end tests above green; DB integration tests run with a DB, skip without (existing `TestDatabase` convention).
- [ ] Frontend tests green; `pnpm` frontend checks (lint/test) pass.
- [ ] `tools/arch-check` passes; **a new arch-check rule asserts `platform-admin-data` depends on no `domains/*` crate** (enforces the seam-only boundary).
- [ ] OpenAPI single-source preserved: the three new endpoints enter `openapi_document()`; `manifest()` stays context-free.
- [ ] End-to-end manual check: start api + console, the "Data" page lists identity users and opens a detail (the slice's real proof).

### Honest statement of scope
Read-only, single entity. Writes (create/update/delete), multiple entities, fine-grained RBAC, and Remote/Wasm admin-data are all out of scope, left to later specs. Acceptance is proven by the working slice + layered green tests + seams in place.
