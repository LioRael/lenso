# Native Lenso service plus Rust log aggregation

This example sends synthetic logs to the Rust `demo/batch` service through an
injected generated `LogsClient`, consumes progress using an async iterator, and
records a batch in a native TypeScript plugin. The server also exports `demo/frontend`
using the same generated `logsContract`, forwarding aggregate and progress to that
native plugin after setup. Only the selected aggregate
operation is declared for finite CLI JSON output. Progress is not automatically
exposed through Manage, MCP or Console.

## One source-workspace entry

Requirements: Bun 1.4.3, Cargo and a POSIX shell (macOS/Linux source
development; Windows has not been verified). From the Lenso repository root:

```sh
export LENSO_RUNTIME_ROOT=/absolute/path/to/lenso-runtime
export LENSO_RUST_ROOT=/absolute/path/to/lenso-rust
bun run dev:runtime
```

This prepares the **explicit local SDK candidate**, frozen-installs TS dependencies,
uses cached builds of the required framework packages, generates the TS binding
from Runtime's one contract, and starts kernel → native Rust batch → TS frontend.
The three checkouts can live anywhere. No package publication or dependency pin
changes are needed. `LENSO_RUNTIME_PORT` selects a port (default 7331); a conflict
fails without borrowing or stopping the existing listener.

Once prepared, the direct entry is:

```sh
lenso dev --workspace examples/runtime/lenso.workspace.ts
```

`lenso.workspace.ts` declares process boundaries only; `lenso.config.ts` still owns
the TS plugins and typed instance dependencies. Customize commands, cwd, watches,
timeouts and ready/activate callbacks through `defineWorkspace`. Config imports
acquire no resources. States include real PID, candidate instance, version,
elapsed time, errors and retained old instance information. Ctrl-C closes
dependents before dependencies; unexpected service exit closes the workspace.

- TS implementation edits use the existing Engine source graph, restarting only
  the TS frontend.
- Rust source edits build only `logs-batch` and affected Cargo dependencies in an
  owned staging directory. Unchanged source timestamps and Cargo artifacts remain
  reusable. A compile failure reports `failed` and `retained`; old calls still work.
- A successful candidate must register, become ready **under its unique token**,
  and pass `RuntimeHost.promote` before new calls move. Old assigned streams drain
  for up to 5 seconds; process shutdown has an 8-second bound. No state migration,
  retry, replay or reconnect fallback occurs.
- A different contract, including a version bump, cannot replace a running
  selection. The frontend's client binding is prepared at startup; TS and Rust
  service candidates use generated bindings from the canonical contract. TS
  candidates bundle their binding into an owned staging artifact, without
  overwriting the running frontend's client when promotion is rejected.
  Rust bindings regenerate only when canonical contract/generator changes. Intentional
  incompatible upgrades require a coordinated workspace restart, not live coercion.
- The kernel is not watched/replaced automatically: replacing it would fail all
  in-flight work. Stop and start the workspace when changing kernel code.
  TS SDK/framework changes require rerunning `bun run dev:runtime` to refresh
  prepared SDK bytes and cached framework output; business edits do not do that.

Choose the separate TS service implementation at launch with
`LENSO_BATCH_IMPLEMENTATION=ts`. It exports the same generated contract through a
native TS App; the frontend still makes an explicitly remote Service call.
For a transport-free TS application, `bun run --cwd examples/runtime offline`
uses the same ordinary business function, local bounded Memory Cache, scoped
Log and Core, with no Runtime listener. `batch.aggregate` is now async; callers
must await it. `batchPlugins` is the explicit local dependency list used by
both native and Runtime Apps.

The Rust native App uses its own local Log/Cache and the retained aggregator,
not a remote cache/logger. After its staged build, use `logs-batch --native`
with a single `{logs:[...]}` JSON input on stdin, or run each plugin's standalone
native example. `lenso-log-plugins` still means log reading/aggregation, not
structured logging. The public Runtime `logs` contract is unchanged.

`LENSO_BATCH_CACHE_TTL_MS` selects a finite TTL in milliseconds (default 60000,
range 0..86400000); invalid present values fail startup, zero disables storage.
The cache holds only synthetic batch summaries, never authentication or
authorization decisions. SHA-256 of the input selects a local key. Calls for
the same key share loading, while aborting one caller does not cancel another.
Cache scopes and instance names do not establish permissions. Performance sets
TTL zero for all paths so it does not compare warmed cache hits with computation.

The single contract is Runtime's `examples/contracts/logs.json`. Integers are
canonical safe JSON integers, optional differs from null, decimal money/time
formats are not invented, and unsupported schemas fail in the existing generator.
Internal plugin methods and the local journal do not become remote operations.

`RuntimeHost.status()` exposes actual instances, exports, contract versions,
selection, recent trace/parent IDs and measured queue/execution/total durations.
Kernel `execution_ms` includes host validation/scheduling, **not only business
computation**. Existing authorized Runtime Manage returns this same snapshot.
This example has no Auth or DB, so it reports no fabricated reads or DB counters.
Initialization emits one event per actual TS setup; Rust readiness reports its
successfully initialized instance count and startup elapsed time. No request
identity/transaction is cached globally. Console UI lives in its separately owned
repository and is not changed here.

All Runtime control is **trusted local loopback**, not authenticated management,
cross-machine production security or end-user identity propagation. No ordinary
business operation is automatically exposed to Manage, MCP or agents.

## Prepare once

From the Lenso repository root, run `bun scripts/prepare-runtime.ts` before
`bun install --frozen-lockfile`, using the root manifest's exact Bun version
(1.4.3). This prepares the pinned public SDK in `.runtime-deps/sdk` and retains
the verified source in `.runtime-deps/source`, without Cargo or a sibling checkout.
Preparation always rebuilds and preserves the prior ready artifact on build
failure. The generated output is owned by preparation; do not store user files
there. Build framework dependencies before
this package: core, Engine, CLI, Manage (for the optional runtime management entry),
then `packages/runtime`, `packages/cache` and `packages/log`. Their normal exports
resolve to built `dist`. The root `dev:runtime` entry builds these through the
existing cached dependency graph.

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

The retained `bun run dev:companions` entry is the earlier prebuilt-binary workflow.
The outer launcher owns the kernel and Rust batch process once,
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
lenso inspect logs aggregate --root examples/runtime --json
```

With an already running kernel and batch, call the declared business operation:

```sh
printf '%s\n' '{"logs":[{"level":"info","message":"synthetic request","duration_ms":12}]}' |
  lenso call logs aggregate --root examples/runtime --stdin --json
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
