import { migratePostgresTaskQueue } from "@lenso/tasks/postgres";
import { sql } from "drizzle-orm";
import { drizzle } from "drizzle-orm/node-postgres";
import pg from "pg";
import { readConfig, reportFailure } from "./config";

async function main() {
  const config = readConfig();
  await migratePostgresTaskQueue(config);
  const pool = new pg.Pool({ connectionString: config.connectionString });
  pool.on("error", () => reportFailure("Migration database connection"));
  try {
    await drizzle({ client: pool }).execute(sql`
      CREATE TABLE IF NOT EXISTS task_example_reports (
        report_id text PRIMARY KEY,
        sum double precision NOT NULL,
        count integer NOT NULL,
        updated_at timestamptz NOT NULL
      )
    `);
    console.log("Queue and report table migrated.");
  } finally {
    await pool.end();
  }
}

if (import.meta.main) {
  await main().catch(() => reportFailure("Migration"));
}
