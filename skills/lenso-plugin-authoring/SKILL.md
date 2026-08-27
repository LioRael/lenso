---
name: lenso-plugin-authoring
description: Implement or change one removable Lenso Plugin, including typed Capability edges, configuration, lifecycle, packaging, local execution, and deletion proof. Use when behavior changes; route App-owned selection to lenso-app-configuration and generic Host mechanics to lenso-runtime-extension.
---

# Lenso Plugin Authoring

Plugin is the only public behavior and distribution unit. It owns the behavior,
state meaning, policy, and managed work that disappear when the Plugin is
removed. Capability is its collaboration contract; execution class is a Host
mechanism, not a second product type.

## Workflow

1. **Map ownership.** Locate repository instructions, Plugin package and lock,
   Capability packages, generated artifacts, configuration schema/defaults,
   target Adapter, Host Catalog registration, Plugin Root fixture, and checks.
2. **Write the Plugin card.** Record deletion boundary, owned facts, lifecycle,
   final authorization, provided and required Capabilities, configuration,
   resources, and first observable behavior. Use `lenso-business-planning` if
   any fact still has two plausible owners.
3. **Choose an authoring path.** Read [authoring paths](references/authoring.md).
   Native Rust uses `#[lenso::plugin]`, `#[provides]`, `PluginConfig`, typed
   Ports, and generated linked registration. Portable third-party behavior uses
   `lenso plugin new` and the guest SDK.
4. **Implement explicit edges.** Generated lowering owns descriptors, endpoints,
   factories, and clients. A consumer receives only Plan-bound dependencies.
   Keep another Plugin's private types, storage, and tables outside this package.
5. **Own one fresh Instance generation.** Omit configuration when stateless.
   Use lifecycle only for Plugin-owned resources or managed work. External
   ingress waits for App readiness. Restart must not share mutable generation
   state or leak tasks/resources.
6. **Expose availability, not activation.** A linked native factory makes the
   Plugin available in the Host Catalog; App configuration determines whether
   an Instance differs from Host defaults. External packages are added under
   `plugins/<plugin-id>/plugin.lenso-plugin/`.
7. **Prove the complete path.** Follow [verification](references/verification.md):
   check, exercise success and honest failures, package when portable, add and
   run through a real consumer, then remove the Plugin and resolve again.

Return the Plugin owner and deletion boundary, provided/required Capabilities,
package/factory/entrypoint paths, lifecycle and state choices, Host Catalog or
Plugin Root changes, generated artifacts, exact checks, behavior proof, removal
proof, and delivery state.
