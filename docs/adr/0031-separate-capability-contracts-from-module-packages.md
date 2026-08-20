# Separate Capability contracts from Module packages

A Module will have one minimal Interface describing its identity, execution entry, provided and required Capabilities, configuration shape, and lifecycle and failure semantics. Each Capability contract belongs to a `namespace.name@major` series and its Descriptor has an independent full semantic version, so different Module packages and package versions can satisfy the same Interface. App Composition resolves compatible Descriptor ranges to one exact version before boot; the Kernel executes that result rather than comparing schemas, digests, or compatibility matrices. Lenso will use Cargo, npm, OCI, and their lock mechanisms for distribution and integrity instead of retaining a Kernel-owned Module Release, signature, digest, attestation, or compatibility-admission model.

## Consequences

- One cohesive Module may provide several Capabilities without adding feature-specific fields to its Interface.
- A Capability declares whether it is `local` to a compatible Runtime Adapter or `portable` across Runtime Adapters. Only portable Capabilities require a cross-runtime descriptor and wire semantics.
- HTTP, Console, Story, telemetry, persistence, and migration behavior are expressed behind Capabilities or inside the owning Module rather than as privileged Manifest sections.
- Module package versions and Capability contract versions evolve independently.
- The initial portable Capability Descriptor is a small language-independent document that declares stable Operations and interaction kinds while referencing JSON Schema 2020-12 for data shapes; neither Rust traits nor TypeScript interfaces become its source of truth, and it does not introduce a general-purpose Lenso IDL. The first acceptance App must bind one portable Capability to both Rust and Bun implementations.
- One Descriptor and Schema source generates the Rust and TypeScript data types, consumer bindings, and provider bindings for a portable Capability. Generated artifacts may ship in contract packages, but parallel handwritten cross-language Interfaces are unsupported because they can drift.
- The Kernel passes Module configuration as opaque data. The Module validates it while preparing; schema declarations exist for authoring tools and Console, not to give the Kernel domain knowledge.
- App Composition contains only non-sensitive configuration or secret references. A Module that needs dynamic resolution declares a Secrets Capability; secret values never enter Composition, ordinary Invocation Context, or Runtime Diagnostics.
- Production provenance or admission policy may be added outside the Kernel, but is not a prerequisite for the initial local runtime.
