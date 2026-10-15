import { createAuth, defineSource, realm } from "@lenso/auth";
import { createAuthPlugin } from "@lenso/auth/plugin";
import { defineApp, definePlugin, startApp } from "@lenso/core";
import { createTaskQueue, type TaskQueue } from "@lenso/tasks";
import { createPostgresTaskProvider } from "@lenso/tasks/postgres";
import { drizzle } from "drizzle-orm/node-postgres";
import pg from "pg";
import { migrateTaskExample } from "./migrate";
import { createOwnershipStore } from "./ownership";
import { createTaskRecipe } from "./recipe";
import { createReportService } from "./report-service";
import * as schema from "./schema";
import { createReportTask } from "./task";

// This disposable demo requires an explicit local URL. Application compositions
// import recipe.ts and supply their own existing Auth/DB/queue resource plugins.
export function localRecipeConfig(connectionString: string | undefined) {
  const message =
    "Set TASK_RECIPE_DATABASE_URL to a local PostgreSQL database named lenso_recipe_<name>.";
  if (!connectionString) throw new Error(message);
  let url: URL;
  try {
    url = new URL(connectionString);
  } catch {
    throw new Error(message);
  }
  if (
    !["postgres:", "postgresql:"].includes(url.protocol) ||
    !["localhost", "127.0.0.1", "[::1]"].includes(url.hostname) ||
    !/^\/lenso_recipe_[a-z0-9_]+$/.test(url.pathname) ||
    url.search ||
    url.hash
  )
    throw new Error(message);
  return { connectionString, queueName: "recipe-reports" };
}

export function createLocalTaskRecipeApp(connectionString: string, evidence: () => string | null) {
  const config = localRecipeConfig(connectionString);
  const authentication = createAuthPlugin({
    id: "existing-auth",
    setup: () =>
      createAuth(
        realm(
          "local-recipe",
          defineSource<string | null>({
            async verify(credential) {
              // Synthetic credentials only; replace this source with your existing
              // verifier. No signup, session issuance or real user account is added.
              if (credential === null) return { status: "absent" };
              const subjectId =
                credential === "local-recipe-alice"
                  ? "alice"
                  : credential === "local-recipe-bob"
                    ? "bob"
                    : null;
              return subjectId ? { status: "verified", subjectId } : { status: "rejected" };
            },
          }),
        ),
      ),
  });
  const resources = definePlugin({
    id: "existing-report-resources",
    async setup(context) {
      const pool = new pg.Pool({ connectionString: config.connectionString });
      context.onCleanup(() => pool.end());
      pool.on("error", () => console.error("Local recipe database connection failed."));
      const db = drizzle({ client: pool, schema });
      const reports = createReportService(db);
      const task = createReportTask(reports);
      // The provider borrows this exact pool. Closing the queue does not close it.
      const provider = await createPostgresTaskProvider({ pool, queueName: config.queueName });
      let queue: TaskQueue | undefined;
      context.onCleanup(() => (queue ? queue.close() : provider.close()));
      queue = createTaskQueue({ provider, tasks: [task] });
      return { queue, task, reports, ownership: createOwnershipStore(db, config.queueName) };
    },
  });
  const recipe = createTaskRecipe({
    id: "report-jobs",
    auth: authentication,
    resources,
    evidence,
  });
  return {
    definition: defineApp({ plugins: [authentication, resources, recipe.plugin] }),
    recipe,
    resources,
  };
}

export async function runTaskRecipeDemo(connectionString: string) {
  const { definition, recipe, resources } = createLocalTaskRecipeApp(
    connectionString,
    () => "local-recipe-alice",
  );
  const app = await startApp(definition);
  try {
    const service = app.get(recipe.plugin);
    const reportId = `recipe-${crypto.randomUUID()}`;
    const submitted = await service.submit({ reportId, rows: [3, 5, 8] });
    // Execution is explicit. A long-lived worker may instead use startWorker().
    await app.get(resources).queue.runBatch({ maxJobs: 10, timeoutMs: 10_000 });
    const status = await service.query(submitted);
    const report = await service.report({ reportId });
    if (status?.state !== "succeeded" || report?.sum !== 16 || report.count !== 3)
      throw new Error("Local recipe did not complete its report. Check the queue worker.");
    return { ...submitted, status, report };
  } finally {
    await app.stop();
  }
}

if (import.meta.main) {
  await (async () => {
    const config = localRecipeConfig(process.env.TASK_RECIPE_DATABASE_URL);
    if (process.argv.includes("--migrate")) {
      await migrateTaskExample(config);
      console.log("Local recipe queue, report and ownership tables migrated.");
    } else {
      console.log(JSON.stringify(await runTaskRecipeDemo(config.connectionString), null, 2));
    }
  })().catch(() => {
    console.error(
      "Task recipe failed. Set TASK_RECIPE_DATABASE_URL to an owned local lenso_recipe_<name> database, run recipe-demo.ts --migrate, and check PostgreSQL availability. Credentials and driver details are omitted.",
    );
    process.exitCode = 1;
  });
}
