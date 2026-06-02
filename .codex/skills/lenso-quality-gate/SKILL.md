---
name: lenso-quality-gate
description: Use when validating, reviewing, preparing commits, running CI-equivalent checks, choosing verification commands, or finalizing Lenso changes across Rust, contracts, generated SDK, Runtime Console, migrations, architecture guardrails, or Git staging and commit workflows.
---

# Lenso Quality Gate

## Purpose

Use this skill to choose verification commands and prepare clean final changes. The worktree may contain unrelated user edits; do not revert, reformat, stage, or commit unrelated files.

## Preflight

Before staging or broad formatting, inspect the worktree:

```sh
git status --short
git diff -- <path>
```

If touching a file that is already modified, preserve the existing change unless the user explicitly asks to replace it.

## Command Map

Use `just` from the repository root.

```sh
just fmt-check
just rust-check
just test
just generated-check
just arch-check
just sdk-check
just console-check
just check
just ci
```

Narrow checks:

```sh
cargo check --locked -p <package> --all-targets
cargo test --locked -p <package>
pnpm --dir apps/runtime-console run test
pnpm --dir apps/runtime-console run typecheck
pnpm --dir packages/ts-sdk run typecheck
```

## Verification Selection

- Rust-only behavior: `cargo test --locked -p <package>` or `just test` for broad changes.
- Architecture or domain boundaries: `just arch-check`.
- Generated contracts or SDK: `just generated-check`, then `just sdk-check` if SDK behavior changed.
- Runtime Console: `just console-check` for substantial UI changes.
- Cross-cutting Rust, contracts, SDK, console, or CI changes: `just check`.
- Pre-PR or when matching GitHub Actions matters: `just ci`.

## Generated Files

Generated files are committed but not hand-edited:

- `contracts/*`
- `packages/ts-sdk/src/generated/*`

If generated files differ, include both the source change and generated output.

## Commit Workflow

Only stage requested files:

```sh
git add <specific-paths>
git diff --cached --name-only
git diff --cached
```

Use concise Conventional Commits:

```text
<type>[optional scope]: <imperative summary>
```

Recommended types: `feat`, `fix`, `chore`, `docs`, `refactor`, `test`, `perf`.

Keep the subject under 72 characters when practical and do not end it with a period.
