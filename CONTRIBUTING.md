# Contributing to Lenso vNext

`next` is a clean vNext runtime workspace. The maintained v0.3.x implementation
is developed and released from `main`.

## Workflow

1. Read `CONTEXT.md`, the ADR index, and the relevant vNext architecture note.
2. Create a worktree from the latest `origin/next` with `wt switch --create`.
3. Keep the change behind a small Interface and an explicit Module, Driver, or
   Adapter seam.
4. Run the workspace checks below.
5. Commit with a Conventional Commit and open a pull request targeting `next`.

## Checks

```sh
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
```

The pull-request workflow also compile-checks the portable plan and Kernel for
`wasm32-unknown-unknown` and `wasm32-wasip2`.

## Scope

Do not restore v0.3.x Service, Provider, System Plane, Console, Story, Auth,
PostgreSQL, migration, release, or TypeScript Service Kit code to this branch.
If a feature needs one of those concepts, express it first as a vNext
Capability, ordinary Module, Execution Adapter, authoring tool, or a separate
repository.

## Commits

Use:

```text
<type>[optional scope]: <imperative summary>
```

Stage only files belonging to the requested change. Keep generated lockfile
changes together with the manifest change that caused them.
