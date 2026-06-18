# Getting Started

This guide is the first-user happy path for running Lenso locally and installing
a remote module. It avoids marketplace hardening flows; you choose a manifest
URL, install it, restart services, and inspect the loaded module.

## Prerequisites

- Rust toolchain compatible with the workspace.
- `just`.
- Docker for local Postgres.

For a blank Rust module-authoring project outside this repository, install the
published facade crate instead:

```sh
cargo add lenso@0.2.1
```

That crate exposes serializable module declarations and manifest linting. Local
host development in this backend repository still uses the workspace crates.

For a blank host project, start from the starter template:

```sh
cp -R crates/lenso-cli/templates/starter-host ../my-lenso-host
cd ../my-lenso-host
cp .env.example .env
docker compose up -d postgres
cargo run --bin migrate
cargo run --bin api
```

Prefer the scaffolder when it is available:

```sh
cargo run -p lenso-cli -- host init ../my-lenso-host
```

Run `cargo run --bin worker` in a second shell. The template depends on this
backend repository's Git-pinned `lenso-host` package. `lenso-host` wraps the
API, worker, migration boot helpers, and a narrow linked HTTP route authoring
surface; generated hosts should pin it to a tag or commit for reproducible
builds. The starter exposes `GET /v1/app/status` plus
`GET`/`POST /v1/app/items` as the first host-owned linked routes and data
surface.
Published `lenso-cli` builds include the prebuilt Runtime Console and copy it
into generated hosts, so `cargo run --bin api` serves the console at
`/console` without requiring Node.js or pnpm in the host project.
To refresh the hosted console later, upgrade `lenso-cli` and run
`lenso host update-console` from the host project.

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
the first-party auth modules and platform story surface. Starter hosts use
`LENSO_COMPOSITION_PROFILE=core` and do not install auth by default; add auth
modules through host composition only when the app needs them. Non-local
environments must set `LENSO_COMPOSITION_PROFILE=core` or
`LENSO_COMPOSITION_PROFILE=demo` explicitly.

Development bearer tokens such as `Bearer dev-service:admin` are accepted only
for local/development API environments. Do not use them as deployment
credentials.

## Run The Local Services

Start Postgres and apply migrations:

```sh
just db-up
just migrate
```

Start the API and worker in separate shells:

```sh
just api
just worker
```

Generated hosts serve the bundled Runtime Console at `/console` when they were
created by a published CLI build. When developing this repository from source,
run the Runtime Console from the sibling `../lenso-runtime-console` repository
or run `just console-build` before packaging.

## Install The Example Module Manually

User-facing examples live in
[LioRael/lenso-examples](https://github.com/LioRael/lenso-examples). This
backend repository does not ship JavaScript example modules or manage their
package dependencies.

Clone and start the example module in a separate checkout:

```sh
git clone https://github.com/LioRael/lenso-examples ../lenso-examples
```

Start the example module from the `lenso-examples` repository, then install its
manifest here with the same command a user would run.

Install its manifest:

```sh
lenso module install http://127.0.0.1:4100/lenso/module/v1/manifest
```

Restart the local services and open the Runtime Console. The module should be
available through the Modules/Data surfaces and its remote calls should appear
in operations views after use.

Remote sources and Runtime Console package exports are loaded at process
startup. After installing a module, restart the API, worker, and Runtime Console.

To run the backend part of this flow without opening the frontend:

```sh
just first-user-smoke
```

## Release Check

Before cutting a local release branch or tag, run:

```sh
just release-check
```

`release-check` runs the backend repository quality gate, including the CLI
scaffolded-host check.
