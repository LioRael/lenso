# AGENTS.md

Guidance for coding agents working in this repository.

## Project Shape

Lenso is a Rust-first modular monolith with a Vite/React Runtime Console and a generated TypeScript SDK.

- `apps/api`: Axum HTTP API.
- `apps/worker`: background worker and outbox relay.
- `apps/migrate`: deterministic migration runner.
- `apps/runtime-console`: Vite/React operator console.
- `crates/platform-*`: shared platform primitives for config, HTTP, runtime, module contracts, admin backends, testing, migrations, outbox, errors, health, and telemetry.
- `crates/platform-module`: module framework contracts: `ModuleManifest` for serializable data, `ModuleBinding` for behavior, `LinkedBinding` for compile-time modules, and `AdminDataSource` for schema-admin reads.
- `crates/platform-admin`: Runtime Console observability backend mounted under `/admin/runtime/*`; it observes runtime/outbox/story tables and must not depend on business domains.
- `crates/platform-admin-data`: schema-admin backend mounted under `/admin/data/*`; it serves generic module data through `AdminSurface::Schema` and `AdminDataSource`, with no domain dependencies.
- `crates/app-bootstrap`: composition root that enumerates concrete modules for the API and worker.
- `domains/*`: business capabilities. Domains should stay modular and avoid cross-domain imports.
- `contracts/*`: committed OpenAPI, JSON Schema, event, error, and runtime contracts.
- `packages/ts-sdk`: generated TypeScript SDK from `contracts/openapi/app-api.v1.yaml`.
- `tools/*`: generators and architecture checks.
- `infrastructure/local`: local Postgres and optional OpenTelemetry collector.

Read `docs/architecture/overview.md` and `docs/architecture/rules.md` before making architecture-level changes.

## Do Not Disturb Unrelated Work

The worktree may contain user changes. Do not revert, reformat, stage, or commit unrelated files. If a task requires touching a file that already has changes, inspect the diff first and preserve the user's work.

## Common Commands

Use `just` as the root task runner.

- `just`: list available recipes.
- `just install`: install pnpm dependencies for the SDK and Runtime Console.
- `just fmt`: format Rust and Runtime Console code.
- `just fmt-check`: check Rust and Runtime Console formatting.
- `just rust-check`: run `cargo check --locked --workspace --all-targets`.
- `just test`: run Rust workspace tests.
- `just generate`: regenerate contracts and the TypeScript SDK.
- `just generated-check`: regenerate committed artifacts and fail if they differ from git.
- `just arch-check`: run architecture guardrails.
- `just sdk-check`: typecheck `packages/ts-sdk`.
- `just console-check`: format-check, lint, typecheck, and build `apps/runtime-console`.
- `just check`: run the full local quality gate, excluding dependency installation.
- `just ci`: run the same quality gate used by GitHub Actions, including frozen pnpm installs.

For local services:

- `just db-up`: start local Postgres.
- `just migrate`: run migrations.
- `just api`: start the API on the configured HTTP host/port.
- `just worker`: start the worker.
- `just console`: run the Runtime Console with seeded data.
- `just console-api`: run the Runtime Console against `http://localhost:3000`.
- `just observability-up`: start Postgres plus the optional OpenTelemetry collector profile.
- `just down`: stop local infrastructure.

## Generated Artifacts

Generated files are committed but must not be edited by hand.

- Update Rust/OpenAPI/event sources first.
- Run `just generate`.
- Run `just generated-check` before finishing.
- Generated SDK files live under `packages/ts-sdk/src/generated`.
- Contract artifacts live under `contracts`.

If generated files change, include the source change and generated output together.

## Rust Guidelines

- Keep the workspace locked with `cargo ... --locked` for checks and tests.
- Prefer existing platform crates over new shared abstractions.
- Keep domain modules vertical and capability-oriented.
- Do not introduce DDD/Clean Architecture folder names inside domains: `api`, `application`, `domain`, or `infrastructure`.
- Do not add cross-domain imports inside domain source code.
- Register module wiring only in `crates/app-bootstrap`; platform crates must expose seams and stay free of concrete domain dependencies.
- Keep module data and behavior split: serializable declarations belong in `ModuleManifest`; source-specific behavior belongs behind narrow traits such as `ModuleBinding` and `AdminDataSource`.
- Prefer explicit SQL and existing migration patterns.
- Keep error responses aligned with the platform error model and committed schemas.

## Project Architecture Memory

Claude Code project memory was imported into Codex on 2026-06-03. Keep these design decisions current:

- Lenso is moving from a fixed modular monolith toward an installable module framework. The current source of truth is `platform-module`, not the older `platform-domain`/`DomainDescriptor` model.
- Step 1 is done: `DomainDescriptor` was split into owned, serializable `ModuleManifest` data plus narrow `ModuleBinding` behavior. Only `LinkedBinding` is implemented today; future `Remote` and `Wasm` loading sources should be added as new bindings/sources without collapsing the manifest/behavior split.
- Step 2 schema-admin is done as a read-only vertical slice. `AdminSurface::Schema(AdminSchema)` is manifest data; `AdminDataSource` is the behavior seam returning `serde_json::Value`; identity provides the first User schema/list/detail implementation.
- Do not re-add `#[non_exhaustive]` to producer-constructed structs `AdminSchema`, `EntitySchema`, `FieldSchema`, or `AdminPage`; it blocks struct literal construction from other crates. Keep it on consumer-matched enums such as `FieldType` and `AdminSurface`.
- `platform-admin` is runtime observability, not business CRUD. `platform-admin-data` is schema-admin business data. Both are platform crates and must not depend on concrete domains; `app-bootstrap` injects the module/data registries.
- OpenAPI is single-source through `utoipa-axum`: put `#[utoipa::path]` on real handlers and register routes with `OpenApiRouter::routes(routes!(handler))`. `apps/api/src/openapi.rs::openapi_document()` must stay pure and context-free because generators, arch checks, and sync tests call it outside a runtime.

## Runtime Console Guidelines

The console lives in `apps/runtime-console` and uses Vite, React, Tailwind CSS, TanStack Query/Router, Base UI, Lucide icons, Ultracite, Oxfmt, and Oxlint.

- Use `pnpm --dir apps/runtime-console ...` when running package scripts directly.
- Prefer existing UI primitives under `src/components/ui`.
- Keep operational screens dense, scannable, and workflow-focused.
- Do not add ESLint, Prettier, or Biome unless the project intentionally migrates away from Ultracite/Oxfmt/Oxlint.
- Validate substantial console changes with `just console-check`.

## TypeScript SDK Guidelines

The SDK in `packages/ts-sdk` is generated from OpenAPI.

- Do not hand-edit `packages/ts-sdk/src/generated`.
- Keep SDK checks reproducible through local package dependencies, not global tools.
- Use `just sdk-check` for typechecking.

## CI Expectations

GitHub Actions runs `just ci` on pull requests and pushes to `main`.

Before claiming work is complete, run the narrowest meaningful verification for the change. For cross-cutting changes to Rust, contracts, SDK, console, or CI, run `just ci`.

If a command fails because network access is blocked while installing dependencies, rerun the same command with the required approval rather than changing project files to work around the sandbox.

## Commit Guidance

Only stage files that belong to the requested change. Use targeted `git add` paths, inspect `git diff --cached --name-only`, and leave unrelated modified or untracked files alone.

Use concise Conventional Commits for new commits:

```text
<type>[optional scope]: <imperative summary>
```

Recommended types:

- `feat`: user-facing feature or capability.
- `fix`: bug fix or behavioral correction.
- `chore`: tooling, CI, dependencies, generated maintenance, or repository housekeeping.
- `docs`: documentation-only changes.
- `refactor`: code restructuring without intentional behavior changes.
- `test`: tests or test utilities.
- `perf`: performance improvement.

Keep the subject under 72 characters when practical, use lowercase type names, and do not end the subject with a period. Use a body only when the reason, migration note, or verification detail is not obvious from the diff.

Examples:

- `feat(runtime-console): add trace layout tests`
- `fix(api): preserve request correlation ids`
- `chore: align project workflows`
- `docs: add agent contributor guide`
