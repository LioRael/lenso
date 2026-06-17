# lenso-cli

Command-line interface for the Lenso backend framework.

## Install

```sh
cargo install lenso-cli
```

## Scaffold a host application

```sh
lenso host init my-app
cd my-app
cp .env.example .env
docker compose up -d postgres
cargo run --bin migrate
cargo run --bin api
```

The package name defaults to the target directory name and can be overridden with
`--name`. Pass `--force` to scaffold into a non-empty directory.

The generated host depends on the transitional `lenso-host` crate while the
stable public host facade is still being designed. See
[`docs/architecture/framework-public-surface.md`](../../docs/architecture/framework-public-surface.md)
for the host-facade roadmap.
