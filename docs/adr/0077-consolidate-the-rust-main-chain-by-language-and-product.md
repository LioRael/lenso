# ADR 0077: Consolidate the Rust main chain by language and product

- Status: accepted
- Date: 2026-09-21
- Supersedes: ADR 0064 repository ownership and extraction decision
- Retains: ADR 0064 portable dependency direction and product ownership rules

## Context

ADR 0064 separated the portable core from Rust runtimes, protocol tooling,
Engine, CLI, and Bun integration. That extraction made boundaries visible, but
the resulting repositories changed together frequently and required temporary
published versions for ordinary framework development. Repository count became
an operational boundary that did not match ownership or release needs.

Lenso also has a real cross-language surface. Combining Rust framework code,
TypeScript SDKs, the Site, UI, Marketplace, and downstream products in one
repository would recreate a mixed-language release knot. The lasting boundary
must therefore follow language and genuine product ownership, while package and
module boundaries continue to enforce runtime dependencies.

## Decision

`LioRael/lenso` owns the frequently co-evolving Rust framework main chain:

- Plan, Kernel, Runtime Drivers, Execution Adapters, and Host mechanics;
- Engine APIs and authoring implementation;
- the Rust CLI, Rust SDKs, protocol tooling, and language-neutral `spec/` data;
- optional Rust framework integrations, including Web packages, when their
  change cadence and dependency direction justify workspace membership.

The workspace does not make every crate part of the Kernel. Kernel and Plan
remain independent of Web, Auth, CLI, Engine conventions, and concrete
Execution Adapters. Crates remain separate when they have a public consumption
surface, target boundary, macro boundary, heavy optional engine, or independent
release need. Internal development dependencies use Cargo workspace or path
resolution; packaged-consumer validation remains required before release.

`LioRael/lenso-js` owns JavaScript and TypeScript SDKs, Bun/Node authoring, Web
client integration, and the JavaScript half of cross-language fixtures. It does
not reimplement the core resolver. Rust/Bun conformance uses an explicit
checkout or packaged artifact rather than sibling-directory assumptions.

The Site, Lenso UI, Marketplace backend, and downstream products remain
separate products. Product Plugins such as Auth remain with their owners unless
a later decision establishes sustained shared-Rust ownership. Old source
repositories retain history until separately authorized remote archival; they
are no longer the active source layout.

## Migration rules

- Preserve public package identities, licenses, and merge-parent Git history.
- Move source, tests, examples, CI, and current documentation, but not build
  output, work plans, qualification ledgers, duplicated status reports, or
  stale experiments.
- Update all active paths and examples to the new owner. Historical ADR links
  may continue to identify the revision they describe.
- Keep language-neutral protocol fixtures under `spec/`; generated Rust and
  TypeScript code stays in its language repository.
- Do not redirect, archive, delete, publish, or deploy a remote repository as an
  implied part of local consolidation.

## Consequences

Framework-wide Rust changes can be compiled and tested atomically, while Cargo
crate boundaries still express portability and release isolation. TypeScript
packages have one language-native workspace and release surface. Cross-language
tests become explicit integration gates rather than hidden local path coupling.

The workspace is larger and its CI must remain selective: one normal Rust gate,
target-specific checks where required, and explicit packaged or cross-language
gates. Repository size is not permission to add unrelated business Plugins to
the Kernel or to accumulate migration evidence in the source tree.
