# Durable report-job recipe

[`createTaskRecipe`](src/recipe.ts) is ordinary editable code that borrows the
host's exact Auth plugin and resource plugin. It exposes `submit`, `query` and
`report` as explicit operation declarations; it neither connects a database nor
starts a worker during import. The existing business service still checks object
ownership and Auth audiences. No Console or generator process is required.

## Run locally

Complete the clean install/build commands in
[the Notes recipe](../notes/RECIPES.md#run-from-a-clean-repository). Create a dedicated
local PostgreSQL database named `lenso_recipe_<name>` using your local database
administrator. Then:

```sh
export TASK_RECIPE_DATABASE_URL=postgresql://recipe_owner@127.0.0.1:55483/lenso_recipe_demo
bun run --cwd examples/tasks recipe:migrate
bun run --cwd examples/tasks recipe

TASK_RECIPE_TEST_DATABASE_URL="$TASK_RECIPE_DATABASE_URL" \
  bun test examples/tasks/src/recipe-postgres.test.ts
```

The displayed URL is a local fixture example; use your owned server, role and port.
The demo refuses remote hosts, query parameters and database names outside its
`lenso_recipe_` convention. No database is auto-created. Migration needs permission
to create the queue schema/tables plus report and ownership tables. Runtime access
needs the corresponding table permissions; the demo does not grant permissions.

`recipe:migrate` is explicit. The demo starts an app, enqueues `[3,5,8]`, runs one
bounded worker batch, reads the persisted report (`sum: 16`, `count: 3`), and closes
its resources. Its two local credentials are a synthetic verifier, not real user
accounts. Do not copy that verifier into an application.

## Adapt an existing app

```ts
const recipe = createTaskRecipe({
  id: "report-jobs",
  auth: existingAuthPlugin,
  resources: existingTaskResourcesPlugin,
  evidence: readTrustedLaunchCredential,
});
export const operations = recipe.operations; // explicit CLI allowlist
// Add recipe.plugin to the existing defineApp plugins; retain existing instances once.
```

The resource plugin supplies the existing queue, report task, report service and
`OwnershipStore` (`TaskServiceResources` in `authorized-service.ts`). It is the
owner of connections and queue lifecycle. The recipe does not create another
Auth, DB pool or permissions model. `createTaskService` also accepts the existing
Auth service directly; the original `createAuthorizedTaskService` wrapper remains
compatible for applications that intentionally own a verifier there.

The demo creates one explicit `pg.Pool` and shares it between Drizzle and
`createPostgresTaskProvider({ pool })`. The pool owner closes it once; provider and
queue cleanup do not close borrowed pools. Sharing a pool does **not** make queue
acceptance and ownership recording one transaction. This example offers no
exactly-once or transactional outbox guarantee. Existing task idempotency and
ownership checks still apply.

`evidence()` is trusted launch identity, suitable for the local CLI demo. A
multi-user HTTP/MCP host must bind evidence per invocation using its existing
request/context adapter. Do not use a global mutable credential for concurrent
users or accept a subject ID in job input. A CLI operation declaration does not
mount MCP/Console or grant authorization. You can remove operation declarations
and call the ordinary service directly. Use the existing Engine/CLI `inspect`
with a host config exporting the chosen declarations; do not infer service methods
from setup.

Workers are explicit: run a bounded `queue.runBatch` or use the existing
`startWorker` under the host's cleanup. A request cancellation does not cancel a
durable job. The focused PostgreSQL test stops the producer, runs a fresh worker
app, then starts a third reader app; job and owner-scoped report data survive all
three. It also rejects missing credentials and another user's query/cancel/retry,
report access and overwrite attempt.

If the demo fails, check the local URL/database, explicit migration, then the
worker. Public errors omit connection strings and driver details. A pending job
means it has been accepted, not executed. Production verifier/session-store and
remote database integrations are intentionally host-owned and were not exercised
by this local fixture. See [measured verification](../notes/RECIPE_VERIFICATION.md).
