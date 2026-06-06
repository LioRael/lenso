# Console Package Template

Use this checklist when adding a Runtime Console frontend package.

Console packages let a module contribute a frontend surface without deep-importing
Runtime Console internals. The package can live in this monorepo today and move
to an external repository later if `@lenso/runtime-console-api` is published.

## Package Shape

Generate the standard package skeleton and host registration first:

```sh
pnpm create:console-package billing
```

Use `--dry-run` to preview file changes, and pass options such as
`--label "Billing"` or `--route /data/billing` when defaults are not enough.

Create a package under:

```text
apps/runtime-console/packages/<package-name>
```

Minimal files:

```text
packages/<package-name>/
  package.json
  src/
    index.tsx
    manifest.ts
    page.tsx
    index.test.tsx
```

`package.json`:

```json
{
  "name": "@lenso/<package-name>",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "exports": {
    ".": "./src/index.tsx"
  },
  "peerDependencies": {
    "@lenso/runtime-console-api": "workspace:*",
    "react": "^19.1.0"
  }
}
```

Add package-specific peer dependencies only when the package imports them
directly.

## Manifest

Define the manifest through the host API:

```ts
import { defineConsolePackageManifest } from "@lenso/runtime-console-api";

export const billingConsoleManifest = defineConsolePackageManifest({
  area: "data",
  exportName: "billingConsoleModule",
  icon: "database",
  id: "billing",
  label: "Billing",
  packageName: "@lenso/billing-console",
  requiredCapabilities: ["billing.read"],
  route: "/data/billing",
  source: "installed",
  surfaceName: "billing",
  version: "workspace",
} as const);
```

Use `source: "first_party"` only for platform-owned packages that should be
treated as built-in. Most module packages should use `source: "installed"`.

The host maps manifest fields to Rust `ConsoleSurface` metadata before resolving
installed packages: `surfaceName` becomes `name`, `packageName` becomes
`package.name`, `exportName` becomes `package.export`, and
`requiredCapabilities` becomes `required_capabilities`. `id`, `source`, and
`version` stay on the frontend install manifest and are not sent as console
surface fields.

## Business Module Wiring

For a real module-owned frontend, declare the same package reference in the
Rust `ModuleManifest.console` surface. The backend declaration is what API mode
uses to decide whether an installed frontend package should appear in Runtime
Console navigation.

```rust
use platform_module::{ConsoleArea, ConsolePackage, ConsoleSurface};

ModuleManifest::builder("billing")
    .capabilities(vec!["billing.read".to_owned()])
    .console(vec![ConsoleSurface {
        name: "billing".to_owned(),
        label: "Billing".to_owned(),
        area: ConsoleArea::Data,
        route: "/data/billing".to_owned(),
        package: ConsolePackage {
            name: "@lenso/billing-console".to_owned(),
            export: "billingConsoleModule".to_owned(),
        },
        icon: Some("database".to_owned()),
        required_capabilities: vec!["billing.read".to_owned()],
    }])
```

Keep these values aligned with the frontend manifest:

- Rust `ConsoleSurface.name` = frontend `surfaceName`
- Rust `ConsoleSurface.package.name` = frontend `packageName`
- Rust `ConsoleSurface.package.export` = frontend `exportName`
- Rust `ConsoleSurface.required_capabilities` = frontend `requiredCapabilities`
- Rust `ConsoleSurface.route` = frontend `route`

Add a module test that asserts the manifest declares the surface and passes
manifest linting. Use `domains/identity/src/module.rs` and
`apps/runtime-console/packages/identity-console` as the reference
implementation.

## Module Export

Export a console module from the package entrypoint:

```tsx
import { defineConsoleModule } from "@lenso/runtime-console-api";

import { billingConsoleManifest } from "./manifest";
import { BillingConsolePage } from "./page";

export const billingConsoleModule = defineConsoleModule({
  id: billingConsoleManifest.id,
  surfaces: [
    {
      area: billingConsoleManifest.area,
      component: BillingConsolePage,
      icon: billingConsoleManifest.icon,
      label: billingConsoleManifest.label,
      path: billingConsoleManifest.route,
    },
  ],
});

export { billingConsoleManifest } from "./manifest";
export { BillingConsolePage } from "./page";
```

## Host Registration

Update these host files:

- `apps/runtime-console/package.json`
  - Add `"@lenso/<package-name>": "workspace:*"`.
  - Add `packages/<package-name>/src` to the `test` script.
- `apps/runtime-console/tsconfig.json`
  - Add a `paths` alias for the package entrypoint.
  - Add `packages/<package-name>/src` to `include`.
- `apps/runtime-console/vite.config.ts`
  - Add a `resolve.alias` entry for the package.
- `apps/runtime-console/oxlint.config.ts`
  - Add `packages/<package-name>/src/**/*.{ts,tsx}` to the app override.
- `apps/runtime-console/src/console-package-manifest-exports.ts`
  - Import and append the package manifest.
- `apps/runtime-console/src/console-package-module-exports.ts`
  - Import the manifest and module export.
  - Add `[consolePackageKey(manifest)]: module`.

Then update the lockfile:

```sh
pnpm --dir apps/runtime-console install --lockfile-only
```

The host still has to import installed packages at build time. A backend module
can declare any package, but Runtime Console can only mount it after the package
has been added to `package.json`, the Vite/TypeScript aliases, and
`console-package-module-exports.ts`. Missing declarations appear on the Modules
page as install-plan rows.

## Boundary Rules

Console packages must not import Runtime Console internals directly.

Allowed:

- `@lenso/runtime-console-api`
- Local package files such as `./manifest`, `./page`, and `./layout`
- Declared package peer dependencies

Forbidden:

- `src/app/*`
- `src/components/*`
- `src/hooks/*`
- `src/data/*`
- Other package internals

The boundary test lives in:

```text
apps/runtime-console/src/app/console-module-boundary.test.ts
```

If a package needs a new host capability, add it to
`@lenso/runtime-console-api` instead of importing host internals.

## Verification

Run:

```sh
just console-check
```

This covers formatting, linting, Runtime Console tests, package tests,
TypeScript, and production build.
