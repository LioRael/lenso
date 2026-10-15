---
name: lenso-migrate
description: Translate real Lenso TypeScript plugins/applications to native lenso-rust, or Rust back to TypeScript, preserving instance identity, configuration, lifecycle, errors, async/cancellation, storage and Runtime boundaries. Use for a port or parity assessment, not a rewrite of unrelated code; Go mappings are preliminary until its SDK delivery is verified.
---

# Port behavior, not framework spelling

Keep local business functions native. A TS↔Rust port does not require Runtime, a JSON contract, or one package per service. Introduce an explicit transport contract only for a real service boundary.

1. Identify the source behavior and exact installed/pinned target APIs. Record whether the target is current source, published artifact, local candidate or unavailable. Use [Capability map](references/capabilities.md); never infer a Rust crate from a TS package name.
2. Inventory one feature's input/output, precise dependency instances, config, resource ownership, effects, cleanup order, admission/cancellation and persistence. Read only its service, factory and assembly plus the target public contract.
3. Port the ordinary business function first. Wire it to explicit target instances; then attach only the requested HTTP/CLI/Runtime adapter. Preserve authorization at the resource owner and keep identity evidence separate from business data.
4. Resolve non-mechanical differences explicitly using [Semantic translation](references/semantics.md) and, when relevant, [Storage and Runtime](references/boundaries.md). Document a gap instead of supplying an invented compatibility wrapper.
5. Run a native consumer in each language and compare observable values/errors/owned cleanup. For a wire change add a generated cross-language call. For a persistence change use the actual provider or report it untested. Do not treat two compiling programs as proof of all semantics.

Use the [paired native catalog example](references/examples.md) to see exact instance wiring and explicit shutdown with real public APIs. It includes two providers of the same service type and failure rollback, so incorrect instance substitution is observable.

Go has its own `lenso-go` skill and local core API; it is not a mechanically translated Rust lifecycle. Read the target Go delivery before adding Runtime examples or claiming TS/Rust/Go parity. New Rust Limits/Authorization/Audit/Storage/Realtime remain pending relative to this skill's baseline.
