# TypeScript framework versus application

Evidence: `packages/lenso/src/{plugin,lifecycle,config}.ts`, `packages/engine/src/{engine-defaults,engine-authoring}.ts`, `packages/manage/README.md`, `examples/notes`, `templates/workspace` at the recorded baseline.

Core is `packages/lenso`; CLI command behavior is `packages/cli`; Engine is development/build tooling. Optional Web/Auth/DB/Workers/Runtime packages own their adapters. Do not move platform imports into Core or build-time Engine imports into browser/business runtime graphs. Keep changes in the owner of the actual behavior and inspect export maps before adding dependencies.

`definePlugin` supplies a typed ordinary service; the actual plugin object is the dependency key. `requires`, `context.get`, `app.get`, Operation and Manage declarations all retain that object. Same ID is not same instance. Keep new instances fresh per app lifetime where their closures own mutable state.

Ordinary factory config is valid. Use Core `definePluginConfig`/`bindConfig` only when typed multi-source startup config helps. Values are finite frozen data, not live clients; source order replaces top-level fields, not deep-merges. Resource acquisition belongs in setup with immediate cleanup; borrowed resources do not become owned by injection. Engine config and runtime instance config remain different lifetimes.

Authenticate at the trusted entry using the designated Auth instance. In the shared service enforce operation audience, current membership, object/owner/tenant policy. A UI visibility check, caller ID, serialized Actor or business JSON field cannot grant authority. Reuse actual Auth flows rather than a second login implementation. Read `lenso-develop/references/web-auth.md` for the selected boundary.

Define one existing service Operation, then select it explicitly for CLI, MCP or Manage. `defineManage` is optional; it does not open an entry. Contextual operations bind trusted evidence separately from JSON input. Console can reuse the selected Manage capabilities while business authorization remains in the service. Adapters borrow runtime unless their API explicitly owns startup/stop. Keep current caller permission checks current; don't cache Actors in global state. Reuse safe error projectors; unknown/aggregate/cleanup errors stay opaque externally while causes remain internal.

Use Engine `convention`, `discover`, `generate`, `target`, `dev` and `watch` for actual authoring needs. Generated output has one owner and path/edit protection. Do not maintain a second generator or silently patch `.lenso`/`dist`. Code-first authoring is a preference and separately evolving work; only use exports verified in the selected build.

Build affected packages before consumers because normal exports use `dist`; source conditions are an explicit development mode, not a claim that all consumers use source. Keep the root lockfile with its integration owner. One source commit can still be unpublished: packed artifacts or a matched source dependency are different from registry availability.
