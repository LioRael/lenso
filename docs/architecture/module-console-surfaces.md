# Module Frontend Packages

This note specifies the current Runtime Console frontend extension point for
modules. It covers module-owned console pages that are installed into the
host's Vite/React app at build time.

Lenso provides the framework for module frontend packages. It does not ship
product-default business modules. `identity` and `identity-console` are
framework fixtures used to exercise schema-admin, module metadata, and package
registration paths; product projects decide which real business modules to
install.

Console surfaces are distinct from admin surfaces:

- `AdminSurface` describes data-admin rendering under `/admin/data/*`.
- `ConsoleSurface` describes a Runtime Console route and package export that
  should appear in the host shell.

The first implementation extracts the Runtime Stories page into the
`platform-story` console package. It is a first-party platform package bundled
with the host app, but it uses the same manifest shape intended for
module-owned packages. `identity-console` is the installed package fixture.

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

For `platform-story`, Rust and frontend metadata intentionally match:

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
missing lifecycle pieces: dependency resolution, package provenance, version
compatibility, capability enforcement before route mounting, and failure UI
when a package cannot be loaded.

Do not add ad hoc dynamic imports, global objects, or direct host token access as
a shortcut. Runtime Console package loading needs a versioned host API and
explicit trust policy before it can accept third-party code.

## Installation Request Contract

The Runtime Console may derive an installation plan from backend module
metadata, but the browser must not install packages by itself. Installation is a
host-owned operation.

The frontend request shape is intentionally small:

```ts
type ConsolePackageInstallRequest = {
  packageName: string;
  exportName: string;
  requestedByModule: string;
  route: string;
};
```

The corresponding result must report host policy, not just package-manager
success:

```ts
type ConsolePackageInstallResult =
  | { status: "not_configured"; message: string }
  | { status: "rejected"; message: string }
  | { status: "installed"; message: string };
```

The first implementation is a no-op installer that returns `not_configured`.
That keeps missing package discovery and UI state testable without implying that
the Runtime Console can mutate the workspace or install third-party code.

The dev installer lane may return a manual command such as:

```sh
pnpm --dir apps/runtime-console add @lenso/crm-console
```

That command is advisory. A developer still needs to review the package and add
an explicit registry entry before the host can import it.

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

The local developer workflow is supported by:

- `pnpm create:console-package <module>`: creates a workspace package skeleton,
  host registration entries, `console-surface.json`, and a Rust
  `console-surface.rs` snippet.
- `pnpm check:console-packages`: verifies package dependencies, manifest
  exports, module export mappings, and package peer dependencies.
- Runtime Console boundary tests: verify every installed console package imports
  host capabilities only through `@lenso/runtime-console-api`.

Future installers can choose one of these execution lanes:

- A dev-only local tool that updates the host workspace and requires an explicit
  developer action.
- A backend endpoint such as `/admin/data/console-packages/install`, guarded by
  admin auth, provenance checks, lockfile policy, and audit logging.
- A marketplace install protocol with signed packages, version compatibility,
  and declared host API requirements.

All lanes must preserve the same request/result boundary. The host decides
whether a package is trusted, compatible, and allowed; module manifests only
request a package/export.

## Adding A Workspace Console Package

1. Run `pnpm --dir apps/runtime-console create:console-package <module>`.
2. Copy or adapt the generated `console-surface.rs` into the Rust
   `ModuleManifest.console` declaration.
3. Add a Rust module test that compares the manifest surface with
   `console-surface.json`.
4. Implement the package page using `@lenso/runtime-console-api`.
5. Run `pnpm --dir apps/runtime-console check:console-packages`.
6. Run the relevant Rust module tests.
7. Run `just arch-check`, `just generated-check`, and `just console-check`
   before finishing broad framework changes.
