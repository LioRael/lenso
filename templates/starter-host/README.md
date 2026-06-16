# Lenso Starter Host

Minimal host application skeleton for running Lenso as a backend framework from
a blank Rust project.

This template is intentionally transitional. It depends on the backend
repository's temporary `lenso-host` Git package while the stable public host
feature is still being designed for the `lenso` crate. The included Cargo
config uses the system Git client so private repository credentials work the
same way as normal `git clone` commands. Pin the dependency to a tag or commit
before using this outside local experiments.

The binary entrypoints are deliberately thin wrappers around the temporary
`lenso-host` facade. That keeps this template close to the future public host
feature without exposing the internal app or platform crates as its
user-facing API.

## Start

```sh
cp .env.example .env
docker compose up -d postgres
cargo run --bin migrate
cargo run --bin api
```

Run the worker in a second shell:

```sh
cargo run --bin worker
```

The API binds to `HTTP_HOST:HTTP_PORT` from `.env` and serves:

- `GET /health`;
- `GET /v1/app/status`;
- `GET /v1/app/items` for the authenticated user;
- `GET /v1/app/items/{id}` for the authenticated user;
- `PATCH /v1/app/items/{id}` for the authenticated user;
- `DELETE /v1/app/items/{id}` for the authenticated user;
- `POST /v1/app/items` for the authenticated user;
- `GET /openapi.json`;
- `GET /docs`;
- Runtime Console admin APIs under `/admin/*`;
- installed remote module HTTP proxies under `/modules/{module}/http/*`.

## Add A Remote Module

Start a module that exposes a Lenso manifest, then add it to `.env`:

```sh
REMOTE_MODULES=hello-action=http://127.0.0.1:4100/lenso/module/v1
```

Restart `api` and `worker` after changing module configuration.

User-facing remote-module examples live in
<https://github.com/LioRael/lenso-examples>.

## Add A Linked Module

Local Rust modules are registered from `src/lib.rs`:

```rust
use lenso_host::prelude::*;

HostBuilder::new()
    .linked_module(modules::app::linked_module())
    .build()
```

The included `src/modules/app` module is a project-owned skeleton. Rename it
or add modules beside it as your backend grows. It declares a small status
route, an `app.items` table, and item read/write routes so the module has
visible metadata and a real HTTP/data surface in the host registry;
replace them with your real application capabilities as the module grows.
The item table is intentionally app-owned and keyed by `owner_user_id`, which
comes from `ActorContext::User.user_id`. This is the pattern to use for product
profiles, accounts, and other user data instead of adding profile fields to
Lenso's auth anchor.

When the module owns tables, pass its migration list through
`HostLinkedModule::manifest_only(...)`.

The starter's `app` module already includes a first migration:

```text
src/modules/app/migrations/0001_create_app_schema.sql
```

Add application tables there or create another numbered migration beside it,
then run:

```sh
cargo run --bin migrate
```

Add HTTP routes through `src/modules/app/routes.rs`, declare their manifest
metadata in `src/modules/app/mod.rs`, then restart the API.

Create and read starter data:

```sh
curl -sS -X POST http://127.0.0.1:3000/v1/app/items \
  -H 'content-type: application/json' \
  -H 'authorization: Bearer dev-user:usr_demo' \
  -d '{"title":"first item"}' | jq .

curl -sS http://127.0.0.1:3000/v1/app/items \
  -H 'authorization: Bearer dev-user:usr_demo' | jq .

curl -sS http://127.0.0.1:3000/v1/app/items/1 \
  -H 'authorization: Bearer dev-user:usr_demo' | jq .

curl -sS http://127.0.0.1:3000/v1/app/items/1 \
  -H 'authorization: Bearer dev-user:usr_other' | jq .

curl -sS -X PATCH http://127.0.0.1:3000/v1/app/items/1 \
  -H 'content-type: application/json' \
  -H 'authorization: Bearer dev-user:usr_demo' \
  -d '{"title":"renamed item"}' | jq .

curl -sS -X DELETE http://127.0.0.1:3000/v1/app/items/1 \
  -H 'authorization: Bearer dev-user:usr_demo' | jq .
```

## Files

- `src/lib.rs` is the host-owned module composition hook.
- `src/modules/app` is the first project-owned linked module skeleton.
- `src/bin/migrate.rs` delegates to the host migration runner.
- `src/bin/api.rs` delegates to the host API runner.
- `src/bin/worker.rs` delegates to the host worker runner.
- `docker-compose.yml` starts local Postgres.
- `.env.example` keeps local defaults explicit.
