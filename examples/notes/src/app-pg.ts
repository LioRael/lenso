import { createBunSqlPlugin } from "@lenso/db/bun-sql";
import { createNotesPlugin } from "./notes";
import { createPgNotesQueries } from "./queries-pg";
import * as schema from "./schema-pg";

export function createPgNotesPlugins(connection: string) {
  const database = createBunSqlPlugin({ id: "notes-db", connection, schema });
  const notes = createNotesPlugin({ id: "notes", database, queries: createPgNotesQueries });
  return { database, notes, plugins: [database, notes] };
}

export function databaseUrl(): string {
  const value = process.env.DATABASE_URL;
  if (!value)
    throw new Error("Set DATABASE_URL to a PostgreSQL database; run migrate:pg explicitly first");
  return value;
}
