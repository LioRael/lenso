# Rename: Settings → RuntimeConfig

Date: 2026-06-03
Status: Approved (design); pending implementation plan
Supersedes naming established in: `2026-06-02-config-system-design.md`

## Problem

The configuration system ships with two inconsistent naming schemes:

- **Core layer** (`platform-core`) uses `Setting*`: `SettingDescriptor`,
  `SettingsRegistry`, `SettingsSnapshot`, `SettingsProvider`, `AppContext.settings`.
- **Outward layer** (HTTP/DTO/console) uses `Config*`: `/admin/config/*`,
  `ConfigDescriptorDto`, OpenAPI tag `admin-config`.

Worse, the Runtime Console exposes a **"Settings"** nav item (`/settings`) that
actually manages *service configuration*. In an operator console, "Settings"
naturally reads as "settings for the console itself" (theme, preferences). That
word is being squatted by the wrong concept.

## Decision

Unify on a single semantic: the system manages a service's **runtime
configuration**.

- **Static config** (env/files, infrastructure: DB URL, ports, CORS) keeps the
  `AppConfig` / `ctx.config` family. Unchanged.
- **Dynamic, console-editable config** is renamed `Setting*` → `RuntimeConfig*`.
- **Outward HTTP/console contract** stays `config` (already correct from the
  operator's view).
- The word **"Settings"** is freed in the console for a future "console-self
  settings" surface.

`RuntimeConfig*` (rather than bare `Config*`) avoids a head-on collision with the
existing static `AppConfig` family and carries the right meaning: "configuration
that can change at runtime."

## Rename Map (Rust core — `platform-core`)

| Current | New |
| --- | --- |
| `mod settings` / `src/settings/` dir | `mod runtime_config` / `src/runtime_config/` |
| `SettingDescriptor` | `RuntimeConfigDescriptor` |
| `SettingType` | `RuntimeConfigType` |
| `SettingScope` | `RuntimeConfigScope` |
| `SettingSource` | `RuntimeConfigSource` |
| `SettingsRegistry` | `RuntimeConfigRegistry` |
| `SettingsSnapshot` | `RuntimeConfigSnapshot` |
| `SettingsProvider` | `RuntimeConfigProvider` |
| `StaticSettingsProvider` | `StaticRuntimeConfigProvider` |
| `PostgresSettingsProvider` | `PostgresRuntimeConfigProvider` |
| `SnapshotCell` | `RuntimeConfigCell` |
| `SettingAuditEntry` | `RuntimeConfigAuditEntry` |
| `StoredSetting` | `StoredRuntimeConfig` |
| `AppContext.settings` | `AppContext.runtime_config` |
| `with_settings_provider` | `with_runtime_config_provider` |
| `DomainDescriptor::with_settings` | `DomainDescriptor::with_runtime_config` |
| `DomainDescriptor.settings` field | `DomainDescriptor.runtime_config` field |
| `app_bootstrap::setting_descriptors` | `app_bootstrap::runtime_config_descriptors` |
| `install_settings_registry` | `install_runtime_config_registry` |
| `settings_registry()` (admin) | `runtime_config_registry()` |
| `identity::config::SETTINGS` | `identity::config::RUNTIME_CONFIG` |

Descriptor **field** names (`key`, `scope`, `value_type`, `default`, `editable`,
`restart_only`, `description`) are unchanged.

`CONFIG_NOTIFY_CHANNEL` (name and value `config_changed`) is unchanged.

## Prerequisite rename (collision avoidance)

`platform-core::config` already defines a small static struct `RuntimeConfig`
(holds only `worker_poll_interval_ms`), part of the `AppConfig` family. It must be
renamed first to free the `RuntimeConfig*` name for the dynamic system:

- `RuntimeConfig` struct → `WorkerConfig`
- `AppConfig.runtime` field → `AppConfig.worker`
- Re-export in `lib.rs` updated accordingly.

## Unchanged (confirmed)

- Static config family: `AppConfig`, `ctx.config`, `DatabaseConfig`, `HttpConfig`,
  `TelemetryConfig`, `AuthConfig`, `ServiceConfig`, `ModuleConfig`.
- HTTP routes `/admin/config/*`; OpenAPI tag `admin-config`; operation ids
  `admin_config_*`.
- All HTTP DTOs: `ConfigDescriptorDto`, `ConfigValueDto`, `ConfigWriteRequest`,
  `ConfigWriteResponse`, `ConfigAuditDto`, and their list responses.
- DB schema `config`; tables `config.setting_values`, `config.setting_audit`;
  migration `0007_create_config_schema`.
- LISTEN/NOTIFY channel `config_changed`.
- Generated artifacts (OpenAPI YAML, TS SDK) — outward DTO names are unchanged, so
  `just generate` produces no diff. `just generated-check` must still pass.

## Console changes

- Nav item label `Settings` → `Configuration`; route `/settings` → `/config`.
- `src/pages/settings-page.tsx` → `src/pages/config-page.tsx`; component
  `SettingsPage` → `ConfigPage`; route binding updated in `app/router.tsx`.
- Internal local symbols in the page that say "settings" in a service-config sense
  may be renamed to "config" for clarity, but this is cosmetic and not required for
  correctness.
- The `Settings` Lucide icon may stay or be swapped (e.g. `SlidersHorizontal`);
  cosmetic.

## Scope & Risk

- Pure mechanical rename: **no behavior change**. ~200 Rust symbol sites, ~4
  console files, plus the prerequisite `RuntimeConfig`→`WorkerConfig` rename.
- No DB migration, no route change, no contract/SDK regeneration needed.
- Guarded at each step by `cargo check --locked --workspace --all-targets` and the
  full `just check` gate (incl. Postgres-backed integration tests).

## Verification

- `just check` passes (fmt, rust-check, all tests, generated-check, arch-check,
  sdk-check, console-check).
- `just generated-check` shows no diff (outward contract unchanged).
- The config integration tests (`settings_provider`/`config_console`, themselves
  renamed if they reference renamed symbols) still pass against Postgres.
- No remaining references to the old `Setting*` core symbols (grep clean).
