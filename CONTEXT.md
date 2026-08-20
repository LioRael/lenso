# Lenso vNext Context

This file is the canonical vocabulary and routing context for vNext design and
implementation. The current repository source still implements the legacy
Service-oriented framework; do not infer vNext product semantics from existing
`Host`, `Service`, `Provider`, `Surface`, `System Plane`, or `platform-*` names.

## Product

Lenso is a local-first, language-independent modular application runtime. An
App statically composes trusted Modules through explicit Capability bindings.
The Rust Kernel owns only volatile execution mechanisms and compiles for native
and WebAssembly hosts. Product behavior, protocols, persistence, observability,
Console, agent harnesses, game servers, and future distributed facilities are
ordinary optional Modules and Adapters.

The first supported authoring profiles are native Rust Modules linked into an
App and Bun Modules executed through a child-process Adapter. An App can start
with neither database nor Console nor Story.

## Canonical language

**App** — the product and composition unit executed by one Kernel instance.

**Module** — a cohesive package of behavior with Descriptor data, lifecycle,
provided Capabilities, and required Capabilities. One package may provide
several Capabilities.

**Module Instance** — one App-local configuration of a Module package. Several
Instances may use the same package under different keys and bindings.

**Capability** — a deep role Interface identified by a
`namespace.name@major` series and an independently versioned Descriptor.

**Operation** — one stable named request, stream, or event interaction inside a
Capability.

**App Composition** — the declarative, language-independent authoring source
that names Module Instances, configuration references, and explicit Capability
bindings.

**Resolved App Plan** — the exact, immutable, validated execution input
materialized before boot from App Composition and ordinary package-manager
lockfiles.

**Kernel** — the portable Rust execution engine for graph and binding state,
lifecycle, invocation, bounded admission, managed scopes, readiness,
supervision, and Runtime Diagnostics. Its state is volatile and reconstructable.

**Runtime Driver** — the narrow host seam that advances Kernel tasks and
provides monotonic time, timers, wakeups, and shutdown integration. Native Tokio,
browser/JavaScript, and WASIp2 hosts use different Drivers.

**Execution Adapter** — the host-specific implementation that creates and
controls Module Instance generations and exposes their Capability endpoints.
Native Rust and Bun process execution are separate Adapters.

**Invocation Context** — Kernel-owned Caller Module key, Request ID, deadline,
cancellation, and opaque extensions propagated with one invocation.

**Runtime Failure** — an execution outcome such as unavailable, deadline,
cancelled, resource exhausted, protocol violation, or internal failure. It is
separate from a Capability-defined Domain Error.

**Runtime Diagnostics** — optional, non-blocking, bounded, lossy structural
facts emitted by Kernel. They are neither telemetry policy, audit, nor Story.

**Caller Module** — the direct Module Instance identity established by Kernel.

**ActorAssertion** — a sealed, issuer-owned, audience-bounded identity assertion
carried as a protected Invocation Context extension.

**Actor** — the typed domain projection a target Module derives from an
ActorAssertion before business authorization.

**UI Contribution** — portable Capability data describing navigation, routes,
assets, and declared Capability client requirements for a UI consumer.

**Console** — an optional independent Lenso App composed from ordinary Modules.
It reaches a target App only through an explicitly installed target Connector
Module or through direct bindings in a local development composition.

**Adapter** — a replaceable implementation at a real seam. Infrastructure does
not become a Module merely to satisfy vocabulary.

**Plugin** — informal ecosystem language for something a user adds. The formal
runtime type is Module.

**Extension** — a declared relationship or contribution between Modules, not a
third executable type.

## Hard invariants

- Kernel receives one Resolved App Plan and never discovers, downloads,
  installs, rebinds, or hot-reloads Modules during v1 execution.
- Consumers receive only explicitly bound typed handles. The supported
  Interface has no global Registry lookup.
- Native direct calls avoid serialization. Portable cross-runtime calls preserve
  the same request, stream, event, error, deadline, cancellation, and
  backpressure semantics through Adapter-specific mechanics.
- Rust, Bun, and user-selected UI code are trusted application code. Process or
  browser isolation is documented only where an Adapter actually provides it.
- Kernel owns no PostgreSQL pool, schema, migration, Outbox, Workflow, Auth,
  HTTP server, OpenTelemetry pipeline, Story, Console, or Control Plane.
- Stateful Modules own their data and migrations. They use private Adapters or
  deep semantic Capabilities; there is no universal State Module.
- Runtime Diagnostics never become a durable audit or correctness mechanism.
- Control Plane, remote Module placement, discovery, replicas, and dynamic
  composition are deferred until a real distributed use case requires them.

## Context routes

- Read [`docs/architecture/lenso-vnext.md`](docs/architecture/lenso-vnext.md)
  before changing vNext concepts, Kernel responsibilities, Module lifecycle,
  Capability semantics, Runtime Drivers, Execution Adapters, Console shape, or
  persistence ownership.
- Read ADRs 0030 onward for the authoritative decision and consequences behind a
  specific seam. ADRs 0001-0029 describe the legacy architecture and are not
  normative for vNext.
- Read
  [`docs/architecture/future-directions/distributed-module-runtime.md`](docs/architecture/future-directions/distributed-module-runtime.md)
  when a change mentions microservices, remote Modules, discovery, deployment,
  replicas, or Control Plane behavior.
- Read [`docs/roadmaps/lenso-vnext-validation.md`](docs/roadmaps/lenso-vnext-validation.md)
  before implementing the new crates or claiming a vNext milestone complete.
