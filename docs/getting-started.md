# Getting Started

This guide is the first-user happy path for running Lenso locally and installing
a remote module. It avoids marketplace hardening flows; you choose a manifest
URL, install it, restart services, and inspect the loaded module.

## Prerequisites

- Rust toolchain compatible with the workspace.
- `just`.
- Docker for local Postgres.
- Node 24 and `pnpm`.

## Install Dependencies

```sh
just install
```

## Run The Local Services

Start Postgres and apply migrations:

```sh
just db-up
just migrate
```

Start the API, worker, and Runtime Console in separate shells:

```sh
just api
just worker
just console-api
```

Open the Runtime Console at the Vite URL printed by `just console-api`.

## Run The Release Demo

Use the release demo to verify the current remote-module path without keeping
long-running services open:

```sh
just demo-release
```

The demo starts the example `hello-action` remote module, reads its manifest,
checks its schema-admin, HTTP route, and runtime function endpoints, then runs
the same install command a user would run:

```sh
lenso module add <manifest-url>
```

The install writes local `REMOTE_MODULES` config and a console package install
plan. Restart the API, worker, and Console after installing a real module.

## Install The Example Module Manually

Start the example module:

```sh
node examples/remote-modules/hello-action/src/server.mjs
```

Install its manifest:

```sh
lenso module add http://127.0.0.1:4100/lenso/module/v1/manifest
lenso console-package apply-plan
```

Restart the local services and open the Runtime Console. The module should be
available through the Modules/Data surfaces and its remote calls should appear
in operations views after use.

## Release Check

Before cutting a local release branch or tag, run:

```sh
just release-check
```

This runs the full repository quality gate plus `just demo-release`.
