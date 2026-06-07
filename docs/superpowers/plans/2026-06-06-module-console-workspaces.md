# Module Console Workspaces Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Runtime Console workspace switching where `System` is host-owned and modules can declare their own top-level workspaces through `ConsoleSurface.navigation`.

**Architecture:** Extend the serializable `platform-module` console contract first, expose it through existing module metadata and generated OpenAPI/SDK artifacts, then update the Runtime Console package API and shell to build a workspace navigation tree from manifest data. Routing remains flat and route-based; only sidebar organization changes.

**Tech Stack:** Rust, serde, utoipa, Axum/OpenAPI generation, TypeScript, React 19, TanStack Router, Vitest, Tailwind CSS, Lucide icons, `just`.

---

## File Structure

- `crates/platform-module/src/console.rs`: owns `ConsoleSurface` and the new `ConsoleNavigation`, `ConsoleWorkspaceRef`, and `ConsoleNavigationGroup` data types.
- `crates/platform-module/src/lib.rs`: re-exports the new console navigation types for module manifests.
- `crates/platform-module/src/manifest.rs`: validates workspace/group fields and capability references without moving logic into the frontend.
- `crates/platform-admin-data/src/dto.rs`: continues to expose `Vec<ConsoleSurface>`; utoipa should include nested navigation schemas once the Rust type changes.
- `contracts/openapi/app-api.v1.yaml`: regenerated OpenAPI artifact.
- `packages/ts-sdk/src/generated/*`: regenerated SDK artifacts.
- `apps/runtime-console/packages/console-package-api/src/index.ts`: extends console package manifests with optional `navigation` and maps them to Rust-style surface metadata.
- `apps/runtime-console/src/app/console-module-api.ts`: extends route/navigation contribution types with workspace metadata.
- `apps/runtime-console/src/app/console-module-resolver.ts`: normalizes backend `ConsoleSurface.navigation` metadata and preserves package/capability gating.
- `apps/runtime-console/src/app/console-modules.tsx`: builds flat routes and workspace navigation separately.
- `apps/runtime-console/src/app/console-workspace-navigation.ts`: new focused model file for workspace tree building, sorting, fallback, and active workspace selection.
- `apps/runtime-console/src/components/runtime/runtime-console-shell.tsx`: renders the workspace switcher and workspace-local menu.
- `apps/runtime-console/src/components/runtime/command-palette.tsx`: keeps global navigation commands and uses workspace-aware subtitles/search text.
- `apps/runtime-console/packages/story-console/console-surface.json`: marks Stories as `system`.
- `apps/runtime-console/packages/identity-console/console-surface.json`: fixture creates the `identity` workspace so local mock and API modes both exercise the workspace switcher.
- `apps/runtime-console/packages/console-package-cli/src/index.mjs`: generated console packages include `navigation` in JSON, TS manifest casts, and Rust snippets.
- `docs/architecture/module-console-surfaces.md`: documents workspace navigation as part of the console surface contract.
- `docs/superpowers/specs/2026-06-06-module-console-workspaces-design.md`: remains the approved design reference.

## Task 1: Add Rust Console Navigation Contract

**Files:**
- Modify: `crates/platform-module/src/console.rs`
- Modify: `crates/platform-module/src/lib.rs`
- Modify: `crates/platform-module/src/manifest.rs`

- [ ] **Step 1: Write failing serde and lint tests**

Add tests near the existing console manifest tests in `crates/platform-module/src/manifest.rs`.

```rust
#[test]
fn console_surface_navigation_round_trips() {
    let surface = ConsoleSurface {
        name: "contacts".to_owned(),
        label: "Contacts".to_owned(),
        area: ConsoleArea::Data,
        route: "/crm/contacts".to_owned(),
        package: crate::ConsolePackage {
            name: "@lenso/crm-console".to_owned(),
            export: "crmConsoleModule".to_owned(),
        },
        icon: Some("users".to_owned()),
        required_capabilities: vec!["crm.contacts.read".to_owned()],
        navigation: Some(crate::ConsoleNavigation {
            workspace: crate::ConsoleWorkspaceRef {
                id: "crm".to_owned(),
                label: "CRM".to_owned(),
                icon: Some("briefcase".to_owned()),
            },
            group: Some(crate::ConsoleNavigationGroup {
                id: "customers".to_owned(),
                label: "Customers".to_owned(),
                icon: None,
                order: Some(20),
            }),
            order: Some(10),
        }),
    };

    let json = serde_json::to_string(&surface).expect("serialize");
    let back: ConsoleSurface = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(back, surface);
}

#[test]
fn console_navigation_lints_empty_workspace_label() {
    let manifest = ModuleManifest::builder("crm")
        .capabilities(vec!["crm.contacts.read".to_owned()])
        .console(vec![ConsoleSurface {
            name: "contacts".to_owned(),
            label: "Contacts".to_owned(),
            area: ConsoleArea::Data,
            route: "/crm/contacts".to_owned(),
            package: crate::ConsolePackage {
                name: "@lenso/crm-console".to_owned(),
                export: "crmConsoleModule".to_owned(),
            },
            icon: None,
            required_capabilities: vec!["crm.contacts.read".to_owned()],
            navigation: Some(crate::ConsoleNavigation {
                workspace: crate::ConsoleWorkspaceRef {
                    id: "crm".to_owned(),
                    label: "".to_owned(),
                    icon: None,
                },
                group: None,
                order: None,
            }),
        }])
        .build();

    let subjects: Vec<_> = lint_module_manifest(ModuleSource::Linked, &manifest)
        .into_iter()
        .map(|lint| lint.subject)
        .collect();

    assert!(subjects.contains(&"console.surface.contacts.navigation.workspace.label".to_owned()));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test --locked -p platform-module console_surface_navigation_round_trips console_navigation_lints_empty_workspace_label
```

Expected: FAIL because `ConsoleSurface.navigation`, `ConsoleNavigation`, `ConsoleWorkspaceRef`, and `ConsoleNavigationGroup` do not exist.

- [ ] **Step 3: Add the Rust data types**

Update `crates/platform-module/src/console.rs`:

```rust
//! Runtime Console contribution contracts.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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

Keep the existing `ConsoleArea` and `ConsolePackage` definitions below these types.

- [ ] **Step 4: Re-export the new types**

Update `crates/platform-module/src/lib.rs`:

```rust
pub use console::{
    ConsoleArea, ConsoleNavigation, ConsoleNavigationGroup, ConsolePackage, ConsoleSurface,
    ConsoleWorkspaceRef,
};
```

- [ ] **Step 5: Add navigation linting**

In `crates/platform-module/src/manifest.rs`, update `lint_console_surfaces` after the existing package export checks:

```rust
        if let Some(navigation) = &surface.navigation {
            lint_console_navigation(&subject, navigation, lints);
        }
```

Add helper functions near the existing console validation helpers:

```rust
fn lint_console_navigation(
    subject: &str,
    navigation: &crate::ConsoleNavigation,
    lints: &mut Vec<ModuleManifestLint>,
) {
    let workspace_subject = format!("{subject}.navigation.workspace");
    if !valid_console_navigation_id(&navigation.workspace.id) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: format!("{workspace_subject}.id"),
            message: "Console workspace id should be a path-safe identifier.".to_owned(),
            suggestion: "Use ASCII letters, digits, underscore, or hyphen.".to_owned(),
        });
    }
    if !present(&navigation.workspace.label) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: format!("{workspace_subject}.label"),
            message: "Console workspace is missing an operator-facing label.".to_owned(),
            suggestion: "Set a short workspace label such as CRM.".to_owned(),
        });
    }
    if let Some(group) = &navigation.group {
        let group_subject = format!("{subject}.navigation.group");
        if !valid_console_navigation_id(&group.id) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Warning,
                subject: format!("{group_subject}.id"),
                message: "Console navigation group id should be a path-safe identifier.".to_owned(),
                suggestion: "Use ASCII letters, digits, underscore, or hyphen.".to_owned(),
            });
        }
        if !present(&group.label) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Warning,
                subject: format!("{group_subject}.label"),
                message: "Console navigation group is missing an operator-facing label.".to_owned(),
                suggestion: "Set a short group label such as Customers.".to_owned(),
            });
        }
    }
}

fn valid_console_navigation_id(value: &str) -> bool {
    valid_console_surface_name(value)
}
```

- [ ] **Step 6: Update existing `ConsoleSurface` literals**

Every Rust `ConsoleSurface` literal now needs `navigation: None` unless the task intentionally assigns navigation. Update literals in:

```bash
rg -n "ConsoleSurface \\{" crates modules apps -S
```

For existing surfaces in this task, add:

```rust
navigation: None,
```

- [ ] **Step 7: Run tests to verify Task 1 passes**

Run:

```bash
cargo test --locked -p platform-module console_surface_navigation_round_trips console_navigation_lints_empty_workspace_label
cargo check --locked -p platform-module --all-targets
```

Expected: PASS.

- [ ] **Step 8: Commit Task 1**

```bash
git add crates/platform-module/src/console.rs \
  crates/platform-module/src/lib.rs \
  crates/platform-module/src/manifest.rs \
  crates/app-bootstrap/src/lib.rs \
  crates/platform-admin-data/src/handlers.rs \
  modules/identity/src/module.rs
git diff --cached --name-only
git commit -m "feat(module): add console navigation metadata"
```

Before committing, remove any path from the index that was not touched by this task.

## Task 2: Regenerate Contracts And SDK

**Files:**
- Modify: `contracts/openapi/app-api.v1.yaml`
- Modify: `packages/ts-sdk/src/generated/*`
- Test: generated freshness commands

- [ ] **Step 1: Run generation**

Run:

```bash
just generate
```

Expected: OpenAPI and generated SDK files update to include `ConsoleNavigation`, `ConsoleNavigationGroup`, and `ConsoleWorkspaceRef` schemas.

- [ ] **Step 2: Inspect generated diffs**

Run:

```bash
git diff -- contracts/openapi/app-api.v1.yaml packages/ts-sdk/src/generated
```

Expected: only schema/type additions or `ConsoleSurface` shape updates related to `navigation`.

- [ ] **Step 3: Validate generated freshness and SDK**

Run:

```bash
just generated-check
just sdk-check
```

Expected: PASS.

- [ ] **Step 4: Commit Task 2**

```bash
git add contracts/openapi/app-api.v1.yaml packages/ts-sdk/src/generated
git diff --cached --name-only
git commit -m "chore: regenerate console navigation contracts"
```

## Task 3: Extend Console Package API And Generator

**Files:**
- Modify: `apps/runtime-console/packages/console-package-api/src/index.ts`
- Modify: `apps/runtime-console/packages/console-package-api/src/index.test.ts`
- Modify: `apps/runtime-console/packages/story-console/console-surface.json`
- Modify: `apps/runtime-console/packages/story-console/src/manifest.ts`
- Modify: `apps/runtime-console/packages/identity-console/console-surface.json`
- Modify: `apps/runtime-console/packages/identity-console/src/manifest.ts`
- Modify: `apps/runtime-console/packages/console-package-cli/src/index.mjs`

- [ ] **Step 1: Write failing package API tests**

Add this test to `apps/runtime-console/packages/console-package-api/src/index.test.ts`:

```ts
test("maps package manifest navigation to Rust console surface metadata", () => {
  const manifest = defineConsolePackageManifest({
    area: "data",
    exportName: "crmConsoleModule",
    icon: "database",
    id: "crm",
    label: "Contacts",
    navigation: {
      group: {
        id: "customers",
        label: "Customers",
        order: 20,
      },
      order: 10,
      workspace: {
        icon: "briefcase",
        id: "crm",
        label: "CRM",
      },
    },
    packageName: "@lenso/crm-console",
    requiredCapabilities: ["crm.contacts.read"],
    route: "/crm/contacts",
    source: "installed",
    surfaceName: "contacts",
    version: "workspace",
  } as const);

  expect(consoleSurfaceFromPackageManifest(manifest)).toEqual({
    area: "data",
    icon: "database",
    label: "Contacts",
    name: "contacts",
    navigation: {
      group: {
        id: "customers",
        label: "Customers",
        order: 20,
      },
      order: 10,
      workspace: {
        icon: "briefcase",
        id: "crm",
        label: "CRM",
      },
    },
    package: {
      export: "crmConsoleModule",
      name: "@lenso/crm-console",
    },
    required_capabilities: ["crm.contacts.read"],
    route: "/crm/contacts",
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
pnpm --dir apps/runtime-console exec vitest run packages/console-package-api/src/index.test.ts
```

Expected: FAIL because `navigation` is not part of the package manifest types or mapper.

- [ ] **Step 3: Add TypeScript package navigation types**

Update `apps/runtime-console/packages/console-package-api/src/index.ts`:

```ts
export interface ConsolePackageNavigation {
  workspace: ConsolePackageWorkspaceRef;
  group?: ConsolePackageNavigationGroup;
  order?: number;
}

export interface ConsolePackageWorkspaceRef {
  id: string;
  label: string;
  icon?: ConsoleSurfaceIcon;
}

export interface ConsolePackageNavigationGroup {
  id: string;
  label: string;
  icon?: ConsoleSurfaceIcon;
  order?: number;
}
```

Add to `ConsolePackageManifest`:

```ts
  navigation?: ConsolePackageNavigation;
```

Add to `ConsoleSurfaceManifest`:

```ts
  navigation?: ConsolePackageNavigation;
```

Update `consoleSurfaceFromPackageManifest`:

```ts
  if (manifest.navigation) {
    surface.navigation = manifest.navigation;
  }
```

- [ ] **Step 4: Add build-time fallback navigation to fixture package manifests**

Update `apps/runtime-console/packages/story-console/console-surface.json`:

```json
{
  "area": "runtime",
  "exportName": "storyConsoleModule",
  "icon": "workflow",
  "id": "platform-story",
  "label": "Stories",
  "navigation": {
    "order": 20,
    "workspace": {
      "icon": "settings",
      "id": "system",
      "label": "System"
    }
  },
  "packageName": "@lenso/story-console",
  "requiredCapabilities": ["runtime.stories.read"],
  "route": "/runtime/stories",
  "source": "first_party",
  "surfaceName": "stories",
  "version": "workspace"
}
```

Update `apps/runtime-console/packages/identity-console/console-surface.json`:

```json
{
  "area": "data",
  "exportName": "identityConsoleModule",
  "icon": "database",
  "id": "identity",
  "label": "Identity",
  "navigation": {
    "order": 60,
    "workspace": {
      "icon": "database",
      "id": "identity",
      "label": "Identity"
    }
  },
  "packageName": "@lenso/identity-console",
  "requiredCapabilities": ["identity.users.read"],
  "route": "/data/identity",
  "source": "installed",
  "surfaceName": "identity",
  "version": "workspace"
}
```

Update each package `src/manifest.ts` cast to include a typed `navigation` field with the same object shape.

- [ ] **Step 5: Update CLI skeleton generation**

In `apps/runtime-console/packages/console-package-cli/src/index.mjs`, add `navigation` to the generated `consoleSurfaceContract` before writing `console-surface.json`:

```js
navigation: {
  order: 10,
  workspace: {
    icon,
    id: moduleId,
    label,
  },
},
```

Update the generated Rust snippet to include:

```rust
    navigation: Some(platform_module::ConsoleNavigation {
        workspace: platform_module::ConsoleWorkspaceRef {
            id: "${moduleId}".to_owned(),
            label: "${label}".to_owned(),
            icon: Some("${icon}".to_owned()),
        },
        group: None,
        order: Some(10),
    }),
```

Update the generated TS manifest cast to include:

```ts
  readonly navigation: {
    readonly order: 10;
    readonly workspace: {
      readonly icon: "${icon}";
      readonly id: "${moduleId}";
      readonly label: "${label}";
    };
  };
```

Update the generated `defineConsoleModule` surface to pass:

```ts
      navigation: ${manifestName}.navigation,
```

- [ ] **Step 6: Run package API tests**

Run:

```bash
pnpm --dir apps/runtime-console exec vitest run packages/console-package-api/src/index.test.ts
pnpm --dir apps/runtime-console check:console-packages
```

Expected: PASS.

- [ ] **Step 7: Commit Task 3**

```bash
git add apps/runtime-console/packages/console-package-api/src/index.ts \
  apps/runtime-console/packages/console-package-api/src/index.test.ts \
  apps/runtime-console/packages/story-console/console-surface.json \
  apps/runtime-console/packages/story-console/src/manifest.ts \
  apps/runtime-console/packages/identity-console/console-surface.json \
  apps/runtime-console/packages/identity-console/src/manifest.ts \
  apps/runtime-console/packages/console-package-cli/src/index.mjs
git diff --cached --name-only
git commit -m "feat(console): carry workspace metadata in packages"
```

## Task 4: Build Workspace Navigation Model

**Files:**
- Create: `apps/runtime-console/src/app/console-workspace-navigation.ts`
- Create: `apps/runtime-console/src/app/console-workspace-navigation.test.ts`
- Modify: `apps/runtime-console/src/app/console-module-api.ts`
- Modify: `apps/runtime-console/src/app/console-module-resolver.ts`
- Modify: `apps/runtime-console/src/app/console-module-metadata.ts`
- Modify: `apps/runtime-console/src/app/console-module-metadata.test.ts`
- Modify: `apps/runtime-console/src/app/console-modules.tsx`
- Modify: `apps/runtime-console/src/app/console-modules.test.tsx`

- [ ] **Step 1: Write failing workspace model tests**

Create `apps/runtime-console/src/app/console-workspace-navigation.test.ts`:

```ts
import { describe, expect, test } from "vitest";

import {
  activeWorkspaceIdForPath,
  buildWorkspaceNavigation,
  SYSTEM_WORKSPACE,
} from "./console-workspace-navigation";
import type { ConsoleNavigationItem } from "./console-module-api";

const items: ConsoleNavigationItem[] = [
  {
    icon: "activity",
    label: "Overview",
    moduleId: "host",
    path: "/overview",
    navigation: {
      order: 0,
      workspace: SYSTEM_WORKSPACE,
    },
  },
  {
    icon: "database",
    label: "Contacts",
    moduleId: "crm",
    path: "/crm/contacts",
    navigation: {
      group: {
        id: "customers",
        label: "Customers",
        order: 20,
      },
      order: 10,
      workspace: {
        icon: "database",
        id: "crm",
        label: "CRM",
      },
    },
  },
  {
    icon: "database",
    label: "Deals",
    moduleId: "crm",
    path: "/crm/deals",
    navigation: {
      group: {
        id: "pipeline",
        label: "Pipeline",
        order: 10,
      },
      order: 10,
      workspace: {
        icon: "database",
        id: "crm",
        label: "CRM",
      },
    },
  },
];

describe("console workspace navigation", () => {
  test("builds system and module-declared workspaces", () => {
    expect(buildWorkspaceNavigation(items)).toMatchObject([
      {
        id: "system",
        label: "System",
        items: [
          {
            label: "Overview",
            path: "/overview",
          },
        ],
      },
      {
        id: "crm",
        label: "CRM",
        groups: [
          {
            id: "pipeline",
            label: "Pipeline",
            items: [
              {
                label: "Deals",
                path: "/crm/deals",
              },
            ],
          },
          {
            id: "customers",
            label: "Customers",
            items: [
              {
                label: "Contacts",
                path: "/crm/contacts",
              },
            ],
          },
        ],
      },
    ]);
  });

  test("selects active workspace from route path", () => {
    const workspaces = buildWorkspaceNavigation(items);

    expect(activeWorkspaceIdForPath(workspaces, "/crm/contacts")).toBe("crm");
    expect(activeWorkspaceIdForPath(workspaces, "/unknown")).toBe("system");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
pnpm --dir apps/runtime-console exec vitest run src/app/console-workspace-navigation.test.ts
```

Expected: FAIL because the model file does not exist.

- [ ] **Step 3: Extend console module API types**

Update `apps/runtime-console/src/app/console-module-api.ts`:

```ts
export type ConsoleNavigationMetadata = {
  workspace: ConsoleWorkspaceRef;
  group?: ConsoleNavigationGroup;
  order?: number;
};

export type ConsoleWorkspaceRef = {
  id: string;
  label: string;
  icon?: ConsoleSurfaceIcon;
};

export type ConsoleNavigationGroup = {
  id: string;
  label: string;
  icon?: ConsoleSurfaceIcon;
  order?: number;
};
```

Add `navigation?: ConsoleNavigationMetadata;` to `ConsoleModuleSurface`,
`ConsoleRouteContribution`, and `ConsoleNavigationItem`.

- [ ] **Step 4: Implement workspace model**

Create `apps/runtime-console/src/app/console-workspace-navigation.ts`:

```ts
import type {
  ConsoleNavigationGroup,
  ConsoleNavigationItem,
  ConsoleSurfaceIcon,
  ConsoleWorkspaceRef,
} from "./console-module-api";

export const SYSTEM_WORKSPACE = {
  icon: "settings",
  id: "system",
  label: "System",
} as const satisfies ConsoleWorkspaceRef;

export type ConsoleWorkspaceNavigation = {
  id: string;
  label: string;
  icon?: ConsoleSurfaceIcon;
  items: ConsoleNavigationItem[];
  groups: ConsoleWorkspaceNavigationGroup[];
};

export type ConsoleWorkspaceNavigationGroup = ConsoleNavigationGroup & {
  items: ConsoleNavigationItem[];
};

export function navigationForItem(
  item: ConsoleNavigationItem
): Required<Pick<ConsoleNavigationItem, "navigation">>["navigation"] {
  return (
    item.navigation ?? {
      workspace: SYSTEM_WORKSPACE,
    }
  );
}

export function buildWorkspaceNavigation(
  items: ConsoleNavigationItem[]
): ConsoleWorkspaceNavigation[] {
  const workspaces = new Map<string, ConsoleWorkspaceNavigation>();

  for (const item of items) {
    const navigation = navigationForItem(item);
    const workspace = ensureWorkspace(workspaces, navigation.workspace);

    if (!navigation.group) {
      workspace.items.push(item);
      continue;
    }

    let group = workspace.groups.find(
      (candidate) => candidate.id === navigation.group?.id
    );
    if (!group) {
      group = {
        ...navigation.group,
        items: [],
      };
      workspace.groups.push(group);
    }
    group.items.push(item);
  }

  return [...workspaces.values()].map(sortWorkspace).sort(compareWorkspaces);
}

export function activeWorkspaceIdForPath(
  workspaces: ConsoleWorkspaceNavigation[],
  path: string
): string {
  for (const workspace of workspaces) {
    if (workspace.items.some((item) => routeMatchesPath(item.path, path))) {
      return workspace.id;
    }
    for (const group of workspace.groups) {
      if (group.items.some((item) => routeMatchesPath(item.path, path))) {
        return workspace.id;
      }
    }
  }
  return SYSTEM_WORKSPACE.id;
}

function ensureWorkspace(
  workspaces: Map<string, ConsoleWorkspaceNavigation>,
  workspaceRef: ConsoleWorkspaceRef
): ConsoleWorkspaceNavigation {
  const existing = workspaces.get(workspaceRef.id);
  if (existing) {
    return existing;
  }
  const workspace: ConsoleWorkspaceNavigation = {
    groups: [],
    id: workspaceRef.id,
    items: [],
    label: workspaceRef.label,
  };
  if (workspaceRef.icon) {
    workspace.icon = workspaceRef.icon;
  }
  workspaces.set(workspace.id, workspace);
  return workspace;
}

function sortWorkspace(
  workspace: ConsoleWorkspaceNavigation
): ConsoleWorkspaceNavigation {
  return {
    ...workspace,
    groups: workspace.groups
      .map((group) => ({
        ...group,
        items: [...group.items].sort(compareItems),
      }))
      .sort(compareGroups),
    items: [...workspace.items].sort(compareItems),
  };
}

function compareWorkspaces(
  left: ConsoleWorkspaceNavigation,
  right: ConsoleWorkspaceNavigation
): number {
  if (left.id === SYSTEM_WORKSPACE.id) {
    return -1;
  }
  if (right.id === SYSTEM_WORKSPACE.id) {
    return 1;
  }
  return left.label.localeCompare(right.label) || left.id.localeCompare(right.id);
}

function compareGroups(
  left: ConsoleWorkspaceNavigationGroup,
  right: ConsoleWorkspaceNavigationGroup
): number {
  return (
    (left.order ?? Number.MAX_SAFE_INTEGER) -
      (right.order ?? Number.MAX_SAFE_INTEGER) ||
    left.label.localeCompare(right.label) ||
    left.id.localeCompare(right.id)
  );
}

function compareItems(
  left: ConsoleNavigationItem,
  right: ConsoleNavigationItem
): number {
  return (
    (left.navigation?.order ?? Number.MAX_SAFE_INTEGER) -
      (right.navigation?.order ?? Number.MAX_SAFE_INTEGER) ||
    left.label.localeCompare(right.label) ||
    left.path.localeCompare(right.path)
  );
}

function routeMatchesPath(route: string, path: string): boolean {
  return path === route || path.startsWith(`${route}/`);
}
```

- [ ] **Step 5: Preserve navigation through module building**

In `apps/runtime-console/src/app/console-modules.tsx`, update `buildConsoleNavigation`:

```ts
    if (route.navigation) {
      item.navigation = route.navigation;
    }
```

In `apps/runtime-console/src/app/console-module-resolver.ts`, add `navigation?: ConsoleNavigationMetadata;` to backend metadata and pass it through only after package resolution. Imported type should include `ConsoleNavigationMetadata`.

- [ ] **Step 6: Add metadata tests**

Update `apps/runtime-console/src/app/console-module-metadata.test.ts` so the first navigation assertion includes `navigation`:

```ts
navigation: {
  order: 20,
  workspace: {
    icon: "settings",
    id: "system",
    label: "System",
  },
},
```

Add a new test proving backend navigation is used:

```ts
test("keeps backend workspace metadata in navigation", () => {
  expect(
    navigationFromConsoleModuleMetadata(
      [
        {
          console: [
            {
              navigation: {
                order: 10,
                workspace: {
                  icon: "database",
                  id: "crm",
                  label: "CRM",
                },
              },
              package: {
                export: "identityConsoleModule",
                name: "@lenso/identity-console",
              },
              required_capabilities: ["identity.users.read"],
            },
          ],
        },
      ],
      ["identity.users.read"]
    )[0]?.navigation
  ).toEqual({
    order: 10,
    workspace: {
      icon: "database",
      id: "crm",
      label: "CRM",
    },
  });
});
```

- [ ] **Step 7: Run app model tests**

Run:

```bash
pnpm --dir apps/runtime-console exec vitest run src/app/console-workspace-navigation.test.ts src/app/console-module-metadata.test.ts src/app/console-modules.test.tsx
```

Expected: PASS.

- [ ] **Step 8: Commit Task 4**

```bash
git add apps/runtime-console/src/app
git diff --cached --name-only
git commit -m "feat(console): build workspace navigation model"
```

## Task 5: Render Workspace Switcher In The Shell

**Files:**
- Modify: `apps/runtime-console/src/components/runtime/runtime-console-shell.tsx`
- Modify: `apps/runtime-console/src/components/runtime/command-palette.tsx`
- Test: `apps/runtime-console/src/app/console-workspace-navigation.test.ts`

- [ ] **Step 1: Add active workspace behavior test**

Extend `apps/runtime-console/src/app/console-workspace-navigation.test.ts`:

```ts
test("falls back to system when selected workspace is unavailable", () => {
  const workspaces = buildWorkspaceNavigation(items);
  const availableIds = new Set(workspaces.map((workspace) => workspace.id));

  expect(availableIds.has("unknown")).toBe(false);
  expect(activeWorkspaceIdForPath(workspaces, "/unknown")).toBe("system");
});
```

- [ ] **Step 2: Run test**

Run:

```bash
pnpm --dir apps/runtime-console exec vitest run src/app/console-workspace-navigation.test.ts
```

Expected: PASS if Task 4 implemented fallback correctly.

- [ ] **Step 3: Update shell imports**

In `apps/runtime-console/src/components/runtime/runtime-console-shell.tsx`, add imports:

```ts
import { useRouterState } from "@tanstack/react-router";
import {
  activeWorkspaceIdForPath,
  buildWorkspaceNavigation,
  type ConsoleWorkspaceNavigation,
  SYSTEM_WORKSPACE,
} from "../../app/console-workspace-navigation";
```

Keep the existing Lucide icon registry and add icons used by workspace metadata:

```ts
  briefcase: BriefcaseBusiness,
  users: Users,
```

Import `BriefcaseBusiness` and `Users` from `lucide-react`.

- [ ] **Step 4: Build workspace state**

Inside `RuntimeConsoleShell`, after `consoleNavigation`:

```ts
  const routerState = useRouterState();
  const workspaceNavigation = useMemo(
    () => buildWorkspaceNavigation(primaryNavItems),
    [primaryNavItems]
  );
  const routeWorkspaceId = activeWorkspaceIdForPath(
    workspaceNavigation,
    routerState.location.pathname
  );
  const [selectedWorkspaceId, setSelectedWorkspaceId] = usePersistedLayout(
    "runtime-console:selected-workspace",
    SYSTEM_WORKSPACE.id
  );
  const activeWorkspaceId = workspaceNavigation.some(
    (workspace) => workspace.id === routeWorkspaceId
  )
    ? routeWorkspaceId
    : selectedWorkspaceId;
  const activeWorkspace =
    workspaceNavigation.find((workspace) => workspace.id === activeWorkspaceId) ??
    workspaceNavigation.find((workspace) => workspace.id === SYSTEM_WORKSPACE.id) ??
    workspaceNavigation[0];
```

If TypeScript reports that `primaryNavItems` lacks `navigation`, move `primaryNavItems` typing to `ConsoleNavigationItem[]` and include `navigation` values for built-in host pages.

- [ ] **Step 5: Give built-in host pages system navigation**

When constructing `primaryNavItems`, set system navigation on host-owned items:

```ts
const systemNavigation = {
  workspace: SYSTEM_WORKSPACE,
} as const;
```

For Overview, Operations, Module Registry, Data, and Configuration, include `navigation: systemNavigation`.

- [ ] **Step 6: Replace flat nav render with workspace switcher and tree**

Add local components at the bottom of `runtime-console-shell.tsx`:

```tsx
function WorkspaceSwitcher({
  activeWorkspaceId,
  onSelect,
  workspaces,
}: {
  activeWorkspaceId: string;
  onSelect: (workspaceId: string) => void;
  workspaces: ConsoleWorkspaceNavigation[];
}) {
  return (
    <div className="grid gap-px border-b border-(--border-subtle) p-2 max-lg:flex max-lg:min-w-max max-lg:border-b-0">
      {workspaces.map((workspace) => {
        const Icon = iconRegistry[workspace.icon ?? "settings"];
        const active = workspace.id === activeWorkspaceId;
        return (
          <button
            aria-pressed={active}
            className={`sidebar-nav-item flex h-7 w-full items-center gap-2 px-2 font-mono text-xs transition-colors max-lg:min-w-8 max-lg:justify-center max-lg:px-2 ${
              active
                ? "bg-(--accent-soft) text-(--foreground)"
                : "text-(--secondary) hover:bg-(--hover) hover:text-(--foreground)"
            }`}
            key={workspace.id}
            onClick={() => onSelect(workspace.id)}
            title={workspace.label}
            type="button"
          >
            <Icon size={13} strokeWidth={1.5} />
            <span className="sidebar-copy min-w-0 overflow-hidden whitespace-nowrap max-lg:hidden">
              {workspace.label}
            </span>
          </button>
        );
      })}
    </div>
  );
}

function WorkspaceMenu({
  workspace,
}: {
  workspace: ConsoleWorkspaceNavigation | undefined;
}) {
  if (!workspace) {
    return null;
  }
  return (
    <div className="grid gap-px max-lg:flex max-lg:min-w-max">
      {workspace.items.map((item) => (
        <NavLink key={item.path} {...item} />
      ))}
      {workspace.groups.map((group) => (
        <div className="grid gap-px max-lg:flex" key={group.id}>
          <div className="sidebar-group-label px-2 pt-2 font-mono text-[10px] uppercase tracking-[0.06em] text-(--muted) max-lg:hidden">
            {group.label}
          </div>
          {group.items.map((item) => (
            <NavLink key={item.path} {...item} />
          ))}
        </div>
      ))}
    </div>
  );
}
```

Update the `<nav>` body to:

```tsx
<nav className="max-lg:overflow-x-auto">
  <WorkspaceSwitcher
    activeWorkspaceId={activeWorkspace?.id ?? SYSTEM_WORKSPACE.id}
    onSelect={setSelectedWorkspaceId}
    workspaces={workspaceNavigation}
  />
  <div className="p-2">
    <WorkspaceMenu workspace={activeWorkspace} />
  </div>
</nav>
```

- [ ] **Step 7: Keep command palette global**

In `apps/runtime-console/src/components/runtime/command-palette.tsx`, update console item subtitle/search text:

```ts
const workspaceLabel = item.navigation?.workspace.label ?? "System";
const consoleItems: CommandItem[] = consoleNavigation.map((item) => ({
  action: () => void navigate({ to: item.path }),
  id: `console:${item.moduleId}:${item.path}`,
  searchText:
    `go to ${item.label} ${workspaceLabel} ${item.moduleId} ${item.path}`.toLowerCase(),
  subtitle: `${workspaceLabel} / ${item.moduleId}`,
  title: `Go to ${item.label}`,
}));
```

- [ ] **Step 8: Run shell checks**

Run:

```bash
pnpm --dir apps/runtime-console exec vitest run src/app/console-workspace-navigation.test.ts src/app/console-module-metadata.test.ts
pnpm --dir apps/runtime-console run typecheck
```

Expected: PASS.

- [ ] **Step 9: Commit Task 5**

```bash
git add apps/runtime-console/src/components/runtime/runtime-console-shell.tsx apps/runtime-console/src/components/runtime/command-palette.tsx apps/runtime-console/src/app/console-workspace-navigation.test.ts
git diff --cached --name-only
git commit -m "feat(console): add workspace switcher"
```

## Task 6: Update Rust Fixtures, Docs, And Quality Gates

**Files:**
- Modify: `modules/identity/src/module.rs`
- Modify: `docs/architecture/module-console-surfaces.md`
- Modify: `apps/runtime-console/docs/console-package-template.md`
- Review generated output: `contracts/openapi/app-api.v1.yaml`
- Review generated output: `packages/ts-sdk/src/generated/*`

- [ ] **Step 1: Update identity Rust manifest**

In `modules/identity/src/module.rs`, declare an `identity` workspace so the
fixture exercises module-created workspace switching in API mode.

- [ ] **Step 2: Add or update identity manifest test**

In the existing `#[cfg(test)]` module in `modules/identity/src/module.rs`, add:

```rust
#[test]
fn manifest_declares_identity_console_workspace_navigation() {
    let manifest = manifest();
    let surface = manifest.console.first().expect("identity console surface");
    let navigation = surface.navigation.as_ref().expect("navigation metadata");

    assert_eq!(navigation.workspace.id, "identity");
    assert_eq!(navigation.workspace.label, "Identity");
    assert_eq!(navigation.workspace.icon.as_deref(), Some("database"));
    assert_eq!(navigation.order, Some(60));
}
```

- [ ] **Step 3: Run Rust fixture tests**

Run:

```bash
cargo test --locked -p identity manifest_declares_identity_console_workspace_navigation
cargo check --locked --workspace --all-targets
```

Expected: PASS.

- [ ] **Step 4: Update architecture docs**

In `docs/architecture/module-console-surfaces.md`, update the Manifest Contract section so it includes:

```markdown
- `navigation`: optional workspace metadata. Missing metadata defaults to the
  host `System` workspace. Modules may create their own workspace by declaring a
  workspace id, label, and optional icon; the first slice supports one optional
  group level inside a workspace. The `system` workspace id is reserved for the
  host; module surfaces should omit `navigation` when they belong in System.
```

Add a short example:

```json
{
  "name": "contacts",
  "label": "Contacts",
  "area": "data",
  "route": "/crm/contacts",
  "navigation": {
    "workspace": { "id": "crm", "label": "CRM", "icon": "briefcase" },
    "group": { "id": "customers", "label": "Customers", "order": 20 },
    "order": 10
  }
}
```

- [ ] **Step 5: Update console package template**

In `apps/runtime-console/docs/console-package-template.md`, update the JSON and Rust examples to include the same `navigation` object and `ConsoleNavigation` Rust snippet from Task 3.

- [ ] **Step 6: Regenerate if Rust OpenAPI changed after Task 2**

Run:

```bash
just generate
git diff -- contracts/openapi/app-api.v1.yaml packages/ts-sdk/src/generated
```

Expected: no diff if Task 2 already captured all schema changes; otherwise only navigation-related generated changes.

- [ ] **Step 7: Run final gates**

Run:

```bash
just generated-check
just arch-check
just console-check
just sdk-check
```

Expected: PASS.

- [ ] **Step 8: Commit Task 6**

```bash
git add modules/identity/src/module.rs docs/architecture/module-console-surfaces.md apps/runtime-console/docs/console-package-template.md contracts/openapi/app-api.v1.yaml packages/ts-sdk/src/generated
git diff --cached --name-only
git commit -m "docs: document console workspaces"
```

## Final Verification

- [ ] **Step 1: Confirm worktree state**

Run:

```bash
git status --short
```

Expected: no unstaged or staged files from this implementation.

- [ ] **Step 2: Run broad check if time permits**

Run:

```bash
just check
```

Expected: PASS. If this fails after the task-specific gates passed, inspect whether the failure is related to this work before making changes.

## Self-Review Notes

- Spec coverage: Tasks cover manifest data, backend DTO/OpenAPI propagation, generated SDK, package manifest mapping, workspace navigation model, shell switcher, command palette global search, fixture metadata, docs, and validation gates.
- Placeholder scan: The plan uses concrete paths, commands, expected results, and code snippets for every code-changing task.
- Type consistency: The plan uses `ConsoleNavigation`, `ConsoleWorkspaceRef`, `ConsoleNavigationGroup`, and `navigation` consistently across Rust and TypeScript.
