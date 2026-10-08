import { createBunSqlPlugin } from "@lenso/db/bun-sql";
import { postgresSessionStore } from "@lenso/auth/drizzle/pg";
import type { SessionLifetime } from "@lenso/auth/sessions";
import type { NotesPrincipal } from "./auth";
import { createNotesApplication } from "./application";
import { createPgNotesQueries } from "./queries-pg";
import * as schema from "./schema-pg";

export function createPgNotesPlugins(
  connection: string,
  principals: readonly NotesPrincipal[] | (() => readonly NotesPrincipal[]),
  lifetime?: SessionLifetime,
) {
  const database = createBunSqlPlugin({ id: "notes-db", connection, schema });
  return createNotesApplication({
    database,
    store: postgresSessionStore,
    queries: createPgNotesQueries,
    principals,
    lifetime,
  });
}

export function databaseUrl(): string {
  const value = process.env.DATABASE_URL;
  if (!value)
    throw new Error("Set DATABASE_URL to a PostgreSQL database; run migrate:pg explicitly first");
  return value;
}
