# Third-Party Modules

This document defines the target shape for modules contributed outside a Lenso
application repository.

## Definition

A third-party module is not just a Rust crate. It is a package of declarations
and source-specific behavior that the host can inspect, authorize, and load:

```text
module = manifest data + loading source + optional Runtime Console package
```

- Manifest data is the portable contract. It declares identity, version,
  capabilities, HTTP routes, runtime functions, admin surfaces, and console
  surfaces.
- Loading source determines where behavior runs. `Linked` runs in the host
  process, `Remote` runs out of process, and future `Wasm` runs in a sandboxed
  host-controlled runtime.
- Runtime Console packages are optional frontend contributions loaded by the
  host console registry.

The host owns policy. A module declares what it needs and implements its own
behavior; it does not own host auth, runtime queues, retries, story records,
HTTP proxy policy, or install trust.

## Default Source

Third-party ecosystem modules should default to `Remote`.

`Linked` modules are for first-party application code, framework fixtures, and
local project-owned modules that intentionally compile into the host. They can
use `modules/<name>` and `crates/app-bootstrap` registration.

`Remote` modules are the right default for external contributors because they
are language-independent, can be versioned and deployed separately, and keep the
host free of third-party code execution.

`Wasm` remains a future source for stronger sandbox and marketplace scenarios.
Do not model Wasm as a variant of `Remote`; it needs separate execution,
permission, and packaging rules.

## Contributor Package Shape

A third-party module package should be understandable without cloning a Lenso
host application:

```text
lenso-billing/
  lenso.module.json
  backend/
    src/
    openapi.yaml
  console/
    package.json
    src/
      index.tsx
      manifest.ts
      page.tsx
  contracts/
    events/
    runtime-functions/
  README.md
```

The exact backend language is not prescribed. The module must expose the remote
module protocol that `platform-module-remote` expects.

## Manifest Contract

The module manifest is the source of truth for install-time inspection. A remote
module should expose it through the remote module protocol at a stable base URL
such as `https://example.com/lenso/module/v1`:

```text
GET /lenso/module/v1/manifest
```

The manifest should map to the same `ModuleManifest` data model used by linked
modules. A representative shape:

```json
{
  "name": "billing",
  "version": "0.1.0",
  "capabilities": ["billing.read", "billing.write"],
  "http_routes": [],
  "runtime": {
    "functions": []
  },
  "admin": {
    "kind": "schema"
  },
  "console": [
    {
      "name": "billing",
      "label": "Billing",
      "area": "data",
      "route": "/data/billing",
      "package": {
        "name": "@vendor/lenso-billing-console",
        "export": "billingConsoleModule"
      },
      "required_capabilities": ["billing.read"]
    }
  ]
}
```

The host may cache the manifest, lint it through `platform-module`, and reject
or degrade modules that request unsupported surfaces.

## Runtime Console Package

Third-party frontend code must be packaged as a normal npm package that imports
host capabilities only through `@lenso/runtime-console-api`.

Allowed:

- `@lenso/runtime-console-api`
- local package files
- declared package dependencies

Forbidden:

- deep imports from `apps/runtime-console/src/*`
- host bearer tokens
- ad hoc host bridges
- direct access to host stores or query clients

Console packages must declare their install manifest separately from backend
metadata, but the `package.name`, `package.export`, route, and required
capabilities must match the backend `ConsoleSurface`.

## Host Responsibilities

The host is responsible for:

- module source configuration and install policy
- manifest fetch, validation, linting, and compatibility checks
- capability enforcement
- HTTP proxy routing and header policy
- request and response size limits
- runtime queues, retry policy, and worker execution
- persisted Runtime Story and Technical Operations records
- admin action authorization and projection
- console package installation and registry resolution
- future signature, provenance, and marketplace trust checks

Remote modules must not write host runtime tables, consume host outbox rows,
receive caller bearer tokens, or claim host-owned story/function-run records.

## Current Support

Current remote-module support includes:

- remote manifest loading
- schema-admin read data
- schema, declarative custom, and embedded custom admin metadata
- declared host-owned HTTP proxy routes
- remote runtime function execution through host-owned queues
- persisted remote proxy calls and runtime-operation visibility

Current Runtime Console support includes:

- workspace-installed console packages
- package manifests derived into install metadata
- module metadata showing missing frontend package install plans
- boundary checks that forbid package imports from host internals

## Deferred Support

The following are intentionally deferred:

- marketplace install trust and signatures
- automatic npm package installation
- JavaScript bundle loading from module manifests
- Wasm execution
- embedded host bridges
- streaming proxy protocols
- per-module OpenAPI fragment ingestion
- remote event handlers
- write-capable schema-admin CRUD

Each deferred area needs a versioned protocol and host-side enforcement before
third-party modules can rely on it.

## CLI Direction

The local scaffold command is optimized for project-owned linked modules:

```sh
pnpm create:module billing --with-console
```

Third-party scaffolding uses a separate remote-oriented lane:

```sh
pnpm create:module billing --remote --output-dir ../module-packages
lenso module add https://example.com/lenso/module/v1/manifest
lenso console-package apply-plan
```

If the manifest is installed from a local file or non-protocol URL, pass the
runtime module base URL explicitly:

```sh
lenso module add ./lenso.module.json --base-url https://example.com/lenso/module/v1
```

The remote lane should generate a module package, not a host workspace member.
Host installation should record source configuration and surface install plans
without compiling third-party code into the application.

The first CLI install lane writes host-local state only:

- `.env`: appends or replaces `REMOTE_MODULES=<name>=<base_url>`.
- `.lenso/console-package-install-plan.json`: records requested Runtime Console
  packages, exports, routes, and manual `pnpm --dir apps/runtime-console add`
  commands.

`lenso console-package apply-plan` consumes that plan and updates Runtime
Console package dependencies, manifest exports, and module export mappings.
`pnpm --dir apps/runtime-console demo:remote-module-install` runs the same flow
against a temporary host fixture without mutating the working tree.

The plan file is intentionally ignored by git. It is an operator/developer
handoff artifact, not trusted marketplace state.
