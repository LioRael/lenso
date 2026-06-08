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

- module source configuration
- manifest fetch, validation, linting, and compatibility checks
- capability enforcement
- HTTP proxy routing and header policy
- request and response size limits
- runtime queues, retry policy, and worker execution
- persisted Runtime Story and Technical Operations records
- admin action authorization and projection
- console package installation and registry resolution
- optional signature, provenance, and publisher key trust checks when a host
  chooses a curated production policy

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
- low-friction remote install through `lenso module add <manifest-url>` and
  `lenso module marketplace install <manifest-url>`
- host-local publisher key trust through `lenso module publisher list`,
  `lenso module publisher doctor`, `lenso module publisher trust`, and
  `lenso module publisher revoke`
- Module Registry v0 catalog authoring and discovery through
  `lenso module registry add`, `lenso module registry list`,
  `lenso module registry doctor`, `lenso module registry inspect`,
  `lenso module registry review`, `lenso module registry install`, and
  `lenso module registry remove`/`restore`
- local marketplace bundle export/import through `lenso module marketplace export`
  and `lenso module marketplace import`
- machine-readable registry preflight snapshots through
  `lenso module registry doctor --json`
- remote module install CLI that writes local source configuration
- console package apply-plan registration for requested package exports
- module doctor diagnostics for source/package/registry mismatches
- boundary checks that forbid package imports from host internals

## Deferred Support

The following are intentionally deferred:

- marketplace distribution and global trust policy
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
lenso module marketplace install https://example.com/lenso/module/v1/manifest
lenso console-package apply-plan
lenso module doctor
```

The default install path is user-driven: see a module, install from its
manifest, apply the console package plan, restart the host, and verify. It does
not require publisher keys, registry review, history, or bundle import/export.

Module Registry v0 is an optional local catalog and preflight layer, not a
required marketplace gate.
The catalog maps a module name to a remote manifest reference, optional base
URL, capabilities, and console package hints:

```json
{
  "version": 1,
  "modules": [
    {
      "name": "billing",
      "version": "0.1.0",
      "source": "remote",
      "manifestReference": "https://example.com/lenso/module/v1/manifest",
      "baseUrl": "https://example.com/lenso/module/v1",
      "capabilities": ["billing.read"],
      "compatibility": {
        "lenso": {
          "minVersion": "0.1.0",
          "maxVersion": "0.1.0"
        },
        "consolePackageApi": "1"
      },
      "provenance": {
        "publisher": "Acme Billing",
        "sourceRepository": "https://github.com/acme/lenso-billing-module",
        "packageUrl": "https://packages.example.com/lenso-billing-0.1.0.tgz",
        "checksum": "sha256:...",
        "signatureUrl": "https://packages.example.com/lenso-billing-0.1.0.tgz.sig",
        "signatureAlgorithm": "ed25519-detached",
        "publicKeyId": "acme-ed25519-2026"
      },
      "consolePackages": [
        {
          "packageName": "@vendor/lenso-billing-console",
          "exportName": "billingConsoleModule",
          "route": "/data/billing"
        }
      ]
    }
  ]
}
```

`lenso module registry install <module>` delegates to the same host-local
install path as `lenso module add`, so `.env`,
`.lenso/console-package-install-plan.json`, `console-package apply-plan`, and
`module doctor` remain the install contract.
Registry review also enforces compatibility before installation. Catalog
entries can declare supported Lenso host versions and console package API
versions through `compatibility`; incompatible modules are blocked before host
files are written.

Publisher trust is host-local and optional. Hosts that want a curated
production policy can store trusted publisher keys in:

```sh
lenso module publisher trust "Acme Billing" acme-ed25519-2026 --public-key-file ./acme-ed25519.pem
lenso module publisher list
lenso module publisher doctor
lenso module publisher revoke "Acme Billing" acme-ed25519-2026
```

```json
{
  "version": 1,
  "publishers": [
    {
      "publisher": "Acme Billing",
      "publicKeyId": "acme-ed25519-2026",
      "publicKey": "-----BEGIN PUBLIC KEY-----\n...\n-----END PUBLIC KEY-----\n",
      "status": "trusted"
    }
  ]
}
```

Registry review also requires a provenance snapshot for trusted entries:
publisher, source repository, and checksum. When `provenance.packageUrl` and a
`sha256:<hex>` checksum are present, review fetches the package artifact and
blocks installation if the digest does not match. This is checksum verification,
and the Signature Verify v0 gate verifies `ed25519-detached` signatures against
the trusted publisher key selected by `publisher` and `publicKeyId`.
`pnpm --dir apps/runtime-console run demo:module-registry-install` exercises the
same sequence against a temporary host fixture without mutating the working tree.

These trust, signature, provenance, history, doctor, and bundle commands are
advanced hardening tools. They must not be presented as prerequisites for the
normal marketplace install path.

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
`lenso module doctor` checks that `REMOTE_MODULES`, the install plan, Runtime
Console dependencies, and package export mappings agree. Failed checks are
grouped by source, package installation, and registry mapping, with fix
commands next to each issue.
`pnpm --dir apps/runtime-console demo:remote-module-install` runs the same flow
against a temporary host fixture without mutating the working tree.
The operator-facing walkthrough lives in
`apps/runtime-console/docs/remote-module-install-flow.md`.

The plan file is intentionally ignored by git. It is an operator/developer
handoff artifact, not trusted marketplace state.
