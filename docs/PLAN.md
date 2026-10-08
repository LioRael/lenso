# Lenso v1 plan and minimal API

Canonical local repository: `/Users/leosouthey/Projects/framework/lenso`.

Current scope reflects the user's latest instruction: Console will be a separate repository. This deliverable has no Console package, React, Vite, browser UI, or Console contribution protocol. Earlier task-created Console files are preserved outside the repository at the task workspace's console-deferred directory.

Integration owner owns root config, lockfile and example. Delegated SDK owner implemented packages/lenso; Engine owner implemented packages/cli; Web owner implemented packages/web. Ownership returned to integration owner before final scope adjustment.

1. Use Bun 1.4.2, one root bun.lock, thin Turbo build/typecheck/test orchestration.
2. Implement SDK dependency diagnostics and Effect Scope lifecycle behind a plain async public API.
3. Implement CLI Engine discovery/generation/build and supervised fresh-process restarts; no request-path Engine.
4. Call one ordinary async in-memory service via CLI and optional Web/oRPC typed HTTP client.
5. Use oxlint/oxfmt for routine hygiene and keep focused tests. Earlier packaging/feedback checks were one-off acceptance work, not a permanent workflow.

## Minimal public API

`definePlugin<T>({id, requires?, contributions?, setup(context)})` identifies one instance and exposes its ordinary service object. Multiple instances use distinct IDs and exact plugin references. `context.get(plugin)` accesses only declared initialized dependencies. `context.onCleanup(async () => ...)` registers acquired resources during setup.

`defineApp({plugins})`, `validatePlugins(plugins)`, `startApp(app)` provide validated dependency order and a running app exposing `get(plugin)`, `status()`, generic `contributions(kind?)`, and idempotent async `stop()`. Effect is private to resource finalization; business services have no Effect/RPC types. Generic contribution records are metadata, with no Console-specific semantics.

Optional `@lenso/web` supplies `createWebPlugin({requires,router,prefix?})`; its service exposes Fetch. `@lenso/web/client` reuses oRPC RouterClient inference. Engine generates `.lenso/server.ts`, `.lenso/client.ts`, and a diagnostic manifest. The browser client imports the application router type only. No second DI/RPC/bundler, Auth/DB dependency in core, Rust build chain, or platform claims.
