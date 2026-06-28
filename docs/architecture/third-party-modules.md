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

The host owns runtime policy. A module declares what it needs and implements its
own behavior; it does not own host auth, runtime queues, retries, story records,
or HTTP proxy policy. Install trust is operator-owned: explicit manifest URLs are
treated like direct CLI installs, and official catalogs are curated before
publication.

## Default Source

Third-party ecosystem capabilities should default to services: independently
running providers that expose one or more modules through the existing `Remote`
source.

`Linked` modules are for first-party application code, framework fixtures, and
local project-owned modules that intentionally compile into the host. They can
use `modules/<name>` and `crates/lenso-bootstrap` registration.

Services are the right default for external contributors because they are
language-independent, can be versioned and deployed separately, and keep the
host free of third-party code execution.

`Wasm` remains a future source for stronger sandbox and marketplace scenarios.
Do not model Wasm as a variant of `Remote`; it needs separate execution,
permission, and packaging rules.

## Contributor Package Shape

A third-party service package should be understandable without cloning a Lenso
host application:

```text
lenso-billing-service/
  lenso.service.json
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

The exact backend language is not prescribed. The service must expose the
service manifest and module protocol endpoints that `platform-module-remote`
expects.

## Manifest Contract

The service manifest is the source of truth for install-time inspection. A
service should expose it at a stable base URL such as
`https://example.com/lenso/service/v1`:

```text
GET /lenso/service/v1/manifest
```

The service manifest contains one or more module manifests. A representative
shape:

```json
{
  "protocol": "lenso.service.v1",
  "name": "billing-service",
  "version": "0.1.0",
  "capabilities": ["billing.read", "billing.write"],
  "http_routes": [],
  "runtime": {
    "functions": []
  },
  "admin": {
    "kind": "schema"
  },
  "install": {
    "env": {
      "BILLING_API_BASE_URL": "https://billing.example.com"
    },
    "commands": [
      {
        "command": "pnpm --dir ../lenso-runtime-console install",
        "cwd": "."
      }
    ],
    "services": [
      {
        "name": "billing-api",
        "command": "pnpm --dir ../lenso-billing/backend dev",
        "cwd": ".",
        "readyUrl": "https://billing.example.com/lenso/service/v1/status",
        "readyTimeoutMs": 10000,
        "autoStart": true
      }
    ]
  },
  "console": [
    {
      "name": "billing",
      "label": "Billing",
      "area": "data",
      "route": "/data/billing",
      "package": {
        "name": "@vendor/lenso-billing-console",
        "export": "billingConsoleModule",
        "bundleUrl": "./console/billing-console.js",
        "hostApi": "1"
      },
      "required_capabilities": ["billing.read"]
    }
  ]
}
```

The host may cache the manifest, lint it through `platform-module`, and reject
or degrade modules that request unsupported surfaces. `install.env` is written
to the host `.env` by the CLI. `install.commands` are executed only when the
operator passes `--run-install-commands`. `install.services` are written to
`.lenso/module-services.json`; the API and worker start those services before
loading configured services. Host-started services are tracked with
lock/pid files next to `module-services.json` and are stopped when the owning
API/worker process exits; services that are already ready before startup are
treated as external and are not stopped by the host.

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

- module source configuration
- manifest fetch, validation, linting, and compatibility checks
- capability enforcement
- HTTP proxy routing and header policy
- request and response size limits
- runtime queues, retry policy, and worker execution
- persisted Runtime Story and Technical Operations records
- admin action authorization and projection
- console package installation and registry resolution

Services must not write host runtime tables, consume host outbox rows,
receive caller bearer tokens, or claim host-owned story/function-run records.

## Current Support

Current service support includes:

- remote manifest loading
- schema-admin read data
- schema, declarative custom, and embedded custom admin metadata
- read-only declarative admin query values
- declared host-owned HTTP proxy routes
- remote runtime function execution through host-owned queues
- persisted remote proxy calls and runtime-operation visibility

Current Runtime Console support includes:

- workspace-installed console packages
- package manifests derived into install metadata
- module metadata showing missing frontend bundle registrations
- low-friction service install through `lenso service install <manifest-url>`
- service install CLI that writes local source configuration and console
  extension registry entries
- dynamic same-origin bundle loading from `/console/extensions/registry.json`
- boundary checks that forbid package imports from host internals

## Deferred Support

The following are intentionally deferred:

- automatic npm package installation
- JavaScript bundle loading from module manifests
- Wasm execution
- embedded host bridges
- streaming proxy protocols
- per-module OpenAPI fragment ingestion
- write-capable schema-admin CRUD

Each deferred area needs a versioned protocol and host-side enforcement before
third-party modules can rely on it.

## CLI Direction

The local scaffold command is optimized for project-owned linked modules:

```sh
lenso module create billing --with-console
```

Third-party scaffolding uses a separate service lane:

```sh
lenso module catalog add https://example.com/lenso/service/v1/manifest
lenso service install https://example.com/lenso/service/v1/manifest
lenso module uninstall billing
```

The default install path is user-driven: see a service, install from its
manifest, restart the host, reload Runtime Console, and use its modules.
`service install` updates host-local service configuration, applies
manifest-declared `install.env` values, runs opted-in `install.commands`,
writes `install.services`, writes an install receipt to
`.lenso/module-installs.json`, copies declared console bundles to
`.lenso/console/extensions`, and updates
`.lenso/console/extensions/registry.json` when the manifest declares console
packages with `bundleUrl`. `module install <manifest-url>` remains a
compatibility path for legacy remote module manifests.
`module uninstall <name>` removes the host-local service source and any
console extension registry/install-receipt entry for that module; it leaves
module data alone.

`.lenso/module-catalog.json` is the optional discovery list behind Available
Modules. A host can add entries with `lenso module catalog add <manifest-url>`.
The catalog only records module basics, manifest URL, base URL, summary, and
console package hints. The admin API reflects that discovery data back to
Runtime Console with capability counts, host compatibility preflight results,
and archived catalog entries; official catalogs are curated at publication time,
while arbitrary catalog entries remain operator-selected.

When a host has no local catalog, Available Modules preserves the current loaded
service view if any services are already configured. If neither a
local catalog nor loaded services exist, the API falls back to the
read-only `builtin:lenso-official-module-catalog` so a fresh host has an
official discovery source without fetching remote marketplace state.

If the manifest is installed from a local file or non-protocol URL, pass the
runtime module base URL explicitly:

```sh
lenso service install ./lenso.service.json --base-url https://example.com/lenso/service/v1
```

The service lane should generate a service package, not a host workspace
member.
Host installation should record source configuration and extension registry
state without compiling third-party code into the application bundle.

The first CLI install lane writes host-local state only:

- `.lenso/module-catalog.json`: optional local discovery entries for Available
  Modules.
- `.env`: appends or replaces `REMOTE_MODULES=<name>=<base_url>`.
- `.lenso/module-installs.json`: records the module source and host-local writes
  so uninstall does not need to infer what was installed.
- `.lenso/console/extensions/<module>/*.js`: copied third-party Runtime Console
  bundles.
- `.lenso/console/extensions/registry.json`: same-origin dynamic bundle registry
  consumed by the hosted Runtime Console.

Runtime Console can perform the same host-local install write through
`POST /admin/data/available-modules/{module}/install`. The visual path is still
an operator-reviewed install: it writes `.env`, copies declared console bundles,
updates the extension registry, and reports restart/reload follow-up state. It
does not install npm packages or compile third-party code into the official
Runtime Console bundle.

`pnpm demo:remote-module-install` in the `lenso-runtime-console` repository runs
the same flow against a temporary host fixture without mutating the working tree.
The operator-facing walkthrough lives in
`lenso-runtime-console/docs/remote-module-install-flow.md`.

The plan file is intentionally ignored by git. It is an operator/developer
handoff artifact, not a trust database.
