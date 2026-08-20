# Lenso vNext Kernel WebAssembly portability review

## Scope

This note evaluates what is required for the Rust Kernel described by ADRs
0030-0052 to compile to WebAssembly while retaining a native Tokio Runner. It
does not select a final WebAssembly product profile, execution protocol, or
public Rust API.

## Decision outcome

ADR [0053](../adr/0053-run-the-kernel-on-a-portable-runtime-driver.md)
subsequently selected the portable single-owner Kernel and Runtime Driver seam,
native Tokio Driver, compile checks for both documented Wasm targets, and
host-specific Adapter availability described in this review. Browser and WASIp2
product packaging remain separate implementation profiles to prove through the
validation roadmap.

## Summary

Compiling the Kernel to WebAssembly is compatible with the current vNext
direction, but only if Tokio and operating-system services are implementation
details of a **Runtime Driver** below the Kernel. `Execution Adapter` and
`Runtime Driver` are separate seams:

- a Runtime Driver supplies scheduling, monotonic time, wakeups, bounded task
  ownership, and any entropy needed for restart jitter;
- an Execution Adapter creates and controls Module generations and their
  endpoints using facilities available from that host.

The Kernel should be a single-owner asynchronous state machine that depends on
portable futures, durations, cancellation, channels/queues, and Driver handles.
It should not directly call Tokio, `std::process`, signals, filesystem, sockets,
environment variables, or wall-clock time. A native Runner may implement the
Driver with Tokio without making Tokio part of the Kernel contract.

"Kernel compiles to Wasm" must not imply that every Execution Adapter is
available in every Wasm host. A browser-hosted Kernel cannot directly spawn the
initial Bun child process. It would need a JavaScript host import/bridge, or the
Resolved App Plan must reject that execution class. This follows the existing
Adapter-aware isolation decision rather than weakening it.

## Target choice

There is no single Rust Wasm target that covers both browser embedding and a
server-side WASI component:

| Target | Appropriate profile | Relevant constraints |
| --- | --- | --- |
| `wasm32-unknown-unknown` | Browser or JavaScript-host embedding | The target imports no host functions by default. `std::fs` returns errors, `std::thread::spawn` panics, and other OS-backed `std` facilities are stubs. Scheduling, timers, I/O, and shutdown must therefore come from explicit JS/host bindings. |
| `wasm32-wasip2` | Server-side component runtime | The target emits a Component Model component, requires a WASI Preview 2/component-capable runtime, and documents full `std` support. WASI 0.2 provides standardized clocks, polling/streams, preopened filesystem access, sockets, CLI facilities, and HTTP, subject to host grants. The Rust target is Tier 2 and its page states that it is not tested in rustc CI. |

Evidence: Rust documents the minimal and stubbed environment of
[`wasm32-unknown-unknown`](https://doc.rust-lang.org/stable/rustc/platform-support/wasm32-unknown-unknown.html)
and the component output and runtime requirements of
[`wasm32-wasip2`](https://doc.rust-lang.org/stable/rustc/platform-support/wasm32-wasip2.html).
[WASI 0.2](https://wasi.dev/releases/wasi-p2) is the current stable WASI release
and is based on the Component Model and WIT.

Recommendation: make the Kernel crate compile for both targets in CI. Treat
`wasm32-unknown-unknown` as the stronger OS-independence check and
`wasm32-wasip2` as a distinct integration profile. Select and version separate
Runner artifacts; do not pretend one Wasm binary is portable across both host
ABIs.

## Async execution and time

Tokio is suitable for the native Runtime Driver, not as an unconditional Kernel
dependency. Tokio's current official documentation says that stable Wasm
support is limited to `sync`, `macros`, `io-util`, `rt`, and `time`; enabling
other features, including `full`, fails to compile. It also warns that timers
panic on Wasm platforms without timer support and that an indefinitely idle
runtime panics rather than blocking. Tokio's Wasm networking remains unstable
and qualified. See [Tokio WASM support](https://docs.rs/tokio/latest/tokio/#wasm-support).

The Runtime Driver should provide at least:

- a local task spawn/join/cancel facility with lifecycle ownership;
- cooperative yield or an equivalent scheduler wake mechanism;
- monotonic `now` and `sleep_until`/timer registration;
- a small entropy source for configured backoff jitter, or a deterministic
  Driver-provided alternative;
- a way for the Runner to request shutdown and drive the Kernel root future;
- explicit capability metadata for optional facilities such as parallel task
  execution.

Kernel deadlines should be represented as durations or opaque monotonic
instants originating from the Driver, never as `std::time::SystemTime`. WASI's
official [monotonic clock interface](https://github.com/WebAssembly/wasi-clocks/blob/main/wit/monotonic-clock.wit)
defines non-decreasing instants plus timer subscriptions, so it can implement
this seam. A browser/JS Driver can use host scheduling and timers. For its task
bridge, [`wasm_bindgen_futures::spawn_local`](https://docs.rs/wasm-bindgen-futures/latest/wasm_bindgen_futures/fn.spawn_local.html)
runs a `'static` future on the current thread and does not require `Send`.

The Driver abstraction must preserve the existing semantics: cancellation is
cooperative, queues remain bounded, timers cannot claim to reverse external
effects, and shutdown is deadline-bounded.

## Threads, `Send`, and `Sync`

The portable baseline should be single-threaded. The Rust target list exposes a
separate `wasm32-wasip1-threads` target but no corresponding stable
`wasm32-wasip2-threads` target; `wasm32-unknown-unknown` explicitly cannot use
`std::thread::spawn`. See Rust's [platform support table](https://doc.rust-lang.org/rustc/platform-support.html).
WebAssembly thread proposals or host-specific workers may be optional later,
but must not be necessary for Kernel correctness.

Tokio's ordinary [`spawn`](https://docs.rs/tokio/latest/tokio/task/fn.spawn.html)
requires both the future and output to be `Send`, while Tokio
[`spawn_local`](https://docs.rs/tokio/latest/tokio/task/fn.spawn_local.html) and
the wasm-bindgen task bridge accept `!Send` futures on one thread. Consequently,
the Kernel should not make a multi-thread Tokio executor or universal `Send`
task bound part of its portable contract.

A promising structure is:

1. the Kernel control state and mandatory managed work use a local Driver lane;
2. a native Tokio Driver may additionally expose parallel `Send` execution;
3. native Capability handles may remain thread-safe where useful, but
   JavaScript host objects and other `!Send` values stay inside the owning
   Adapter/Driver lane;
4. Resolved Plan validation rejects a Module execution requirement that the
   chosen Driver cannot supply.

The exact trait shape is still an implementation question. In particular, the
prototype should compare a mandatory `spawn_local` plus optional `spawn_send`
profile with a single portable `Send` baseline. The former is more compatible
with browser APIs; the latter is simpler for native module authors but may force
all JS-facing state behind Adapter-local indirection.

## Host facilities and Adapter placement

The following facilities must not be Kernel dependencies:

| Facility | Native profile | WebAssembly consequence |
| --- | --- | --- |
| child processes | Bun process Execution Adapter using Tokio/OS process support | Not available as a baseline WASI or browser operation. Requires a custom host Adapter/import, or the Plan is unsupported. |
| OS signals | Native Runner converts signals into a Kernel shutdown request | The Wasm host owns lifecycle. The component/JS entrypoint must expose shutdown or cancellation; Kernel must not install handlers. Tokio source excludes process and signal modules on WASI. |
| filesystem | Module or Adapter uses native APIs | `unknown-unknown` requires host bindings; WASI access is limited to host-provided preopened directories and remains Module/Adapter behavior. |
| network/HTTP | Ingress, transport, or other ordinary Modules | Browser and WASI expose different APIs and authority models. Network listeners/clients remain host-specific Adapters or Modules, not Kernel services. |
| environment/config loading | native Runner/Tooling | Runner supplies the already typed Resolved App Plan. Kernel never reads environment variables or files. |
| process exit | native Runner | Kernel returns a terminal outcome; the embedding host decides how to stop or recreate the Wasm instance. |

Tokio's own source gates [`process` and `signal` away from WASI](https://docs.rs/tokio/latest/src/tokio/macros/cfg.rs.html).
WASI documents filesystem access as capability-oriented and relative to
host-provided directory handles in the official
[filesystem proposal](https://github.com/WebAssembly/wasi-filesystem), not as
ambient access to the host namespace.

This separation also means that native linked Rust Modules are only available
in a Wasm Runner when they and their dependencies compile into that Wasm
artifact. A future Component Model Module Adapter would be a separate execution
class, not an automatic consequence of compiling the Kernel to `wasm32-wasip2`.

## Panic, traps, and supervision

Both documented Rust targets use `panic=abort` by default. Rust now describes an
experimental path to Wasm unwinding using the exception-handling proposal, but
it currently requires rebuilding the standard library with nightly flags and
the `wasm32-wasip2` page still defaults to abort. This is not a suitable v1
recovery contract. See Rust's
[`wasm32-unknown-unknown` unwinding section](https://doc.rust-lang.org/stable/rustc/platform-support/wasm32-unknown-unknown.html#unwinding).

Recommendations:

- do not rely on `catch_unwind` to isolate Kernel or in-process Module faults;
- model a panic/trap according to the truthful isolation boundary reported by
  the Execution Adapter;
- if the Kernel itself traps, the outer JS/WASI host may record best-effort
  diagnostics and recreate the entire Wasm instance, but in-instance recovery
  is not guaranteed;
- keep Module generation restart semantics only where the selected Adapter can
  actually recreate that generation.

This is consistent with ADR 0048: semantic conformance does not imply identical
physical fault isolation.

## Suggested crate boundary

The following dependency direction would preserve both native performance and
Wasm portability:

```text
contracts / resolved-plan types
              ↓
portable kernel engine (no Tokio, WASI, wasm-bindgen, OS I/O)
              ↓
runtime-driver API
        ↙                 ↘
native Tokio Driver     Wasm Driver
        ↓                 ↓
native Runner           JS Runner or WASIp2 Runner
        ↓                 ↓
host-supported Execution Adapters
```

Execution Adapters may depend on a Driver or directly on their host SDK, but
the Kernel must see only their lifecycle/endpoint/isolation interface. This
keeps the current native typed dispatch path possible; Driver indirection is
needed for scheduling and environment services, not for every native
Capability call.

## Explicit unknowns to resolve by prototype

1. Is the first Wasm product browser/JS embedding, server-side WASIp2, or both?
   This determines artifact ABI and host SDK, not the portable Kernel semantics.
2. Does the browser profile need Rust Modules that retain JS `!Send` values
   across awaits? The answer affects the local/parallel task trait split.
3. Where does a browser-hosted Kernel's Bun executor live: a Bun/Node embedding
   host, a remote connector, or not in that profile?
4. Does the WASIp2 profile require raw sockets, `wasi:http`, or only imported
   portable Capability endpoints? Tokio does not currently provide a sufficiently
   strong documented Wasm/WASIp2 guarantee to assume native-equivalent I/O.
5. Should the Wasm artifact export a long-lived start/stop API or be driven as a
   standard `wasi:cli/command` component? The latter's run-to-completion model may
   not be the ideal embedding contract for an interactive Kernel.
6. What minimum Rust/WebAssembly feature set and runtime matrix will be supported?
   The Rust targets are Tier 2, evolve their default Wasm features, and Wasm
   engines differ in component support.

## Minimum verification gate

Before accepting a Wasm portability ADR, a spike should:

- compile the same Kernel core for native, `wasm32-unknown-unknown`, and
  `wasm32-wasip2` without target-specific `cfg` inside the core engine;
- run graph validation, staged activation/rollback, bounded request/event/stream
  admission, deadline, cancellation, diagnostics dropping, and shutdown tests
  with a deterministic test Driver;
- run one browser/JS Driver test using a local task and timer;
- run one WASIp2 component test using monotonic clock/polling;
- demonstrate Plan rejection for an unavailable child-process Adapter;
- demonstrate that native Rust typed dispatch does not acquire serialization or
  per-call virtual host-I/O overhead merely because a Wasm Driver exists.
