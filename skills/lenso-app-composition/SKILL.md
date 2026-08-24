---
name: lenso-app-composition
description: Edit a Lenso App project to select locked Module packages, keyed Instances, configuration, Capability contracts and bindings, execution classes or lanes, then check and materialize the immutable Resolved App Plan. Use for graph choices rather than Module behavior.
---

# Lenso App Composition

Select the exact Modules that make one App and resolve the entire executable
graph before boot. App Composition owns choices; package managers own package
acquisition and locks; Kernel receives only canonical Resolved Plan bytes.

## Workflow

1. **Load the authoring authority.** Find repository instructions, `lenso.json`,
   package manifests/locks, Capability Descriptors and generated bindings,
   configuration Schemas, optional profiles, the last approved Plan, and the
   installed `lenso --help`. Read the
   [project document recipe](references/project-document.md). Finish when every
   path is classified as authored, generated, package-manager-owned, or
   canonical output.
2. **Select locked packages.** Add Cargo, Bun, npm, OCI, source, or trusted UI
   inputs through their ordinary package managers. Record the runtime package
   identity, package-manager name when different, source, requested version,
   exact locked revision, manifest, and lockfile. Preview every authoring-tool
   edit. Finish when each selected Module package resolves to one immutable
   ordinary lock entry or OCI digest.
3. **Declare keyed Instances.** Give every Instance a stable App-local key,
   package, exact entrypoint, non-secret configuration or secret references,
   configuration Schema when non-empty, provided endpoints, required
   Capabilities, execution class, and optional lane. The same package may appear
   under several keys with different configuration. Finish when each Instance
   can be recreated from the project document without ambient registry state.
4. **Bind every requirement.** Follow
   [bindings and resolution](references/bindings-and-resolution.md). Bind `one`,
   `optional`, and `many` requirements to explicit provider keys; preserve
   deterministic provider order for `many`. Finish when no required edge is
   missing, ambiguous, incompatible, duplicated, or part of a forbidden
   request/stream activation cycle.
5. **Declare contract inputs and policy.** Add one exact Descriptor version for
   every used Capability and only the generated language projections this
   project owns. Omit `rust` or `typescript` when its projection ships from a
   different package; omission does not weaken checks for declared artifacts.
   Set request admission, Event capacity, and sensitive references only in the
   authoring surfaces that own them. Use other execution policy only when the
   installed project schema exposes it. Finish when owned generated bindings,
   endpoint tables, and requirement versions agree.
6. **Apply optional profiles or placement.** Read
   [Web profiles and lanes](references/web-profiles-and-lanes.md) only for a
   target Web UI or replicated-lane App. Expand every recipe into ordinary
   Module Instances and bindings in the Plan. Finish when the profile/lane adds
   no hidden runtime graph.
7. **Check and resolve.** Run the installed CLI's check and resolve workflows,
   then review both the project diff and canonical Plan diff. Finish when
   package locks, execution classes, entrypoints, configuration Schemas,
   sensitive references, contract freshness, endpoints, bindings, and lane
   placement all pass before boot.
8. **Run exact bytes and prove removal.** Run only the reviewed canonical Plan.
   Change any package, configuration, binding, execution setting, or profile by
   editing and resolving again, then restart the App. Remove each optional
   Module package/Instance/binding in a focused fixture and resolve the
   remainder. Finish when no running Kernel mutation or hidden install state is
   needed.

Return the project and Plan paths, packages selected, keyed Instances,
bindings and cardinalities, execution classes and placement, validation
results, package/contract freshness evidence, reviewed diffs, run evidence,
and whether removing each optional Module leaves a valid Composition.
