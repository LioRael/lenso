# Semantics that need an explicit translation

## Exact dependencies and configuration

| TS source | Rust target | Reverse-port consequence |
| --- | --- | --- |
| Factory returns `Plugin<Service>` object | `Instance::new(name, plugin)` owns one definition; `.handle()` gives `Handle<Service>` | Preserve one plugin object through composition, not reconstructed equal IDs |
| `requires: [provider]` | `dependencies() -> vec![provider.key()]` | Declare the original provider object before `context.get` |
| `context.get(provider)` / `app.get(provider)` | `setup.get(&handle)` / `running.get(&handle)` returns `Arc<Service>` | Sharing a TS object is not Rust Arc ownership or Send/Sync safety |
| Distinct object for each provider | Distinct Instance, even with same plugin/service type | Use distinct diagnostic IDs/names; never resolve by type or string |
| Optional factory argument | Explicit `Option<Handle<_>>` plus matching declared edge | An absent branch must not call undeclared get |

Rust plugins move into App and consume `self` at setup. Clone handles before moves; do not clone the owning Instance. Plain Rust data/factory arguments need no JSON. TS ordered sources replace whole top-level fields; Rust Config selects explicit JSON and JSON-valued env pointers over serde defaults. Neither rule is a deep merge. Config values are not connections; validate before acquisition and mark sensitive diagnostics deliberately.

## Setup, rollback and cleanup

| Boundary | TS Core | Rust Core `e5dfafb` | Go local core |
| --- | --- | --- | --- |
| Start | Validate graph/config, serial setup | Validate graph/config, serial setup in owned Tokio task | Plan/Validate then serial Setup(ctx, setup) |
| Register | `context.onCleanup(fn)` returns async disposer | `setup.on_cleanup(phase, fn)` returns CleanupHandle | `setup.OnCleanup(fn)` returns error; no disposer API |
| Stop | Global sequential LIFO, attempts all | Seal → all Entrances → drain admitted work → Resources; reverse order within phases | Global sequential LIFO, attempts all |
| Repeat stop | Shared Promise, including failure | Shared owned completion/result | Once/shared result; ctx controls each wait |
| Failed partial setup | Its registered callbacks included | Its callbacks included, also unwind panics | Its callbacks included, synchronous callback panic recovered |
| Retained service | Core get rejects after stop; service must define its own state/drain | Gate at outer method entry rejects new work; keep WorkGuard through awaits | Get rejects after Close starts; retained object enforces its own state |

Independent nodes can have different valid topological orders in each implementation. Preserve dependency order and phase/resource invariants rather than comparing complete traces. A TS cleanup registration has no Rust phase argument. Port by identifying entrances and resources; don't flatten Rust's phased order without an explicit TS host pre-drain. Runtime's TS starter already provides its own pre-drain. A registered finalizer releases owned resources; it does not compensate external writes. Never close borrowed clients/bindings automatically.

Keep normal operation failure and cleanup failure together. TS can aggregate both in an AggregateError; Rust can return an application-owned error enum/struct carrying both, or match both Results; Go can `errors.Join`. Do not return immediately with `?`/throw after startup and skip shutdown, or overwrite an original business failure with the shutdown error. Keep raw internal causes off external output.

## Async, admission and cancellation

TS Promise abandonment does not cancel work. Core's `startApp` signal belongs to config preflight (checked before setup); `PluginContext` has no universal business AbortSignal. Methods receive cooperative signals only if the app explicitly passes them. Core has no automatic request gate/drain.

Rust futures are lazy and dropping a future normally drops its execution state. Current Core startup owns an internal task and shuts down an unacknowledged app; a cancelled waiter is not a startup timeout. An acknowledged app still needs explicit shutdown. Keep Tokio alive; stuck setup/guards/cleanup do not get forced completion. Older Core differs.

Use Rust WorkGate **once** at outer service/host entry, hold through nested awaits, and let dependency-local Log/Cache calls reuse admission. Don't reenter after sealing, wait for shutdown while holding a guard, or acquire dependencies with a fresh RunningApp::get after sealing. Background tasks that still use resources must retain accepted admission until they finish.

Go checks ctx around Setup and passes it to callbacks; a callback ignoring ctx can block. Close(ctx) cancels that caller's wait, not cleanup. Cleanup receives context.WithoutCancel(startCtx); give blocking cleanup its own deadline. Cancelling startup ctx after success does not auto-close the App. No universal goroutine kill or Rust-style gate exists.

Sync JS assumptions do not become safe Rust concurrency: services require Send+Sync, async cleanup closures own captures, sync locks must not span await. Rust blocking IO/CPU work needs an explicit execution strategy; don't block Tokio or spawn unlimited tasks. TS async functions may still perform synchronous work on the event loop. Neither language automatically propagates trace context into detached work.
