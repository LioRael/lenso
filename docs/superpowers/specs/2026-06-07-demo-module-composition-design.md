# Demo Module Composition

**Date:** 2026-06-07
**Status:** Implemented in linked composition profile slice
**Scope:** Move `identity` and `notifications` from implicit product defaults into an explicit demo composition profile while preserving their value as framework fixtures.

---

## Context

`identity` and `notifications` are useful linked modules. They exercise public
HTTP routes, schema-admin reads, runtime functions, outbox event handling,
story display descriptors, migrations, OpenAPI generation, SDK generation, and
Runtime Console package navigation.

They are also currently registered as if they were product-default modules:

- `crates/app-bootstrap` always lists them in `LINKED_MODULE_ENTRIES`.
- `apps/migrate` always applies their migrations.
- `apps/api/src/openapi.rs` always includes linked module routes and the
  document-level `identity` tag.
- Local tests and Runtime Console fixtures depend on them as integration
  examples.

That is useful for development, but it makes Lenso look like it prescribes an
identity business domain. The project direction should be clearer: platform
capabilities are core; `identity` and `notifications` are demo fixtures.

## Goals

- Keep `identity` and `notifications` in the repository as high-value linked
  module fixtures.
- Make the running composition explicit: `core` contains platform-owned
  surfaces; `demo` adds the linked fixture modules.
- Ensure API, worker, migrations, OpenAPI, runtime declarations, admin module
  metadata, and console package metadata all use the same composition decision.
- Keep the default local developer experience able to show a multi-workspace
  Runtime Console without extra setup.
- Document that new product projects should replace the demo profile rather
  than treating `identity` as a required Lenso module.

## Non-Goals

- Do not delete `modules/identity`, `modules/notifications`, or their console
  packages.
- Do not introduce dynamic module installation or marketplace trust rules.
- Do not change remote module loading semantics.
- Do not redesign Runtime Console navigation.
- Do not remove identity OpenAPI or SDK helpers in this slice; they should
  remain available when the demo profile is selected.
- Do not add auth/RBAC behavior for real identity management.

## Approaches Considered

| Approach | Shape | Decision |
|----------|-------|----------|
| Delete `identity` and `notifications` | Remove fixture modules and rewrite tests around synthetic platform-only data. | Rejected. It would throw away the best end-to-end module-framework sample. |
| Compile-time Cargo feature | Gate fixture crates behind a feature such as `demo-modules`. | Rejected for the first slice. It makes OpenAPI, migrations, local commands, and CI matrix behavior harder to keep aligned. |
| Runtime composition profile | Add an explicit profile in the composition root: `core` or `demo`. | Recommended. It keeps one runtime decision flowing through app-bootstrap, migrations, worker, OpenAPI, and tests. |

## Key Decisions

| Decision | Choice | Why |
|----------|--------|-----|
| Fixture status | Keep identity/notifications as demo linked modules | They are valuable integration fixtures but should not look mandatory. |
| Profile names | `core` and `demo` | Simple language that separates platform-only operation from sample business modules. |
| Local default | `demo` initially | Existing local commands, tests, console screenshots, and SDK examples keep working while the product story becomes explicit. |
| Production guidance | Projects should choose `core` or define their own composition | Avoids shipping demo business routes by accident once a real product module set exists. |
| Platform story module | Always core | Stories is host-owned Runtime Console infrastructure, not a demo business module. |
| Remote modules | Loaded after the selected linked profile | Remote source loading stays orthogonal to linked demo selection. |

## Proposed Shape

Add a small composition-profile model in `crates/app-bootstrap`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositionProfile {
    Core,
    Demo,
}
```

`platform-core` should add a string config field:

```rust
pub struct ModuleSourcesConfig {
    pub linked_profile: String,
    pub remote: Vec<RemoteModuleSourceConfig>,
}
```

`AppConfig::from_env()` should populate it from `LENSO_COMPOSITION_PROFILE`,
defaulting to `demo`. `app-bootstrap` should parse that string into
`CompositionProfile` at the composition boundary. This keeps platform-core from
depending on concrete app-bootstrap profile semantics while still making the
runtime choice part of one shared config object.

Recommended environment value:

```text
LENSO_COMPOSITION_PROFILE=core|demo
```

Default behavior for the first slice:

- local/default profile: `demo`
- invalid value: startup and generation paths return a configuration error
- context-free OpenAPI generation: use the same default profile unless the
  generator command sets the profile explicitly

## Module Sets

Core linked modules:

- `platform-story`

Demo linked modules:

- all core linked modules
- `identity`
- `notifications`

`identity` remains the demo module that exercises schema-admin, public HTTP
routes, events, runtime declarations, migrations, and the `Identity` console
workspace. `notifications` remains the demo module that consumes
`identity.user_registered.v1` and enqueues welcome-email runtime work.

## API And Worker Flow

API startup should use the selected profile when building:

- runtime config descriptors
- loaded linked modules
- admin module metadata
- story display descriptors
- linked HTTP routes

Worker startup should use the same selected profile when building:

- linked modules
- event handler registry
- function registry
- lifecycle activation declarations
- runtime config descriptors

Remote modules should continue loading from `REMOTE_MODULES` after the selected
linked set is built.

## Migrations

`apps/migrate` should collect migrations from the selected profile:

- `core`: platform and runtime migrations only
- `demo`: platform, runtime, identity, and notifications migrations

The migration function should not hand-chain fixture migrations in the app once
profiles exist. It should call app-bootstrap so the module set and migration set
cannot drift.

## OpenAPI And SDK

OpenAPI assembly should use the selected profile when merging linked HTTP
routes. In the first slice, committed contracts can remain demo-profile
contracts so existing identity SDK helpers continue to compile.

Follow-up work can add a separate core-only contract target if a project wants
a product starter without demo endpoints.

## Runtime Console

The Runtime Console should not special-case this profile decision:

- In demo profile, `/admin/data/modules` exposes `identity`, `notifications`,
  and `platform-story`; the console shows System plus Identity workspace.
- In core profile, `/admin/data/modules` exposes platform-owned entries and any
  configured remotes; the console should still work with System only.
- Build-time fixture packages can remain installed so mock mode can demonstrate
  multiple workspaces.

## Documentation Updates

Update architecture docs to say:

- `identity` and `notifications` are demo fixtures, not product defaults.
- `app-bootstrap` owns composition profile selection.
- Local/demo commands use the demo profile to keep examples rich.
- Product projects can use core profile and register their own modules.

## Testing Strategy

Add focused coverage for:

- profile parsing and defaults
- `core` linked manifest/module enumeration excludes identity/notifications
- `demo` linked manifest/module enumeration includes identity/notifications
- linked HTTP route owners are profile-aware
- migration collection is profile-aware
- OpenAPI route assembly follows the profile used by the document builder
- Runtime Console module metadata still shows the Identity workspace in demo
  mode and does not require it in core mode

Run at least:

```sh
cargo test --locked -p app-bootstrap
cargo test --locked -p platform-core
just generated-check
just arch-check
just console-check
just sdk-check
```

Use broader `just check` if the implementation changes OpenAPI defaults,
migration behavior, or committed generated artifacts.

## Risks

- Existing tests assume identity routes are always present. They should either
  select the demo profile explicitly or move to profile-specific assertions.
- If OpenAPI generation and API startup resolve different profiles, generated
  SDK output can drift from served routes. Keep profile resolution centralized.
- If local default changes to `core` too early, the workspace switcher may look
  absent again because only System exists. Keep demo as local default until
  another non-system example module exists.

## Rollout

1. Add profile model and profile-aware linked module enumeration.
2. Route API, worker, migrations, OpenAPI, and tests through that profile.
3. Keep committed contracts in demo profile for the first slice.
4. Update docs and local command notes.
5. Later, consider a core-only starter contract once Lenso has a real product
   module template replacing the identity fixture.
