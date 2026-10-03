# Generated linked Stream App proof

This is a source Plugin with only the official Stream Endpoint Capability. The
staged harness compiles these same bytes into Native and linked Workers Apps,
checks their resolved Plans, and runs an HTTP corpus against the real Hosts.
The Workers observer additionally checks pending-read cancellation with another
session active, repeated cancellation capacity, terminal errors, half close,
chunk and aggregate response limits, and unread-session retirement/recovery.

Run `tests/linked-stream-smoke.py` stages `prepare`, `dependencies`, `native`,
`workers`, and `smoke` from this crate, supplying `--core`, `--js`, `--cli`,
`--bindgen`, `--wrangler`, and a new task-owned `--out` directory. Use the exact
qualified Workers runtime and wasm-bindgen version required by the generator.
Set `RUSTUP_TOOLCHAIN` to the repository toolchain, `CARGO_BUILD_JOBS=2`, and a
separate `CARGO_TARGET_DIR`. Cached dependencies can use `CARGO_NET_OFFLINE=true`.
Each build logs to the supplied directory; only a successful smoke stage writes
`RESULT.json`. Child Hosts are owned and stopped by that stage.

The immediate runtime failure is the last Native corpus case: its fixture Plan
uses restart policy `never`, so Kernel supervision correctly makes that provider
unavailable afterward. A 5xx unavailable-provider response cannot count as a
domain terminal. A socket disconnect before the head flush is an error outcome,
not successful EOF. The separate holding case proves first chunk before terminal.

`probe.mjs` is a test observer around the generated `worker.mjs`; it is never
copied into a production generated App by the builder. No durable data, finance,
external proof authority, deployed Workers, or attack tests are involved.

For a Workers-only correction after a recorded Native corpus pass, use
`workers-smoke` to reuse its generated Plan and fixture identity without repeating
Native execution. Preserve that prior pass evidence; this stage reports reuse
explicitly and still requires every strict Workers assertion.
