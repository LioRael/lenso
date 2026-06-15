# Lenso Starter Host

Minimal host application skeleton for running Lenso as a backend framework from
a blank Rust project.

This template is intentionally transitional. It depends on the backend
repository's host crates through Git dependencies while the stable public host
facade is still being designed. The included Cargo config uses the system Git
client so private repository credentials work the same way as normal `git clone`
commands. Pin those dependencies to a tag or commit before using this outside
local experiments.

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

## Files

- `src/bin/migrate.rs` applies platform and module migrations.
- `src/bin/api.rs` starts the Axum API host.
- `src/bin/worker.rs` starts the outbox relay and runtime worker.
- `docker-compose.yml` starts local Postgres.
- `.env.example` keeps local defaults explicit.
