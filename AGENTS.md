# AGENTS.md

Guidance for coding agents working in this repository.

Before planning, changing, or executing a framework release, read the authoritative
[Lenso release runbook](https://github.com/LioRael/lenso-release/blob/main/docs/release-runbook.md).
Do not infer production authority from repository write access or bypass the reviewed release plan.

## Project Shape

Lenso is a Rust-first modular monolith backend. Runtime Console source lives in the sibling `../lenso-runtime-console` repository.

- `crates/lenso-api`: Axum HTTP API.
- `crates/lenso-worker`: background worker and outbox relay.
- `crates/lenso-migrate`: deterministic migration runner.
- `crates/platform-*`: shared platform primitives for config, HTTP, runtime, module contracts, admin backends, testing, migrations, outbox, errors, health, and telemetry.
- `crates/lenso`: public Rust facade crate for serializable module-authoring declarations: `ModuleManifest`, admin surfaces, HTTP route metadata, runtime/event/lifecycle declarations, console surfaces, story display metadata, and manifest lints.
- `crates/platform-module`: internal module behavior seams plus compatibility re-exports: `ModuleBinding`, `LinkedBinding` for compile-time modules, `AdminDataSource` for schema-admin reads, and `AdminActionSource` for executable admin actions.
- `crates/platform-admin`: Runtime Console observability backend mounted under `/admin/runtime/*`; it observes runtime/outbox/story tables and must not depend on concrete modules.
- `crates/platform-admin-data`: schema-admin backend mounted under `/admin/data/*`; it serves generic module data through `AdminSurface::Schema` and `AdminDataSource`, with no concrete-module dependencies.
- `crates/lenso-bootstrap`: composition root that enumerates concrete modules for the API and worker.
- `modules/*`: product or fixture capabilities packaged as modules. Modules should stay vertical and avoid cross-module imports.
- `contracts/*`: committed OpenAPI, JSON Schema, event, error, and runtime contracts.
- `tools/*`: generators and architecture checks.
- `infrastructure/local`: local Postgres and optional OpenTelemetry collector.

Read `docs/architecture/overview.md` and `docs/architecture/rules.md` before making architecture-level changes.

## Do Not Disturb Unrelated Work

The worktree may contain user changes. Do not revert, reformat, stage, or commit unrelated files. If a task requires touching a file that already has changes, inspect the diff first and preserve the user's work.

## Common Commands

Use `just` as the root task runner.

- `just`: list available recipes.
- `just fmt`: format Rust code.
- `just fmt-check`: check Rust formatting.
- `just rust-check`: run `cargo check --locked --workspace --all-targets`.
- `just test`: run Rust workspace tests.
- `just generate`: regenerate contracts.
- `just generated-check`: regenerate committed artifacts and fail if they differ from git.
- `just arch-check`: run architecture guardrails.
- `just check`: run the default local quality gate, excluding dependency installation and slow smoke checks.
- `just smoke-check`: run slower scaffolded-host smoke checks.
- `just ci`: run the same quality gate used by GitHub Actions.

## Validation Strategy

Default to narrow validation during feature work. Run full `just check` locally
only for high-risk changes such as contracts, migrations, runtime or remote
module core paths, package/release gates, or when explicitly requested. For
normal feature slices, use focused local gates that match the changed surface
and rely on GitHub Actions for full workspace coverage.

For local services:

- `just db-up`: start local Postgres.
- `just migrate`: run migrations.
- `just api`: start the API on the configured HTTP host/port.
- `just worker`: start the worker.
- `just observability-up`: start Postgres plus the optional OpenTelemetry collector profile.
- `just down`: stop local infrastructure.

## Generated Artifacts

Generated files are committed but must not be edited by hand.

- Update Rust/OpenAPI/event sources first.
- Run `just generate`.
- Run `just generated-check` before finishing.
- Contract artifacts live under `contracts`.

If generated files change, include the source change and generated output together.

## Rust Guidelines

- Keep the workspace locked with `cargo ... --locked` for checks and tests.
- Prefer existing platform crates over new shared abstractions.
- Keep modules vertical and capability-oriented.
- Do not introduce DDD/Clean Architecture folder names inside modules: `api`, `application`, `domain`, or `infrastructure`.
- Do not add cross-module imports inside module source code.
- Register module wiring only in `crates/lenso-bootstrap`; platform crates must expose seams and stay free of concrete-module dependencies.
- Keep module data and behavior split: serializable declarations belong in `ModuleManifest`; source-specific behavior belongs behind narrow traits such as `ModuleBinding` and `AdminDataSource`.
- Prefer explicit SQL and existing migration patterns.
- Keep error responses aligned with the platform error model and committed schemas.

## Project Architecture Memory

Claude Code project memory was imported into Codex on 2026-06-03. Keep these design decisions current:

- Lenso is moving from a fixed modular monolith toward an installable module framework. The current declaration source of truth is the public `lenso` facade crate plus `platform-module` behavior seams, not the older `platform-domain`/`DomainDescriptor` model.
- Step 1 is done: `DomainDescriptor` was split into owned, serializable `ModuleManifest` data plus narrow `ModuleBinding` behavior. `ModuleManifest` and pure declarations now live in `crates/lenso`; `platform-module` re-exports them for workspace compatibility and owns behavior seams. Only `LinkedBinding` is implemented today; future `Remote` and `Wasm` loading sources should be added as new bindings/sources without collapsing the manifest/behavior split.
- Step 2 schema-admin is done as a read-only vertical slice. `AdminSurface::Schema(AdminSchema)` is manifest data; `AdminDataSource` is the behavior seam returning `serde_json::Value`; auth provides the current User schema/list/detail implementation.
- Do not re-add `#[non_exhaustive]` to producer-constructed structs `AdminSchema`, `EntitySchema`, `FieldSchema`, or `AdminPage`; it blocks struct literal construction from other crates. Keep it on consumer-matched enums such as `FieldType` and `AdminSurface`.
- `platform-admin` is runtime observability, not business CRUD. `platform-admin-data` is schema-admin business data. Both are platform crates and must not depend on concrete modules; `lenso-bootstrap` injects the module/data registries.
- OpenAPI is single-source through `utoipa-axum`: put `#[utoipa::path]` on real handlers and register routes with `OpenApiRouter::routes(routes!(handler))`. `crates/lenso-api/src/openapi.rs::openapi_document()` must stay pure and context-free because generators, arch checks, and sync tests call it outside a runtime.
- Durable Workflow compensation persists completed effects and deterministic compensation order in the owning Service Store. Compensation request publication is atomic with local dispatch state, remote reversal remains inside the remote Service Inbox transaction, and the Workflow reaches `compensated` only after a declared completion Event confirms the stable effect and compensation identities. Rejected compensation uses the distinct `compensation_failed` state with intervention evidence; no distributed transaction is introduced.

## Runtime Console Guidelines

The Runtime Console is developed in the sibling `../lenso-runtime-console`
repository. This backend repository owns the admin APIs, generated contracts,
and `ModuleManifest.console` declarations consumed by that frontend.

- Keep backend `ModuleManifest.console` data aligned with the frontend package surface contracts.
- Validate substantial frontend changes inside `../lenso-runtime-console`.

## CI Expectations

GitHub Actions runs `just ci` on pull requests and pushes to `main`.

Before claiming work is complete, run the narrowest meaningful verification for the change. For cross-cutting backend changes to Rust, contracts, or CI, run `just ci`. For changes that affect Runtime Console behavior, also run the relevant checks in `../lenso-runtime-console`.

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

## Agent skills

### Issue tracker

Issues and PRDs are tracked in this repository's GitHub Issues. See `docs/agents/issue-tracker.md`.

### Triage labels

Triage uses the five default canonical labels. See `docs/agents/triage-labels.md`.

### Domain docs

Domain documentation uses a single-context layout. See `docs/agents/domain.md`.
