# Lenso documentation

This directory contains durable architecture, API, component, and contributor
documentation for the Rust workspace.

## Start here

- [`../CONTEXT.md`](../CONTEXT.md) defines canonical vocabulary and invariants.
- [`architecture/lenso-vnext.md`](architecture/lenso-vnext.md) describes the
  runtime shape.
- [`architecture/lenso-authoring.md`](architecture/lenso-authoring.md) explains
  authoring and Plan resolution.
- [`adr/README.md`](adr/README.md) indexes current and superseded decisions.
- [`components/`](components/) documents the Engine, Runtime, protocols, and Web
  packages now built in this workspace.
- [`agents/skills.md`](agents/skills.md) documents the public skill pack.

Current validation belongs in Cargo tests and CI. Release receipts, temporary
qualification ledgers, implementation status reports, work plans, and research
notes do not live in the source tree. A durable behavioral or ownership change
should update the public documentation or receive an ADR; Git history preserves
the migration material that led to it.

The previous v0.3.x source and documentation remain available from the
`lenso@0.3.47` tag and Git history.
