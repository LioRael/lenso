---
name: lenso-module-authoring
description: Implement or change removable Lenso product behavior after its Module boundary and Capability roles are known. Covers native Rust, Bun, Web, stateful, and cross-cutting Modules; route contract, Composition, or host-mechanics work to their owning workflows.
---

# Lenso Module Authoring

Implement one vertical product capability as an ordinary Module. A Module owns
the behavior, state meaning, policy, and managed work that disappear when the
Module is removed. Its execution mechanism changes the factory and Adapter,
not the product type.

## Workflow

1. **Map the authority.** Locate the Module package, repository instructions,
   package-manager manifest and lock, Capability contract package, generated
   artifacts, target Execution Adapter, App Composition, and repository gates.
   Inspect the installed dependency source or current public API instead of
   assuming names from this skill. Finish with one path and owner for every
   artifact that will change.
2. **Write the Module card.** Record the deletion boundary, owned facts,
   lifecycle, final authorization, provided and required Capabilities,
   configuration, external resources, and first observable behavior. Use
   `lenso-business-planning` while any fact, policy, or lifecycle still has two
   plausible owners. Finish when deleting the Module would remove every item
   on the card.
3. **Read the implementation branch.** Read only the branches the selected
   Module needs:
   - [native Rust](references/native-rust.md) for a statically linked Cargo
     package and `NativeModuleFactory`;
   - [Bun](references/bun.md) for a child-process Module built with
     `@lenso/bun-module`;
   - [Web and UI](references/web-and-ui.md) for HTTP endpoints, Web Ingress, a
     Web Shell, Browser Adapter, or UI Contribution; and
   - [stateful and cross-cutting](references/stateful-and-cross-cutting.md) for
     owned persistence, migrations, Auth, Secrets, Audit, OpenTelemetry,
     Workflow, or similar optional behavior.
   Finish when the package layout, factory/entrypoint, and supported Operation
   kinds are concrete.
4. **Implement the Capability edges.** A provider implements the generated or
   Adapter-local Provider Interface and exposes the generated endpoint. A
   consumer constructs its generated Client or typed handle only from the
   lifecycle context's explicit `ModuleDependencies`. Change the Descriptor or
   Operation through `lenso-capability-authoring`; keep another Module's
   private types, storage, and tables outside this package. Finish when every
   cross-Module call maps to one declared requirement and binding.
5. **Implement one fresh generation.** Decode and validate opaque
   configuration; construct the exact endpoint set; reserve reversible
   resources in `prepare`; initialize against active dependencies and spawn
   generation-owned work in `activate`; release work and resources in
   `deactivate`. External ingress waits for the App Ready Gate. Finish when a
   restart creates no shared mutable generation state or leaked task/resource.
6. **Compose the Instance.** Use `lenso-app-composition` to select the package,
   entrypoint, keyed Instance, non-secret configuration or secret references,
   execution class, provided endpoints, requirements, and bindings. Register a
   native factory in the Runner when the selected Adapter requires static
   linking. Finish when the resolved Plan, package lock, and factory identity
   agree exactly.
7. **Prove behavior and deletion.** Follow
   [Module verification](references/verification.md). Exercise success, a
   Domain Error, a Runtime Failure or startup rejection, lifecycle cleanup,
   and the smallest real consumer path. Remove the Module from the test
   Composition and resolve again. Finish when behavior and removal are both
   evidenced without a Kernel feature branch.

Return the Module owner and shape, deletion boundary, provided and required
Capabilities, package/factory/entrypoint paths, lifecycle and state choices,
Composition changes, generated artifacts, exact checks, behavior proof,
removal proof, and delivery state.
