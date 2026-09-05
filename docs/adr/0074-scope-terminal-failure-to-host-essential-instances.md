---
status: accepted
---

# Scope terminal Plugin failure to Host-essential instances

Date: 2026-09-04.
Status: **Accepted by the repository owner; implemented for explicitly supported
Host/executor profiles on 2026-09-05.**
Amends: ADRs 0032, 0046, and 0048 on explicit Host/profile adoption.

## Context

A dependency required by one consumer is not necessarily required for the
App's minimum useful operation. The existing required-path rule can terminate
the App when a metrics store fails solely because a statistics Plugin requires
it, even when the product's essential editor does not use that path.

The Host knows the product's minimum useful operation. Neither Kernel nor a
Plugin author should infer that policy from an arbitrary consumer's required
binding. See the approved
[failure-scope walkthrough](../proposals/2026-09-04-plugin-fault-scope.md)
for examples and the implementation baseline.

## Decision

Keep strict all-selected initial startup under ADR 0046. After readiness, the
Host's essential logical instances and their transitive required dependencies
determine whether exhausted runtime recovery is terminal for the App.

The Host declares the essential selected instances as composition policy.
Resolution validates those declarations and computes a fixed point by following
resolved `one` requirements from consumer to provider. Shared providers and
transitive providers are included. Optional bindings and zero-or-more
collections do not imply indispensable members; if a specific member is
indispensable, express a required dependency or declare that instance essential.
There is no inferred quorum, failover group, or alternate-account selection.

Materialize the effective terminal policy into a complete immutable Plan before
execution. Kernel executes that policy without understanding editor, statistics,
or other product meanings. Recompute it through supported Plan Transitions or
Generation changes when composition changes. A Plugin cannot downgrade an
authorization requirement through an optional declaration or self-assigned
importance level. Host policy and ordinary authorization remain authoritative.

On actual runtime failure, preserve existing bounded supervision and stable
handles. While a provider is unavailable, calls through its binding report
unavailability; supported recreation reconnects that same logical instance.
An essential instance exhausting recovery causes terminal App failure and
bounded shutdown. A nonessential instance remains unavailable with its cause
visible, while unrelated work continues. Do not automatically destroy consumers,
replay calls, or substitute another provider. Consumer runtime failures are
supervised separately using the same resolved policy. A policy that forbids
restart, or an Adapter without recreation, reaches the same terminal decision
without inventing an extra attempt.

A domain error, rejected authorization, cancellation, or invocation timeout
alone is not proof that an instance is unhealthy. Diagnostics retain desired
instances and affected required paths; an unavailable dependency does not mean
that every independent operation on a live consumer has failed.

## Guarantees retained

- Every selected instance must prepare and activate before initial App readiness.
  Restarting the whole App applies that requirement again. This ADR does not
  permit partial startup or silently remove desired instances from a Plan.
- Disablement, removal, and replacement remain validated desired-state changes.
  Reject an edit that leaves a selected consumer without its required provider;
  recovery never undoes user disablement.
- Cleanup remains bounded and effects remain uncertain where the underlying
  operation cannot establish otherwise. Code rollback is not data rollback.
- Logical importance is not physical isolation. Native abort, a shared-process
  exit, or Driver/Runner failure can affect multiple instances or the whole App.
  Adapters report the actual scope; apply the policy to all affected instances.
  No logical policy promises survival of a process abort or non-cooperative code.
- Named requirements under ADR 0073 and this fault policy have independent
  adoption. Updating an SDK must not silently change either policy.

## Adoption and compatibility

Existing Plans and supported Host/runtime combinations retain their prior
semantics until explicit adoption. An absent essential-instance declaration is
not permission to weaken an existing Host's behavior. The adopting Host reviews
the effective closure and identifies which instances cease to cause terminal
failure. Existing explicit critical instances remain essential unless deliberately
changed under that policy.

Specify executable format/profile support against the actual release baseline.
An old executor may consume a lowering only if it preserves the exact chosen
semantics; otherwise reject before activation. Silently discarding the new policy
or reverting to the old any-required-consumer rule is not compatible lowering.
Acceptance reserves no schema number or new command syntax.

## Owners and acceptance evidence

Host owns essential-instance selection. Core owns portable Plan data and
validation, terminal decisions, and structural diagnostics. Drivers/Runners own
scheduling and shutdown; Adapters own physical failure and recreation. Plugins
retain domain health, durable effects, and recovery policy within their contracts.

Delivery must show: strict startup failure; successful bounded recreation with
unchanged binding; nonessential required-path exhaustion without App exit;
essential/transitive/shared-provider exhaustion with terminal failure; optional
absence versus bound failure; rejection of invalid removal; retained old-plan
semantics; unsupported-profile rejection; and truthful physical isolation.
Use portable conformance plus supported real Adapter paths. Design acceptance
is not evidence that the current runtime implements these cases.
