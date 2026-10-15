# Start with the native service

Use the smallest existing template: `templates/cli` for one service/operation, `templates/bun-web` for HTTP, `templates/workspace` only for genuinely separate app entries/shared code, `templates/workers` for a Worker. These are suggestions, not mandatory directory layouts. An existing useful `contracts/` or `infrastructure/` directory does not need removal.

The ordinary `createGreetingService` in Bun Web can be called without Core. Its plugin only binds config/dependencies; the HTTP router and CLI operation reuse that service. For exact two-instance dependencies and rollback, the repository's `lenso-migrate` skill has runnable native catalog assets and a verification helper. For a real DB/Auth service, inspect only `examples/notes/src/notes.ts` and the selected resource/entry files.

Use one real plugin object per instance, pass it into the consuming factory, put it in `requires`, and resolve that same object with `context.get`. Install both provider and consumer in the App. Do not create another provider with the same ID, scan methods or build a global service locator.

Acquire resources in setup and immediately register cleanup; factories/config module imports only declare the graph. Call the returned service directly. Wrap app use so stop runs on success/failure and retains both errors if they fail together. A service object retained after stop needs its own closed/drain behavior; the framework cannot revoke arbitrary native references.

Public package exports normally resolve to built `dist`. For a source checkout, prepare the pinned SDK, frozen-install and build affected packages before the consumer. The validated [Bun Web quickstart](https://github.com/LioRael/lenso/blob/main/docs/BUN_WEB_QUICKSTART.md) installs packed public artifacts in an independent app. It does not establish npm publication.

Share browser types through type-only exports and browser-safe clients; keep listeners/config/Engine out of that dependency graph. Extract `packages/` only for an actual shared/build/release boundary. The `lenso-best-practices` skill contains the broader repository decisions without replacing this API workflow.

The DX follow-up delivered local commit `b79aa1e`; read [recipe availability and reuse](recipes.md) for login/private-file/task factories and static assembly diagnostics. Those files are not part of the earlier source baseline until selected/integrated.
