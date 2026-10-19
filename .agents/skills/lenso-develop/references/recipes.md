# Delivered login, private-file and durable-task recipes

Availability: these recipes are included in the repository source at `6dae6eca8499dcec126cc181a5daca9f1de292a0`. They are example source, not a registry-publication claim. Inspect the named files in the selected checkout and verify the application's installed dependency exports before adapting them; source availability does not establish support in an installed package version.

- Login/private files: `examples/notes/RECIPES.md`, `src/private-files.ts`, `src/recipes/web.ts`.
- Static assembly view: `examples/notes/src/recipes/inspect.ts`.
- Durable tasks: `examples/tasks/RECIPE.md`, `src/recipe.ts`.
- Delivery evidence and limitations: `examples/notes/RECIPE_VERIFICATION.md`.

`createPrivateNotesFiles({database, authentication, storages, queries})` borrows exact installed provider objects. It creates no DB, Auth, bucket or migration. Its policy is Notes-specific realm/audiences, owner and fixed tenant: another application's Auth needs an explicit adapter, not a fabricated actor or silent second verifier.

`createRecipeWeb({authentication, login?, files?: {plugin, storageId}})` selects removable HTTP entries. Omit files for login-only; set `login: false` for files-only; omit Web/listener for services-only. Login exchanges an existing Notes key for a managed **bearer** session; it is not password signup, OAuth or a browser login UI. Upload is a bounded raw body; server-selected tenant/storage and authenticated ownership stay outside business JSON.

`inspectRecipes(target)` projects existing public inspect declarations, source, providers and missing dependencies. It imports trusted config but does not run setup, evaluate Config sources, open storage or infer methods/routes. `declared` is not healthy, migrated, listening or authorized.

`createTaskRecipe({id, auth, resources, evidence})` returns `{plugin, operations}` over existing `TaskServiceResources`; it borrows resources and exposes submit/query/report explicitly. The demo's verifier is synthetic. `evidence()` is a trusted launch identity; a multi-user host must bind evidence per invocation. Sharing a DB pool does not make queue acceptance and ownership recording one transaction. Worker startup and migrations remain explicit, with no exactly-once guarantee.

After selecting the delivered source and building its dependencies, use its documented entrypoints:

```sh
bun run --cwd examples/notes recipes:inspect
bun run --cwd examples/notes recipes:demo
bun test examples/notes/test/recipes.test.ts examples/notes/test/files.test.ts examples/tasks/src/recipe.test.ts
```

The DX owner's checked-in evidence reports Notes/Tasks typechecks, 8 tests/124 assertions, real Engine readiness/login/shutdown, SQLite file restart and PostgreSQL task restart persistence. This skill task inspected source/report and did not repeat those integration tests. The R2 result is a separate local workerd provider test, **not** a complete Auth+D1+R2 deployment. Follow the recipe's explicit disposable database/migration instructions only when that boundary is part of the authorized task.

## Repeated local service calls without repeated setup

For matching Engine/CLI/native source builds, the existing greeting entry is the smallest runnable scene: `examples/greeting/src/server.ts` lends one app to its Web listener and `serveDevOperations`, while `lenso.config.ts` and the oRPC router reuse `greetingInput`/`greetingOutput`. Use `dev --connect`, explicit connected inspect, generation-pinned call and bounded recent metadata as documented in the CLI guide. Do not install old registry versions and assume these source exports exist; do not force a Runtime dependency or reconstruct this path with pack/vendor overrides. This scene is not an Agent loop or deployment recipe.
