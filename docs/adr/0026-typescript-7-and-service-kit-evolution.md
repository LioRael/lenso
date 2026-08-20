---
status: accepted
---

# Upgrade active TypeScript dependencies and prepare Service Kit extraction

> **Status: superseded for vNext.** Retained as historical and v0.3.x
> maintenance context. [ADR 0030](0030-rebuild-lenso-as-a-local-first-modular-runtime.md)
> and ADRs 0031 onward are normative for vNext.

Active repositories that use TypeScript dependencies will pin direct TypeScript
dependencies to `7.0.2`. A TypeScript 7 incompatibility is a migration blocker:
the repository must upgrade the surrounding toolchain or replace the incompatible
integration. A TypeScript 6 sidecar is not an accepted escape hatch. Historical
worktrees and retired repositories are not migration targets.

The `lenso-cli` npm package remains one genuine release unit with its native Rust
CLI. Its npm distribution adapter moves from hand-written JavaScript to
TypeScript compiled to CommonJS-compatible `bin/lenso.js`; the npm name, command,
signal forwarding, platform binary matrix, and Rust behavior remain unchanged.
The release workflow explicitly builds the adapter before shim checks and package
dry-runs, rather than relying on `prepack`.

`@lenso/service-kit` remains owned by the framework repository until its extraction
gate is satisfied. The future standalone repository contains the TypeScript Service
authoring and delivery surface, while Rust Service Contract/schema definitions are
authoritative. Rust-to-TypeScript generation produces reviewable committed inputs,
and CI rejects stale generated output. The current parity seam is explicit:
`crates/lenso-service/schemas/lenso-service.v1.schema.json` generates
`sdk/typescript/packages/service-kit/src/generated/service-contract-schema.ts`,
checked by `pnpm --dir sdk/typescript check:service-contract-schema`. Extraction requires contract parity,
independent CI, an independent release dry-run, and a versioned installation proof
from an external consumer. Each repository releases independently; no synchronized
cross-repository version is introduced.

## Consequences

- Direct TypeScript dependencies are upgraded per canonical active repository and
  validated with that repository's typecheck, build, tests, and release dry-run.
- Toolchains that embed TypeScript, such as documentation or application build
  tools, must be upgraded or adapted to TypeScript 7 rather than silently retaining
  a second compiler path.
- Generated JavaScript is a build artifact. `bin/lenso.js` is not a second hand-
  maintained implementation of the CLI adapter.
- Service Kit contract semantics have one authoritative source; an independently
  published TypeScript package cannot invent a parallel contract truth.
- Service Kit extraction is a repository ownership change, not a mechanical
  TypeScript/Rust split.
