# Execution target capability matrix

Status: design contract. This document defines the source and admission rules
for a machine-checkable matrix; it does not assert current qualification for
any Driver, Adapter, Environment, or Infrastructure Implementation.

## Purpose

One Plugin model does not mean every selected execution target can perform
every interaction. The matrix makes the actual contract visible before
resolution and startup, so a Host or CLI can reject an incompatible selection
instead of discovering it after a Plugin is running.

The matrix answers the admission question:

    Can this exact execution target admit the required interaction and facility?

Runtime and deployment receipts answer the separate evidence question:

    Which exact Environment and Infrastructure combination has been exercised?

An admitted matrix cell is not a release or qualification claim.

## Matrix axes

Each matrix row is one exact target descriptor. Its identity includes the
Execution Environment, Runtime Driver, Execution Adapter, execution class or
implementation form, and profile revision needed to make an unambiguous
admission decision. A target must not inherit a cell from a similarly named
target.

The required columns are grouped by concern:

| Group | Columns |
| --- | --- |
| Interaction kinds | Request, Stream, Event, WebSocket |
| Host boundary | Host Imports |
| Implementation forms | Native Process, Wasm Component, Remote |
| Target contexts | Browser, Workers |

Cells use these admission values:

| Value | Meaning |
| --- | --- |
| admitted | The descriptor and conformance scope explicitly allow the requirement for this exact target. |
| rejected | The target must fail preflight when the requirement is selected. |
| not-assessed | No owner-backed matrix fact is available. It is never treated as admitted. |

The matrix is intentionally not prefilled with guessed cells. Its values come
from the owner of the concrete Driver or Execution Adapter, not from a generic
list of possible execution classes.

## Source and maintenance rule

Every target owner must provide a locked, revision-bound target capability
descriptor. It may be generated from adapter code and target-specific
conformance declarations, or maintained as an owner-reviewed source artifact
with a matching conformance test. The descriptor records:

    target identity
    source revision
    interaction-kind cells
    host-import cells
    implementation-form cells
    target-context cells
    explicit rejection reasons

The generated or maintained descriptor is the source for a matrix view. A
documentation table, release note, or example may display the view but cannot
invent a cell. Changing an admitted cell requires changing the descriptor and
its target conformance evidence in the owning repository.

The Rust implementations, interaction vocabulary, and CLI explanation live in
this workspace. The JavaScript fixture and SDK side lives in `lenso-js`.

The portable core does not become the owner of target implementations or their
operational facts.

## Resolver and CLI behavior

The existing resolver remains the only graph-resolution authority. Once the
Host has selected an Environment Profile and an exact implementation, resolver
preflight compares the Plugin's declared interaction and facility requirements
with that selected target descriptor.

An incompatible Plan candidate is rejected before materializing or admitting a
Plan. The diagnostic identifies the requirement, selected target identity, and
the descriptor's rejection reason. For example:

    Plugin requires bidirectional Stream.
    Selected implementation does not admit Stream.

Or:

    Selected Workers profile does not admit this interaction.

The CLI calls the same preflight contract during authoring, check, and inspect
flows. It may render the matrix explanation, but it must not duplicate a second
selection algorithm or silently substitute another Artifact.

## Relation to Environment and Infrastructure

The target descriptor describes execution admission. It does not select a
Plugin's storage semantics or claim that a backing resource is available.
Environment and Infrastructure are composed under
[ADR 0076](../adr/0076-separate-execution-environments-from-infrastructure-implementations.md);
a target can admit a Workers interaction while a particular Workers plus D1
combination remains unassessed or rejected by Host resource policy.

The descriptor also does not replace the opaque per-Plugin runtime profile or
execution class fields in existing Plans. It is a Host and Adapter admission
input, not a new App Composition type or global service lookup.

## Delivery boundary

This contract does not add a runtime registry, populate guessed target cells,
or turn admission into a deployment claim. A target change includes descriptor
generation or maintenance, focused conformance, and resolver or CLI preflight;
environment qualification remains an external runtime receipt.
