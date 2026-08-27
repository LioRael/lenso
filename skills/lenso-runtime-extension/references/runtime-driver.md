# Runtime Driver recipe

A Runtime Driver advances the portable single-owner Kernel on one host task
lane. It provides monotonic time, sleeping, yielding/parking, bounded local task
scheduling, jitter, and Runner-requested shutdown state. It does not create
Plugin factories, endpoints, process protocols, or product policy.

## 1. Implement the current Interface

Inspect the selected `lenso-kernel::RuntimeDriver` trait first. The current
shape is:

```rust,ignore
pub trait RuntimeDriver: Clone + 'static {
    fn now(&self) -> Duration;
    fn sleep_until(&self, deadline: Duration) -> LocalBoxFuture<'static, ()>;
    fn yield_now(&self) -> LocalBoxFuture<'static, ()>;
    fn wait_for_runtime_event(
        &self,
        deadline: Duration,
    ) -> LocalBoxFuture<'static, ()>;
    fn jitter(&self, maximum: Duration) -> Duration;
    fn spawn_local(&self, task: LocalTask) -> Result<DriverTask, SpawnError>;
    fn shutdown_requested(&self) -> bool;
}
```

Use a host monotonic clock relative to Driver startup. `sleep_until` and
`wait_for_runtime_event` take Driver-relative deadlines; wall-clock changes
must not affect them. `spawn_local` returns Driver-owned cancellation and
completion evidence. A panic/host abnormality becomes `TaskOutcome::Failed`;
cooperative cancellation becomes `Cancelled`.

The Driver is intentionally `!Send`-friendly and lane-local. Do not add
thread-safe Plugin state, work stealing, or cross-lane migration to make one
host runtime convenient.

## 2. Keep host APIs behind the implementation

For a Tokio Driver, translate `Instant`, `sleep_until`, `spawn_local`, `Notify`,
and Ctrl-C state without importing Tokio into Kernel. Browser and WASIp2 Drivers
use their host timer/task/event sources behind the same Interface. Target-gated
fallback implementations should fail or provide a truthful limited smoke; they
must not make unavailable host behavior appear supported.

Current source anchors:

- `LioRael/lenso/crates/lenso-kernel/src/driver.rs` for the Interface;
- `LioRael/lenso-runtime-rust/crates/lenso-runner/src/lib.rs` for Tokio;
- `crates/lenso-browser-driver` and `crates/lenso-wasip2-driver` in that runtime
  repository for WebAssembly host examples; and
- the deterministic Driver in `LioRael/lenso` for exact time/control tests.

## 3. Prove Driver behavior

At minimum, prove:

- monotonic `now`, exact/ordered timers, and no early deadline completion;
- `yield_now`/event parking makes progress without a busy loop;
- spawn completion, cooperative cancellation, and task failure classification;
- bounded shutdown observation and wake-up of a parked lane;
- deterministic or bounded jitter; and
- the same Kernel lifecycle/request/supervision conformance on the new Driver.

Compile the portable core for every claimed target and run a real target smoke
when the host is available. A native mock does not prove browser or WASIp2
timer/task semantics.

## Completion

The Driver branch is complete when Kernel uses no new host dependency, one
lane passes portable conformance, real host timers/tasks/shutdown pass a smoke,
and all abnormal host outcomes map to explicit task/runtime results.
