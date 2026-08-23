---
name: lenso-business-planning
description: Turn an unclear product outcome into vertical Lenso Module cards, Capability edges, and one executable slice with explicit ownership and proof. Use before contract, Module, or Composition work when facts, policy, lifecycle, or trust still have competing owners.
---

# Lenso Module-first Planning

Turn a product outcome into the smallest set of Modules whose removal also
removes their product complexity. Planning ends with artifact-level handoffs,
not a framework diagram.

## Workflow

1. **Frame the outcome.** Identify the actor, useful result, authoritative
   facts, trust boundary, final authorization owner, and first observable
   success and honest failure. Finish when the outcome can be stated without
   naming Lenso machinery.
2. **Apply the deletion test.** Read
   [the Module test](references/module-test.md) when a concern could belong to
   product behavior, composition, or runtime infrastructure. Classify every
   concern by the complexity removed with it rather than its package, process,
   UI, or database shape. Finish when every concern maps to one Module,
   Capability, Composition, Plan, Driver, Adapter, Runner, or Kernel owner.
3. **Write vertical Module cards.** Group behavior while data ownership,
   lifecycle, authorization, failure policy, and change cadence align. Give
   every mutable fact one Module owner. A process split is an Execution Adapter
   choice, not a new product type. For each Module record deletion boundary,
   facts, rules, lifecycle, provided/required roles, configuration/resources,
   and observable proof. Finish when no card needs another Module's private
   code or tables.
4. **Name collaboration roles.** Introduce a Capability only where one Module
   needs a stable role from another. Name its consumer goal, provider
   responsibility, Operations, interaction kinds, and cardinality; leave
   private implementation details inside the Module. Finish when every
   cross-Module edge can be implemented as an explicit requirement/binding.
5. **Cut one tracer slice.** Select the fewest Module Instances and
   bindings that deliver one useful transition, its authorization, one honest
   failure, and observable evidence. Finish when removing any selected piece
   makes the slice unusable or unprovable.
6. **Check against a worked handoff.** Read the
   [support-ticket example](references/worked-example.md) for the required
   specificity. Mark later Operations, UI, scale, deployment, and durable
   guarantees explicitly rather than designing them into the first slice.
   Finish when the slice names concrete Instances, edges, artifacts, and tests.
7. **Hand off.** Use [planning output](references/planning-output.md). Route
   contract work to `lenso-capability-authoring`, behavior to
   `lenso-module-authoring`, selections and bindings to
   `lenso-app-composition`, and host mechanics to
   `lenso-runtime-extension`. Finish when each remaining action has one primary
   skill and a checkable completion state.

Planning is complete only when every concern has one owner, every cross-Module
edge has an explicit Capability, and the first slice can become one immutable
Resolved App Plan.
