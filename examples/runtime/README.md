# Native Lenso service plus Rust log aggregation

This example sends synthetic logs to the Rust `demo/batch` service through an
injected generated `LogsClient`, consumes progress using an async iterator, and
records a batch in a native TypeScript plugin. The server also exports `demo/frontend`
using the same generated `logsContract`, forwarding aggregate and progress to that
native plugin after setup. Only the selected aggregate
operation is declared for finite CLI JSON output. Progress is not automatically
exposed through Manage, MCP or Console.

## Prepare once

From the Lenso repository root, run `bun scripts/prepare-runtime.ts` before
`bun install --frozen-lockfile`, using the root manifest's exact Bun version
(1.4.3). This prepares the pinned public SDK in `.runtime-deps/sdk` and retains
the verified source in `.runtime-deps/source`, without Cargo or a sibling checkout.
Preparation always rebuilds and preserves the prior ready artifact on build
failure. The generated output is owned by preparation; do not store user files
there. Build framework dependencies before
this package: core, Engine, CLI, Manage (for the optional runtime management entry),
then `packages/runtime`. Their normal exports resolve to built `dist`.

Set `LENSO_RUNTIME_ROOT` to the absolute runtime checkout. Build Rust independently,
using the runtime repository's documented Cargo commands. By default this example
starts `$LENSO_RUNTIME_ROOT/target/debug/lenso-runtime` and
`$LENSO_RUNTIME_ROOT/target/debug/logs-batch`; override them with
`LENSO_RUNTIME_KERNEL` and `LENSO_RUNTIME_BATCH`. Select an unoccupied loopback port
with `LENSO_RUNTIME_PORT` (default 7331). TypeScript startup never compiles Rust.

From this directory, `bun run generate` regenerates `src/generated/logs.ts` using
the single contract source at
`../../.runtime-deps/source/examples/contracts/logs.json` and the pinned generator
at `../../.runtime-deps/source/tools/generate.ts`. SDK preparation and contract
generation do not build Rust; `LENSO_RUNTIME_ROOT` is only needed for independently
built binaries. The generator also emits Rust
types; they are ignored here because the Rust batch service owns its own generated
types. Do not edit the generated TypeScript client by hand.

## Independent development lifetimes

Run `bun run dev`. The outer launcher owns the kernel and Rust batch process once,
selecting borrowed-companion mode in the initial process environment (Bun's
implicit child environment can cache values before later mutations).
then starts the public Engine TypeScript supervisor. `lenso.engine.ts` borrows those
companions in this mode. Editing TypeScript restarts only the supervised Lenso
application; the kernel and batch retain their process identity. Native plugin
state resets with that application restart.

To change Rust, run `cargo build -p logs-batch --bin logs-batch` in the Runtime
checkout, rebuilding only that service and any changed dependencies.
Enter `restart-batch` in the running example terminal to restart only the batch
service. This does not invalidate the TypeScript graph or restart the kernel.
In-flight calls affected by a batch restart fail; there is no retry or replay.
Kernel replacement requires stopping and starting the outer launcher.

`SIGINT`/`SIGTERM` stops the TypeScript supervisor first, then the owned batch,
probe host and kernel in reverse acquisition order. A failed setup also drains
registered cleanup. Only these owned processes and the chosen port are touched.

Running the ordinary `lenso dev --root examples/runtime` instead uses the Engine
hook to own companions. That standard supervisor restarts the Engine session and
its owned kernel on TypeScript edits. Use the outer launcher for selective reload.

## CLI inspection and call

Inspection imports assembly declarations only, without starting Rust:

```sh
bun packages/cli/src/bin.ts inspect logs aggregate --root examples/runtime --json
```

With an already running kernel and batch, call the declared business operation:

```sh
printf '%s\n' '{"logs":[{"level":"info","message":"synthetic request","duration_ms":12}]}' |
  bun packages/cli/src/bin.ts call logs aggregate --root examples/runtime --stdin --json
```

CLI `call` does not load Engine config or start companions. Each call owns a fresh
Lenso app, so its local batch count starts fresh. The example's server uses the same
service, not another business handler.

The server uses `startRuntimeApp`: all native setup succeeds before explicit
Runtime exports are bound and readiness is published. Its native logs plugin
requires the exact Runtime client and local journal; late `exports(running)` binding
avoids a circular `requires` edge. Forwarded calls carry the inbound trace ID,
call ID as parent ID, and cancellation signal into the ordinary native methods.
The duplex path is Rust caller → kernel → TypeScript native logs → kernel → Rust
batch. With the server running, set `LENSO_RUNTIME_SERVICE=frontend` when invoking
the Runtime repository's `logs-client` binary to exercise this path.

Finite CLI calls continue to use public Core `startApp`. Their Runtime host is
registered-only: outbound calls work, but they never bind or publish the server's
exports. Setup failure never activates; activation failure stops the owned app,
retaining cleanup failures as well.

The optional read-only runtime Manage factory is provided by
`@lenso/runtime/manage` but is deliberately not installed or granted a fake local
actor in this example. A real host must bind its existing verified context and
authorization policy before selecting its status operation.
