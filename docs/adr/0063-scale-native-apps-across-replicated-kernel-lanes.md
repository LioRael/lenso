# Scale native Apps across replicated single-owner Kernel lanes

Native Runners will scale an App by running the unmodified single-owner Kernel
as one replica per declared Execution Lane and placing Module Instances on
those lanes through the Resolved App Plan. The Module Instance remains the only
concurrency unit: an Instance executes serially, and parallelism comes from
placing more Instances on more lanes, never from threads inside business code.
A global work-stealing Kernel, runtime Instance migration, and handler-level
thread-pool offloading are rejected.

This extends ADR 0047 (bounded admission, single-owner local lane) and
ADR 0053 (portable Runtime Driver): the optional parallel execution those
decisions reserved for native hosts is realized as replicated lanes rather
than as `Send` bounds inside the portable Kernel.

## Consequences

- Each Execution Lane runs one unmodified single-owner Kernel replica on its
  own thread with its own Runtime Driver. Browser and WASIp2 Runners are the
  one-lane special case; the portable Kernel gains no thread-specific code
  paths, locks, or `Send` bounds.
- Placement is Resolved App Plan data: declared at composition time,
  validated before boot, and auditable and diffable like every other Plan
  decision. Business code never references lanes, threads, or pools.
- Capability semantics are placement-independent. Deadline, cancellation,
  Domain Error, and admission behavior are identical for same-lane,
  cross-lane, and cross-process invocation; only cost differs.
- Same-lane invocation keeps the zero-serialization native path. Cross-lane
  invocation uses `Send` message passing without serialization. Cross-process
  invocation keeps Adapter-owned serialization. Contract generation must mark
  whether a Capability's types support cross-lane transfer, and Plan
  validation must reject a placement whose bindings cross lanes without it.
- Placement automation happens by emitting a new Resolved App Plan from
  observed evidence and re-applying it. The runtime never migrates a live
  Instance, steals tasks across lanes, or rebalances silently.
- Kernel diagnostics expose per-lane CPU time, per-Instance queue depth, and
  cross-lane message share as first-class evidence for placement decisions.
- A blocking call freezes only its own lane. Modules that must block belong
  on a dedicated lane or behind an Execution Adapter; SDKs must not offer a
  same-lane blocking escape hatch.
- A total-order singleton Instance remains bounded by one core on every
  runtime. That ceiling is business structure addressed by key-based
  sharding in App Composition, not a Kernel defect to be fixed with threads.

## Verification gate

Before this decision is considered implemented, a native Runner must run the
same portable Kernel on at least two lanes from one Resolved App Plan;
cross-lane invocation must pass the same conformance suite as same-lane
invocation for deadline, cancellation, Domain Error, and `ResourceExhausted`
behavior; a checked-in benchmark must show near-linear throughput scaling for
a shared-nothing fixture as lanes increase; and browser and WASIp2 smoke tests
must remain unchanged as the one-lane case.
