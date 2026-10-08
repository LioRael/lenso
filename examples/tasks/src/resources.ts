import { createTaskQueue } from "@lenso/tasks";
import { createPostgresTaskProvider } from "@lenso/tasks/postgres";
import { drizzle } from "drizzle-orm/node-postgres";
import pg from "pg";
import { readConfig, reportFailure } from "./config";
import { createReportService } from "./report-service";
import * as schema from "./schema";
import { createReportTask } from "./task";

export async function openResources() {
  const config = readConfig();
  const pool = new pg.Pool({ connectionString: config.connectionString });
  pool.on("error", () => reportFailure("Business database connection"));
  try {
    const service = createReportService(drizzle({ client: pool, schema }));
    const task = createReportTask(service);
    const provider = await createPostgresTaskProvider(config);
    let queue;
    try {
      queue = createTaskQueue({ provider, tasks: [task] });
    } catch (error) {
      await provider.close();
      throw error;
    }
    return {
      queue,
      task,
      service,
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
