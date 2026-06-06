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
