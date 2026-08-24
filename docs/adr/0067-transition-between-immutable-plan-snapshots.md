# ADR 0067: Transition between immutable Plan Snapshots

- Status: proposed
- Date: 2026-08-25
- Amends: ADR 0045, ADR 0046
- Extends: ADR 0032, ADR 0047, ADR 0063
- Contracts:
  [`../architecture/plugin-authoring-and-resolution.md`](../architecture/plugin-authoring-and-resolution.md),
  [`../architecture/dynamic-plugins.md`](../architecture/dynamic-plugins.md)

## Context

ADR 0045 materializes one immutable Resolved App Plan before boot and ADR 0046
activates it all-or-nothing. Those decisions correctly reject runtime
discovery and graph mutation, but they also fix the unit of immutability at
the whole App. Under that rule every change — installing one additive Tool,
editing one configuration value, updating one provider — requires resolving
and restarting a complete new App, or (under the dynamic-Plugin draft) staging
a complete parallel App Generation. For large or stateful Apps that cost is
real: warm state is lost or must be handed off, and the common small change
pays the price of the rare structural one.

The Kernel already contains the mechanisms an incremental path needs: staged
readiness, dependency-ordered activation, generation-scoped resources and
cancellation, and stable consumer handles across provider restarts (ADR 0032).
What is missing is one sanctioned way to move between two valid graphs.

## Decision

**An App executes a totally ordered sequence of immutable Plan Snapshots. The
Kernel gains exactly one new mechanism: atomically apply a validated Plan
Transition from the running snapshot to its successor at a quiescent point.
Everything else — discovery, installation, resolution, policy, delta
classification — stays above the Kernel in the Reconciler.**

### Snapshots and Transitions

A Plan Snapshot is a complete Resolved App Plan under the existing rules; the
first snapshot is the boot Plan and ADR 0045 semantics apply to each snapshot
unchanged. A Plan Transition is a resolver-produced document binding the exact
digests of two adjacent snapshots and the exact instance-level delta: added
Module Instances, removed Module Instances, replaced provider Instances, and
changed binding sets. The Kernel validates a Transition against the running
snapshot as strictly as it validates a boot Plan, applies it completely, or
rejects it leaving the running snapshot fully authoritative. Kernel input
remains dead, fully resolved data.

### Hot-applicable deltas (V1 whitelist)

The Reconciler classifies each delta. A Transition is hot-applicable only
when every element falls in this whitelist:

- **`many`/`keyed_many` membership change**: add or remove provider Instances
  of a set-valued binding; consumers already face a set and observe the new
  membership only at the atomic switch point.
- **Interface-identical provider replacement**: replace a provider Instance
  whose Module Descriptor Interface digest is unchanged, reusing the ADR 0032
  stable-consumer-handle restart mechanism to retarget the handle.
- **Configuration-only change**: replace one Instance with a new generation of
  itself under the existing restart contract.

Everything else is structural — Execution Lane topology or Placement changes,
Execution Class or Adapter changes, host build changes, provided-Interface
changes, `one` binding rewiring beyond identical replacement, and stateful
replacement without a State Compatibility Receipt — and applies as a complete
App Generation swap under ADR 0046 and the dynamic-Plugin control-plane
contract. The whitelist may grow only through a new decision with conformance
evidence, never through runtime discretion.

### Transition protocol

Applying a hot Transition follows staged all-or-nothing semantics at
Transition scope:

1. added and replacement Instances prepare and become ready while the running
   snapshot keeps serving;
2. the Kernel commits the Transition at a quiescent point for the affected
   bindings — no in-flight invocation observes a mixed binding set, and work
   admitted before the commit completes against the old set under existing
   cancellation and deadline rules;
3. removed and replaced Instances deactivate through ordinary generation-
   scoped teardown after the commit; and
4. any failure before commit unwinds the staged Instances and leaves the
   running snapshot authoritative; failure after commit is ordinary
   supervision on the new snapshot.

Reverting a hot Transition is a new forward Transition to a snapshot equal to
the predecessor when the whitelist permits; otherwise rollback is Generation
rollback. The running snapshot digest, applied Transition digests, and their
outcomes are durable control-plane records and appear in diagnostics and
provenance.

### Invariant restatement

ADR 0045's invariant is amended, not discarded: the Kernel executes only
immutable, completely resolved snapshots, and between snapshots only a
validated atomic Transition. The Kernel still never discovers, installs,
downloads, selects versions, benchmarks, falls back, or interprets product
policy. ADR 0046's staged all-or-nothing activation now applies at two scopes:
whole-App (boot and Generation swap) and Transition.

## Consequences

- Installing an additive extension, updating an Interface-identical provider,
  and changing configuration no longer restart the App or lose warm state;
  the App-facing experience of ADR 0065 becomes cheap in its common cases.
- The Kernel state machine grows one epochal mechanism and its conformance
  surface: the deterministic test Driver must exhaustively exercise Transition
  timing against invocation, cancellation, restart, and shutdown. This is the
  largest new risk and the reason the whitelist starts minimal.
- The plan-data model gains snapshot sequence identity and the Transition
  document; `lenso-app-plan` owns their serializable forms.
- Reproducibility strengthens rather than weakens: an App's history is a
  totally ordered, digest-chained sequence of snapshots and Transitions
  instead of one opaque restart trail.
- The Generation swap path remains fully load-bearing for structural change,
  so a conservative deployment may disable hot Transitions entirely and
  retain correct, slower behavior.
