# Contributing to Lenso

Thanks for contributing. Lenso is a Rust-first modular monolith with a Vite/React
Runtime Console and a generated TypeScript SDK. This guide covers the workflow,
conventions, and quality gates for changes. For deeper context, read
[`README.md`](README.md), [`AGENTS.md`](AGENTS.md),
[`docs/architecture/overview.md`](docs/architecture/overview.md), and
[`docs/architecture/rules.md`](docs/architecture/rules.md).

## Prerequisites

- Rust toolchain compatible with the workspace (`rust-version = 1.94`).
- [`just`](https://github.com/casey/just) as the root task runner.
- Node 24 and `pnpm` for the SDK and Runtime Console.
- Docker if you want local Postgres via `just db-up`.

Install frontend dependencies once:

```sh
just install
```

## Development Workflow

1. **Branch** off `main` for your change.
2. **Make the change.** Keep edits scoped to the task — do not reformat, revert, or
   stage unrelated files (see [AGENTS.md](AGENTS.md#do-not-disturb-unrelated-work)).
3. **Regenerate artifacts** if you touched Rust/OpenAPI/event sources:
   ```sh
   just generate
   ```
4. **Verify** with the narrowest meaningful gate (see below).
5. **Commit** using Conventional Commits.
6. **Open a PR.** CI runs `just ci`; it must pass.

Typical local loop:

```sh
just db-up
just migrate
just api      # and `just worker` for background work
just console  # Runtime Console with seeded data
```

## Quality Gates

Run the narrowest verification that covers your change. For cross-cutting changes
to Rust, contracts, SDK, console, or CI, run the full gate.

| Command | Scope |
| --- | --- |
| `just rust-check` | `cargo check --locked --workspace --all-targets` |
| `just test` | Rust workspace tests |
| `just arch-check` | architecture guardrails |
| `just sdk-check` | typecheck `packages/ts-sdk` |
| `just console-check` | format-check, lint, typecheck, build the console |
| `just generated-check` | regenerate artifacts and fail if they differ from git |
| `just check` | full local quality gate (no dependency install) |
| `just ci` | the exact gate GitHub Actions runs, with frozen pnpm installs |

Run `just` to list all recipes.

## Architecture Rules

The architecture checker (`just arch-check`) and CI fail on:

- DDD/Clean Architecture folder names inside domains: `api`, `application`,
  `domain`, `infrastructure`.
- Cross-domain imports inside domain source code.
- Missing or stale OpenAPI / contract artifacts.
- Stale generated TypeScript SDK files.
- Missing event payload contracts for current events.

When working in Rust:

- Keep the workspace locked with `cargo ... --locked`.
- Prefer existing platform crates over new shared abstractions.
- Keep domain modules vertical and capability-oriented; no cross-domain imports.
- Prefer explicit SQL and existing migration patterns.
- Keep error responses aligned with the platform error model and committed schemas.

## Generated Artifacts

Generated files are committed but **must not be hand-edited**. Update the source,
then regenerate:

1. Edit Rust/OpenAPI/event sources.
2. Run `just generate`.
3. Run `just generated-check` before finishing.

Generated SDK files live under `packages/ts-sdk/src/generated`; contract artifacts
live under `contracts`. Always include the source change and regenerated output in
the same commit.

## Runtime Console

The console lives in `apps/runtime-console` (Vite, React, Tailwind, TanStack
Query/Router, Base UI, Lucide, Ultracite, Oxfmt, Oxlint).

- Run scripts with `pnpm --dir apps/runtime-console ...`.
- Prefer existing UI primitives under `src/components/ui`.
- Keep operational screens dense, scannable, and workflow-focused.
- Do not add ESLint, Prettier, or Biome.
- Validate substantial changes with `just console-check`.

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

- `feat(runtime-console): drill into story heatmap cells`
- `fix(api): preserve request correlation ids`
- `docs: add contributor guide`

## Pull Requests

- Keep PRs focused on a single concern.
- Ensure `just ci` passes locally before requesting review.
- Include source and regenerated artifacts together when generated files change.
- Describe the change, the verification you ran, and any migration notes.
