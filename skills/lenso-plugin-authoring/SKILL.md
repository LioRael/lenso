---
name: lenso-plugin-authoring
description: Implement or change removable Lenso product behavior as one Plugin Contract and its supported executable implementations. Use for linked Rust, portable Rust, or Bun Plugin behavior, lifecycle, packaging, conformance, and deletion proof; route App-owned configuration and generic Host mechanics elsewhere.
---

# Lenso Plugin Authoring

Plugin is the only public behavior and distribution unit. One Plugin Release
owns one runtime-independent Contract and may carry several exact executable
implementations. Capability is its collaboration contract; execution class is
a Host mechanism, not a second product type.

## Workflow

1. **Map ownership and support.** Locate repository instructions, exact package
   versions and locks, Capability sources/generated projections, Plugin
   Contract, implementation targets, target Adapters, Host Catalog or Plugin
   Root fixture, and repository gates. Run the installed `lenso plugin --help`
   or owner-package help before selecting a workflow. Finish when every API and
   command comes from current source rather than an architecture target.
2. **Write the Plugin card.** Record deletion boundary, owned facts, lifecycle,
   final authorization, provided and required Capabilities, configuration,
   resources, and first observable behavior. Use `lenso-business-planning` if
   any fact still has two plausible owners.
3. **Choose one shipped authoring path.** Read
   [authoring paths](references/authoring.md). Use the CLI scaffold for an
   ordinary portable Rust tool, the `lenso` facade for a linked native Plugin,
   or `@lenso/bun` for a Bun request Plugin. If the needed Capability kind,
   target, packaging path, or SDK is not supported by the selected versions,
   stop at that prerequisite instead of inventing glue. Finish when one owner
   repository supplies the API and executable proof path.
4. **Keep one Contract across implementations.** Configuration Schema/defaults,
   Capabilities, restart policy, criticality, and state semantics belong to the
   Plugin Contract. Entrypoint, target, Execution Class, and immutable runtime
   package identity belong to each implementation. Host policy owns exact
   implementation selection before Plan resolution. Generated lowering owns
   descriptors, endpoints, factories, clients, and runtime entrypoints.
   Finish when every implementation projects the same Contract or is split
   into a different Plugin Release.
5. **Implement explicit edges.** A consumer receives only Plan-bound
   dependencies. Keep another Plugin's private types, storage, and tables
   outside this package. Requirement cardinality is declared by Plugin source;
   provider selection is derived by Host Slot policy and the resolver.
6. **Own one fresh Instance generation.** Omit configuration when stateless.
   Use lifecycle only for Plugin-owned resources or managed work. External
   ingress waits for App readiness. Restart must not share mutable generation
   state or leak tasks/resources.
7. **Expose availability, not activation.** A linked native factory makes the
   Plugin available in the Host Catalog; App configuration determines whether
   an Instance differs from Host defaults. External packages are added under
   `plugins/<plugin-id>/plugin.lenso-plugin/`.
8. **Prove the complete path.** Follow [verification](references/verification.md):
   check, exercise success and honest failures, package when supported, add and
   run through a real consumer, prove cross-implementation equivalence for
   every published implementation, then remove the Plugin and resolve again.

Return the Plugin owner and deletion boundary, provided/required Capabilities,
Contract path, implementation matrix, package/factory/entrypoint paths,
lifecycle and state choices, Host Catalog or Plugin Root changes, generated
artifacts, exact checks, behavior and cross-implementation proof, removal proof,
unsupported prerequisites, and delivery state.
