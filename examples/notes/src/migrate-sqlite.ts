import { Database } from "bun:sqlite";
import { fileURLToPath } from "node:url";
import { drizzle } from "drizzle-orm/bun-sqlite";
import { migrate } from "drizzle-orm/bun-sqlite/migrator";

export function migrateSqlite(filename: string) {
  const client = new Database(filename);
  try {
    migrate(drizzle(client), {
      migrationsFolder: fileURLToPath(new URL("../migrations/sqlite", import.meta.url)),
    });
  } finally {
    client.close();
  }
}

if (import.meta.main) {
  const filename = process.env.SQLITE_PATH;
  if (!filename) throw new Error("Set SQLITE_PATH to the SQLite file to migrate");
  migrateSqlite(filename);
}
