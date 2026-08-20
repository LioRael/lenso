# Lenso vNext validation roadmap

This roadmap proves the vNext architecture with executable tracer bullets. It
is not a release calendar. The `next` branch contains only the vNext runtime;
`main` remains the v0.3.x maintenance and release line.

Implementation is tracked by issue
[#577](https://github.com/LioRael/lenso/issues/577) and child tickets #578
through #603. Each ticket starts from the latest `origin/next` and targets
`next`.

## 0. Workspace reset

The `next` workspace starts with only the immutable App Plan, portable Kernel,
native Runtime Driver, deterministic test Driver, and their governance. The
old Service/Provider/System Plane workspace is removed atomically with this
skeleton.

Complete when the workspace has no old Cargo members, contracts, fixtures,
database or process infrastructure, TypeScript Service Kit, legacy release
workflow, or v0.3.x implementation documentation.

## 1. Portable Kernel

Extend the small Kernel without adding host services to its core. Use the
deterministic Runtime Driver to prove Plan validation, staged activation,
readiness, shutdown, cancellation, supervision, and diagnostics dropping.

Complete when the same Kernel engine passes native tests and compile-checks for
`wasm32-unknown-unknown` and `wasm32-wasip2` without target-specific branches
in the core state machine.

## 2. Native Rust vertical slice

Generate one portable request Capability, statically link two interchangeable
Rust providers, and run them through typed direct handles without serialization.
Exercise multiple Instances, explicit bindings, opaque configuration, Domain
Errors, Runtime Failures, restart generations, and App Ready gating.

Complete when swapping the selected provider changes only App Composition and
the Resolved App Plan.

## 3. Bun Adapter evidence spike

Implement the smallest Bun Execution Adapter and compare framed stdio with a
mature RPC stack against the same logical protocol and conformance cases.
Keep one process per Module Instance as the initial default while measuring
startup, steady-state latency, streaming, cancellation, memory, crash recovery,
and diagnostics.

Complete when evidence selects the first supported Adapter codec and the
losing prototype can be deleted without changing a Capability contract.

## 4. Cross-runtime conformance

Run one generated portable Capability with Rust consumer/Bun provider and Bun
consumer/Rust provider. Add bidirectional stream and multi-subscriber event
fixtures, including partial admission, queue exhaustion, late frames, unknown
Domain Errors, invalid wire values, and provider crashes.

Complete when every supported consumer/provider combination passes one
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
allowlisted Connector Adapter.

Complete when the same UI and Capability seams can also run as an embedded
local-development composition without a special Console type in Kernel.

## 7. Migration decisions

Every future retained behavior must map to exactly one of: Kernel mechanism,
ordinary vNext Module, Execution Adapter, authoring tool, compatibility Adapter,
or retirement. Do not mechanically port the old platform crates, Host, Service,
Provider, Module Release, Surface, Story, migration, Outbox, or System Plane
abstractions.
