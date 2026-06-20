# Module Frontend Packages

This note specifies the current Runtime Console frontend extension point for
modules. It covers module-owned console pages that are installed into the
host's Vite/React app at build time.

Lenso provides the framework for module frontend packages. It does not ship
product-default business modules; product projects decide which real business
modules to install.

Console surfaces are distinct from admin surfaces:

- `AdminSurface` describes data-admin rendering under `/admin/data/*`.
- `ConsoleSurface` describes a Runtime Console route and package export that
  should appear in the host shell.

The first implementation extracts the Runtime Stories page into the
`platform-story` console package. It is a first-party platform package bundled
with the host app, but it uses the same manifest shape intended for
module-owned packages.

## Manifest Contract

`ModuleManifest.console` is pure serializable data. A console surface declares:

- `name`: stable surface identifier inside the module.
- `label`: navigation label.
- `area`: one of `runtime`, `operations`, `data`, or `configuration`.
- `route`: absolute Runtime Console route path.
- `package.name`: frontend package name.
- `package.export`: named export that provides the console module.
- `icon`: optional host-known icon id.
- `required_capabilities`: capabilities the host should require before showing
  or mounting the surface.
- `navigation`: optional workspace metadata. Missing metadata defaults to the
  host `System` workspace with host-defined area ordering, keeping runtime
  surfaces such as Stories ahead of lower-priority system pages. Modules may
  create their own workspace by declaring a workspace id, label, and optional
  icon; the first slice supports one optional group level inside a workspace.
  The `system` workspace id is reserved for the host; module surfaces should
  omit `navigation` when they belong in System.

Example surface metadata:

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

Workspace ownership rules:

- Host-owned platform surfaces omit `navigation` and let the shell place them in
  `System`.
- Business modules create their own workspace by declaring
  `navigation.workspace`; there is no hardcoded `Modules` workspace.
- `system` is reserved for the host and must not be claimed by module
  manifests.
- A module may group pages one level deep with `navigation.group`; deeper menu
  trees and cross-module shared workspaces are deferred until ownership policy is
  explicit.

For `platform-story`, Rust and frontend package/export metadata intentionally
match; backend navigation may still be omitted so the host applies the System
default:

```text
module: platform-story
package: @lenso/story-console
export: storyConsoleModule
surface: stories
route: /runtime/stories
capability: runtime.stories.read
```

Module-owned frontend packages should keep the Rust and TypeScript declarations
aligned through a package-local `console-surface.json` contract. The Runtime
Console package manifest imports that JSON, while Rust module tests can read the
same file and compare it with `ModuleManifest.console`.

Manifest lint rules for console declarations live in `platform-module`.
Backends expose the lint results through module metadata; the Runtime Console
renders those results and must not duplicate the lint logic locally.

## Frontend Module Contract

The current Runtime Console registry is static build-time installation:

- `defineConsoleModule` defines a frontend module contribution.
- `defineConsolePackageManifest` describes the package/export/route contract.
- Each module contributes one or more route surfaces with a path, label, area,
  component, and optional icon.
- The host builds routes, navigation, and command palette entries from the same
  registry.
- The host imports trusted package entrypoints through
  `console-package-manifest-exports.ts` and `console-package-module-exports.ts`.
- Duplicate route paths fail during registry construction.

This keeps the framework explicit and reviewable while preserving the shape
needed for a future package install step.

Frontend modules should import host capabilities through the host facade, not
through deep app paths. The current facade is `runtimeConsoleHostApi`, which
exposes narrow groups for runtime context, queries, hooks, routing helpers,
common UI, runtime UI, mock data, and story helpers.

The boundary is enforced by Runtime Console tests:

- A module may import its own files and the host facade.
- A module package may declare ordinary peer dependencies.
- A module must not deep-import host `pages`, `hooks`, `components`, or `data`
  internals directly.
- Host code must not deep-import a module's private files; it imports the
  module entry point.

## Relationship To Package Installation

Runtime package installation is intentionally explicit. The current
implementation supports trusted workspace packages:

1. The backend manifest declares the console surface and package/export names.
2. The frontend package is available to the host build.
3. The host registry imports the package export and contributes its routes.
4. The module uses the host facade instead of private host imports.

A later installable-module step should preserve those boundaries and add the
missing lifecycle pieces: dependency resolution, version compatibility,
capability enforcement before route mounting, and failure UI when a package
cannot be loaded.

Do not add ad hoc global objects or direct host token access as a shortcut.
Third-party Runtime Console package loading must go through the versioned host
API and the host-owned same-origin extension registry.

## Installation Contract

The Runtime Console may show install and reload status from backend module
metadata, but the browser must not install packages by itself. Installation is a
host-owned operation: the CLI or admin API copies an already-built bundle into
`.lenso/console/extensions/<module>/` and writes
`.lenso/console/extensions/registry.json`. Installing a module is the operator's
trust decision for that module's declared console bundle.

## Installed Package Registry

The host resolves package exports through an explicit installed package
registry, not through arbitrary runtime import strings. A registry entry binds a
manifest-declared package/export to a trusted module object:

```ts
import { crmConsoleModule } from "@lenso/crm-console";

export const installedConsolePackages = [
  {
    packageName: "@lenso/crm-console",
    exportName: "crmConsoleModule",
    module: crmConsoleModule,
    source: "installed",
    version: "0.1.0",
  },
];
```

This registry is intentionally static. It gives Vite a real import to bundle,
lets reviewers inspect package additions in source control, and keeps unknown
package exports visible as Missing Console Packages until the host explicitly
trusts and registers them.

## Runtime Bundle Registry

The API service hosts the Runtime Console from `LENSO_CONSOLE_DIST_DIR`
(default `.lenso/console/dist`) under `/console/*`, with client-side route
fallback to `index.html`. Runtime users should receive this directory as part
of the service release artifact; they should not need Node.js, pnpm, or the
frontend source repository.

For local development or release packaging, build and install the hosted console
dist from the sibling frontend repository:

```sh
just console-build
```

When the frontend repository is not next to this backend checkout, pass
`RUNTIME_CONSOLE_ROOT=/path/to/lenso-runtime-console`.
The build script also creates an empty extension registry when none exists.

Installed third-party modules may provide already-built console bundles. The
service exposes those bundles from the same origin under `/console/extensions/*`
from `LENSO_CONSOLE_EXTENSIONS_DIR` (default `.lenso/console/extensions`); the
Runtime Console reads
`/console/extensions/registry.json` before creating its router and registers
compatible bundle exports as `runtime_bundle` packages.
Bundles that render React components must externalize React and the console host
API to the stable same-origin host entries under `/console/extensions/host/*`.
Console package styles should reference Lenso's Tailwind token contract at build
time with `@reference "@lenso/runtime-console-api/theme.css"` when they need
semantic utilities such as `bg-surface`; this is a build-time reference and must
not be emitted as a second host theme.

The first registry shape is deliberately small:

```json
{
  "version": 1,
  "bundles": [
    {
      "packageName": "@vendor/crm-console",
      "exportName": "crmConsoleModule",
      "entry": "/console/extensions/crm/entry.js",
      "hostApi": "1",
      "styles": ["/console/extensions/crm/entry.css"]
    }
  ]
}
```

Rules:

- Bundle entries must be same-origin URLs served by the host.
- Bundle styles, when present, must also be same-origin CSS assets served by the
  host and are loaded before the JavaScript bundle import.
- `hostApi` must match the Runtime Console host API version.
- The exported value must be a `ConsoleModule`.
- Unsupported host API versions, cross-origin entries, and malformed exports are
  rejected before route registration.

This is not arbitrary browser-side package installation. The host installation
lane is still responsible for downloading, verifying, and placing bundle files
in the configured console extension directory.

The local developer workflow is supported by:

- `lenso console-package create <module>` (or the local
  `pnpm create:console-package <module>` alias): creates a workspace package
  skeleton, host registration entries, `console-surface.json`, and a Rust
  `console-surface.rs` snippet.
- `pnpm check:console-packages`: verifies package dependencies, manifest
  exports, module export mappings, and package peer dependencies.
- Runtime Console boundary tests: verify every installed console package imports
  host capabilities only through `@lenso/runtime-console-api`.

Future installers can choose one of these execution lanes:

- A dev-only local tool that updates the host workspace and requires an explicit
  developer action.
- A backend endpoint such as `/admin/data/console-packages/install`, guarded by
  admin auth, lockfile policy, and audit logging.
- An official marketplace install protocol with curated packages, version
  compatibility, and declared host API requirements.

All lanes must preserve the same request/result boundary. The host decides
whether a package is trusted, compatible, and allowed; module manifests only
request a package/export.

## Adding A Workspace Console Package

1. Run `pnpm create:console-package <module>` in the `lenso-runtime-console`
   repository.
2. Copy or adapt the generated `console-surface.rs` into the Rust
   `ModuleManifest.console` declaration.
3. Add a Rust module test that compares the manifest surface with
   `console-surface.json`.
4. Implement the package page using `@lenso/runtime-console-api`.
5. Run `pnpm check:console-packages` in the Runtime Console repository.
6. Run the relevant Rust module tests.
7. Run `just arch-check`, `just generated-check`, and the relevant Runtime
   Console checks before finishing broad framework changes.
