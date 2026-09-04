---
status: proposed
---

# Name Plugin dependencies and persist permitted instance choices

Date: 2026-09-04.
Status: proposed; this document does not yet amend ADR 0070 or executable formats.

The [approved authoring design](../proposals/2026-09-04-plugin-authoring-and-lifecycle.md)
requires a Plugin to use two instances of the same Capability without exposing
Plan construction to its author. This ADR specifies the dependency mechanism;
the first implementation specification is tracked in
[Issue #695](https://github.com/LioRael/lenso/issues/695).

## Problem

The current resolver indexes Host attachments by consumer Instance and
Capability ID. Kernel dependency accessors and generated clients also select
by Capability. A source database and a destination database therefore cannot
be independently identified merely by giving their Rust fields different names.

ADR 0070 supports unique matching and Host-private attachments, but does not
allow an App owner to resolve ambiguity by selecting an instance. Recomputing
unique matching after every installation can also invalidate a previously
unambiguous choice. These are contract changes, not only SDK conveniences.

## Proposed decision

### Name the requirement, keep the Capability identity

Each declared dependency has a stable `requirement_id` scoped to its consuming
Plugin Contract. New source uses an explicit attribute such as
`#[dependency(id = "source")]`; the Rust field may be renamed while retaining
that ID. Named IDs use lowercase ASCII letters, digits, underscores, and
hyphens, start with a letter, and contain at most 64 characters.

Requirement metadata carries the ID, Capability ID, exact resolved Descriptor
version, and cardinality. A binding carries the consumer Instance, requirement
ID, provider Instance, Capability identity/version, and existing admission
information. Two requirements may use the same Capability and even the same
provider; duplicate requirement IDs are rejected.

Generated code obtains a dependency view scoped to one requirement before
constructing a client. It never connects both fields using all bindings for the
Capability. Kernel validates and executes the supplied identities; it does not
choose providers, read choice files, or interpret account semantics. Request,
Stream, and Event lookup must preserve the requirement scope even though the
first new portable authoring path is Request-only.

### Keep Host authority explicit

A Host attachment is either fixed or selectable within Host-owned candidate
constraints. Existing private attachments migrate as fixed. Hosts must opt in
to user selection; an empty or absent selectable policy grants no extra access.

For each single or optional requirement, resolution proceeds as follows:

1. Enforce Host policy, contract compatibility, and authority ceilings.
2. If a saved choice exists, validate it and use exactly that target. A missing,
   disabled, incompatible, or newly forbidden target is an error, not fallback.
3. Otherwise use a permitted Host default, then a unique legal candidate. With
   several candidates, report a named choice. With none, fail a required
   dependency or select absence for an optional dependency.
4. Return proposed choices alongside the resolved candidate; never write during
   resolution. Persist choices only through an explicit authoring transaction.

A fixed attachment that conflicts with saved intent requires an explicit
migration/reset. A Host upgrade cannot silently select a different account.
The first wave persists single and optional choices. Existing `many` aggregation
retains its Host-defined membership and ordering; this ADR does not silently
freeze an extension collection or add a user-defined list-selection UI.

### Use one per-instance choice file

The proposed local representation is TOML in
`plugins/<plugin-id>/<instance>.dependencies`, beside the existing configuration
and disabled marker. The extension is deliberately not `.toml`: old parsers
must not mistake this file for another Plugin Instance.

```toml
schema_version = 1

[choices.source]
plugin = "company.database"
instance = "production"

[choices.destination]
plugin = "company.database"
instance = "archive"

[choices.optional_audit]
none = true
```

Every entry has exactly one target pair or `none = true`. Unknown schema keys,
duplicate IDs, malformed instance identities, symlinks, special files, and
files over 256 KiB always fail validation. Choice files refer to a known
configured or Host-default consumer; a choice file alone does not enable a
Plugin. Disabled or otherwise unselected consumers retain their choices.
Their file syntax, schema, and identity encoding are still validated, but
requirement existence, cardinality, target availability, and current Host
permission are checked when the consumer is selected again. For selected
consumers, absence is legal only for optional requirements. Report dormant
invalid choices during inspection without making them an active dependency
failure. A removed requirement must be explicitly removed or migrated before
reactivation, rather than silently orphaned.

This is App intent, not generated code or a disposable cache. Export/copy of
Plugin Root includes it. There is no second author-maintained binding source
under `.lenso/`, no arbitrary Plan editing, and no credential material in the
file. Read-only inspection can show candidates without materializing choices.

Resetting a choice is a validated authoring operation: remove that entry from
the candidate, rerun current default/unique selection, present its effect, and
commit the replacement choice. Ambiguity or an invalid new target leaves the
old file unchanged. Manual deletion of the file is an explicit loss of saved
intent and is treated like a fresh selection, not a hidden cache repair.

### Materialize choices before the first activation

Managed startup resolves a candidate and, if it contains new implicit single
or optional choices, publishes those choices through the authoring authority
before preparation or activation. It then executes the exact resulting
snapshot. A startup command authorizes this initialization of App intent;
inspection and validation commands remain read-only. A publication conflict
or write failure stops startup before Plugin initialization and asks for repair
or a retry, rather than executing a selection that was never saved.

A read-only deployment must provision all such choices before launch. A failed
activation retains the committed choices, reports startup failure, and does
not claim that the App was ever ready. Future starts use the same choices until
an explicit change. Selecting an optional absence is also a persisted choice.

An explicit repair transaction may start from an unresolved root: it validates
the complete repaired candidate, retaining every unaffected valid saved choice.
Do not require the broken root to become ready before repairing it. A repair
does not authorize guessing replacement targets for other broken choices.

### Publish choices consistently with other App changes

Extend the existing Plugin Root revision, lock, and compare-and-swap authority;
do not create an independent selector service. The existing revision is semantic:
extend it with parsed choice intent alongside configuration, enablement,
package identity, and declared resource inputs. Formatting-only edits do not
change this revision. Preserve source-byte digests separately for files that
a transaction will replace, so a user's comments or formatting are not lost.
Mutable Plugin-owned data is not part of the authoring revision. Revalidate the
Host catalog/policy used to resolve the proposal before publication; a changed
Host must not inherit approval of a different candidate. A stale proposal
fails before writing. Adding a provider must first preserve the old valid
choices of existing consumers, including choices that were previously implicit.

Multi-file changes require staged bytes and recoverable publication under the
same writer lock. Readers obtain a consistent snapshot under that authority.
A recovery journal may record preimages, digests, and transaction completion,
but is operational recovery state, not a second source of binding intent.
Recovery restores an entirely committed snapshot or its predecessor before
normal resolution; it never combines half of each. User modifications detected
outside the expected revision cause a conflict, not an unconditional rollback.

The first implementation wave publishes changes while the managed App is
stopped and reports that activation is pending. Host startup and authoring
mutation must coordinate to exclude a start-versus-write race; merely checking
whether a PID appears alive is insufficient. This stays within the local Host
and authoring authority, without a selector daemon or a new public concept.

An existing live-update path may continue only for formats and changes it
already supports. Live publication of these new named-choice changes is
deferred until its Host can coordinate staging, publication, activation, and
recovery. In particular, a stateful replacement must stop the old writer before
opening the replacement's exclusive resources. After that stop, candidate
failure means the App remains stopped; restoration is possible only after
checking data compatibility. The CLI must not promise that the old App remains
active through a controlled restart, or that reverting a choice file undoes
external data effects.

### Version the structural change

The inspected Plan schema is version 2. Named requirements require the next
schema version (3 against that baseline) and corresponding versioned Plugin
Contract/Bundle representation (V4 against the inspected V3 release format).
The implementation must recheck version allocation before publishing. Simply
adding fields that older readers ignore is unsafe: it could reconnect two
fields to the same provider.

Carry requirement identity through generated metadata, native handles, process
initialization, and guest import handles. Advance the relevant existing
handshake/profile revision and reject unsupported peers before business calls;
keep the existing transport instead of introducing a parallel protocol. A Host
Catalog and its executable must agree on this feature. Older tools reject the
new root entry/format; downgrade instructions must not recommend deleting it
to make the error disappear.

Old packages with one requirement per Capability can normalize into a reserved
internal legacy identity derived losslessly from the Capability ID. This is an
input translation into the new model, not a second runtime. Old Capability-only
accessors remain valid only when one declared requirement matches; ambiguity
must fail even when both requirements happen to select the same provider.

When rebuilding a Plugin with named fields, migrate a previous single
requirement only if exactly one new field matches its Capability and
cardinality. Splitting one old requirement into two fields requires a deliberate
choice. Exact provider Instance identities are preserved; no migration chooses
by installation order or guesses from labels. New named fields must retain
their stable IDs across source refactors. Renaming the ID itself is a breaking
configuration change in the first wave; alias/mapping syntax is deferred.

## Alternatives

- **Keep Host-private selection only:** preserves ADR 0070 but cannot provide
  ordinary multi-account selection without custom Host code for each case.
- **Invent a Capability ID per field:** confuses a consumer's source/destination
  roles with the provider's interface identity and duplicates contracts.
- **Use one `many` dependency and filter in business code:** exposes selection
  policy and unrelated providers to the Plugin, and leaves mistakes until call
  time.
- **Re-resolve every choice on every startup:** makes installing another legal
  provider change established behavior or cause unexpected ambiguity.
- **Store bindings in business configuration:** collides with Plugin-owned
  schemas and makes Host routing authority look like business data.
- **Keep choices only in `.lenso/` cache:** makes a cache deletion change account
  selection and loses intent when exporting Plugin Root.

## Consequences and scope

This proposal amends ADR 0070's public choice restriction if accepted. It keeps
immutable Plans, Host authority, strict readiness, and ADR 0071 implementation
equivalence. It does not add partial boot, automatic fallback, shared-state
migration, scheduling APIs, or new Wasm system access.

`lenso-app-plan` owns identities, validation, and pure selection; Kernel owns
scoped handle routing only. `lenso-protocols` owns generated client projections;
`lenso-runtime-rust` owns authoring and execution lowering and Bundle versions.
The `lenso-app-authoring` library in `lenso-cli` owns local persistence; product
Hosts own runtime coordination and admissible choices. Public terminology stays
at a named dependency and a selected instance.

Acceptance requires separate source/destination calls to reach their selected
providers, choices to survive unrelated installations and restart, old packages
to preserve behavior, and incompatible peers or interrupted publication to
fail without silently changing providers. Detailed work ordering and review
criteria belong to the linked implementation Issue, not a duplicate local plan.
