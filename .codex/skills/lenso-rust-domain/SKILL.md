---
name: lenso-rust-domain
description: Use when changing Lenso Rust code in apps/api, apps/worker, apps/migrate, crates/platform-*, crates/app-bootstrap, or domains/*, especially when adding module capabilities, HTTP routes, repositories, migrations, runtime jobs, outbox events, admin data, platform primitives, or architecture-level Rust behavior.
---

# Lenso Rust Domain

## Purpose

Use this skill for Rust changes in Lenso's modular monolith. Keep domain boundaries explicit, prefer existing platform crates, and validate with the narrowest Rust and architecture checks that match the change.

## First Read

For architecture-level changes, read:

- `docs/architecture/overview.md`
- `docs/architecture/rules.md`

Inspect local diffs before editing files that are already modified:

```sh
git status --short
git diff -- <path>
```

## Repository Shape

- `apps/api`: Axum HTTP API. OpenAPI document-level metadata and router assembly live in `apps/api/src/openapi.rs`; per-endpoint contracts are `#[utoipa::path]` annotations on the handlers themselves.
- `apps/worker`: background worker and outbox relay composition.
- `apps/migrate`: deterministic migration runner.
- `crates/platform-*`: shared primitives for config, HTTP, runtime, module contracts, admin backends, testing, migrations, outbox, errors, health, and telemetry.
- `crates/platform-module`: module contracts. `ModuleManifest` is owned, serializable data; `ModuleBinding` is a narrow behavior seam; `LinkedBinding` is the current compile-time source; `AdminDataSource` is the schema-admin read seam.
- `crates/platform-admin`: Runtime Console observability backend (`/admin/runtime/*`) with no domain dependencies.
- `crates/platform-admin-data`: schema-admin backend (`/admin/data/*`) that reads generic module data via `AdminSurface::Schema` + `AdminDataSource`, with no domain dependencies.
- `crates/app-bootstrap`: composition root that enumerates concrete modules for both the API and the worker.
- `domains/*`: business capabilities with vertical domain structure.

## Domain Rules

Use the flat domain layout:

```text
domains/{domain}/
  migrations/
  src/
    lib.rs
    config.rs
    module.rs
    public.rs
    routes/
    dto/
    commands/
    queries/
    models/
    repositories/
    events/
    jobs/
    runtime/
    tests/
```

Do not add domain folders named `api`, `application`, `domain`, or `infrastructure`.

Do not import another domain's internals from domain source code. Use stable public interfaces for synchronous calls and events for asynchronous cross-domain work. Shared behavior belongs in `crates/platform-*`, not in a business domain.

Register modules only in `crates/app-bootstrap`, never by hand-wiring in `apps/*`. A domain's module contribution is exposed from its `module.rs` as a `Module`/`ModuleManifest` using `platform-module`; HTTP routes are merged via `app-bootstrap::merge_domain_http`; story-display metadata via `story_display_descriptors`.

Keep module data and behavior split. Pure declarations such as name, capabilities, story-display metadata, and `AdminSurface::Schema` belong in `ModuleManifest`; source-specific behavior belongs behind narrow traits such as `ModuleBinding`; admin record reads belong behind `AdminDataSource`.

Do not add concrete domain dependencies to `platform-admin` or `platform-admin-data`. `platform-admin` observes runtime/outbox/story tables; `platform-admin-data` serves schema-admin data through injected module registries.

## Implementation Workflow

1. Find the owning domain or platform crate before adding abstractions.
2. Keep business logic inside `domains/*`; keep runtime registration in the module binding, jobs, or `runtime/`.
3. Use explicit SQL and existing migration patterns for schema changes.
4. For commands that write data and emit events, write both inside the same Postgres transaction using the transactional outbox.
5. Keep error responses aligned with the platform error model.
6. Add or update tests near the changed behavior.

## Validation

Run the narrowest meaningful check first:

```sh
cargo check --locked -p <package> --all-targets
cargo test --locked -p <package>
```

For boundary, contract, generated-artifact, or cross-domain changes, run:

```sh
just arch-check
```

For broad Rust changes, run:

```sh
just rust-check
just test
```

If contracts or generated SDK output are affected, use `$lenso-contracts-sdk`.
