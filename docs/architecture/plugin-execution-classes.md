# Plugin execution classes

Status: current execution contract.

An Execution Class selects the Adapter that runs a Plugin Instance. It changes
execution mechanics, not the Plugin's product identity or Capability contract.

## One behavior unit

```text
Plugin Release
  -> Plugin Descriptor
  -> exact Artifact or Host-linked factory
  -> one Execution Class
  -> Plugin Instance in the Resolved App Plan
```

A native built-in, Wasm Component, embedded JavaScript package, trusted dynamic
library, or process executable is still a Plugin. Authors do not create a
second behavior object to reach one of these runtimes.

The generated Plugin Descriptor owns:

- Plugin ID and Release version;
- configuration Schema and safe package defaults;
- provided and required Capabilities;
- entrypoint, restart policy, criticality, and state contract; and
- the selected Execution Class and exact executable identity.

The Host Catalog decides which Releases and Execution Classes are allowed. The
resolver selects exact inputs before staging. Runtime never benchmarks,
negotiates, or falls back to another Artifact after failure.

## Current classes

| Execution Class | Input | Isolation | Intended status |
| --- | --- | --- | --- |
| `lenso.native-rust@1` | Exact statically linked factory | In-process | Stable Host-linked Plugins |
| `lenso.wasm-component@1` | Verified Component Artifact | In-process sandbox | Portable bundled Plugins |
| `lenso.quickjs@1` | Verified immutable ESM graph | In-process sandbox | Embedded JavaScript Plugins |
| `lenso.native-dylib@1` | Verified native library | In-process, trusted | Experimental trusted Plugins |
| product process classes | Verified executable plus protocol | Child process | Product-specific adapters |

Support is capability- and interaction-specific. An Adapter must reject a Plan
before readiness when it cannot implement a declared request, stream, event,
state, cancellation, or supervision contract.

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

The portable V2 Plugin Bundle has one Plugin entry and one exact main Artifact.
The receiver verifies the complete bundle before resolution and reopens the
admitted Artifact by digest and size before execution. Multi-Artifact or
data-only packages require a future explicit bundle version; they are not
smuggled in through an internal behavior abstraction.

## Conformance

Every Execution Class must prove the interaction kinds it claims through the
shared conformance surface: admission, ordering, backpressure, cancellation,
late outcomes, shutdown, restart, and Generation drain. Unsupported behavior
fails closed before the App becomes ready.

See [Plugin Generation control plane](dynamic-plugins.md) for staging and
routing and [Plugin Root and App resolution](plugin-root-resolution.md) for the
author-facing model.
