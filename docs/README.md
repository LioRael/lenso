# Lenso vNext documentation

This directory contains the design evidence for the vNext `main` branch.

## Normative

- [`CONTEXT.md`](../CONTEXT.md) defines vocabulary, ownership, and invariants.
- [`architecture/lenso-vnext.md`](architecture/lenso-vnext.md) describes the
  runtime shape.
- [`adr/README.md`](adr/README.md) indexes normative ADRs 0030–0074 and the
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

## Approved Plugin authoring design

[`Plugin authoring design: approved baseline`](proposals/2026-09-04-plugin-usage-walkthrough.md)
records the repository owner's 2026-09-04 approval of the author interface,
default behavior, ownership, fault scope, and adoption boundaries. Its examples
remain illustrative; acceptance does not establish shipped SDK/runtime support.

[ADR 0073](adr/0073-name-and-persist-plugin-dependencies.md) accepts named
dependencies, configuration-time choice materialization, and read-only startup.
[ADR 0074](adr/0074-scope-terminal-failure-to-host-essential-instances.md) accepts
Host-essential terminal failure scope after readiness while retaining strict
startup. Both require supported implementation and explicit adoption; exact
formats and versions remain implementation-specification work.
[Issue #695](https://github.com/LioRael/lenso/issues/695) tracks the owner-local
delivery tasks, starting with [implementation specification #699](https://github.com/LioRael/lenso/issues/699).

The [Rust/TypeScript authoring comparison](proposals/2026-09-04-multilingual-plugin-authoring.md)
shows one behavior in both languages, with common dependency identities,
construction/cleanup ownership, generated contracts, and explicit runtime support
limits. Its code is proposed syntax, not current SDK usage.

The [product declaration pipeline](proposals/2026-09-04-plugin-declaration-pipeline.md)
specifies SDK build outputs, offline bundle admission, and runtime binding using
an Agent-owned TS tool calling Rust stores. It distinguishes the accepted target
declaration extraction from the current Bun describe-script implementation.

The [cancellation and cleanup review](proposals/2026-09-04-plugin-cancellation-and-cleanup.md)
follows construction failure, late completion, invocation cancellation, and safe
resource release across Rust and TS, with bounded cleanup and Adapter limits.

The [adoption and delivery boundary](proposals/2026-09-04-plugin-adoption-and-delivery.md)
closes the design set with separate SDK/dependency/fault-policy adoption,
existing-source compatibility, and the approved complete Rust/TS Request slice.
Implementation must follow each task's specification and prerequisite gates;
the design approval does not perform a release.

[`Plugin authoring, dependency selection, and lifecycle`](proposals/2026-09-04-plugin-authoring-and-lifecycle.md)
retains the earlier approved overall direction and exploratory examples as
design context. Use the approved baseline for current target semantics;
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
