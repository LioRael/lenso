# Application shape follows actual ownership

A small app can keep one `lenso.config.ts` and a service file. For HTTP, the existing `templates/bun-web` uses `src/greeting.ts`, `src/router.ts`, `src/server.ts`. A useful larger shape is `src/plugins/<feature>/` containing service, schema, plugin factory and its selected adapters. These are suggestions: existing `features/`, `modules/` or flat layouts can express the same boundaries.

Use `apps/server` and `apps/web` only when there are genuinely separate entries/builds. `templates/workspace` demonstrates a shared `plugins/greeting` workspace package; its directory name is incidental. `packages/` is appropriate for multiple consumers or independent distribution, not mandatory for every service. A single app need not start with an `apps/` directory.

- Business functions take ordinary typed inputs/dependencies. They can run without Core. Plugins own resource construction and exact instance wiring; HTTP/CLI/MCP/Console adapters call the same business implementation.
- Share types with `import type` and browser-safe public exports. The workspace example exposes `@app/server/client` as a type-only entry; browser code imports `@lenso/web/client`, never server setup, env/filesystem readers or Engine.
- Optional capability means an explicit absent branch or supplied provider, not hidden string lookup or a global container. Multiple instances of the same capability keep different identities and names.
- Keep resources with their owner: new DB/client/listener handles register immediate cleanup; external platform bindings and supplied clients remain borrowed. Config module imports must not listen/connect. Core's startup preflight may read explicit sources but is not the place for persistent migrations.
- Keep one source of business schemas when adapters need validation; a local function does not need a JSON transport contract. An existing contracts directory may hold useful shared types; retain it where it has a purpose.

Engine's default convention discovers `lenso.config.ts`, `src/server.ts` and `src/router.ts`; generated files belong under `.lenso`, builds under `dist`. `lenso.engine.ts` can replace the convention with an explicit owner and custom entry (see workspace `runtime/main.ts`), while keeping default generators/targets. Keep a canonical config or re-export for inspect/call. Engine reduces assembly boilerplate; it does not auto-publish every native method or replace runtime ownership.

For persisted state, identify the real store, transaction owner, schema/migration command and deployment order before changing data. Startup should not silently create/migrate tables. In-memory examples are deliberately ephemeral. Task queues can repeat effects after lease loss; file metadata and object writes are not one atomic transaction. Keep existing idempotency, compensation and reconciliation at those boundaries.

A useful handoff includes the selected app entry, exact instances/resources, commands that exercised the real adapter and any intentionally unrun persistence/UI boundary. Reuse the Bun Web quickstart for packed-consumer installation rather than introducing a new scaffold system.
