# Lenso

A Bun-first plugin framework with ordinary async services. Core owns instance dependencies and resource cleanup. Build-time Engine extensions and optional Web, Auth, Drizzle and Workers adapters remain separate from business code. Console belongs in a future independent repository.

## Run locally

```sh
cd /Users/leosouthey/Projects/framework/lenso
bun install --frozen-lockfile
bun dev
```

The included greeting starts on loopback and reports its actual URL, enabled plugins and readiness. In another terminal:

```sh
bun run cli inspect greeting greet --root examples/greeting --json
bun run cli call greeting greet '{"name":"Ada"}' --root examples/greeting --json
bun run client Ada
```

CLI and Web reuse the same input schema and service. Each CLI call starts a fresh app; HTTP shares the running instance's in-memory counter. Restart resets this counter. `LENSO_PORT` changes the listener port; `LENSO_URL` changes the sample client URL.

## Packages

| Package | Purpose | Usage |
| --- | --- | --- |
| `lenso` | `definePlugin`, `defineApp`, validation, Promise lifecycle | API example below |
| `@lenso/engine` | typed discovery, generation, extensible build targets and dev scheduling | [Engine API and plugins](packages/engine/README.md) |
| `lenso-cli` | command parsing, explicit service calls, terminal presentation and exit codes | [CLI contracts](docs/CLI.md), [development output](packages/cli/README.md) |
| `@lenso/web` | Fetch, oRPC and streaming request ownership | [Web API](packages/web/README.md) |
| `@lenso/auth` | provider adapters, typed middleware and shared service authorization | [Auth API](packages/auth/README.md) |
| `@lenso/db` | native Drizzle PostgreSQL, Bun SQLite and D1 resources | [Database and Notes](docs/DATABASE.md) |
| `@lenso/workers` | request-owned Fetch app and platform bindings | [Workers API](packages/workers/README.md), [local D1 example](examples/workers/README.md) |

[Minimal templates](templates/README.md) consume real packed packages outside the workspace. They are template contents; no scaffold command or npm release is implied. PostgreSQL, SQLite and local D1 Notes use real storage and explicit migrations.

## Core API

```ts
import { defineApp, definePlugin, startApp } from 'lenso';

const clock = definePlugin({
  id: 'clock.primary',
  setup(context) {
    const timer = setInterval(() => {}, 1000);
    context.onCleanup(() => clearInterval(timer));
    return { async now() { return new Date().toISOString(); } };
  },
});
const report = definePlugin({
  id: 'report', requires: [clock],
  setup(context) {
    const service = context.get(clock);
    return { async run() { return service.now(); } };
  },
});
const app = await startApp(defineApp({ plugins: [report, clock] }));
try { console.log(await app.get(report).run()); }
finally { await app.stop(); }
```

IDs identify distinct instances. Dependencies use exact plugin references, and a plugin can access only declared initialized dependencies. Acquire resources during setup and register cleanup immediately. Setup failure rolls back registered resources, including the failing plugin's resources. Cleanup runs sequentially in global LIFO order, attempts every callback and aggregates failures. Concurrent/repeated `stop()` returns the same Promise. Business code stays ordinary async.

`onCleanup` returns an async disposer: `const release = context.onCleanup(close)`.
Use `await release()` for early release instead of calling `close()` separately.
Repeated calls and automatic shutdown share one completion, including failures;
shutdown waits for an early release still in progress. Registration remains
setup-only. For an external listener, register its removal once with
`onCleanup(() => emitter.removeEventListener(name, listener))`; core does not
discover arbitrary listeners or infer cleanup from a returned service.

Consumers can accept a small structural `Plugin<ServiceInterface>` rather than
import a specific provider; [Notes](examples/notes/src/notes.ts) already uses this
pattern with native database types and `NotesQueries`. Every `requires` edge is
mandatory and binds the exact supplied instance. Missing instances and duplicate
IDs fail before setup. If a factory has an optional provider argument, declare
and resolve it only when supplied, with an explicit absent-provider branch.
There is no implicit provider selection or permanently pending startup.
Dependencies are not child scopes: each plugin owns its registered resources,
and dependents release before dependencies during automatic app shutdown.
An optional `source: {file, export?, line?, column?}` on a plugin gives assembly
and lifecycle diagnostics an explicit declaration location; otherwise CLI uses
the actual app config path. Static inspect describes declarations, not service
health or methods returned by setup.

## Development boundaries

Use package.json for the pinned tools and scripts. The workspace has one Bun lockfile, TypeScript 7.0.2, thin Turbo orchestration and oxlint/oxfmt. Rebuild changed framework packages before running consumers: exports resolve to dist. `.lenso` and `dist` are reproducible framework-owned output; edit source/config and regenerate. Config top-level code must avoid resource acquisition. Root tests are focused; the PostgreSQL integration test requires `LENSO_TEST_DATABASE_URL` pointing to a disposable test database.

Plugins and config are trusted code, not a sandbox. CLI exposes only declared operations and does not invent a user identity. Auth adapters provide identity/policy seams, not a deployed account system. Cancellation is cooperative; detached work needs explicit ownership and registration. The included Bun server binds loopback and rejects foreign Host/Origin. Workers examples are local; cloud deployment, provider credentials, MCP/AI runtime and Rust extensions are outside this deliverable.
