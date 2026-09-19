# AGENTS.md

This repository's `main` branch contains only Lenso vNext. The final v0.3.x
source is retained by the `lenso@0.3.47` tag and Git history.

## Start safely

- Read [`CONTEXT.md`](CONTEXT.md), [`docs/adr/README.md`](docs/adr/README.md),
  [`docs/architecture/lenso-vnext.md`](docs/architecture/lenso-vnext.md), and
  the relevant ADR 0030–0074 before changing architecture.
- Route product planning, Capability, Plugin, App Composition, and host-runtime
  work through the canonical [`skills/`](skills/) pack. Use `lenso-start` when
  ownership is unclear; see the [Agents and skills guide](docs/agents/skills.md)
  for installation and maintenance.
- Create vNext worktrees from the latest `origin/main` with
  `wt switch --create`; do not edit the primary worktree when an isolated
  worktree is available.
- Preserve unrelated dirty work. Inspect `git status` and diffs before
  touching an overlapping file.
- Run Rust commands directly with the repository's configured Cargo toolchain
  and standard Cargo configuration.

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

Use the narrowest meaningful check for the changed behavior. Rust changes may
need formatting, Clippy, affected package checks, or affected tests; workflow,
script, Land-skill, and documentation changes use focused syntax, link, and
configuration checks. The candidate `quality` job remains the authoritative
native/WASM proof when the final change needs the full workspace gate.

Keep hand-written Rust files navigable:

- Target 300–600 lines for an ordinary module. Treat 600 lines as a prompt to
  look for a named responsibility boundary, not as a reason to slice blindly.
- Split by ownership, invariants, and change cadence. Prefer a small module
  interface over pass-through files or broad visibility.
- Generated files are exempt; generator source, fixtures, and tests are not.

The CI workflow is the source of truth for the portable WebAssembly checks.

## Delta delivery

- Delta is an optional delivery path. When a task uses Delta, develop, review,
  and land through its managed checkout; the repository's delivery path does
  not use GitHub pull requests. Contributors and maintainers using another
  agent or plain Git follow [`CONTRIBUTING.md`](CONTRIBUTING.md) instead.
- Use a Delta-managed checkout directly. Do not create a nested Worktrunk
  worktree inside it; Worktrunk instructions apply to Codex-managed workspaces.
- The detailed landing procedure is
  [`.agents/skills/land/SKILL.md`](.agents/skills/land/SKILL.md). A final
  candidate is based on the current `origin/main`, has a recorded full base
  SHA, and is pushed once to a unique `delta/verify/<task>/<attempt>` ref.
- Accept candidate CI only when the `CI` workflow was triggered by that
  candidate ref and its `quality` job succeeded for the exact candidate SHA.
  The `quality` job includes the native workspace checks and both portable
  WebAssembly proofs; local results are not substitutes for its GitHub status.
- Choose local checks by changed-file class. Workflow, executable-script,
  Land-skill, build-configuration, and unknown-path changes receive focused
  syntax/configuration checks plus the final `quality` gate; do not repeat a
  full workspace gate when the changed behavior does not require it.
- Fetch `origin/main` again after candidate CI. If it advanced and the
  candidate is not already reachable from it, integrate the candidate with the
  new base and repeat review and CI. If the candidate is already reachable,
  keep its SHA unchanged and record the current remote tip. Otherwise
  fast-forward the exact verified SHA to `main`, then read the remote SHA back.
  Verify the candidate remains an ancestor of the remote tip. Use normal
  pushes only; never force-push or rewrite a verified commit.
- Landing is separate from publication and deployment. The
  `.github/workflows/release-plz.yml` workflow is dispatch-only: its default
  mode is a read-only dry-run for an explicitly supplied landed SHA and
  release set. Publishing requires a separate, explicit version authorization;
  this migration does not publish packages.

## Changes and commits

Use `apply_patch` for focused edits and stage only requested files. Use
Conventional Commits with a concise imperative subject under 72 characters.
Do not hand-edit generated lockfiles when Cargo can regenerate them.

Do not add compatibility shims or a `legacy/` directory to make removed
v0.3.x code compile. If a retained behavior needs a vNext home, first state its
Interface and owner, then add the smallest deep Plugin or Adapter seam.
