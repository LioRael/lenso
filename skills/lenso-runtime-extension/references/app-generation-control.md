# App Generation control

Use this branch for Host mechanics that replace or recover a complete immutable
Plan snapshot. Plugin identity, package admission, implementation selection,
Plugin Root resolution, and product Slot policy stay above portable Kernel;
Plugin behavior stays in Plugins.

## Current boundary

The structural runtime is implemented in `lenso-plugin-control-plane` and
`lenso-runner`: immutable `ResolvedGeneration`, durable compare-and-set
supervision, fenced routing epochs and Leases, complete-lane staging and
readiness, atomic switch, bounded drain, standby, rollback, terminal-failure
reconciliation, recovery, and shutdown.

This does not prove a package marketplace or remote distribution product.
Automatic acquisition, durable product Session fencing, marketplace flows, Hot
Plan Transition, and stable graduation of preview execution classes require
separate evidence.

## Preserve the authority chain

One resolved candidate must close exact canonical bytes for the Plan, selected
implementation and Artifact Set, Effective Host Grants, Host Build, resolution
authority, and Generation Spec. Every
later stage verifies those authorities; it must not execute Plugin code to
discover metadata, broaden a grant, repair a binding, substitute an Artifact,
or select a newer Release.

Use one bounded command controller and one durable Supervisor authority. Fence
stale recovered supervisors with Supervisor and Routing epochs. A route pins
one Generation through a Lease, so admitted work never migrates during switch,
drain, standby, or rollback. Stage every declared lane behind one Ready Gate;
reject partial lane startup.

Treat graceful Host exit separately from Generation retirement. Recovery must
restage durable live records and complete only the recorded authorized rollback
edge. Terminal active failure either performs that exact automatic rollback or
fences routing and retires the failed Generation; never route to a failed
standby.

## Do not mutate Kernel

Kernel executes one immutable Plan Snapshot and validated transitions. It has
no Plugin registry, downloader, filesystem resolver, Store, Plugin Root, mutable
binding graph, or implementation fallback. A structural change uses a fresh
Generation. Use an in-place
Plan Transition only when the accepted whitelist and conformance prove that the
change is hot-applicable; Hot Plan Transition remains incomplete today.

## Proof

Exercise canonical serialization and digest closure, stale CAS rejection,
complete-lane readiness, switch fencing, old-Generation Lease drain, deadline
handling, standby retention/expiry, exact rollback, active and standby terminal
failure, crash recovery, and bounded shutdown. Add a real product Host smoke;
control-plane unit tests alone do not prove product routing or Session
provenance.

Return the authority versions/digests, controller and durable-store owner,
epoch/Lease behavior, readiness and switch result, drain/standby policy,
rollback authorization, recovery evidence, product Host smoke, and incomplete
product-chain layers.
