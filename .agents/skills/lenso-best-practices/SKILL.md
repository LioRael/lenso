---
name: lenso-best-practices
description: Choose or review repository structure and boundaries for Lenso framework work, business applications, Rust/Go native services, Runtime integration, and Console composition. Use for cross-repository design or best-practice guidance; use the focused development skill for a single API edit.
---

# Lenso repository practices

Favor the smallest native design that satisfies the task. Ordinary functions own business behavior; plugins wire exact dependencies and resources; entry adapters reuse those services. Local calls do not need contracts or Runtime. These are defaults with escape hatches, not a directory policy or authorization to restructure an entire repository.

1. Identify the consuming repository, installed/pinned revision, existing entry, resource owners and actual requested change. Check the relevant source ledger before treating another branch as available.
2. Choose only the applicable role below. Preserve useful existing structure; never delete code because its directory is named `contracts`, `deploy` or `infrastructure`.
3. Start with a vertical feature slice. Extract a workspace package only when a real second consumer, independent build/platform boundary or release owner needs it. Keep dependency flow toward shared business services and native resources, with transport/UI at the edge.
4. Prefer existing Engine conventions and narrow extensions to duplicated launch/generation scripts. Configuration declares values; setup acquires resources. Auth verifies trusted evidence; resource services enforce authorization. Export capabilities only at the boundary that needs them.
5. Verify the concrete changed behavior with the cheapest sufficient existing proof. Preserve necessary money, permission, persistence and lifecycle validation. Avoid snapshots of static wording, duplicated implementation tests and benchmark/coverage projects unrelated to the task.

Read by owner:

- App size, feature slices and browser/server separation: [Business applications](references/applications.md).
- TS Core, Engine, Config, Web, Auth and Manage: [TypeScript framework](references/typescript.md).
- Native Rust and emerging Go: [Native frameworks](references/native.md).
- Cross-process wire/generation and source-first status: [Runtime](references/runtime.md).
- Console host/page/interaction ownership: [Console](references/console.md).
- Source baselines, pending work and handoff evidence: [Evidence](references/evidence.md).

For implementation, reuse `lenso-develop`, `lenso-diagnose`, `lenso-rust`, `lenso-go`, `lenso-runtime` or `lenso-migrate` when that skill is available in the chosen checkout. Do not install skills globally or alter another repository as a side effect of applying these practices.
