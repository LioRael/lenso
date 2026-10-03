---
name: lenso-plugin-authoring
description: Implement Lenso behavior through linked Rust, portable Rust, Bun, or selected file conventions. Start with one required implementation and prove affected behavior; add contract, lifecycle, packaging and removal checks when changed or claimed.
---

# Lenso Plugin Authoring

Plugin is the only public behavior and distribution unit. One Plugin Release
owns one runtime-independent Contract and may carry several exact executable
implementations. Capability is its collaboration contract; execution class is
a Host mechanism, not a second product type.

## Workflow

For a stateless local handler: inspect current help/source, edit through the
existing macros, run affected tests and a real `dev`/Host request, then report
the result. The rules below explain conditional ownership boundaries, not
eight mandatory checkpoints. Do not add unused lifecycle, targets or languages.

1. **Map ownership and support.** Locate repository instructions, exact package
   versions and locks, Capability sources/generated projections, Plugin
   Contract, implementation targets, target Adapters, Host Catalog or Plugin
   Root fixture, and repository gates. Run the installed `lenso plugin --help`
   or owner-package help before selecting a workflow. Finish when every API and
   command comes from current source rather than an architecture target.
2. **Identify the behavior owner.** For a small, clear request, use a short
   statement of the owning Plugin, required target and observable result and
   proceed to code. Ordinary helper modules need no Plugin identity. Write a
   Plugin card or use `lenso-business-planning` only when facts, lifecycle or
   authorization still have competing owners. Introduce Capability authoring
   only for a real collaboration edge.
3. **Choose one available authoring path.** Read exactly one path first:
   - [optional file conventions](references/paths/conventions.md) for CLI
     commands, App Console pages/services, or Agent Tool source entries;
   - [portable Rust Agent Tool](references/paths/portable-rust.md) for the CLI
     scaffold and Wasm/Process Release;
   - [linked native Rust](references/paths/linked-rust.md) for a Host-linked
     Plugin and generated registration; or
   - [Bun request Plugin](references/paths/bun.md) for a generated TypeScript
     Provider behind the Bun Adapter.
   If the needed Capability kind, target, packaging path, or SDK is not
   supported by the selected versions, stop at that prerequisite instead of
   inventing glue. Finish when one owner repository supplies the API and real
   execution proof path.
4. **Keep one Contract across implementations.** Apply
   [the shared Contract and lifecycle rules](references/contract-and-lifecycle.md).
   Host policy owns exact implementation selection before Plan resolution.
   Finish when every implementation projects the same Contract or is split
   into a different Plugin Release.
5. **Implement explicit edges.** A consumer receives only Plan-bound
   dependencies. Keep another Plugin's private types, storage, and tables
   outside this package. Requirement cardinality is declared by Plugin source;
   provider selection is derived by Host Slot policy and the resolver.
6. **Own one fresh Instance generation.** Omit configuration when stateless.
   Use lifecycle only for Plugin-owned resources or managed work. External
   ingress waits for App readiness. Restart must not share mutable generation
   state or leak tasks/resources. Finish when preparation, activation,
   deactivation, cleanup, and recreation evidence match the Contract.
7. **Expose availability, not activation.** A linked native factory makes the
   Plugin available in the Host Catalog; App configuration determines whether
   an Instance differs from Host defaults. External packages are added under
   `plugins/<plugin-id>/plugin.lenso-plugin/`.
8. **Prove the affected path.** Follow [verification](references/verification.md):
   run affected checks and a real consumer/request. Add lifecycle, removal,
   packaging and cross-implementation proof only when changed or claimed.
   Local source does not require `pack` or a frozen release. Implement only the
   target/storage needed now, using existing macros and lowering. Preserve
   per-target support validation and resource binding with shared App semantics.

Return the implemented result, changed paths, actual checks and behavior,
remaining blockers and delivery state. Include lifecycle, contracts or
implementation matrices only when they help assess this change.
