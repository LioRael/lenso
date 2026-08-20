# Layer the Rust implementation around the portable Kernel

The vNext Rust implementation will use one-way responsibility layers: contract
and Resolved Plan data, the portable Kernel engine and its narrow public
Interfaces, host Runtime Drivers and Execution Adapters, Module authoring SDKs,
and build/code-generation tooling. These are architectural layers rather than a
requirement to create one shallow crate for every noun.

## Consequences

- Kernel owns only graph and binding state, lifecycle, request/stream/event
  invocation, bounded admission, managed scopes, readiness, supervision, and
  Runtime Diagnostics.
- Kernel receives a typed, already validated Resolved App Plan and an explicit
  set of Runtime Driver and Execution Adapter implementations. Runners and
  tooling parse files, inspect package-manager outputs, and load environment
  configuration.
- A thin App Runner installs the selected Driver and Adapters, starts Kernel,
  translates host shutdown into a Kernel request, and handles the terminal
  outcome. It does not acquire HTTP, database, Console, Auth, or Worker product
  responsibilities.
- The public `lenso` facade may re-export stable App-authoring Interfaces, but it
  cannot repeat the current pattern of conditionally exposing Console, System
  Plane, Workload Control, HTTP, migrations, and business runtime internals.
- Rust and TypeScript Module SDKs expose the same logical Module lifecycle and
  Capability contracts. Neither SDK depends on Kernel implementation details,
  and `Service Kit` does not remain a peer authoring model.
- Existing `platform-*`, Service, Provider, Host, and module-management crates
  are migration sources, not mandatory compatibility layers in the new crate
  graph.
