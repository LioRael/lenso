# Lenso vNext documentation

This directory contains the design evidence for the vNext `main` branch.

## Normative

- [`CONTEXT.md`](../CONTEXT.md) defines vocabulary, ownership, and invariants.
- [`architecture/lenso-vnext.md`](architecture/lenso-vnext.md) describes the
  runtime shape.
- [`adr/README.md`](adr/README.md) indexes normative ADRs 0030–0072 and the
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

[`Plugin authoring design: consolidated review`](proposals/2026-09-04-plugin-usage-walkthrough.md)
is the current review entrypoint for the author interface, default behaviors,
ownership, fault scope, and adoption boundaries. It includes one integrated
example and distinguishes retained contracts from proposed changes. It is not
implementation approval or a shipped SDK reference.

[ADR 0073](adr/0073-name-and-persist-plugin-dependencies.md) is the narrower
named-dependency proposal. It recommends configuration-time materialization and
read-only startup; file formats and version allocation remain open.
The [fault-scope companion](proposals/2026-09-04-plugin-fault-scope.md) proposes
Host-defined terminal failure impact after readiness while retaining strict
startup. Both remain proposed. [Issue #695](https://github.com/LioRael/lenso/issues/695)
tracks the remaining review decisions before an implementation specification.

The [Rust/TypeScript authoring comparison](proposals/2026-09-04-multilingual-plugin-authoring.md)
shows one behavior in both languages, with common dependency identities,
construction/cleanup ownership, generated contracts, and explicit runtime support
limits. Its code is proposed syntax, not current SDK usage.

The [product declaration pipeline](proposals/2026-09-04-plugin-declaration-pipeline.md)
specifies SDK build outputs, offline bundle admission, and runtime binding using
an Agent-owned TS tool calling Rust stores. It distinguishes the proposed
declaration extraction from the current Bun describe-script implementation.

The [cancellation and cleanup review](proposals/2026-09-04-plugin-cancellation-and-cleanup.md)
follows construction failure, late completion, invocation cancellation, and safe
resource release across Rust and TS, with bounded cleanup and Adapter limits.

The [adoption and delivery boundary](proposals/2026-09-04-plugin-adoption-and-delivery.md)
closes the design set with separate SDK/dependency/fault-policy adoption,
existing-source compatibility, and a proposed complete Rust/TS Request slice.
The set is ready for final review; implementation and release remain unapproved.

[`Plugin authoring, dependency selection, and lifecycle`](proposals/2026-09-04-plugin-authoring-and-lifecycle.md)
retains the earlier approved overall direction and exploratory examples as
design context. Use the consolidated review for the latest candidate semantics;
accepted ADRs continue to define current normative behavior.

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
