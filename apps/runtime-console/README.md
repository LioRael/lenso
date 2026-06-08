# Runtime Console

Frontend prototype for the Lenso Runtime Console.

The console runs with seeded data by default, and switches core runtime views to the
local backend when `VITE_RUNTIME_CONSOLE_MODE=api` and `VITE_API_BASE_URL` are set.

Local API calls use the development service token:

```text
Authorization: Bearer dev-service:admin:runtime.stories.read,remote_crm.contacts.read,remote_crm.contacts.sync
```

## Run

```bash
cd apps/runtime-console
pnpm install
pnpm dev
```

Open:

```text
http://localhost:5174
```

## Backend Wiring

Start the backend and worker from the repo root:

```bash
just db-up
just migrate
just api
just worker
```

Run the console against the local API:

```bash
VITE_RUNTIME_CONSOLE_MODE=api VITE_API_BASE_URL=http://localhost:3000 pnpm dev
```

Override the development service token when needed:

```bash
VITE_API_AUTH_TOKEN=dev-service:admin:runtime.stories.read,remote_crm.contacts.read,remote_crm.contacts.sync pnpm dev
```

## Remote Module API QA

From the repo root, start a full remote-module Runtime Console demo:

```bash
just console-api-demo
```

Then seed and verify the remote story path:

```bash
just console-api-qa
```

Useful focused commands:

```bash
just console-api-fixture
just console-api-smoke
```

The QA fixture creates a remote proxy call with
`correlation_id = corr_console_api_fixture`, then verifies the Remote Calls page
data, Runtime Story remote node/timeline shape, Technical Operations, payloads,
and logs.

If Postgres is already running and migrated:

```bash
SKIP_DB_SETUP=1 just console-api-demo
```

If default ports are busy:

```bash
REMOTE_MODULE_ADDR=127.0.0.1:4101 HTTP_PORT=3001 VITE_API_BASE_URL=http://localhost:3001 CONSOLE_PORT=5176 just console-api-demo
```

## Architecture

- `src/app`: router and root providers.
- `src/components/ui`: small Tailwind-composed primitives.
- `src/components/runtime`: Runtime Console shell, search, command palette, drawer, timeline nodes.
- `src/data`: seeded mock runtime data.
- `src/hooks`: keyboard and runtime query hooks with API/mock switching.
- `src/lib`: formatting, query client, and ky HTTP client foundation.
- `src/pages`: route-level screens.
- `packages/console-package-api`: public host API for console package authors.
- `packages/story-console`: first-party Story workbench package.
- `packages/identity-console`: installed module package fixture used to exercise
  framework wiring; it is not a product-default business module.

## Console Packages

Runtime Console frontend modules are local workspace packages under `packages/*`.
They must import host capabilities through `@lenso/runtime-console-api`, define a
`ConsolePackageManifest`, and export a `ConsoleModule`.

Lenso provides the package framework and fixtures. Product projects choose and
own their real business modules.

Generate a linked Rust module scaffold first when starting a new project module:

```bash
pnpm create:module billing
```

This creates `modules/billing`, adds it to the Rust workspace, and registers it
in `crates/app-bootstrap` as a linked module.

Add `--with-console` to generate and register the matching Runtime Console
package in the same command:

```bash
pnpm create:module billing --with-console
```

For a third-party remote package that should not compile into the host
workspace, use the remote scaffold:

```bash
pnpm create:module billing --remote --output-dir ../modules
lenso module catalog add https://example.com/lenso/module/v1/manifest
lenso module add https://example.com/lenso/module/v1/manifest
lenso console-package apply-plan
```

Run `pnpm demo:remote-module-install` for a temp-directory smoke demo of that
flow. See `docs/remote-module-install-flow.md` for the full host and module
author workflow.

See `docs/console-package-template.md` before adding a package. The short path is:

1. Add `packages/<name>/package.json`.
2. Define `src/manifest.ts` with `defineConsolePackageManifest`.
3. Export `<name>ConsoleModule` from `src/index.tsx`.
4. Register the package in host dependencies, aliases, test includes, manifest exports, and module export mapping.
5. Run `pnpm check:console-packages`, `pnpm install --lockfile-only`, and `just console-check`.

For the standard workspace package shape, generate the frontend skeleton and host
registration with:

```bash
pnpm create:console-package billing
```

The underlying CLI command is:

```bash
pnpm exec lenso-console-package create billing
```

The generator also writes `console-surface.json` and `console-surface.rs` so the
frontend manifest and Rust `ModuleManifest.console` declaration can share the
same package/export/route/capability values.

## Checks

The console uses Ultracite with the Oxlint/Oxfmt provider:

- `oxlint.config.ts` extends `ultracite/oxlint/core`, `ultracite/oxlint/react`, and `ultracite/oxlint/tanstack`.
- `oxfmt.config.ts` extends `ultracite/oxfmt`.
- No ESLint, Prettier, or Biome stack is configured.

```bash
pnpm format
pnpm format:check
pnpm lint
pnpm check:console-packages
pnpm typecheck
pnpm build
pnpm check
```
