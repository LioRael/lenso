# Statically link native Rust Modules in v1

The first native Rust Execution Adapter will compile Module packages into the
App binary as ordinary Cargo dependencies. App Composition still selects
Module Instances, configuration, and bindings, while generated typed provider
traits and stable handles dispatch directly without portable serialization.

## Consequences

- v1 does not define a stable Rust dynamic-library ABI, load arbitrary `cdylib`
  artifacts, or require `unsafe` plugin loading. Rust packages are rebuilt with
  the App toolchain.
- Generated `CapabilityKey<T>`, provider traits, and `Handle<T>` keep consumers
  typed. Kernel may erase implementation types while constructing the graph,
  but consumer code performs no string Registry lookup or public `Any`
  downcast.
- A native handle remains stable across an Adapter-supported provider
  generation restart and does not expose the concrete provider struct.
- Static linking does not make the Module graph implicit. The Resolved App Plan
  remains the authority for which linked factories are instantiated and how
  their Capabilities bind.
- A future dynamic library, Wasm Component, or out-of-process Rust Adapter must
  justify its own seam and conformance behavior rather than weakening the first
  native path in advance.
