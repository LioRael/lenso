# Materialize a Resolved App Plan before boot

App authors will describe an App through a language-independent declarative App
Composition. Build and authoring tools may expose Rust or Bun builders, but they
must materialize one immutable Resolved App Plan before Kernel boot. That Plan
contains the exact Module Instances, entrypoints, Capability Descriptors,
explicit bindings, configuration references, execution Adapters, and
supervision settings selected for the run.

## Consequences

- Cargo, npm or Bun, OCI, and their ordinary lockfiles resolve and acquire
  packages before boot. Kernel does not download packages, evaluate SemVer, or
  select an artifact at runtime.
- Every `one`, `optional`, and `many` requirement has an explicit binding in the
  resolved Plan. Authoring tools may suggest or generate bindings, but Kernel
  never chooses the first matching provider or consults a global Registry.
- Plan validation rejects missing or ambiguous required bindings, invalid
  entrypoints, incompatible execution classes, and forbidden activation cycles
  before any Module is prepared.
- The Resolved App Plan is execution input, not a new Module Release format,
  package registry, signature policy, or artifact-verification system.
- Changing Composition, bindings, execution settings, or Kernel-level
  configuration requires materializing a new Plan and restarting the App in
  v1. A Module may still expose its own dynamic business-configuration
  Capability without mutating the Plan.
