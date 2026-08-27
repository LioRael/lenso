# AGENTS.md

This repository's `main` branch contains only Lenso vNext. The final v0.3.x
source is retained by the `lenso@0.3.47` tag and Git history.

## Start safely

- Read [`CONTEXT.md`](CONTEXT.md), [`docs/adr/README.md`](docs/adr/README.md),
  [`docs/architecture/lenso-vnext.md`](docs/architecture/lenso-vnext.md), and
  the relevant ADR 0030–0070 before changing architecture.
- Route product planning, Capability, Plugin, App Composition, and host-runtime
  work through the canonical [`skills/`](skills/) pack. Use `lenso-start` when
  ownership is unclear; see the [Agents and skills guide](docs/agents/skills.md)
  for installation and maintenance.
- Create vNext worktrees from the latest `origin/main` with
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
  Driver; keep host-specific Plugin execution in an Execution Adapter.
- Keep main-repository product ownership limited to `lenso-app-plan`,
  `lenso-kernel`, and Kernel-owned runtime conformance under ADR 0064. Do not
  add inward dependencies on a concrete Driver, Adapter, Capability, Plugin,
  CLI, or example.
- Kernel executes only immutable, completely resolved Plan Snapshots. It may
  apply only an ADR 0067 validated atomic Plan Transition between adjacent
  snapshots; discovery, installation, version selection, product policy,
  unvalidated graph mutation, and fallback provider behavior remain forbidden.
- Use the canonical terms Host, Plugin Root, App, Plugin, Plugin Instance,
  Capability, Port, Slot, App Composition, Plan Snapshot, Plan Transition,
  Reconciler, App Generation, Kernel, Runtime Driver, and Execution Adapter.
  App Definition and Module are retired public terms; existing `Module*` code
  identifiers are private migration details only.
- Do not reintroduce Service, Provider, System Plane, Console, Story, Auth,
  PostgreSQL, migration, Outbox, Workflow, release, digest, or compatibility
  crates into the Kernel workspace.

## Validation

Use the narrowest meaningful check, then run the full workspace gate for
cross-cutting runtime or workspace changes:

```sh
cargo fmt --all -- --check
cargo xtask check-core-repository-boundary
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
```

Keep hand-written Rust files navigable:

- Target 300–600 lines for an ordinary module. Treat 600 lines as a prompt to
  look for a named responsibility boundary, not as a reason to slice blindly.
- Keep a cohesive core module at or below 1000 lines. Crossing that limit
  requires an explicit architecture rationale and a committed split plan.
- Split by ownership, invariants, and change cadence. Prefer a small module
  interface over pass-through files or broad visibility.
- Generated files are exempt; generator source, fixtures, and tests are not.
- Run `cargo xtask check-rust-module-size`. Listed legacy debt may not grow,
  and new hand-written Rust files may not exceed 1000 lines.

The CI workflow is the source of truth for the portable WebAssembly checks.

## Changes and commits

Use `apply_patch` for focused edits and stage only requested files. Use
Conventional Commits with a concise imperative subject under 72 characters.
Do not hand-edit generated lockfiles when Cargo can regenerate them.

Do not add compatibility shims or a `legacy/` directory to make removed
v0.3.x code compile. If a retained behavior needs a vNext home, first state its
Interface and owner, then add the smallest deep Plugin or Adapter seam.
