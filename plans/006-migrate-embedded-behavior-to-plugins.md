# Plan 006: Migrate embedded Host behavior to Plugins

## Status

- **Priority**: P1
- **Effort**: XL
- **Risk**: HIGH
- **Depends on**: ADR 0069 and the Plan 005 candidate proof

## Goal

Make Plugin the only authored, selected, configured, and executed application
behavior unit. Embedded, bundled, and installed describe distribution and user
authority; they do not create different programming models.

## Public target

```text
lenso plugin new <id> [--runtime rust-wasm|embedded-rust|bun|quickjs|process]
lenso plugin check
lenso plugin dev
lenso plugin pack          # when the distribution is independently packaged
lenso app add <plugin>
lenso app remove <plugin-instance>
```

An App Definition selects `plugins`, including required embedded Releases. It
does not expose `modules`, Module Descriptors, or runtime factories.

## Migration sequence

1. Add `embedded-rust` as a Plugin target that reuses the existing native Host
   implementation behind generated Plugin source and diagnostics.
2. Add Plugin-named source attributes and Descriptor evidence; keep old Module
   attributes as deprecated read compatibility only.
3. Teach App authoring to accept `plugins` and lower it to the existing runtime
   plan without changing Kernel semantics.
4. Migrate first-party Apps and examples, proving that required base behavior,
   optional bundled behavior, and installed behavior share one identity model.
5. Remove `lenso module ...` and `modules` authoring fields with explicit
   migration diagnostics after the compatibility window.
6. Rename private `Module*` runtime identifiers separately where doing so
   improves maintainability; do not make that rename a product gate.

## Boundaries

- Capability remains the versioned contract between Plugins.
- Runtime Driver and Execution Adapter remain Host mechanics, not Plugins.
- Kernel still receives one complete immutable execution input and performs no
  discovery, acquisition, or product policy.
- A required embedded Plugin may be non-removable by an end user while still
  being replaceable by the App owner at build time.

## Done criteria

- Normal CLI help contains Plugin and App, not Module.
- A new embedded behavior project uses only Plugin vocabulary from scaffold to
  App selection.
- The same Plugin identity can move from embedded to bundled distribution
  without rewriting its business implementation or Capability contract.
- First-party Harness base behavior is expressed as required embedded Plugins.
- Compatibility removal has tests and actionable migration errors.
