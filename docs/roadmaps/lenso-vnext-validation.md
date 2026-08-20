# Lenso vNext validation roadmap

This roadmap proves the architecture with tracer bullets. It is not a release
calendar and does not authorize compatibility work in the legacy Kernel.

Implementation is tracked by issue
[#577](https://github.com/LioRael/lenso/issues/577) and child tickets #578 through
#603. Those tickets branch from and merge into `next`. The `main` branch remains
the v0.3.x maintenance and release line until an explicit vNext cutover.

## 1. Portable Kernel skeleton

Build new contract and Kernel packages beside the legacy implementation. Use a
deterministic test Runtime Driver to prove Plan validation, activation order,
rollback, readiness, shutdown, stable bindings, bounded admission, cancellation,
supervision, and diagnostics dropping.

Complete when the same Kernel engine passes native tests and compile-checks for
`wasm32-unknown-unknown` and `wasm32-wasip2` without target-specific branches in
the core state machine.

## 2. Native Rust vertical slice

Generate one portable request Capability, statically link two interchangeable
Rust providers, and run them through typed direct handles without serialization.
Exercise multiple Instances, explicit bindings, opaque configuration, Domain
Errors, Runtime Failures, restart generations, and App Ready gating.

Complete when swapping the selected provider changes only App Composition and
the Resolved App Plan.

## 3. Bun Adapter evidence spike

Implement the smallest Bun executor and compare framed stdio with a mature RPC
stack against the same logical protocol and conformance cases. Keep one process
per Module Instance as the initial default while measuring startup, steady-state
latency, streaming, cancellation, memory, crash recovery, and diagnostics.

Complete when evidence selects the first supported Adapter codec and the losing
prototype can be deleted without changing a Capability contract.

## 4. Cross-runtime interaction conformance

Run one generated portable Capability with Rust consumer/Bun provider and Bun
consumer/Rust provider. Add bidirectional stream and multi-subscriber event
fixtures, including partial admission, queue exhaustion, late frames, unknown
Domain Errors, invalid wire values, and provider crashes.

Complete when every Rust and TypeScript consumer/provider combination passes one
transport-independent black-box suite.

## 5. Optional product Modules

Create protocol, Auth, stateful, Secrets, OpenTelemetry, and Story examples only
through ordinary Capabilities. At least one App runs without all of them; one
stateful fixture owns its schema and explicit migration command; one ingress
fixture demonstrates protocol-neutral Credential Evidence and target-owned
authorization.

Complete when deleting any fixture Module requires no Kernel feature flag or
residual product hook.

## 6. Console as an App

Compose an independent Console App with replaceable Operator Identity, Access,
Shell, UI catalog, and target client Modules. Connect it to a target through an
allowlisted Connector Adapter. Demonstrate a trusted custom UI Contribution,
Runtime Diagnostics, and an explicit business operation without runtime graph
mutation or a System Plane.

Complete when the same UI/Capability seams can also run as an embedded local
development composition and neither form requires a special Console type in
Kernel.

## 7. Migration decision

Map each useful legacy feature to one of: new Kernel mechanism, ordinary vNext
Module, Execution Adapter, authoring tool, compatibility Adapter, or retirement.
Do not mechanically port `platform-*`, Host, Service, Provider, Module Release,
Surface, Story, migration, Outbox, and System Plane abstractions.

Complete when every retained behavior has one vNext owner and every legacy
compatibility path lives outside the portable Kernel.
