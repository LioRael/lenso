# AGENTS.md

This repository's `next` branch contains only Lenso vNext. `main` is the
separate v0.3.x maintenance and release line.

## Start safely

- Read [`CONTEXT.md`](CONTEXT.md), [`docs/adr/README.md`](docs/adr/README.md),
  [`docs/architecture/lenso-vnext.md`](docs/architecture/lenso-vnext.md), and
  the relevant ADR 0030–0057 before changing architecture.
- Create vNext worktrees from the latest `origin/next` with
  `wt switch --create`; do not edit the primary worktree when an isolated
  worktree is available.
- Preserve unrelated dirty work. Inspect `git status` and diffs before
  touching an overlapping file.
- Run Rust commands through
  `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo` when
  available so sibling worktrees share the configured target cache.

## Architecture rules

- Keep the portable Kernel independent of Tokio, OS APIs, network, filesystem,
  database, process, product, and release concerns.
- Keep authoring data in `lenso-app-plan`; keep host scheduling in a Runtime
  Driver; keep host-specific Module execution in an Execution Adapter.
- A Resolved App Plan is immutable and complete before boot. No runtime
  discovery, installation, graph mutation, dynamic rebinding, or fallback
  provider behavior.
- Use the canonical terms App, Module, Module Instance, Capability, Operation,
  App Composition, Resolved App Plan, Kernel, Runtime Driver, and Execution
  Adapter.
- Do not reintroduce Service, Provider, System Plane, Console, Story, Auth,
  PostgreSQL, migration, Outbox, Workflow, release, digest, or compatibility
  crates into the Kernel workspace.

## Validation

Use the narrowest meaningful check, then run the full workspace gate for
cross-cutting runtime or workspace changes:

```sh
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
```

The CI workflow is the source of truth for the portable WebAssembly checks.

## Changes and commits

Use `apply_patch` for focused edits and stage only requested files. Use
Conventional Commits with a concise imperative subject under 72 characters.
Do not hand-edit generated lockfiles when Cargo can regenerate them.

Do not add compatibility shims or a `legacy/` directory to make removed
v0.3.x code compile. If a retained behavior needs a vNext home, first state its
Interface and owner, then add the smallest deep Module or Adapter seam.
