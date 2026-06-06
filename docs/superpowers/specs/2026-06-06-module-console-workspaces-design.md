# Module Console Workspaces

**Date:** 2026-06-06
**Status:** Approved design, ready for implementation planning
**Scope:** Let Runtime Console navigation switch between the host system workspace and module-declared workspaces without flattening every module page into the primary sidebar.

---

## Context

Runtime Console module pages are currently declared through `ModuleManifest.console`.
Each `ConsoleSurface` names one route, package export, area, label, icon, and
capability set. The host uses that metadata plus the build-time package registry
to mount routes and build navigation.

That contract is enough to install pages, but it flattens every module
contribution into one sidebar. As more modules contribute multiple pages, the
sidebar becomes a mixed list of host system pages, platform fixtures, and
business module pages.

The desired shape is not a fixed `Modules` bucket. Modules should be able to
create their own top-level console workspaces, such as `CRM`, `Billing`, or
`Support`, and then organize pages inside those workspaces. Host-owned pages
remain in the `System` workspace.

## Goals

- Add a Runtime Console switcher for navigation workspaces.
- Keep `System` as the host-owned workspace for operational and platform pages.
- Let modules create their own workspaces through serializable manifest data.
- Keep routes stable and independent from the selected workspace.
- Preserve the existing Runtime Console visual language: dense, scannable, and
  workbench-like.
- Keep command palette search global across all workspaces.
- Avoid frontend-only route guessing. Navigation ownership should come from
  module metadata.

## Non-Goals

- No hardcoded `Modules` workspace.
- No arbitrary module ownership of another module's workspace in the first
  slice.
- No dynamic package import or browser-side package installation.
- No redesign of Runtime Console pages outside the shell navigation.
- No change to `AdminSurface` or schema-admin rendering.
- No unbounded recursive menu tree. The first slice supports workspace plus one
  optional group level.

## Approaches Considered

| Approach | Shape | Decision |
|----------|-------|----------|
| Group menu only | Keep one sidebar and group items under headings. | Rejected. It classifies pages but still makes module growth compete with system pages. |
| Nested menu only | Keep one sidebar and allow parent/child menu items. | Rejected as the primary model. It helps multi-page modules but does not separate system and business workspaces. |
| Fixed `System / Modules` switch | Add two hardcoded workspaces. | Rejected. `Modules` is too generic and hides the fact that modules should be able to create named workspaces. |
| Module-declared workspace switch | Host owns `System`; modules declare their own workspaces and page groups. | Recommended. It keeps system pages stable and lets installed modules own their navigation identity. |

## Key Decisions

| Decision | Choice | Why |
|----------|--------|-----|
| Top-level model | Workspace switcher | Separates host/system workflows from module-owned workflows without stuffing modules into a generic bucket. |
| Built-in workspace | `system` with label `System` | Current pages are system/platform functions and should remain host-owned. |
| Module ownership | A module may create workspaces it owns | Prevents host-side naming guesses and lets a module expose a real product area. |
| Cross-module workspace sharing | Deferred | Shared workspaces need ownership and permission rules. The first slice should avoid ambiguous ownership. |
| Menu depth | Workspace plus one optional group | Supports multi-page modules while keeping the shell compact. |
| Route behavior | Routes stay stable | Navigation organization must not rewrite route semantics or break deep links. |
| Command palette | Global | Users should be able to reach any page regardless of the selected workspace. |
| Selection behavior | Workspace is derived from the active route, with remembered manual switch fallback | Deep links show the correct workspace; idle switching still feels persistent. |

## Manifest Shape

Extend `ConsoleSurface` with optional navigation metadata:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ConsoleSurface {
    pub name: String,
    pub label: String,
    pub area: ConsoleArea,
    pub route: String,
    pub package: ConsolePackage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub navigation: Option<ConsoleNavigation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ConsoleNavigation {
    pub workspace: ConsoleWorkspaceRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<ConsoleNavigationGroup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ConsoleWorkspaceRef {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ConsoleNavigationGroup {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
}
```

`navigation` is optional for backward compatibility. Missing navigation metadata
defaults to the host `system` workspace and uses the existing `area` ordering.

Example module-owned workspace:

```rust
ConsoleSurface {
    name: "contacts".to_owned(),
    label: "Contacts".to_owned(),
    area: ConsoleArea::Data,
    route: "/crm/contacts".to_owned(),
    package: ConsolePackage {
        name: "@lenso/crm-console".to_owned(),
        export: "crmConsoleModule".to_owned(),
    },
    icon: Some("users".to_owned()),
    required_capabilities: vec!["crm.contacts.read".to_owned()],
    navigation: Some(ConsoleNavigation {
        workspace: ConsoleWorkspaceRef {
            id: "crm".to_owned(),
            label: "CRM".to_owned(),
            icon: Some("briefcase".to_owned()),
        },
        group: Some(ConsoleNavigationGroup {
            id: "customers".to_owned(),
            label: "Customers".to_owned(),
            icon: None,
            order: Some(20),
        }),
        order: Some(10),
    }),
}
```

## Workspace Ownership Rules

The first slice should use simple, host-verifiable rules:

- `system` is reserved for the host.
- A module may attach surfaces to `system` only when the host marks the package
  as first-party or the surface is already explicitly trusted by the installed
  package registry.
- A module may create workspace IDs derived from its module name or declared
  package identity.
- A module may attach multiple surfaces to its own workspace.
- Two modules declaring the same non-system workspace ID should produce a
  manifest lint unless a later shared-workspace policy is introduced.

These rules keep module navigation self-contained while leaving room for future
marketplace or suite-style workspaces.

## Runtime Console Shell

The shell should render a compact workspace switcher above the workspace-local
menu:

```text
[ System | CRM | Billing ]

System
  Overview
  Runtime
  Operations
  Module Registry
  Data
  Configuration

CRM
  Dashboard
  Customers
    Contacts
    Accounts
  Settings
```

Behavior:

- The active route selects the workspace containing that route.
- Manual switch changes the visible menu but does not navigate until the user
  chooses a page.
- If a selected workspace has no currently available routes after capability
  filtering, the shell falls back to `System`.
- Collapsed sidebar shows workspace icons and active route icons; labels remain
  available through titles and accessible names.
- Mobile keeps the existing horizontal nav pattern, with workspace switcher as a
  compact segmented control above the menu row.

## Data Flow

1. Backend exposes `ConsoleSurface.navigation` through `/admin/data/modules`.
2. Runtime Console normalizes backend metadata with build-time package
   manifests as fallback.
3. Installed package resolution still gates which module routes can mount.
4. Capability filtering happens before navigation tree building.
5. Navigation builder groups available route contributions by workspace and
   optional group.
6. Router mounting remains route-based and flat; only the sidebar tree changes.

This keeps routing, package trust, and navigation organization as separate
concerns.

## Error Handling And Lints

Manifest linting should live in `platform-module`:

- Empty workspace ID or label.
- Invalid workspace or group ID format.
- Duplicate non-system workspace IDs across modules.
- Surface group declared without navigation workspace.
- Unknown icon IDs remain warnings, not hard failures, because frontend icon
  support can evolve.
- Ambiguous ordering is allowed; stable fallback sort uses label, then route.

Runtime Console should render backend-provided manifest lints on the Module
Registry page and should not duplicate lint rules locally.

## Testing

Implementation planning should include:

- `platform-module` serde tests for `ConsoleNavigation`.
- `platform-module` manifest lint tests for workspace ID, group ID, and
  duplicate workspace ownership.
- `platform-admin-data` DTO/API tests proving navigation metadata appears in
  module metadata.
- Runtime Console model tests for building workspace navigation from metadata.
- Runtime Console shell tests for active-route workspace selection and fallback
  when a workspace has no available routes.
- Console package API tests for mapping package manifests to Rust console
  surface metadata with navigation included.

For the first implementation slice, `just generated-check` and
`just console-check` are the meaningful gates. If OpenAPI or generated SDK
artifacts change, include regenerated outputs with the source changes.

## Deferred Slices

- Shared workspaces owned by a suite or marketplace publisher.
- Workspace-level permissions beyond page-level capability filtering.
- User-customized workspace order or pinned pages.
- Dynamic package loading.
- Rich workspace landing pages.
- More than one nested menu level.
