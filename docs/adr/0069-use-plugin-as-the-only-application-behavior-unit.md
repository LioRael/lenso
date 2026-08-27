---
status: accepted
---

# Use Plugin as the only application-behavior unit

Lenso exposes Plugin as the single authored, selected, configured, and executed
application-behavior unit. Behavior embedded in a Host, bundled with a product,
or installed later differs only by distribution and user authority; it does
not become a separate Module abstraction. Capability contracts and runtime
Driver or Execution Adapter mechanics remain separate because they are not
application behavior. Existing `Module*` implementation types may remain as a
private compatibility lowering while they are migrated, but Module is no
longer a public product, authoring, App Definition, diagnostic, or operational
concept.

## Considered options

- Keeping Module for built-ins preserves existing code but forces App owners to
  learn a second behavior model and makes installability change identity.
- Treating Plugin only as packaging leaves the authoring and runtime seams
  split, recreating the dual path this decision removes.

## Consequences

Base behavior is represented by required embedded Plugin Releases pinned by an
App. Optional bundled and installed Releases use the same Plugin lifecycle.
Public `lenso module ...` authoring and `modules` App Definition fields must be
retired through explicit migration errors; internal Kernel migration can occur
incrementally without blocking the public simplification.
