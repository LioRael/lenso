# Allow Adapter-specific Capability dispatch

All execution Adapters will preserve the same Capability contract, invocation context, cancellation, backpressure, domain-error, and runtime-failure semantics, but they need not use the same dispatch mechanism. Native Rust bindings may use typed direct calls without serialization, while a cross-runtime binding may use generated types and a wire protocol; conformance is measured at the Interface rather than by forcing every call through RPC.

## Consequences

- Kernel dispatch never automatically retries an in-flight Operation after provider failure. The caller or an explicit resilience Module may retry only when the Capability contract makes that safe.
- The first Bun Runtime Adapter may default to one child process per Module Instance, but process topology is not part of the Kernel Interface. A later Adapter may execute several trusted Bun Modules together without becoming another graph or binding authority.
- Framing, codecs, and transport belong to a Portable Invocation Adapter rather than the Kernel. A Bun Executor receives the Kernel-resolved Module keys, configuration, bindings, and exact Descriptor versions and may confirm or execute them, but cannot discover or resolve a second graph.
- The first Bun wire implementation remains an evidence question. A throwaway prototype will compare a small framed-stdio Adapter with a mature RPC stack before either becomes a supported contract; the Adapter conformance suite is decided before the transport.
- Read-only observation may report dispatch behavior, but no Module can install an implicit global interceptor that changes another binding. Authentication, caching, resilience, and similar behavior enter through explicit Wrapper or Adapter Modules.
- Fine-grained loops that cannot afford portable failure and serialization semantics stay inside one Module rather than pretending to be cross-runtime Capability calls.
