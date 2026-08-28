---
status: accepted
---

# Publish one Plugin Contract with multiple implementations

A Plugin Release owns one runtime-independent Contract and may carry multiple
exact executable implementations. Each implementation names one Execution
Class, entrypoint, target, and immutable runtime package identity. Host policy
selects exactly one compatible implementation before App resolution lowers the
Release into a `PluginDescriptor` and immutable Plan Snapshot.

Every implementation in one Release must expose the same configuration Schema,
defaults, provided and required Capabilities, restart policy, criticality, and
state contract. A build or admission mismatch is rejected; variant-specific
Capabilities would be different Plugin Releases rather than conditional
behavior hidden behind one identity.

The Bundle and Plugin Root resolver own implementation selection. Kernel and
Execution Adapters still receive only one exact selected runtime input per
Plugin Instance. They never benchmark, negotiate, retry through, or fall back
to another implementation after readiness or invocation failure. Changing the
selected implementation is a structural App Generation change.

Rust authoring may generate Wasm Component, native dylib, and Process
entrypoints around one Plugin implementation. TypeScript authoring may generate
QuickJS and Bun entrypoints only while source stays within the portable Plugin
SDK; importing Bun- or Node-specific facilities narrows the published
implementation set instead of pretending to preserve portability. A Bun
standalone executable is a Process implementation, while a Bun script remains
a Bun Adapter implementation.

V2 single-Artifact Bundles lower to one implementation. The new multi-
implementation Bundle version is generated rather than hand-authored, verifies
every Artifact and Contract projection before admission, and preserves one
deterministic implementation preference owned by the Host.

## Consequences

- Plugin authors maintain one behavior implementation and publish the runtime
  targets their source genuinely supports.
- App owners install and configure one Plugin rather than choosing a Plugin
  type; Host policy selects execution mechanics.
- Hosts can prefer sandboxed portable implementations and admit trusted native
  implementations without changing product identity.
- Cross-implementation conformance becomes release evidence: the same Contract
  inputs must produce the same observable success, Domain Error, cancellation,
  and state semantics.
- Stateful implementation switches still require explicit state compatibility
  evidence; common source does not prove common storage or memory layout.
