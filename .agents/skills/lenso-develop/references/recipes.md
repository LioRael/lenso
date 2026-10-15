# Delivered login, private-file and durable-task recipes

Availability: local DX commit `b79aa1e`, based on `74139b0`. The skill checkout itself remains on the earlier source baseline; these files become usable only after selecting/integrating that delivery. This is not a registry-publication claim. Inspect the named files in the actual checkout before importing them.

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
