# Configuration System Design

Date: 2026-06-02
Status: Approved (design); pending implementation plan

## Goal

Introduce a configuration system for Lenso that supports:

- **Multi-service**: settings scoped per service (`api`, `worker`) or shared (`*`),
  with a single environment per deployment.
- **Layered config**: the existing static `AppConfig` (env/files) remains the base
  layer for infrastructure and secrets; a new dynamic layer (stored in Postgres)
  overlays it for application/feature settings.
- **Console online editing**: a marked subset of settings is editable from the
  Runtime Console, validated server-side, with edits propagating to running
  instances near-instantly.

## Non-Goals

- Multi-environment in a single instance. Each deployment serves exactly one
  environment (`APP_ENV`); its database holds only that environment's config.
- Making infrastructure/secret config (DB URL, ports) live-editable. Those stay in
  the static `AppConfig`. Sensitive/infra keys are either absent from the dynamic
  registry or marked `restart_only` / non-`editable`.
- Multi-tenant config rows. Scope dimensions are `(service)` only.

## Architecture Overview

```
Static base (existing AppConfig from env/files)  ← infra, secrets, DB URL, ports
        ▼ overlaid by
Dynamic layer (Postgres, console-editable)       ← typed, registered keys only
        ▼ resolved per (service)
Live snapshot in each instance                    ← read via ctx.settings
```

Five components, each with a single responsibility:

1. **`SettingDescriptor` registry** — domains and platform crates declare editable
   keys at composition time. Mirrors the existing `DomainDescriptor` pattern. Only
   registered keys are visible to the console and runtime; unknown keys are rejected.
2. **`config` Postgres schema** — `config.setting_values` (current value per
   `(service, key)`) and `config.setting_audit` (who/when/old→new).
3. **`SettingsProvider`** — `Arc<dyn SettingsProvider>` on `AppContext`, holding the
   live in-memory snapshot merged over registered defaults.
4. **Propagation** — write path emits a platform event and `NOTIFY config_changed`;
   every instance `LISTEN`s and refreshes its snapshot.
5. **Console API + UI** — read/write/audit endpoints in `platform-admin` under
   `/admin/config/*`, plus a Runtime Console settings screen.

The static `AppConfig` is unchanged in meaning. Application/domain code keeps
reading `ctx.config.*` for infrastructure; dynamic values are read through the new
`ctx.settings` provider.

### Crate placement

- Core types (`SettingDescriptor`, `SettingType`, `SettingScope`,
  `SettingsProvider` trait, `PostgresSettingsProvider`, `StaticSettingsProvider`,
  snapshot) live in `platform-core` alongside the existing `config.rs`, or a new
  `settings` module within `platform-core`. (Implementation plan to confirm exact
  module boundary; default is a new `platform-core::settings` module.)
- The `config` schema migrations live under `crates/platform-core/migrations` and
  are registered in `PLATFORM_MIGRATIONS`.
- Console HTTP handlers/DTOs live in `crates/platform-admin`.
- Descriptor aggregation lives in `crates/app-bootstrap` (composition root).

## Data Model

### Descriptor (declared in Rust, per domain / platform crate)

```rust
pub enum SettingScope {
    Shared,                 // stored under service '*'
    Service(&'static str),  // e.g. "api", "worker"
}

pub enum SettingType {
    Bool,
    Int { min: Option<i64>, max: Option<i64> },
    Float { min: Option<f64>, max: Option<f64> },
    String,
    Enum(&'static [&'static str]),
    Json,
}

pub struct SettingDescriptor {
    pub key: &'static str,            // e.g. "notifications.welcome_email.enabled"
    pub scope: SettingScope,
    pub value_type: SettingType,
    pub default: serde_json::Value,   // fallback when no DB row exists
    pub editable: bool,               // false => console shows read-only
    pub restart_only: bool,           // true => persists now, applies on restart
    pub description: &'static str,
}
```

Domains expose `&'static [SettingDescriptor]` and attach it via a new
`DomainDescriptor::with_settings(...)` builder. The composition root aggregates all
domain + platform descriptors, mirroring `story_display_descriptors()`.

### Postgres `config` schema

```sql
create schema if not exists config;

create table config.setting_values (
    service     text        not null,   -- 'api' | 'worker' | '*'
    key         text        not null,
    value       jsonb       not null,
    updated_at  timestamptz not null,
    updated_by  text,                    -- actor from RequestContext
    primary key (service, key)
);

create table config.setting_audit (
    id          uuid        primary key,
    service     text        not null,
    key         text        not null,
    old_value   jsonb,                   -- null on first set
    new_value   jsonb       not null,
    actor       text,
    changed_at  timestamptz not null
);

create index on config.setting_audit (service, key, changed_at desc);
```

### Resolution rule

For a running service `S`, the effective value of `key`:

1. DB row for `(S, key)` if present, else
2. DB row for `('*', key)` if present, else
3. the descriptor's `default`.

Validation against `value_type` runs on write (reject bad input, `422`) and on
snapshot load (invalid stored value falls back to default and logs a warning).

## Read API

`AppContext` gains one field:

```rust
pub settings: Arc<dyn SettingsProvider>,
```

The provider holds an `ArcSwap<Snapshot>` for lock-free reads. Two access styles:

```rust
// Typed: deserialize a domain settings struct, merged over defaults
let cfg: WelcomeEmailConfig = ctx.settings.get::<WelcomeEmailConfig>()?;

// Single key
let enabled: bool = ctx.settings.get_value("notifications.welcome_email.enabled")?;
```

A domain defines a plain `#[derive(Deserialize)]` struct whose fields map to its
registered keys; `get::<T>()` builds it from the current snapshot. Reads never hit
the database.

## Propagation & Lifecycle

### Startup

Each app (`api`, `worker`):

1. Builds the descriptor set from the composition root.
2. Constructs a `PostgresSettingsProvider`, loading the initial snapshot from
   `config.setting_values`.
3. Starts a background `LISTEN config_changed` task.

`migrate` and tests use a `StaticSettingsProvider` (defaults only, no DB/listener).

### Write path (console edit)

```
PUT /admin/config/{service}/{key}
  → look up descriptor (404 if unknown key)
  → reject if !editable (409/403)
  → validate value against value_type (422 on failure)
  → in one transaction:
        upsert config.setting_values
        insert config.setting_audit
  → publish ConfigChanged event (outbox)
  → pg_notify('config_changed', '<service>:<key>')
  → if restart_only: response flags "applies on restart"
```

### Propagation

Every instance's listener wakes on `NOTIFY`, reloads the snapshot (full reload in
v1 for simplicity), and `ArcSwap`s it in. Convergence is sub-second. If the listener
connection drops, it reconnects and performs a full reload, so a missed
notification self-heals. `restart_only` keys persist but are not hot-applied.

## Console API Surface

In `platform-admin`, mounted under `/admin/config/*`, OpenAPI-annotated like the
existing observability handlers so DTOs flow into the generated TS SDK.

```
GET  /admin/config/descriptors          → all registered keys (type, default,
                                            editable, restart_only, description),
                                            grouped by domain/service
GET  /admin/config/values               → current effective values + source
                                            (db-row | shared | default)
PUT  /admin/config/{service}/{key}      → validate + write + audit + notify
GET  /admin/config/audit?key=&service=  → change history
DELETE /admin/config/{service}/{key}    → reset to default (delete the DB row)
```

## Console UI

New route in `apps/runtime-console`, using existing `src/components/ui` primitives,
TanStack Query/Router, and the generated TS SDK — following the dense, scannable
operator-screen convention.

- Settings list grouped by domain → service. Each row shows key, current value, a
  source badge (`overridden` / `shared` / `default`), and editable/restart-only
  state.
- Inline edit control typed by `value_type`: toggle (bool), number input with
  min/max (int/float), select (enum), JSON editor (json). Read-only keys are shown
  but disabled. Restart-only keys edit with an "applies on restart" warning.
- Per-key audit drawer (who/when/old→new) from the audit endpoint.
- "Reset to default" deletes the DB row, falling back to shared/default.

## Safety & Audit (v1)

- **Per-key flags**: `editable` and `restart_only` enforced server-side and surfaced
  in the console.
- **Audit trail**: every change records actor/timestamp/old→new in
  `config.setting_audit`.
- **Event**: every change publishes a `ConfigChanged` platform event via the
  existing outbox/event pattern (also used to drive `NOTIFY`).

## Generated Artifacts

New DTOs flow through the existing `just generate` → OpenAPI → TS SDK pipeline. Run
`just generated-check` before finishing. The console consumes typed SDK clients,
not hand-written fetches.

## Testing

- **Provider unit tests**: resolution order, validation, snapshot construction,
  typed `get::<T>()`.
- **API integration test**: write → audit → notify → refresh round trip, matching
  the style of `apps/api/tests/runtime_console.rs`.
- **`StaticSettingsProvider`** for domain tests in `platform-testing`.

## Components & Boundaries Summary

| Unit | Responsibility | Depends on |
| --- | --- | --- |
| `SettingDescriptor` + registry | Declare typed editable keys | `serde_json` |
| `config` schema + migrations | Persist values + audit | `platform-core` migrations |
| `SettingsProvider` (Postgres/Static) | Live snapshot + typed reads | `config` schema, descriptors |
| LISTEN/NOTIFY task | Propagate edits to instances | `sqlx`, provider |
| `platform-admin` config handlers | Read/write/audit HTTP API | provider, descriptors, outbox |
| Runtime Console settings screen | Operator editing UI | generated TS SDK |

## Open Implementation Details (for the plan)

- Exact module boundary inside `platform-core` (`settings` module vs. extending
  `config.rs`).
- Whether `NOTIFY` payload carries the changed key to enable a partial refresh
  later (v1 does full reload regardless).
- Mapping of `ConfigChanged` into the existing story/event display metadata.
