# Keep the Module runtime Interface minimal

A Module package will publish Descriptor data and an Execution-Adapter-specific
factory. For each Resolved App Plan entry, the factory creates a fresh Module
Instance generation that participates only in `prepare`, `activate`, and
`deactivate` and supplies the Capability endpoints declared by its Descriptor.
There are no feature-specific lifecycle hooks for HTTP, Console, migration,
Story, admin actions, scheduling, or persistence.

## Consequences

- Kernel passes the Instance key, opaque configuration, resolved dependency
  handles, managed scopes, readiness signal, and generic Invocation Context
  mechanics through phase-appropriate contexts. Module-owned SDK code decodes
  and validates configuration.
- Preparation fails unless the Execution Adapter can supply the exact declared
  Endpoint Set. Missing, duplicate, or undeclared Capability Operations cannot
  be deferred until their first business call.
- A generation may maintain arbitrary volatile in-memory state. Restart creates
  a new generation from configuration and bound Capabilities; Kernel does not
  snapshot or deserialize Rust or JavaScript heaps.
- Execution Adapters translate the logical factory and lifecycle into static
  Rust calls, Bun process protocol messages, future Wasm Component calls, or
  another host mechanism. They cannot resolve a second graph.
- Feature behavior is composed through deep Capability Interfaces and ordinary
  Modules. Deleting a Module removes its complexity instead of leaving a
  feature hook and policy residue inside Kernel.
