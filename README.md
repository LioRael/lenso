# Lenso

A Bun-first plugin framework with ordinary async business services. The core SDK is independent of Web, oRPC, Auth, databases, Console and Rust. This local first slice calls the same in-memory greeting service from a CLI or an optional oRPC Fetch server with an inferred typed client.

Console was removed following the user's latest direction. It belongs in a future independent repository. This repository has no Console package, React/Vite UI, or Console-specific contribution protocol.

## Run

Validated tools: Bun **1.4.2**, TypeScript **5.9.3**, Turbo **2.11.7**, Effect **3.22.2**, oRPC server/client **1.15.5**, Zod **4.6.5**. Exact dependency versions and one root `bun.lock` are committed. `mise.toml` pins Bun. Node **26.10.0** was available for tool execution; application runtime and package management use Bun.

```sh
cd /Users/leosouthey/Projects/framework/lenso
bun install --frozen-lockfile
bun run typecheck
bun run test
bun run build

# Business-only config; does not install/start the Web plugin.
bun run cli call greeting greet '{"name":"Ada"}' --root examples/greeting

# Development server, only on 127.0.0.1:3000. Builds first.
bun run dev
```

In another terminal:

```sh
bun run client Ada
# Expected message: Hello, Ada!; runtime status: greeting, web, http-listener
bun run client x
# Expected business validation error and nonzero exit.
```

CLI calls start/stop an isolated app each time; HTTP calls share the server's current in-memory counter. Invalid names do not increase it. Restart resets it. This is not persistence.

```sh
# With bun run dev active: changes Hello -> Welcome, verifies a new PID and one listener,
# restores the source, then confirms Hello again. Requires macOS lsof.
bun scripts/feedback-check.ts

# After build: packs the real three packages, installs into independent temporary consumers,
# checks CLI without Web deps, HTTP roundtrip, client types and browser bundle isolation.
bun run smoke

# Run the actual built server (without dev watcher).
bun examples/greeting/dist/server.js
```

`LENSO_PORT` changes the server port; `LENSO_URL` changes the sample client URL. The feedback check intentionally targets the default local port 3000. No remote Git origin, publishing or deployment is configured.

## Packages and minimum API

- `packages/lenso` (`lenso`): `definePlugin`, `defineApp`, validation and application lifecycle. `lenso/plugin` exposes authoring without loading lifecycle. `lenso/browser` exposes only authoring helpers/types, with no Effect or server implementation.
- `packages/cli` (`lenso-cli`): `check`, `generate`, `build`, `call`, `dev`. Engine discovers explicit `lenso.config.ts`, validates static instances/dependencies and writes `.lenso/manifest.json`, `server.ts` and an optional typed `client.ts`. It uses Bun's bundler, never runs plugin setup during generation and stays outside request handling. Config modules must be free of top-level resource acquisition.
- `packages/web` (`@lenso/web`): optional oRPC Fetch plugin and `@lenso/web/client` transport. Uses oRPC schemas/router/middleware/client without a second RPC abstraction. The generated `#lenso/client` imports router types only.
- `examples/greeting`: one plain async service, business-only app config, optional Web/listener assembly and typed client.

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
  id: 'report',
  requires: [clock],
  setup(context) {
    const service = context.get(clock);
    return { async run() { return service.now(); } };
  },
});
const app = await startApp(defineApp({ plugins: [report, clock] }));
try { console.log(await app.get(report).run()); }
finally { await app.stop(); }
```

Instance IDs must be unique. Dependencies use the exact installed plugin object; different instances of one implementation have different IDs/objects. A plugin may access only its declared initialized dependencies. Acquire resources during setup and register cleanup immediately. Initialization failure rolls back resources, including those already acquired by the failing plugin. Effect Scope finalizes in reverse order, attempts all callbacks and reports collected errors. `stop()` is idempotent, including concurrent callers. Detached promises and arbitrary resource acquisition after setup are not automatically managed.

Generic contribution records remain simple plugin metadata. No page registry or future Console protocol is predefined.

Dev watches the example's `src` and config, requests graceful shutdown, waits for the old child to exit, regenerates and starts a fresh process. It may force-stop only its owned child after five seconds. There is no state migration. Framework package edits require rebuilding package outputs. Generation avoids rewriting unchanged entries.

## Evidence and limits

See [implementation plan](docs/PLAN.md) and [verification record](docs/VERIFICATION.md). Raw CLI/HTTP results, edit timing and independent consumer results are under `output/`.

This is a local prototype with trusted in-process plugins and memory state. Auth composition is verified with a standalone oRPC middleware test, not an account system or core auth policy. The example rejects foreign Host/Origin and binds loopback; it is not a public deployment. Workers, Drizzle/PG/D1, streaming/cancellation beyond registered lifecycle resources, Rust extensions and AI Relay migration have not been implemented or validated. Next work should validate a real app and request cancellation/resource ownership, then independently validate Workers and storage adapters when needed.
