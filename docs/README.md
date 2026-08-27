# Lenso vNext documentation

This directory contains the design evidence for the vNext `main` branch.

## Normative

- [`CONTEXT.md`](../CONTEXT.md) defines vocabulary, ownership, and invariants.
- [`architecture/lenso-vnext.md`](architecture/lenso-vnext.md) describes the
  runtime shape.
- [`adr/README.md`](adr/README.md) indexes normative ADRs 0030–0070 and the
  historical ADRs 0001–0029.
- [`roadmaps/lenso-vnext-validation.md`](roadmaps/lenso-vnext-validation.md)
  defines the implementation evidence sequence.
- [`architecture/future-directions/distributed-module-runtime.md`](architecture/future-directions/distributed-module-runtime.md)
  records deferred distribution decisions.

## Accepted contracts

- [`architecture/dynamic-plugins.md`](architecture/dynamic-plugins.md) defines
  the accepted Plugin control plane and App Generation protocol; its opening
  status records the incomplete implementation evidence.
- [`architecture/plugin-root-resolution.md`](architecture/plugin-root-resolution.md)
  defines the current Plugin Root input and deterministic App resolution
  contract.
- [`architecture/plugin-authoring-and-resolution.md`](architecture/plugin-authoring-and-resolution.md)
  retains the superseded Module/App Definition contract as implementation and
  migration evidence for Slots, proposals, and Plan reconciliation.
- [`architecture/plugin-execution-classes.md`](architecture/plugin-execution-classes.md)
  defines deterministic Artifact-variant selection, Data contributions, and
  the reviewed Process, Wasm, QuickJS, and native-library branches.

## Research

[`research/`](research/) contains dated research and architecture reviews that
support vNext decisions. Research is evidence, not an executable contract.

## Governance

[`agents/README.md`](agents/README.md) indexes issue-tracker conventions,
architecture wayfinding, and the canonical skill-pack usage and maintenance
guide. Root governance lives in [`AGENTS.md`](../AGENTS.md).

The previous v0.3.x implementation documentation is not maintained on `main`.
Its source and release history remain available from the `lenso@0.3.47` tag
and Git history.
