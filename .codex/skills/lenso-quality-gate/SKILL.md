---
name: lenso-quality-gate
description: Use when validating, reviewing, preparing commits, handling short Lenso git requests like "提交 git", "提交这部分", or "合并进 main", running CI-equivalent checks, choosing verification commands, or finalizing changes across Rust, contracts, generated SDK, Runtime Console, migrations, architecture guardrails, staging, commits, and local merge workflows.
---

# Lenso Quality Gate

## Purpose

Use this skill to choose verification commands and prepare clean final changes. The worktree may contain unrelated user edits; do not revert, reformat, stage, or commit unrelated files.

Short requests have project-specific meaning:

- `提交 git`, `commit`, or `提交相关文件的 git`: inspect status, verify the coherent scope, stage the intended files, commit, and check final status.
- `提交这部分的 git`: make a selective/path-scoped commit and leave unrelated changes untouched.
- `合并进 main`, `合一下吧`, or `合并到 main`: finish local integration after validation, not just report branch state.

## Preflight

Before staging or broad formatting, inspect the worktree:

```sh
git status --short
git diff --stat
git diff --name-only
git diff -- <path>
git branch --show-current
```

If touching a file that is already modified, preserve the existing change unless the user explicitly asks to replace it.

Before remote-based merge commands, check whether the repo actually has a remote and tracking branch:

```sh
git remote -v
git rev-parse --abbrev-ref --symbolic-full-name @{u}
```

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

## Staging And Commit Workflow

1. Run the narrowest meaningful gate before staging.
2. Re-check `git status --short`; formatters or generators may have changed the candidate set.
3. Stage by intent:
   - Full coherent commit: `git add <specific paths for the validated scope>`.
   - Selective commit: use path-specific `git add` only.
4. Validate the staged set:

```sh
git diff --cached --name-only
git diff --cached
git diff --cached --check
```

5. Commit with a concise Conventional Commit subject that matches the actual staged scope.
6. Check post-commit status and call out any unrelated remaining changes.

If `.git/index.lock` errors occur in this environment, treat them as git metadata permission problems, not source changes.

## Merge-To-Main Workflow

1. Validate the feature branch/worktree first.
2. Confirm the integration target and remote/tracking setup.
3. If there is no `origin` or tracking branch, use a local merge flow instead of forcing `git pull`.
4. Merge from the main worktree or transfer the validated patch with `git apply --3way` only when a normal local merge is not available.
5. Run the relevant gate on the merged result.
6. Check final branch and worktree status.

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
