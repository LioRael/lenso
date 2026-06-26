---
name: lenso-remote-module-authoring
description: Use when building or editing an out-of-process Lenso service module in JavaScript or TypeScript with `@lenso/remote-module-kit`, including manifests, remote routes, runtime functions, event handlers, and custom admin surfaces.
---

# Lenso Service Module Authoring

## Overview

Use the public remote-module kit for service modules: modules that run outside
the host while the host keeps auth, queues, retries, and visibility. Keep the
module package self-contained and publishable.

## Start Here

```sh
pnpm add @lenso/remote-module-kit@0.1.1
```

Start with the manifest:

- `defineRemoteModule(...)`
- `getRoute(...)`
- `runtimeFunction(...)`
- `adminAction(...)`
- `declarativeCustom(...)`
- `queryValue(...)`
- `serveRemoteModule(...)`

## Host Boundaries

- Keep auth, retries, queues, and visibility on the host.
- Keep remote manifests declarative.
- Do not depend on sibling workspace paths for examples.
- Use the host proxy and console surfaces already provided by Lenso.

## Agent Output

For a service module, leave:

- a manifest URL such as `/lenso/module/v1/manifest`
- declared HTTP routes, runtime functions, event handlers, actions, query values, or custom surfaces that are actually served
- install instructions using `lenso module install <manifest-url>`
- lifecycle instructions using `lenso module service list`, `lenso module service status <module> <service>`, and `lenso module doctor <module> --json`
- a `module-services.local.json` sample or manifest `install.services` declaration when the module has a local service process
- one package or smoke check that proves the module can run outside the host
- Console expectations: Modules should show installed / configured / ready, with Remote Calls and Runtime Story staying host-owned

## Checks

```sh
pnpm package-readiness
```

Use `npm pack --dry-run` before publishing a package change.

## Keep Out

- Do not make the service module responsible for host auth, retries, queues, or observability.
- Do not require a sibling Lenso checkout for a publishable example.
