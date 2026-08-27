---
name: lenso-capability-authoring
description: Create or evolve a Lenso Capability role contract, Descriptor, JSON Schemas, compatibility decision, and generated consumer/provider bindings. Use for explicit Plugin collaboration Interfaces, not private helpers, App configuration bindings, or provider business behavior.
---

# Lenso Capability Authoring

Author one deep role Interface that consumers depend on without knowing a
provider's package, storage, process, or concrete type.

## Workflow

1. **Resolve contract ownership.** Identify the consumer goal, eligible
   providers, every real consumer, current Descriptor or local Interface,
   package owner, generated-file boundary, previously accepted version, and
   repository gates. Business contracts normally stay with their domain;
   deliberately standardized roles with independent reuse may belong in a
   protocol repository. Finish when one package owns the source and release.
2. **Define the role.** Name the Capability `namespace.name@major`. State what
   every consumer may rely on and what providers keep private. Apply
   [contract shape](references/contract-shape.md). Finish when two providers
   could satisfy the role without sharing storage, transport, or internal code.
3. **Choose interaction and portability.** Use request for one terminal result,
   stream for one bounded bidirectional session, and event for volatile
   independently admitted fan-out. Read
   [stream and Event contracts](references/stream-and-event.md) when either is
   present. Mark the contract portable only when supported consumers/providers
   cross Execution Adapters; decide `cross_lane_transfer` from actual placement
   needs. Finish when every Operation has one stable name, interaction kind,
   input/output shape, Domain Error set, and portability decision.
4. **Author one source.** Follow the
   [request Capability recipe](references/request-capability.md) for the package
   layout, Descriptor, Schemas, generator commands, freshness gate, generated
   Provider, and generated Client. Keep runtime failures separate from Domain
   Errors and keep cardinality/bindings in App configuration. Finish when the
   Descriptor and package-local Schemas contain the entire portable contract.
5. **Generate and integrate.** Run the installed
   `lenso-contract-codegen --help`, then its generate and check workflows.
   Compile/typecheck both generated targets that the contract publishes. A
   provider implements the generated Provider Interface; a consumer uses the
   generated Client/handle. For guest execution, use the generator's plan-bound
   host-import bridge output when the selected Adapter supports it; do not
   handwrite a second guest dispatch table. Custom behavior stays outside
   generated files.
   Finish when changing the Descriptor without regeneration makes the
   freshness gate fail.
6. **Classify compatibility and prove behavior.** Follow
   [evolution and verification](references/evolution-and-verification.md).
   Compare with the previous accepted Descriptor, choose patch/minor/new-major
   intentionally, and exercise every changed Operation through at least one
   consumer-provider path. Finish when the version decision, generated diff,
   known/unknown Domain Errors, Runtime Failure path, and cross-runtime vectors
   are observable where applicable.
7. **Hand off ownership.** Route provider/consumer business behavior to
   `lenso-plugin-authoring`; route exact requirement cardinality, provider keys,
   contract inputs, and bindings to `lenso-app-configuration`. Finish when no
   contract decision remains hidden in a Plugin implementation or Plan edit.

Return the role, identity and version, Operations, portability choice, contract
and Schema paths, generator/freshness commands, compatibility result, consumers
and providers exercised, and remaining handoffs.
