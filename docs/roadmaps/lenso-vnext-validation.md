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
in the core state machine, while Browser/JavaScript and WASIp2 Drivers pass
local scheduler, monotonic timer, cancellation, readiness, and shutdown smoke
tests. Host validation rejects an execution class absent from the immutable
Execution Adapter catalog assembled by the selected Runner.

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

## 6. Target-owned App Web UI

Compose an optional Web UI inside one target App from an ordinary Web Shell,
Browser Adapter, business Module, and custom UI Contribution. Bind the Shell to
`many` contribution providers and project generated browser clients only for
portable requirements declared by each contribution and resolved before boot.
Prove that one package can publish explicit backend and UI Module entrypoints
without introducing a Console, Plugin, Surface, or nested-Module runtime type.

Complete when the custom route renders, invokes its App-local business
Capability through a generated client, preserves target-owned authorization,
rejects missing or colliding contribution metadata, and can be removed without
changing Kernel or non-UI Module behavior. The App must also run with no Web UI
selected, and installing or changing UI code must remain a reviewable
authoring-time Composition and Plan change.

An independent Console App and allowlisted target Connector are later
cross-App product work. They enter validation only when a real requirement
needs remote or multiple targets, an independent operator trust domain,
durable cross-target state, or an independent release lifecycle. They are not
prerequisites for the target-owned App Web UI or the initial vNext validation
sequence.

See ADRs [0043](../adr/0043-represent-ui-contributions-as-capabilities.md) and
[0060](../adr/0060-compose-target-web-ui-in-app-and-separate-cross-app-console.md).

## 7. Migration decisions

Every future retained behavior must map to exactly one of: Kernel mechanism,
ordinary vNext Module, Execution Adapter, authoring tool, compatibility Adapter,
or retirement. Do not mechanically port the old platform crates, Host, Service,
Provider, Module Release, Surface, Story, migration, Outbox, or System Plane
abstractions.
