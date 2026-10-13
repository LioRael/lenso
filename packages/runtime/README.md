# @lenso/runtime

Optional Lenso integration for `@lenso/runtime-sdk`. Core and native business plugins
remain ordinary TypeScript services. Nothing is exported to the cross-language
runtime, CLI, MCP, Manage or Console by discovering methods.

## Runtime assembly

`createRuntimePlugin` connects registered but not ready during setup, immediately
registers owned drain/close cleanup, then constructs injected generated clients.
A failed client factory rolls back that connection. Outgoing calls and status are
usable before activation. The SDK owns registration, wire validation, bounded calls and
credit-driven streams; this package does not implement a second transport.

```ts
import { createRuntimePlugin } from "@lenso/runtime";
import { LogsClient } from "./generated/logs";

const runtime = createRuntimePlugin({
  id: "runtime",
  connection: {
    namespace: "demo",
    service: "frontend",
    instance: "one",
    contract: { version: "1", methods: {} },
  },
  clients: (host) =>
    new LogsClient(host, {
      namespace: "demo",
      service: "batch",
    }),
});
```

Consumers declare `requires: [runtime]` and obtain the typed client with
`context.get(runtime).clients`, using that exact installed object. For host-only
usage, supply `clients: () => undefined`.
When `connection.instance` is omitted, the Host uses the running Lenso App's
existing instance ID, so independent app/CLI invocations need no manual ID setup.

Use `startRuntimeApp(app, options?)` instead of Core's `startApp` for a server
publishing Runtime exports. It completes every Core setup, then activates only
the exact installed objects created by `createRuntimePlugin`. Activation failure
stops the app; startup and cleanup errors are both retained.
Its `stop()` first invokes the Runtime hosts' registered cleanup disposers, draining
incoming calls before Core releases native business resources. Core then runs its
normal cleanup with those same idempotent disposers, retaining any failures.
Repeated stop calls share one completion.

To export existing native methods, explicitly supply a wire `connection.contract`
and an `exports(running, clients)` factory. Obtain the exact installed native
service with `running.get(nativePlugin)` and return only the selected SDK handlers.
Late binding permits that native plugin to require Runtime clients without a
dependency cycle. Native methods not selected there stay local.

The existing `handlers(context)` factory still runs during setup for exports
whose native providers are declared in `requires`. Select either `handlers` or
`exports`, never both. Neither factory publishes readiness during Core setup.
Do not infer contracts or exports from a plugin ID or its service methods.

Owners of an existing running app can call `activateRuntimeApp(running, plugins)`
with their explicit plugin list. This helper borrows the app, never stops it,
and shares one activation attempt per exact Runtime service, including concurrent
calls and failed attempts. Its caller owns failure cleanup and must await the
Runtime services' optional `disposer` functions before stopping native resources,
or use the owned `startRuntimeApp` starter instead. Core's ordinary
`startApp` deliberately leaves these hosts registered-only, suitable for finite
outgoing CLI calls that must not publish server exports.

## Engine authoring

`@lenso/runtime/engine` imports the optional Engine peer, never the runtime entry.
`runtimeEnginePlugin` registers a `beforeStart` hook in `lenso.engine.ts`.
Its `kernelBinary` and `services` are prebuilt executables; it never invokes Cargo.
Each service supplies an explicit asynchronous readiness predicate over a probe
host. The helper refuses an occupied loopback port, checks owned process exits,
bounds readiness, and registers child cleanup immediately after each spawn.
`SIGTERM` has a bounded grace period before owned children receive `SIGKILL`.

`startRuntimeCompanions(options, onCleanup)` exposes that same ownership helper to
an outer launcher. Pass a real cleanup registrar and run all registered finalizers
in reverse order even if startup fails. Its returned `restartService(index)` stops
and restarts one prebuilt service; it does not rebuild code or restart the kernel.
The launcher should pass an `AbortSignal` and abort it before shutdown.

The standard Engine supervisor closes its Engine worker after each TypeScript
restart. Therefore an Engine-owned kernel also restarts with normal `lenso dev`.
For independent lifetimes, own companions in an outer launcher and set the inner
Engine plugin's `startCompanions: false`. The example implements this using only
the public `createDevSupervisor` API. No Engine source patches or fake selective
reload claims are needed.

## Explicit read-only management

`@lenso/runtime/manage` additionally needs Engine and Manage peers.
`createRuntimeManage({id, runtime, authorize})` creates a sidecar requiring the exact
runtime instance and returns `{plugin, status, manage}`. Install `plugin` and
explicitly select `status` into the intended adapter's operation allowlist.
The declaration opens no entry and exposes no arbitrary call, restart or mutation.

`status` accepts only `{}` and is a contextual operation. Every invocation awaits
`authorize(trustedContext)` before requesting SDK diagnostics. The host must supply
its current verified Auth/evidence policy through the trusted entry binding:
`{context: {evidence: trustedContext}}`. The typed evidence stays in process;
it is never taken from the operation's JSON input.
For example, a host's callback can use its existing exact Auth service to verify
the context actor and enforce the runtime diagnostics permission. A callback that
blindly returns is not authorization. Business JSON is never identity evidence.
`canList` remains the adapter's current-identity visibility gate, not a replacement
for service authorization. Runtime service names are trusted local process names,
not end-user authentication, and principals are not forwarded to Rust.

## Local package resolution

All SDK imports use its ordinary `@lenso/runtime-sdk` package export. The SDK is a
versioned peer. Before installing this workspace, run from the repository root:

```sh
bun scripts/prepare-runtime.ts
bun install --frozen-lockfile
```

Preparation needs the exact Bun version in the root manifest (1.4.3), but no
existing `node_modules`, Rust toolchain or sibling checkout. The reviewed source
archive is pinned by commit and SHA256 in `scripts/runtime-sdk.json`. Preparation
checks the bounded download before extracting it, installs snapshot dependencies
with frozen lockfile and lifecycle scripts disabled, and runs only `build:sdk`.
The ordinary file dependency resolves to `.runtime-deps/sdk`, containing the public
package manifest and `dist` only. Snapshot source and its build dependencies remain
in `.runtime-deps/source` for contract generation.

Each invocation rebuilds; it never trusts a cached artifact. A failed build leaves
the previous ready package and source intact. `.runtime-deps/.owner` identifies the
generated directory; preparation refuses an unowned directory. Do not keep user
files in this ignored output. `.runtime-deps/provenance.json` records the snapshot,
Bun version and SDK content hashes. `LENSO_RUNTIME_ROOT` selects independently
built example binaries, not a TypeScript import fallback.
