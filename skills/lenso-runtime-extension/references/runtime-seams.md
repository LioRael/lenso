# Runtime seams

Use the **host-facility test**: if the concern exists because a host must make
Kernel or a Module executable, place it in the narrow Driver, Execution
Adapter, or Runner seam. If selecting the product feature creates the concern
and deleting that selection should remove it, use a Module.

| Seam | Owns | Does not own |
| --- | --- | --- |
| Runtime Driver | local task lane, scheduling, monotonic time, timers, cooperative cancellation, progress | Module factories, endpoints, product policy |
| Execution Adapter | Module generation, endpoint mechanics, execution class, isolation, process or wire translation, host-specific failure semantics | graph resolution, Capability selection, business behavior |
| Runner/Generation control | Driver and Adapter catalog, root Kernel future, host shutdown, exact Generation stage/switch/drain/rollback, terminal outcome | package acquisition, Desired State, product Slots/services |
| Authoring tooling | project files, package-manager inspection, validation, code generation, Plan materialization | running graph mutation, Kernel installation state |
| Kernel | portable graph, lifecycle, invocation, admission, readiness, supervision, diagnostics | OS facilities, networks, databases, Auth, UI, transport, product policy |

Apply both tests before editing:

1. **Deletion:** would removing one selected product feature remove this
   concern? If yes, it is Module behavior.
2. **Translation:** which portable Interface would every supported host need to
   implement differently? That Interface identifies Driver, Adapter, or Runner.

Kernel changes require a portable semantic gap proven across more than one host
or by product-neutral conformance. A missing convenience method in one Adapter
is not evidence for a new Kernel feature.

## Boundary cases

- An HTTP or game Ingress Module may own its selected listener, framing,
  transport limits, protocol behavior, and Capability projection. Host-wide
  listener sharing across replicated lanes belongs to the Runner/Host; a
  process or wire bridge belongs to an Execution Adapter only when it is the
  generic mechanism used to execute Module packages.
- A database client or pool is normally a private Module persistence Adapter,
  not a global Module and not Kernel state.
- Bun child-process framing belongs to the Bun Adapter; the TypeScript business
  implementation remains a Module.
- Web Shell, Browser Adapter, and UI Contributions are Modules. The Browser
  Adapter owns browser-to-App transport and generated-client projection;
  browser host scheduling needed to run the portable Kernel remains a Driver
  concern.

Current ownership is physically split: portable Plan, Kernel, and conformance
remain in `lenso`; Rust Drivers, native Adapter, and Runner live in
`lenso-runtime-rust`; Bun integration lives in `lenso-bun-adapter`; protocol
source and code generation live in `lenso-protocols`; authoring lives in
`lenso-cli`; optional Modules live with their product owners. Verify these
locations before editing because repository ownership may evolve.

Use source search rather than repository names alone: `RuntimeDriver`,
`ExecutionAdapter`, `ExecutionAdapterCatalog`, `PreparedNativeApp`,
`NativeModuleRegistry::with_linked_factories`, `ResolvedGeneration`,
`GenerationController`, and `lenso-contract-codegen` are reliable current seam
anchors. `NativeModuleFactory` remains an internal/compatibility seam, not the
ordinary Module authoring API.
