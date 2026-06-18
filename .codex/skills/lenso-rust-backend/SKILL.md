---
name: lenso-rust-backend
description: Use when changing Lenso Rust backend code in crates/lenso-api, crates/lenso-worker, crates/lenso-migrate, crates/platform-*, crates/lenso-bootstrap, domains/*, migrations, runtime/outbox workers, module registration, admin/runtime backends, remote modules, or architecture-level Rust behavior.
---

# Lenso Rust Backend

## Purpose

Use this skill for Rust backend work across the Lenso service kit, apps, module framework, domains, migrations, runtime workers, and admin backends. It replaces the older `lenso-rust-domain` skill, which was too narrow for the current module-framework shape.

## First Read

For architecture-level changes, read:

- `docs/architecture/overview.md`
- `docs/architecture/rules.md`

If the task touches specific module-framework areas, also use:

- `$lenso-module-framework` for `platform-module`, `platform-admin-data`, manifests, bindings, admin surfaces, and lenso-bootstrap module registration.
- `$lenso-remote-modules` for `platform-module-remote`, remote manifests, HTTP proxying, remote runtime functions, or remote/custom admin surfaces.
- `$lenso-contracts-sdk` for OpenAPI, generated contracts, event schemas, runtime function contracts, or SDK output.
- `$lenso-quality-gate` when choosing checks, staging, committing, or merging.

Inspect local diffs before editing files that may already contain user work:

```sh
git status --short
git diff -- <path>
```

## Repository Shape

- `crates/lenso-api`: Axum HTTP API, OpenAPI assembly, and mounted public/admin/remote routers.
- `crates/lenso-worker`: outbox relay, event dispatch, and runtime worker loop.
- `crates/lenso-migrate`: deterministic migration runner.
- `crates/platform-*`: shared platform primitives, runtime, module contracts, admin backends, testing, migrations, outbox, errors, health, telemetry, and remote module support.
- `crates/lenso-bootstrap`: the composition root that enumerates linked and configured remote modules for the API and worker.
- `domains/*`: business capabilities with vertical Rust structure and no cross-domain internal imports.
- `contracts/*` and `packages/ts-sdk`: committed generated artifacts; do not hand-edit generated output.

## Hard Rules

- Do not create DDD or Clean Architecture domain folders named `api`, `application`, `domain`, or `infrastructure`.
- Do not import another domain's internals from domain source code. Use stable public interfaces for synchronous calls and events/runtime functions for asynchronous work.
- Register concrete modules only in `crates/lenso-bootstrap`; platform crates expose seams and must not depend on business domains.
- Keep module data and behavior split. Serializable declarations belong in `ModuleManifest`; source-specific behavior belongs behind narrow traits such as `ModuleBinding`; admin record reads belong behind `AdminDataSource`.
- Keep `platform-admin` as runtime observability and `platform-admin-data` as schema-admin business data. Neither crate should depend on concrete domains.
- Keep OpenAPI single-source through `utoipa-axum`: annotate real handlers and register them with `OpenApiRouter::routes(routes!(handler))`.
- Keep runtime business behavior in the owning domain or module implementation; `platform-runtime` owns queues, retries, persistence, and orchestration, not business logic.
- Use explicit SQL and existing migration patterns.

## Workflow

1. Find the owning domain, platform crate, or app before adding abstractions.
2. Read nearby tests and established module wiring before editing.
3. Update source code before generated artifacts.
4. For commands that write data and emit events, keep the write and outbox insert in the same Postgres transaction.
5. Preserve request/correlation/causation context through runtime, outbox, and admin surfaces.
6. Add or update tests near the changed behavior.
7. Run the narrowest meaningful check before broader gates.

## Validation

Use package-level Rust checks while iterating:

```sh
cargo check --locked -p <package> --all-targets
cargo test --locked -p <package>
```

Use repo gates for boundary or cross-layer work:

```sh
just arch-check
just rust-check
just test
just check
```

If contracts or SDK output are affected, use `$lenso-contracts-sdk`. If preparing a commit or merge, use `$lenso-quality-gate`.
