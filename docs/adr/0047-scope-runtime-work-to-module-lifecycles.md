# Scope runtime work to Module lifecycles

ModuleContext will provide Runtime-Driver-backed managed task and resource
scopes tied to one Module Instance generation. Kernel cancels, drains, and releases work in those scopes
when activation fails, supervision restarts the Instance, or the App stops.
Trusted Modules may use raw runtime APIs, but work created outside the managed
scope is outside Kernel's cleanup guarantees.

## Consequences

- Cancellation and deadlines propagate through every supported Capability
  invocation and managed child task. A timeout reports a `DeadlineExceeded`
  Runtime Failure but does not claim to reverse an external side effect.
- App Composition configures bounded admission for each provider binding or
  Operation, including queue capacity and maximum concurrency. Those settings
  are materialized in the Resolved App Plan rather than embedded in business
  Interfaces or one process-wide worker pool.
- A full admission queue reports `ResourceExhausted`; Kernel never creates an
  unbounded fallback queue or persists an invocation automatically.
- Cooperative cancellation is the portable contract. A Module that ignores
  cancellation can delay graceful shutdown only until the App-wide deadline.
- The portable task contract requires only a single-owner local scheduling lane.
  A native Driver or Execution Adapter may additionally run `Send` work in
  parallel, but Kernel correctness never requires threads or a multi-threaded
  Tokio executor.
- Cleanup order and ownership are deterministic for managed tasks, listeners,
  timers, process handles, and registered resources. Durable application effects
  still require Module-owned idempotency, workflow, or compensation semantics.
