# Module Console Surfaces

This note specifies the current Runtime Console frontend extension point for
modules. It covers module-owned console pages that are installed into the
host's Vite/React app at build time.

Console surfaces are distinct from admin surfaces:

- `AdminSurface` describes data-admin rendering under `/admin/data/*`.
- `ConsoleSurface` describes a Runtime Console route and package export that
  should appear in the host shell.

The first implementation extracts the Runtime Stories page into the
`platform-story` console module. It is still bundled with the host app, but it
uses the same manifest shape intended for later package-based modules.

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

Manifest lint rules for console declarations live in `platform-module`.
Backends expose the lint results through module metadata; the Runtime Console
renders those results and must not duplicate the lint logic locally.

## Frontend Module Contract

The current Runtime Console registry is build-time:

- `defineConsoleModule` defines a frontend module contribution.
- Each module contributes one or more route surfaces with a path, label, area,
  component, and optional icon.
- The host builds routes, navigation, and command palette entries from the same
  registry.
- Duplicate route paths fail during registry construction.

This keeps first-party modules simple while preserving the shape needed for a
future package install step.

Frontend modules should import host capabilities through the host facade, not
through deep app paths. The current facade is `runtimeConsoleHostApi`, which
exposes narrow groups for runtime context, queries, hooks, routing helpers,
common UI, runtime UI, mock data, and story helpers.

The boundary is enforced by Runtime Console tests:

- A module may import its own files and the host facade.
- A module must not deep-import host `pages`, `hooks`, `components`, or `data`
  internals directly.
- Host code must not deep-import a module's private files; it imports the
  module entry point.

## Relationship To Package Installation

Package installation is not implemented yet. The current implementation is the
first-party lane:

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

## Adding A First-Party Console Module

1. Add `ConsoleSurface` declarations to the module manifest.
2. Register the module in `app-bootstrap` so `/admin/data/modules` can expose
   the metadata and lints.
3. Add a frontend module entry under `apps/runtime-console/src/modules/{name}`.
4. Export a `defineConsoleModule(...)` value from that module entry.
5. Use `runtimeConsoleHostApi` for host services and components.
6. Add the module to the console registry.
7. Add tests for route/nav construction and boundary rules.
8. Run `just generated-check`, `just arch-check`, and `just console-check`.
