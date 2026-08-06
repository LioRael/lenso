# Contributing to Lenso

Thanks for contributing. Lenso is a Rust-first modular monolith backend.
Console source lives in the sibling
`lenso-console` repository. This guide covers the backend workflow,
conventions, and quality gates for changes. For deeper context, read
[`README.md`](README.md), [`AGENTS.md`](AGENTS.md),
[`docs/architecture/overview.md`](docs/architecture/overview.md), and
[`docs/architecture/rules.md`](docs/architecture/rules.md).

## Prerequisites

- Rust toolchain compatible with the workspace (`rust-version = 1.94`).
- Cargo and Docker Compose for development commands.
- The sibling `../lenso-console` checkout for Console work.
- Docker if you want local Postgres.

## Development Workflow

1. **Branch** off `main` for your change.
2. **Make the change.** Keep edits scoped to the task — do not reformat, revert, or
   stage unrelated files (see [AGENTS.md](AGENTS.md#do-not-disturb-unrelated-work)).
3. **Regenerate artifacts** if you touched Rust/OpenAPI/event sources:
   ```sh
   cargo run --locked -p lenso-api-contracts --bin generate-contracts
   ```
4. **Verify** with the narrowest meaningful gate (see below).
5. **Commit** using Conventional Commits.
6. **Open a PR.** CI runs the explicit quality gate in `.github/workflows/ci.yml`;
   it must pass.

Typical local loop:

```sh
docker compose -f infrastructure/local/docker-compose.yml up -d postgres
cargo run --locked -p lenso-migrate
cargo run --locked -p lenso-api      # and lenso-worker in another shell
```

## Quality Gates

Run the narrowest verification that covers your change. For cross-cutting backend
changes to Rust, contracts, or CI, run the full backend gate.

| Command | Scope |
| --- | --- |
| `cargo check --locked --workspace --all-targets` | compile the Rust workspace |
| `cargo test --locked --workspace` | Rust workspace tests |
| `cargo test --locked -p lenso-api-contracts --test architecture` | architecture rules |
| `cargo test --locked -p lenso-api-contracts --test generated_artifacts` | committed contract freshness |
| `cargo run --locked -p lenso-api-contracts --bin generate-contracts` | regenerate committed artifacts |
| `.github/workflows/ci.yml` | exact CI quality gate |

## Architecture Rules

The owner integration tests and CI fail on:

- A root `tools/`, `scripts/`, or task-runner file.
- OpenAPI route invariants in the `lenso-api` integration tests.
- DDD/Clean Architecture folder names inside modules: `api`, `application`,
  `domain`, `infrastructure`.
- Cross-module imports inside module source code.
- Missing or stale OpenAPI / contract artifacts.
- Missing event payload contracts for current events.

When working in Rust:

- Keep the workspace locked with `cargo ... --locked`.
- Prefer existing platform crates over new shared abstractions.
- Keep modules vertical and capability-oriented; no cross-module imports.
- Prefer explicit SQL and existing migration patterns.
- Keep error responses aligned with the platform error model and committed schemas.

## Generated Artifacts

Generated files are committed but **must not be hand-edited**. Update the source,
then regenerate:

1. Edit Rust/OpenAPI/event sources.
2. Run `cargo run --locked -p lenso-api-contracts --bin generate-contracts`.
3. Compare the generated output with the committed artifacts before finishing.

Contract artifacts live under `contracts`. Always include the source change and
regenerated output in the same commit.

## Console

The console lives in the sibling `../lenso-console` repository. Backend
changes may still affect Console contracts through `/admin/runtime/*`,
`/admin/data/*`, `ModuleManifest.console`, and generated OpenAPI output.
When changing those contracts, update and verify the frontend repository in the
same local workspace.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/):

```text
<type>[optional scope]: <imperative summary>
```

Types: `feat`, `fix`, `chore`, `docs`, `refactor`, `test`, `perf`.

- Keep the subject under 72 characters and lowercase the type.
- Do not end the subject with a period.
- Add a body only when the reason or migration note isn't obvious from the diff.
- Stage only files that belong to the change — use targeted `git add` paths and
  inspect `git diff --cached --name-only` before committing.

Examples:

- `feat(console): drill into story heatmap cells`
- `fix(api): preserve request correlation ids`
- `docs: add contributor guide`

## Pull Requests

- Keep PRs focused on a single concern.
- Ensure the explicit CI quality gate passes locally before requesting review.
- Include source and regenerated artifacts together when generated files change.
- Describe the change, the verification you ran, and any migration notes.
