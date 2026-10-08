import { createTaskQueue } from "@lenso/tasks";
import { createPostgresTaskProvider } from "@lenso/tasks/postgres";
import { drizzle } from "drizzle-orm/node-postgres";
import pg from "pg";
import type { Logger } from "@lenso/core";
import { readConfig, reportFailure, type TasksConfig } from "./config";
import { createReportService } from "./report-service";
import { createOwnershipStore } from "./ownership";
import * as schema from "./schema";
import { createReportTask } from "./task";

export async function openResources(
  input?: TasksConfig,
  options: { instanceId?: string; pluginId?: string; logger?: Logger } = {},
) {
  const config = input ?? (await readConfig());
  const pool = new pg.Pool({ connectionString: config.connectionString });
  pool.on("error", () => reportFailure("Business database connection"));
  try {
    const db = drizzle({ client: pool, schema });
    const service = createReportService(db);
    const task = createReportTask(service);
    const provider = await createPostgresTaskProvider(config);
    let queue;
    try {
      queue = createTaskQueue({ provider, tasks: [task], ...options });
    } catch (error) {
      await provider.close();
      throw error;
    }
    return {
      queue,
      task,
      service,
      ownership: createOwnershipStore(db, config.queueName),
      async close() {
        try {
          await queue.close();
        } finally {
          await pool.end();
        }
      },
    };
  } catch (error) {
    await pool.end();
    throw error;
  }
}
