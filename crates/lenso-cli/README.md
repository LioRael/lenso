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

## Scaffold a module

```sh
lenso module create billing
```

Add `--with-console` when the linked module should also get a Runtime Console
workspace package:

```sh
lenso module create billing --with-console
```

For a standalone remote package:

```sh
lenso module create billing --remote --output-dir ../module-packages
```

The Runtime Console package generator is available directly as:

```sh
lenso console-package create billing
```

## Install a remote module

```sh
lenso module add https://example.com/lenso/module/v1/manifest
```

`module add` updates `REMOTE_MODULES`, writes the local console package install
plan, and applies Runtime Console package registration when the manifest declares
console packages. Use `--runtime-console-root` when the console app lives outside
the host repository, and `--no-console-plan` when you want to apply the plan
later with:

```sh
lenso console-package apply-plan
```
