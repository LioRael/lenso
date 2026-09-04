---
status: proposed
---

# Name Plugin dependencies and preserve permitted instance choices

Date: 2026-09-04.
Status: **Proposed; no executable format or accepted ADR is changed.**

Read the [consolidated authoring design](../proposals/2026-09-04-plugin-usage-walkthrough.md)
first. [Issue #695](https://github.com/LioRael/lenso/issues/695) tracks review.
This ADR narrows the earlier candidate to the dependency semantics that need a
decision. Per-instance choice files, exact identifier syntax, startup writes,
recovery-journal design, Plan schema 3, and Bundle V4 are no longer selected or
reserved by this proposal. Earlier details remain in Git history.

## Problem

The inspected resolver and client projection identify dependencies primarily
by consumer Instance and Capability. Naming two Rust fields source and
destination is insufficient to bind them independently to two providers of the
same Capability. ADR 0070 supports unique matching and private Host attachments,
but not a general App-owner selection interface.

Dependency injection, typed clients, and stable bindings already exist.
Renaming a Port or adding a field attribute alone does not solve this problem.

## Proposed decision

### Identify the requirement independently

Each declared dependency has a stable identity local to its consumer Contract.
Public identity is independent of private Rust field names. Explicit names such
as source and destination are the preferred authoring candidate; exact syntax
and encoding follow the authoring review.

Requirement metadata carries identity, Capability, resolved Descriptor version,
and cardinality. Bindings preserve requirement identity alongside consumer,
provider, and existing admission information. Duplicate requirement identities
fail validation. Distinct requirements may use the same Capability or provider.

Generated clients connect through a requirement-scoped dependency view. Native,
Process, and applicable guest handle projections must retain that scope for
every supported interaction kind. Unsupported formats fail before execution.
Kernel validates and routes resolved identities; it does not choose providers,
read selection files, or infer business account meaning.

### Preserve Host authority and explicit intent

Existing fixed Host attachments remain Host-owned. They do not require a
duplicate user-maintained selection. A Host may permit selection within an
explicitly constrained candidate set; injection never grants extra authority.

For a selectable single or optional requirement:
1. Enforce Host policy, compatibility, and existing authority ceilings.
2. If saved intent exists, validate and use that exact choice. A missing,
   disabled, incompatible, or forbidden target is an error, not fallback.
3. Without saved intent, use a permitted default or unique candidate. Report
   ambiguity; required absence fails and optional absence remains distinguishable.
4. Pure resolution returns a candidate and any choices to materialize. It does
   not write the Plugin Root.

Preserve selected intent across restart and unrelated installations. Before a
managed edit changes candidate membership, preserve valid pre-existing choices
that the edit would otherwise make ambiguous. Do not infer a replacement from
installation order, labels, or interface type alone.

Existing collection requirements retain Host-defined membership and ordering.
Persisting selected single dependencies must not freeze those collections or
silently add a new list-selection interface.

### Keep persistence separate from business configuration

Saved choices belong to exportable App intent, not private business data or a
disposable cache. Prefer writing them during install/configure. The first-start
policy for a newly hand-authored root and the exact local representation remain
open; read-only inspection never writes.

Use the existing authoring authority, semantic revision, and conflict checks.
Protect source bytes separately when replacing files. Revalidate Host policy
and the complete candidate before publication. Publication/recovery must not
combine inconsistent snapshots or overwrite newer edits. The transaction
implementation should be sized to these operations; this ADR does not mandate
a generic journal, a selector service, or one file layout.

A disabled or otherwise unselected consumer retains its intent. Structural input
validation still applies, while current requirement/target validity is checked
before selecting it again. Inspection reports dormant mismatches without
turning them into active dependency failure elsewhere. Repair may begin from an
unresolved root, but must preserve unaffected valid choices and validate the
complete repaired candidate.

Publishing configured intent is distinct from activating it. The Host and
authoring path must coordinate startup and writes. Use supported update paths;
a raw file publication cannot claim live atomic switching or data rollback.

### Evolve existing contracts explicitly

Named identity changes metadata and routing contracts. Allocate and negotiate
the appropriate format/profile versions against the actual implementation
baseline. Older readers must reject unsupported shapes rather than reconnect
two requirements to one provider.

Legacy input with one requirement per Capability can normalize into the new
identity model. Capability-only lookup remains valid only with one matching
declared requirement, even if multiple requirements select the same provider.
A unique compatible old-to-new requirement mapping can preserve the exact
selected provider. Splitting, removing, or changing public identities requires
explicit migration.

This amends ADR 0070 only if accepted. It retains immutable resolved Plans,
existing cardinalities, Host authority, and ADR 0071 implementation equivalence.
App-critical roots and degraded startup are separate architecture proposals,
not consequences silently introduced by this ADR.

## Owners and review boundary

Core owns requirement/binding identities, pure resolution, and scoped routing.
Protocols owns generated contract/client projections. Runtime owns execution
lowering and executable bundle/profile support. App authoring in the CLI
repository owns local persistence; product Hosts own admissibility and runtime
coordination. Business state and migration remain with their Plugin owners.

Review is complete when equal-Capability requirements have unambiguous
independent identity, selection authority and persistence semantics are agreed,
and migration cannot silently redirect calls. Implementation specifications and
validation evidence follow separately in GitHub Issues.
