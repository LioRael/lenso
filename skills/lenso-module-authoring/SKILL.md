---
name: lenso-module-authoring
description: Implement or change removable Lenso product behavior and, when supported, package it as an installable Plugin Release after its Module boundary and Capability roles are known. Covers native Rust, Bun, Web, stateful, and cross-cutting Modules; route App choices or host mechanics to their owning workflows.
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
   - [native Rust](references/native-rust.md) for the public `lenso` facade,
     generated linked factory, and statically linked Cargo package;
   - [Bun](references/bun.md) for a child-process Module built with the
     official `@lenso/bun` package;
   - [Web and UI](references/web-and-ui.md) for HTTP endpoints, Web Ingress, a
     Web Shell, Browser Adapter, or UI Contribution; and
   - [stateful and cross-cutting](references/stateful-and-cross-cutting.md) for
     owned persistence, migrations, Auth, Secrets, Audit, OpenTelemetry,
     Workflow, or similar optional behavior; and
   - [Plugin distribution](references/plugin-distribution.md) only when the
     Module must ship as an installable, governable Plugin Release.
   Finish when the package layout, factory/entrypoint, and supported Operation
   kinds are concrete.
4. **Implement the Capability edges.** Prefer `#[module]` plus one
   `#[provides(...)]` inherent impl; generated lowering owns Provider traits,
   endpoints, Descriptor bytes, factory construction, and linked registration.
   A consumer declares `Port<Client>` fields and receives only Plan-owned
   dependencies. Change the Descriptor or Operation through
   `lenso-capability-authoring`; keep another Module's private types, storage,
   and tables outside this package. Finish when every cross-Module call maps to
   one declared requirement and binding.
5. **Implement one fresh generation.** Omit configuration for a stateless
   Module. Otherwise derive `ModuleConfig` or select one package-owned complex
   Schema. Use `#[module(lifecycle)]` plus `impl Lifecycle` only when the Module
   owns resources or work. External ingress waits for the App Ready Gate.
   Finish when restart creates no shared mutable generation state or leaked
   task/resource.
6. **Compose the Instance.** Use `lenso-app-composition` to select the package,
   entrypoint, keyed Instance, non-secret configuration or secret references,
   execution lane, and real ambiguity decisions. Generated Descriptors and
   linked factories supply endpoints, requirements, execution class, and
   registration. Finish when the resolved Plan, package lock, generated
   Descriptor, and factory identity agree exactly.
7. **Prove behavior and deletion.** Follow
   [Module verification](references/verification.md). Exercise success, a
   Domain Error, a Runtime Failure or startup rejection, lifecycle cleanup,
   and the smallest real consumer path. Remove the Module from the test
   Composition and resolve again. Finish when behavior and removal are both
   evidenced without a Kernel feature branch.

Return the Module owner and shape, deletion boundary, provided and required
Capabilities, package/factory/entrypoint paths, lifecycle and state choices,
Composition changes, generated artifacts, optional Plugin Release/admission
artifacts, exact checks, behavior proof, removal proof, and delivery state.
