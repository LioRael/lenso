# @lenso/engine

Bun-hosted authoring tools for discovery, generation, build targets and supervised
development. Engine owns processing and resource lifetimes; `@lenso/cli` owns
arguments, terminal presentation, command invocation and exit codes. Engine depends
on `@lenso/core`, never on the CLI. Runtime and browser entries do not depend on Engine.

## Programmatic API

```ts
import { discover, generate, build, EngineError } from "@lenso/engine";

const root = "/path/to/app";
try {
  const discovery = await discover(root);
  const manifest = await generate(root);
  const directory = await build(root);
  // Use discovery.ordered, manifest and directory in your own tool.
} catch (error) {
  if (error instanceof EngineError) {
    // Structured code/phase/plugin/source; original in-process cause is retained.
    handleDiagnostic(error.diagnostic, error.cause);
  } else {
    throw error;
  }
}
```

Finite calls run trusted Engine setup and close registered resources on success
or failure, but never run application plugin setup. `createEngineSession(root, mode)`
provides an explicit lifetime for tools that need stage access: call `prepare()`,
then `session.generate()`, `session.build(entry?)` or `session.dev(event)`, and
always await `session.close()` in a `finally`. `withEngine(session, run)` preserves
both execution and cleanup failures. Error causes are available in process;
`diagnostic(error)` produces safe structured descriptions, not raw error text.
No API prints a banner, installs process signal handlers or sets caller exit codes.

Preparation is single-flight. Processing stages are serial; overlapping calls and
work begun after close reject with structured diagnostics. Close waits for in-flight
setup/hooks before draining cleanup. A hook/finalizer must not await its own session's
close promise. `./dev-worker` is an owned subprocess entry, not an importable plugin
API; bundled tools must keep the Engine package installed so it can be resolved.

```ts
import { createDevSupervisor } from "@lenso/engine";

const supervisor = await createDevSupervisor({
  root,
  onEvent(event) {
    // starting, ready, failed (diagnostic), or exited (code).
    updateStatus(event);
  },
});
try {
  await waitForUserToStop();
} finally {
  await supervisor.close();
}
await supervisor.done;
```

The supervisor watches sources, debounces invalidation and restarts fresh Engine
workers and application processes. `ready` is emitted only after application IPC
and Engine ready hooks succeed. Report actual startup from the application:

```ts
import { reportDevReady } from "@lenso/engine/dev-ready";
reportDevReady({ urls: [server.url.href], capabilities: ["web"] });
```

This lightweight subpath imports no build host. Entries without the signal stay
Starting. On restart/shutdown, runtime termination precedes Engine LIFO cleanup.
`close()` is idempotent and rejects on cleanup failure; `done` settles after
shutdown. Engine worker startup is bounded to 30 seconds, shutdown to 5 seconds.
The embedding tool owns signals and presentation. Observer callbacks must be
synchronous; their thrown exceptions are isolated from resource ownership.

## Engine plugins

The default Bun build needs no extra config. Add a trusted `lenso.engine.ts`
beside `lenso.config.ts` for build-time extensions. Keep imports out of runtime
assembly and browser entries. External packages use this public, dependency-free
authoring protocol, never internal source paths:

```ts
import { defineEngineConfig, defineEnginePlugin } from "@lenso/engine/authoring";

const assets = defineEnginePlugin({
  name: "app/assets",
  setup(context) {
    context.watch("assets");
    context.discover("assets", async () => ["assets/message.txt"]);
    context.generate("asset-index", ({ sources }) => [
      {
        path: "assets.ts",
        content: `export const paths = ${JSON.stringify(sources)};`,
      },
    ]);
  },
});
export default defineEngineConfig({ plugins: [assets] });
```

Setup registers `convention`, `discover`, `generate`, `target`, `dev`, `watch` and
`onCleanup`. Snapshots contain immutable paths and source lists. Discovery appends
existing application sources inside root, outside `.lenso`/`dist`. Generators
return static files relative to `.lenso`. A convention returns
`{config, entry?, router?}`; Web is optional. A target receives `entry` and
`bundle`, the same Bun bundler used by the default target. Select one with
`defineEngineConfig({target: "workers", plugins})`. Convention entry affects
build only; dev uses its explicit entry or `src/server.ts`.

Plugins have unique names. `before`/`after` name ordering constraints; missing
names and cycles fail. Otherwise configuration order is stable. Stage capability
names are unique. Replacement requires the exact current owner:

```ts
context.convention(() => ({ config: "app.ts", entry: "src/index.ts" }), {
  replace: "lenso/defaults",
});
```

The official owner `lenso/defaults` registers the app convention, generators
`manifest`, `server`, `client`, and target `bun` through this same protocol.
`defaultEngineOwner` and `defaultEnginePlugins` are public exports. Replacement
keeps stage position. To replace another plugin, order after it and name it in
`replace`. Conflicting names or outputs never silently overwrite another owner.

All capability registrations return a synchronous, idempotent revoker and
automatically join the registering plugin's session cleanup:

```ts
const revoke = context.generate("asset-index", () => []);
revoke(); // Optional early removal; no separate unregister cleanup is needed.
```

Revocation affects future invocation, including hooks not yet reached in a
running stage; it does not cancel a hook already executing. An old owner's
revoker cannot remove its replacement. Revoking the replacement does not restore
the old owner. Revoking the selected build target produces `unknown-engine-target`.
`watch` also returns a revoker: each call owns one reference, so removing one
registration preserves other owners and static-import invalidation inputs.

The standalone [content plugin](../cli/examples/content-plugin/index.ts) and
[module target plugin](../cli/examples/module-target-plugin/index.ts) demonstrate
external packages with an Engine peer dependency. Install as dev dependencies
and import `contentPlugin`/`moduleTarget` into `lenso.engine.ts`. Content produces
`.lenso/content.ts`; the module target bundles inside `dist/workers` using the
shared bundler. It adds no deployment or credential workflow.

Ownership lives in `.lenso/.engine-files.json`. Output conflicts, traversal,
symlink output paths and edited content fail before writes. Unchanged bytes keep
their mtime; removing a generator removes only tracked, unmodified outputs.
Generation validates the full output set first, but filesystem writes are not
transactional. Unowned existing outputs are rejected; generated headers and
manifest `generatedBy` identify `@lenso/engine`.

Register cleanup immediately on acquiring a resource. Hooks are sequential;
cleanup is sequential LIFO on success, failure and dev shutdown. Registration
closes when setup returns; captured `watch`/`onCleanup` remain usable by hooks
until cleanup starts, including in-flight setup/hooks that close is waiting for.
Capability and watch revocations join the same LIFO stack as resource cleanup.
`onCleanup` returns an async disposer; explicit calls and close await the same
completion, and an early cleanup failure remains visible to close. A finalizer
must not await its own disposer. Clear only resources you own or explicitly
registered; teardown cannot undo sent requests, database writes or other external
side effects. Direct sessions still require `finally { await session.close(); }`,
including after failed preparation.

Discovery hooks run in stable capability order and append validated source
paths. Generators run in that order and collect files before writes; they do not
short-circuit on empty output. The convention and selected target each use one
active provider. Dev hooks are awaited sequentially; their return values are
ignored, not vetoes. A thrown/rejected hook stops its stage with owner/source
attribution. Cleanup instead attempts every entry and aggregates failures.
These are processing/control hooks, not a general business event bus.

`startEngineDevCycle(root)` is the lower-level worker
lifetime: generation and `beforeStart` precede return, `ready()` runs ready hooks,
`close()` waits for cleanup. Embedders must stop their runtime before closing it.

Static imports are invalidation inputs; dynamic reads require `watch(path)`.
Explicit file/directory watches must exist. Generated/cache directories are
excluded to prevent restart loops. Dev reloads a fresh module graph each cycle;
finite in-process discovery follows Bun module caching. Determinism depends on
deterministic config metadata; import-time randomness remains app-owned.

Engine plugins and config are trusted code, not a sandbox. Host path checks
restrict returned output paths, not arbitrary plugin filesystem/process access.
Keep config top-level code free of resource acquisition. `inspect`/`call` use
canonical `lenso.config.ts`, never import Engine config or run Engine hooks;
trusted application config imports can still have top-level side effects.
Retain that canonical config or a re-export when using a custom convention.
Explicit operations/shared validation remain CLI contracts; see [CLI API](../../docs/CLI.md).

Build tools and plugins use this package's public API.
