# Plugin execution classes

Status: current execution contract, not a maturity or qualification authority.

An Execution Class selects the Adapter that runs a Plugin Instance. It changes
execution mechanics, not the Plugin's product identity or Capability contract.

## One behavior unit

```text
Plugin Release
  -> one Plugin Contract
  -> one or more exact implementations
  -> optional explicit Artifact variants per implementation
  -> Host selects one exact implementation and variant
  -> resolved Plugin Descriptor
  -> Plugin Instance in the Resolved App Plan
```

A native built-in, Wasm Component, embedded JavaScript package, trusted dynamic
library, or process executable is still a Plugin. Authors do not create a
second behavior object to reach one of these runtimes.

The generated Plugin Contract owns:

- Plugin ID and Release version;
- configuration Schema and safe package defaults;
- provided and required Capabilities;
- restart policy, criticality, and state contract.

Each generated implementation owns its entrypoint, target, selected Execution
Class, and exact executable identity. Resolving the Contract with one
implementation produces the final Plugin Descriptor consumed by App
resolution.

The Host Catalog decides which Releases and Execution Classes are allowed. The
resolver selects exact inputs before staging. Runtime never benchmarks,
negotiates, or falls back to another Artifact after failure.

## Declared classes

| Execution Class | Input | Isolation | Intended use |
| --- | --- | --- | --- |
| `lenso.native-rust@1` | Exact statically linked factory | In-process | Stable Host-linked Plugins |
| `lenso.wasm-component@1` | Verified Component Artifact | In-process sandbox | Portable bundled Plugins |
| `lenso.quickjs@1` | Verified immutable ESM graph | In-process sandbox | Embedded JavaScript Plugins |
| `lenso.native-dylib@1` | Verified native library | In-process, trusted | Experimental trusted Plugins |
| product process classes | Verified executable plus protocol | Child process | Product-specific adapters |

Support is capability- and interaction-specific. An Adapter must reject a Plan
before readiness when it cannot implement a declared request, stream, event,
state, cancellation, or supervision contract.

This table defines contract vocabulary only. Implementation, release, and
Environment-plus-Infrastructure qualification are separate facts; no table cell
is a standalone claim about a target combination.

## Adapter boundary

An Execution Adapter receives only resolved authority. It may:

- prepare exact Plugin Artifacts or linked factories;
- validate entrypoints and operation tables;
- create one Plugin generation and its endpoint handles;
- enforce execution-specific resource limits;
- translate cancellation and terminal failure; and
- deactivate and release all owned resources.

It may not discover Plugin Root files, choose versions, change configuration,
invent bindings, request additional authority, or select a fallback Artifact.

Handles never cross App Generations. A restart creates a new Plugin generation;
stable consumer handles may be preserved only through the Adapter's explicit
recreation contract.

## Bundle rule

A V2 Plugin Bundle is one Contract with one implementation. A multi-
implementation Bundle carries one canonical Contract plus an ordered,
uniquely-identified implementation set. The receiver verifies the complete
closure, checks declared Contract closure and source-derived descriptors where
available, and selects one implementation through Host policy before Plan materialization.
The admitted Artifact is reopened by digest and size before execution.

Bundle V5 adds publisher-declared implementation groups, each with one or more
uniquely identified executable variants. The publisher must explicitly group
variants; sharing a language, source tree, or Contract does not prove identical
behavior. In a composite Cargo project, each `lenso-cli.implementations` entry
may set `group = "portable"`; when any entry does, every entry must name a
group, and `id` names its variant within that group. V2–V4 wire bytes and
their digest rules remain unchanged. Selection evidence records both IDs and
the exact Artifact; equal-priority compatible variants are an error.
For V5, the Artifact format itself also imposes target mechanics: a Wasm-typed
Artifact requires a Host-admitted `WasmComponent` capability and a native
process Artifact requires `NativeProcess`, even when the publisher omitted an
explicit requirement. A Request-only target cannot select either variant.
This capability check does not itself prove that Wasm bytes are a valid
Component; Artifact validation remains separate. Legacy V2–V4 candidates
retain their signed selection semantics.
V5 also rejects a format mismatch for an official versioned Execution Class:
Process accepts its process Artifact, Wasm Component accepts Wasm, and
Bun/QuickJS accept JavaScript. A third-party Execution Class remains open but
its Adapter must validate the exact Artifact it receives before readiness.

Selection is not runtime fallback. If the selected implementation fails its
Ready Gate or later invocation, the Generation fails through its ordinary
supervision policy. Choosing another implementation requires a newly resolved
Generation.

## Conformance

Every Execution Class must prove the interaction kinds it claims through the
shared conformance surface: admission, ordering, backpressure, cancellation,
late outcomes, shutdown, restart, and Generation drain. Unsupported behavior
fails closed before the App becomes ready.

See [Plugin Generation control plane](dynamic-plugins.md) for staging and
routing and [Plugin Root and App resolution](plugin-root-resolution.md) for the
author-facing model. See the
[execution target capability matrix](execution-target-capability-matrix.md) for
the owner-maintained admission facts that a selected class must supply.
