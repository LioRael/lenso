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
- [`architecture/future-directions/distributed-plugin-runtime.md`](architecture/future-directions/distributed-plugin-runtime.md)
  records deferred distribution decisions.

## Accepted contracts

- [`architecture/dynamic-plugins.md`](architecture/dynamic-plugins.md) defines
  the accepted Plugin control plane and App Generation protocol; its opening
  status records the incomplete implementation evidence.
- [`architecture/plugin-root-resolution.md`](architecture/plugin-root-resolution.md)
  defines the current Plugin Root input and deterministic App resolution
  contract.
- [`architecture/plugin-execution-classes.md`](architecture/plugin-execution-classes.md)
  defines how one Plugin model reaches native, Wasm, QuickJS, process, and
  native-library execution.

## Historical contracts

[`architecture/plugin-authoring-and-resolution.md`](architecture/plugin-authoring-and-resolution.md)
retains the superseded Module/App Definition design only as migration evidence.
It is not a current authoring contract.

## Design proposals

[`Plugin authoring, dependency selection, and lifecycle`](proposals/2026-09-04-plugin-authoring-and-lifecycle.md)
records the approved design direction for Rust-first authoring, named dependency
choices, stateful updates, and failure scope, together with the remaining
implementation decisions. Consult it when specifying those changes; it does not
supersede the accepted contracts or describe shipped APIs.

[Issue #695](https://github.com/LioRael/lenso/issues/695) specifies the first
implementation wave: Rust resource construction, native and Process authoring,
and durable named dependency selection. Its structural dependency changes are
proposed in [ADR 0073](adr/0073-name-and-persist-plugin-dependencies.md); both
remain under review before implementation.

[`Three Plugin usage walkthroughs`](proposals/2026-09-04-plugin-usage-walkthrough.md)
examines a small tool, local durable state, and two instances of one dependency
from creation through removal. Use it to discuss simpler authoring and choice
persistence before treating Issue #695 or ADR 0073 as implementation-ready.

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
