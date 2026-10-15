import { expect, test } from "bun:test";
import { startApp } from "@lenso/core";
import { migrateTaskExample } from "./migrate";
import { createLocalTaskRecipeApp, localRecipeConfig } from "./recipe-demo";

// Opt in only with a dedicated local DB; migration is explicit test setup.
test.skipIf(!process.env.TASK_RECIPE_TEST_DATABASE_URL)(
  "recipe submits, survives app restart, runs a real PostgreSQL worker and reads durable owner-scoped results",
  async () => {
    const config = localRecipeConfig(process.env.TASK_RECIPE_TEST_DATABASE_URL);
    await migrateTaskExample(config);
    const reportId = `recipe-test-${crypto.randomUUID()}`;
    let credential: string | null = "local-recipe-alice";
    const composition = createLocalTaskRecipeApp(config.connectionString, () => credential);
    const producer = await startApp(composition.definition);
    let submitted: { jobId: string };
    try {
      const service = producer.get(composition.recipe.plugin);
      submitted = await service.submit({ reportId, rows: [7, 8, 9] });
      expect(await service.query(submitted)).toMatchObject({ state: "pending" });
      expect(await service.report({ reportId })).toBeNull();
      credential = "local-recipe-bob";
      for (const method of ["query", "cancel", "retry"] as const)
        await expect(service[method](submitted)).rejects.toMatchObject({ code: "FORBIDDEN" });
      await expect(service.report({ reportId })).rejects.toMatchObject({ code: "FORBIDDEN" });
      await expect(service.submit({ reportId, rows: [99] })).rejects.toMatchObject({
        code: "FORBIDDEN",
      });
      credential = null;
      await expect(service.query(submitted)).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    } finally {
      await producer.stop();
    }

    // A new application/connection observes the same job; no in-memory map is used.
    credential = "local-recipe-alice";
    const worker = await startApp(composition.definition);
    try {
      const resources = worker.get(composition.resources);
      await resources.queue.runBatch({ maxJobs: 10, timeoutMs: 10_000 });
      expect(await resources.queue.get(submitted!.jobId)).toMatchObject({
        state: "succeeded",
        result: { sum: 24, count: 3 },
      });
    } finally {
      await worker.stop();
    }

    const reader = await startApp(composition.definition);
    try {
      const service = reader.get(composition.recipe.plugin);
      expect(await service.query(submitted!)).toMatchObject({ state: "succeeded", attempt: 1 });
      expect(await service.report({ reportId })).toEqual({ sum: 24, count: 3 });
      credential = "local-recipe-bob";
      await expect(service.report({ reportId })).rejects.toMatchObject({ code: "FORBIDDEN" });
    } finally {
      await reader.stop();
    }
  },
  30_000,
);
