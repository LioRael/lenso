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

For a blank Rust module-authoring project outside this repository, install the
published facade crate instead:

```sh
cargo add lenso@0.1.0
```

That crate exposes serializable module declarations and manifest linting. Local
host development in this backend repository still uses the workspace crates.

## Configure Local Environment

Start from the committed local defaults:

```sh
cp .env.example .env
```

`.env.example` contains local Postgres, API, CORS, linked composition profile,
logging, and optional OTLP defaults. Module installs may update `REMOTE_MODULES`
in `.env`; that is local runtime configuration, not a registry or
install-history database.

Local development defaults to `LENSO_COMPOSITION_PROFILE=demo`, which includes
the identity and notifications fixture modules. Non-local environments must set
`LENSO_COMPOSITION_PROFILE=core` or `LENSO_COMPOSITION_PROFILE=demo` explicitly.

Development bearer tokens such as `Bearer dev-service:admin` are accepted only
for local/development API environments. Do not use them as deployment
credentials.

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

Use the sibling Runtime Console repository's demo to verify the current
remote-module path without keeping long-running services open:

```sh
pnpm --dir ../lenso-runtime-console demo:release
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

User-facing examples live in
[LioRael/lenso-examples](https://github.com/LioRael/lenso-examples) and depend
on published `@lenso/remote-module-kit@0.1.1` and `@lenso/ts-sdk@0.1.0`
packages instead of sibling workspace paths.

Clone and start the example module in a separate checkout:

```sh
git clone https://github.com/LioRael/lenso-examples ../lenso-examples
pnpm --dir ../lenso-examples install
pnpm --dir ../lenso-examples start:hello-action
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

This runs the backend repository quality gate.
