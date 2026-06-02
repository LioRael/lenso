---
name: lenso-rust-domain
description: Use when changing Lenso Rust code in apps/api, apps/worker, apps/migrate, crates/platform-*, or domains/*, especially when adding domain capabilities, HTTP routes, repositories, migrations, runtime jobs, outbox events, platform primitives, or architecture-level Rust behavior.
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
- `crates/platform-*`: shared primitives for config, HTTP, runtime, the `DomainDescriptor` type, testing, migrations, outbox, errors, health, and telemetry.
- `crates/app-bootstrap`: composition root that enumerates the concrete domains for both the API and the worker.
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

Register domains only in `crates/app-bootstrap`, never by hand-wiring in `apps/*`. A domain's non-HTTP contributions go through a `DomainDescriptor` (`platform-domain`) returned from its `module.rs`; HTTP routes are merged via `app-bootstrap::merge_domain_http`; story-display metadata via `story_display_descriptors`.

## Implementation Workflow

1. Find the owning domain or platform crate before adding abstractions.
2. Keep business logic inside `domains/*`; keep runtime registration in the domain `DomainDescriptor`, jobs, or `runtime/`.
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
