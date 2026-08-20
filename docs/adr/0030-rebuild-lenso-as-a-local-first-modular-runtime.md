# Rebuild Lenso as a local-first modular runtime

Lenso vNext will be a local-first, language-independent modular application runtime rather than an evolution of the existing Rust-first Service framework. An App statically composes Modules whose Interfaces provide and require Capabilities; the Rust Kernel supplies only reconstructable runtime mechanisms, while product features and durable state belong to optional Modules. The first proof will support native Rust and Bun child-process Modules, allow Console and Story to be absent, require no database, and replace one Module implementation without changing the Kernel.

## Consequences

- `App`, `Module`, `Capability`, and `App Composition` become the canonical product language. `Plugin` remains informal ecosystem language, while `Service`, `Provider`, `Autonomous Service`, `Console Surface`, and `Extension` do not remain peer Module types in vNext.
- `Host` also leaves the public product language. The App is the product unit; Kernel and runtime process names describe implementation only.
- Existing contracts may be migrated or adapted outside the new Kernel, but their compatibility requirements do not shape its Interface.
- The initial App Composition is static after boot. Dynamic installation, remote Module execution, service discovery, placement, replicas, and a Control Plane are deferred.
- The Kernel keeps volatile, reconstructable runtime state and has no mandatory durable State Module. Each Module declares any durable Capability it requires.
- Native Rust and Bun child-process Modules are trusted code in the first release. Process isolation contains faults but is not presented as a security sandbox.
- Trusted Modules may use filesystem, network, environment, thread, and runtime APIs directly. Capability Interfaces are the supported composition and lifecycle boundary, not mandatory mediation of operating-system access.
- The portable Kernel engine must compile for native Rust, `wasm32-unknown-unknown`, and `wasm32-wasip2`. Host facilities and available Execution Adapters vary by Runner and do not become Kernel assumptions.
- Console-enabled, Agent Harness, game-server, and similar presets are authoring recipes that materialize ordinary App Composition entries. They do not create Kernel modes or persistent runtime overlays.
- This decision defines the vNext target; it does not claim that the current implementation or glossary already conforms.
- The motivation and constraints for possible distributed execution are retained in [`../architecture/future-directions/distributed-module-runtime.md`](../architecture/future-directions/distributed-module-runtime.md).

## Supersession

For vNext design and implementation, this decision and ADRs 0031 onward
supersede the product model recorded in ADRs 0001-0029. Those documents remain
historical evidence for the current Service-oriented implementation; their
Host, Service, Provider, System Plane, Module Release, Surface, mandatory Story,
and distributed-runtime assumptions are not vNext requirements.
