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
cargo add lenso@0.1.0
```

That crate exposes serializable module declarations and manifest linting. Local
host development in this backend repository still uses the workspace crates.

For a blank host project, start from the transitional starter template:

```sh
cp -R templates/starter-host ../my-lenso-host
cd ../my-lenso-host
cp .env.example .env
docker compose up -d postgres
cargo run --bin migrate
cargo run --bin api
```

Run `cargo run --bin worker` in a second shell. The template depends on this
backend repository's temporary `lenso-host` Git package until a stable public
host feature is available from the `lenso` crate. `lenso-host` only wraps the
API, worker, migration boot helpers, and a narrow linked HTTP route authoring
surface; it is not the final public package boundary. Pin it to a tag or commit
before using the starter outside local experiments. The starter exposes
`GET /v1/app/status` plus `GET`/`POST /v1/app/items` as the first host-owned
linked routes and data surface.

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
the first-party auth modules and platform story surface. Starter hosts can use
`LENSO_COMPOSITION_PROFILE=core` and explicitly install auth modules through
their host composition. Non-local environments must set
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

Start the API and worker in separate shells:

```sh
just api
just worker
```

Run the Runtime Console from the sibling `../lenso-runtime-console` repository
when you need the frontend.

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
