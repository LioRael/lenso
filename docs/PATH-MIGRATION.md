# Local repository naming follow-up — 2026-10-08

Canonical directory: `/Users/leosouthey/Projects/framework/lenso`. Branch: `feat/typescript-v1`. The earlier implementation commit `6fae8cc` remains in its history. No remote repository, push, main merge, credentials or product dependency references were changed.

## Preflight and safe move

Before the move, `framework/lenso-ts` was the new TypeScript repository, clean and at `6fae8cc`, with an actual `.git` directory and one checkout. There were no linked worktrees. Both `framework/lenso` and `framework/lenso-rust` were absent (including symlink conflict checks).

The Rust repository described at the old `framework/lenso` path was not present. Consequently there was no local Rust repository to move, no Rust origin to update and no Rust-linked worktrees available to validate. No old repository was overwritten or merged into the new repository. This is an observed limitation of the specified local paths, not a claim that the old repository was cleaned up.

An example dev supervisor/server was active at the former TypeScript path (PIDs 67388/67395). After confirming its exact command and listener, it was shut down through the supervisor's normal SIGTERM cleanup and both processes were confirmed gone. The entire standalone repository, including `.git`, history, generated outputs and installed dependencies, was moved to the unused canonical directory. `git worktree repair`, `git worktree list --porcelain`, `git rev-parse` and object integrity inspection then confirmed the new root and preserved branch/history. No linked worktree relocation was necessary.

## Active naming and historical evidence

README now starts commands with the canonical directory. The root workspace name and lockfile metadata are `lenso-workspace`. Plan and current verification links use the canonical path. Scripts/examples derive paths from their current directory or `import.meta` and needed no absolute-path rewrite.

Prior feedback timings, PIDs, consumer results and raw logs remain unchanged. The initial verification table explicitly identifies them as pre-migration evidence. Historical former-directory strings are retained solely for traceability; they are not a current product name or instructions. This follow-up did not rerun or replace the earlier 517/412 ms business-edit measurements.

## Fresh checks at framework/lenso

- Regular install and frozen install passed. Frozen install was checked again after the lockfile's root name metadata update.
- Build and typecheck passed, including integration scripts. Eligible Turbo tasks used existing valid cache outputs.
- All 21 focused tests passed, 0 failures, 55 assertions.
- Actual built CLI returned `{"message":"Hello, Rename!","count":1}`.
- Independent fresh tarball consumers passed for SDK, CLI and Web; no workspace source aliases. Core-only consumer had no Web deps; real typed HTTP worked; erroneous client input was rejected by TypeScript; browser client bundle isolation passed (48,427 bytes). New consumer directory is recorded in the separate JSON result.
- `bun run dev` generated entries and started the server from the canonical path, PID 68077. `bun run client Rename` returned the same greeting and three actual ready plugin states.
- The verification supervisor/server (68070/68077) were stopped cleanly. Earlier supervisor/server remain gone; no port 3000 listener remains. Dev is currently stopped.
- Final Git worktree metadata has exactly one root at the canonical path. No remotes are configured.

Evidence: [migration record](../output/path-migration.json), [install/build/typecheck/test log](../output/path-migration-checks.log), [CLI](../output/path-migration-cli.json), [typed HTTP](../output/path-migration-http.log), [independent consumers](../output/path-migration-package-smoke.json).

No blocker remains for the new framework's name/path change. Legacy Rust migration/origin/worktree verification was not performed because that repository was absent at the specified path.
