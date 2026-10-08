import { SQL } from "bun";
import { fileURLToPath } from "node:url";
import { drizzle } from "drizzle-orm/bun-sql";
import { migrate } from "drizzle-orm/bun-sql/migrator";
import { databaseUrl } from "./app-pg";

export async function migratePostgres(connection: string) {
  const client = new SQL(connection);
  try {
    await migrate(drizzle(client), {
      migrationsFolder: fileURLToPath(new URL("../migrations/pg", import.meta.url)),
    });
  } finally {
    await client.close();
  }
}

if (import.meta.main) await migratePostgres(databaseUrl());
