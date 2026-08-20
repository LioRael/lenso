# Lenso vNext architecture

## Status

This document describes the agreed target architecture. It is not a claim that
the current `lenso` workspace implements vNext. ADRs 0030 onward are the
authoritative decisions; this overview routes readers through them without
repeating every detailed invariant.

## Shape

```text
App Composition + Cargo/npm/OCI inputs
                  |
          authoring and build tools
                  |
          Resolved App Plan
                  |
        +---------+----------+
        |      thin Runner   |
        | Runtime Driver     |
        |                    |
        | portable Kernel    |
        | graph / lifecycle  |
        | bindings / invoke  |
        | scopes / supervise |
        | diagnostics        |
        +----+-----------+---+
             |           |
      Native Rust     Bun process       future host-supported
      Adapter         Adapter           Execution Adapters
             |           |
           Module Instances providing and requiring Capabilities
```

The Kernel is deliberately smaller than the product. Its deletion would force
every Runner and Execution Adapter to reimplement deterministic composition,
lifecycle, bounded invocation, cancellation, supervision, and diagnostics.
HTTP, PostgreSQL, Auth, Console, Story, and similar features do not pass this
deletion test because their complexity disappears when their Modules are not
selected.

## Composition and packages

App Composition is declarative and language-independent. Authoring tools combine
it with ordinary package-manager resolution to produce one exact Resolved App
Plan. Kernel executes that Plan and performs no package acquisition, SemVer
selection, schema diff, signature admission, or graph discovery. Module
installation is a reviewable project edit, not a runtime operation.

See ADRs [0031](../adr/0031-separate-capability-contracts-from-module-packages.md),
[0034](../adr/0034-make-app-composition-the-capability-binding-authority.md),
[0045](../adr/0045-materialize-a-resolved-app-plan-before-boot.md), and
[0057](../adr/0057-make-module-installation-an-authoring-operation.md).

## Kernel and hosts

Kernel is one portable asynchronous state machine. A Runtime Driver supplies
host scheduling and monotonic time; Execution Adapters supply Module generation
and endpoint mechanics. Native Tokio is one Driver implementation rather than a
Kernel dependency. The same engine compile-checks for native,
`wasm32-unknown-unknown`, and `wasm32-wasip2`; each host supports only the
Adapters it can actually provide.

See ADRs [0047](../adr/0047-scope-runtime-work-to-module-lifecycles.md),
[0048](../adr/0048-make-supervision-execution-adapter-aware.md),
[0053](../adr/0053-run-the-kernel-on-a-portable-runtime-driver.md), and
[0054](../adr/0054-layer-the-rust-implementation-around-the-portable-kernel.md).

## Modules and lifecycle

A Module package publishes data Descriptors plus factories understood by its
Execution Adapters. A Resolved Plan may instantiate the same package several
times. Boot validates the graph, prepares every Instance, activates providers in
dependency order, and opens one App Ready Gate only after full activation.
Managed tasks and resources belong to one Instance generation; restarts create a
new generation and preserve stable consumer handles when the Adapter supports
recreation.

Native Rust Modules are statically linked Cargo dependencies in v1. Bun Modules
run through the first process Adapter, initially one process per Instance by
default. That topology is an Adapter choice, not a Kernel contract.

See ADRs [0032](../adr/0032-keep-static-capability-bindings-through-provider-restarts.md),
[0046](../adr/0046-use-staged-all-or-nothing-app-activation.md),
[0055](../adr/0055-statically-link-native-rust-modules-in-v1.md), and
[0056](../adr/0056-keep-the-module-runtime-interface-minimal.md).

## Capabilities and invocation

Capabilities are independently versioned deep role Interfaces. A small
Descriptor declares stable Operations and request, stream, or event interaction
kinds. Portable contracts use JSON Schema 2020-12 plus a minimal value profile
and generate Rust and TypeScript clients and providers. Native Rust dispatch can
remain typed and direct; cross-runtime Adapters validate their wire boundary.

Request has one terminal result. Stream is bidirectional with independent
half-close, bounded flow, cancellation, and an explicit terminal outcome. Event
fan-out performs independent bounded admission and reports partial outcomes; it
is volatile and never implies persistence or redelivery.

See ADRs [0033](../adr/0033-use-request-stream-and-event-capability-interactions.md),
[0035](../adr/0035-allow-adapter-specific-capability-dispatch.md),
[0049](../adr/0049-define-a-portable-json-value-contract.md),
[0050](../adr/0050-generate-and-conformance-test-portable-bindings.md),
[0051](../adr/0051-use-explicit-partial-admission-for-ephemeral-events.md), and
[0052](../adr/0052-give-streams-explicit-flow-and-terminal-semantics.md).

## Identity and invocation authority

Kernel establishes Caller Module identity and transports opaque ordinary or
sealed extensions without interpreting their domains. Auth Modules turn
protocol-neutral Credential Evidence into short-lived ActorAssertions. Provider
SDKs validate and project those assertions into typed domain Actors, and the
target Module performs final authorization. There is no ambient System Actor,
universal grants array, or automatic anonymous identity.

See ADRs [0037](../adr/0037-preserve-provenance-of-sealed-invocation-extensions.md),
[0038](../adr/0038-separate-caller-module-actor-assertion-and-actor.md),
[0039](../adr/0039-make-authentication-protocol-neutral-and-explicit.md), and
[0040](../adr/0040-attenuate-actor-assertions-across-capabilities.md).

## State, diagnostics, and product Modules

Kernel state is volatile. Stateful Modules own schema meaning, migrations, and
transaction scope behind private persistence Adapters or deep semantic
Capabilities. Workflow, Outbox, durable Event delivery, Secrets, Audit,
OpenTelemetry, Story, and health are optional Modules or owner-local behavior.

Runtime Diagnostics expose bounded, non-blocking, best-effort structural facts.
Observers cannot block or change App behavior, and diagnostic records exclude
payloads, secrets, configuration, and ActorAssertions.

See ADRs [0036](../adr/0036-expose-non-blocking-runtime-diagnostics.md),
[0041](../adr/0041-keep-persistence-owned-by-stateful-modules.md), and
[0042](../adr/0042-keep-migrations-and-distributed-consistency-out-of-the-kernel.md).

## Console and UI

Production Console is an independent Lenso App composed from ordinary Modules.
Target Apps opt into a thin Connector that exports an explicit portable
Capability allowlist. An embedded direct-binding profile may serve local
development. Console has an independent operator identity domain, while its
PostgreSQL, Outbox, Story, Audit, and target catalog choices remain conditional.

UI Contributions are Capabilities rather than Console-specific Surfaces. A
user-selected local bundle, package, or remote ESM is trusted application code;
generated Browser clients are a composition and ergonomics seam rather than a
same-realm security sandbox.

See ADRs [0043](../adr/0043-represent-ui-contributions-as-capabilities.md) and
[0044](../adr/0044-run-console-as-an-independent-lenso-app.md).

## Deferred distributed direction

Local Interfaces intentionally preserve handles, portable contracts, deadlines,
cancellation, and Adapter seams that could support remote execution later. v1
does not define discovery, placement, replicas, rolling upgrades, dynamic graph
mutation, or a Lenso Control Plane. Revisit them only from a concrete App that
needs distribution; see
[`future-directions/distributed-module-runtime.md`](future-directions/distributed-module-runtime.md).
