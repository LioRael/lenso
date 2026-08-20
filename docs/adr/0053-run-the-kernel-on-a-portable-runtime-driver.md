# Run the Kernel on a portable Runtime Driver

The Kernel will be a single-owner asynchronous state machine over a small
Runtime Driver Interface rather than an application built directly on Tokio or
an operating system. A native Runner supplies a Tokio Driver. Browser/JavaScript
and WASIp2 Runners supply host-specific Drivers while executing the same Kernel
state machine and conformance behavior.

## Consequences

- The portable Kernel depends on futures, bounded queues, cancellation,
  durations, and Driver-owned handles. It does not directly use Tokio,
  `std::process`, OS signals, filesystem, sockets, environment variables,
  wall-clock time, WASI imports, or `wasm-bindgen`.
- The Runtime Driver Interface supplies a local task lane, join and cooperative
  cancellation support, scheduler wake or yield behavior, monotonic time and
  timers, and deterministic or entropy-backed restart jitter. The Runner drives
  the root Kernel future and requests shutdown.
- Kernel deadlines use Driver monotonic instants. Wall-clock correlation belongs
  to diagnostics and telemetry Modules or their host Adapter.
- The portable baseline assumes one owner thread and does not impose universal
  `Send` task bounds. Native Drivers and Execution Adapters may expose optional
  parallel execution without changing Kernel correctness or Capability
  semantics.
- Runtime Driver and Execution Adapter are separate seams. A Driver makes the
  Kernel progress; an Execution Adapter creates Module generations and endpoints
  using facilities that its host actually provides.
- The same Kernel crate is compile-checked for native,
  `wasm32-unknown-unknown`, and `wasm32-wasip2` without target-specific `cfg`
  inside the core state machine. Browser and WASIp2 Runners remain separate
  artifacts because their host ABIs are different.
- A Wasm host does not automatically gain a Bun child-process Adapter. Plan
  validation rejects unavailable execution classes unless the embedding host
  explicitly supplies a suitable bridge.
- Kernel returns a terminal outcome rather than exiting a process. Native,
  browser, and component hosts decide how to terminate or recreate the enclosing
  instance.
- Panic unwinding is not a portability mechanism. A Wasm trap or native abort
  may require the outer Runner to recreate the entire Kernel instance.

## Verification gate

Before this decision is considered implemented, one portable Kernel test suite
must run graph validation, staged activation and rollback, bounded invocation,
deadline, cancellation, diagnostics dropping, and shutdown through a
deterministic test Driver. CI must compile the same engine for both Wasm targets;
browser and WASIp2 smoke tests must exercise a local task and monotonic timer,
and native typed dispatch must retain its no-serialization path.

Primary-source constraints and remaining host-profile questions are recorded in
[`../research/lenso-vnext-wasm-kernel-portability.md`](../research/lenso-vnext-wasm-kernel-portability.md).
