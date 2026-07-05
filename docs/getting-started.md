# Getting Started

This guide is the first-user happy path for running Lenso locally and installing
a service that provides a module. It avoids marketplace hardening flows; you
choose a manifest URL, install it, restart services, and inspect the loaded
module.

## Prerequisites

- Rust toolchain compatible with the workspace.
- `just`.
- Docker for local Postgres.

For a blank Rust module-authoring project outside this repository, install the
facade crate from crates.io:

```sh
cargo add lenso@0.3.16
```

That crate exposes serializable module declarations and manifest linting. Local
host development in this backend repository still uses the workspace crates.

For a blank host project, install the standalone CLI and scaffold the starter:

```sh
cargo install lenso-cli
lenso host init ../my-lenso-app
cd ../my-lenso-app
cp .env.example .env
lenso console update
docker compose up -d postgres
cargo run --bin migrate
cargo run --bin api
```

Run `cargo run --bin worker` in a second shell. The template depends on the
crates.io `lenso` package with the `host` feature enabled. `lenso::host` wraps
the API, worker, migration boot helpers, and a narrow linked HTTP route
authoring surface; generated hosts should pin a crate version for reproducible
builds. The starter exposes `GET /v1/app/status` plus `GET`/`POST
/v1/app/items` as the first host-owned linked routes and data surface.
`lenso console update` downloads the published Runtime Console artifact
and installs it under `.lenso/console`, so `cargo run --bin api` serves
`/console` without requiring Node.js or pnpm in the host project.

## Try The Audit Log Module

For a minimal first-party module smoke in a generated host, install
`audit-log` by name from the official catalog before starting the app:

```sh
lenso host init ../my-lenso-audit-app
cd ../my-lenso-audit-app
cp .env.example .env
lenso module install audit-log
lenso console update
lenso serve
```

Open `http://127.0.0.1:3000/console/modules?module=audit-log` and confirm the
module detail shows the Data Surfaces panel with Audit Events. Open
`http://127.0.0.1:3000/console/data` to inspect the same module data through the
generic admin-data view. The module declares `audit_log.events.read`; grant that
scope to real Console users when they need to read audit event rows.

## Enable Auth Redis Sessions In A Host

Generated hosts can opt into Redis-backed auth session lookup through the auth
module's install profile:

```sh
lenso module install auth --profile redis-session-cache
```

That descriptor-owned profile updates the host `Cargo.toml` so
`lenso-module-auth` builds with `features = ["redis"]`, writes
`REDIS_URL=redis://localhost:6379/0` to `.env`, and records the runtime default
`auth.session_cache=redis` in `.lenso/runtime-config-defaults.json`.

Redis is still an external service decision. The starter Docker Compose file
starts Postgres only, so add or provide Redis before restarting the API and
worker. If `auth.session_cache=redis` is active without `REDIS_URL`, host boot
fails validation instead of silently falling back to Postgres.

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

In production, Runtime Console access uses the host's real auth path. Sign in
with password auth or OIDC, then grant the auth user `console.admin` with
`lenso console bootstrap-admin`. Do not embed service tokens in a browser
Console build.

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

Generated hosts serve the Runtime Console at `/console` after
`lenso console update` installs it under `.lenso/console`. When developing
this repository from source, run the Runtime Console from the sibling
`../lenso-runtime-console` repository or run `just console-build-host <host-root>`.

## Install The Example Service

User-facing examples live in
[LioRael/lenso-examples](https://github.com/LioRael/lenso-examples). This
backend repository does not ship JavaScript example modules or manage their
package dependencies.

Clone and start the example service in a separate checkout:

```sh
git clone https://github.com/LioRael/lenso-examples ../lenso-examples
```

Start the support-ticket service from the `lenso-examples` repository,
then install its manifest here with the same command a user would run.

Install its manifest:

```sh
lenso service install http://127.0.0.1:4110/lenso/service/v1/manifest
```

When the provider is already installed and you want to roll a packaged update,
plan it before applying it:

```sh
lenso service release plan support-suite-provider \
  ../lenso-examples/dist/lenso-service/support-suite-provider/lenso.service-package.json \
  --output .lenso/support-suite-provider.release-plan.json
lenso service policy check .lenso/support-suite-provider.release-plan.json --fail-on breaking
lenso service release apply .lenso/support-suite-provider.release-plan.json
```

The apply step records `.lenso/service-releases.json`; Console Services shows
the latest release risk and recent provider history.

Restart the local services and open the Runtime Console. The `support-ticket`
module should be available through the Modules/Data surfaces, with
`support-suite-provider` shown as its service provider.

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

`release-check` runs the backend repository quality gate without slow smoke
checks. Run the CLI repository's starter smoke checks when touching the
standalone scaffolder.
