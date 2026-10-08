import { createBunSqlPlugin } from "@lenso/db/bun-sql";
import { postgresSessionStore } from "@lenso/auth/drizzle/pg";
import type { SessionLifetime } from "@lenso/auth/sessions";
import { createNotesAuthPlugin, type NotesPrincipal } from "./auth";
import { createNotesPlugin } from "./notes";
import { createPgNotesQueries } from "./queries-pg";
import * as schema from "./schema-pg";

export function createPgNotesPlugins(
  connection: string,
  principals: readonly NotesPrincipal[],
  lifetime?: SessionLifetime,
) {
  const database = createBunSqlPlugin({ id: "notes-db", connection, schema });
  const authentication = createNotesAuthPlugin({
    database,
    store: postgresSessionStore,
    principals,
    lifetime,
  });
  const notes = createNotesPlugin({
    id: "notes",
    database,
    authentication,
    queries: createPgNotesQueries,
  });
  return {
    database,
    authentication,
    auth: authentication,
    notes,
    plugins: [database, authentication, notes],
  };
}

export function databaseUrl(): string {
  const value = process.env.DATABASE_URL;
  if (!value)
    throw new Error("Set DATABASE_URL to a PostgreSQL database; run migrate:pg explicitly first");
  return value;
}
