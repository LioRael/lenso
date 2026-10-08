import { migratePostgresTaskQueue } from "@lenso/tasks/postgres";
import { sql } from "drizzle-orm";
import { drizzle } from "drizzle-orm/node-postgres";
import pg from "pg";
import { readConfig, reportFailure } from "./config";

async function main() {
  const config = await readConfig();
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
    await drizzle({ client: pool }).execute(sql`
      CREATE TABLE IF NOT EXISTS task_example_report_owners (
        report_id text PRIMARY KEY,
        realm_id text NOT NULL,
        subject_id text NOT NULL
      )
    `);
    await drizzle({ client: pool }).execute(sql`
      CREATE TABLE IF NOT EXISTS task_example_job_reports (
        queue_name text NOT NULL,
        job_id text NOT NULL,
        report_id text NOT NULL REFERENCES task_example_report_owners(report_id),
        PRIMARY KEY (queue_name, job_id)
      )
    `);
    console.log("Queue, report and ownership tables migrated.");
  } finally {
    await pool.end();
  }
}

if (import.meta.main) {
  await main().catch(() => reportFailure("Migration"));
}
