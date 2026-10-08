import type { SessionLifetime, SessionStore } from "@lenso/auth/sessions";
import type { Plugin } from "@lenso/core";
import { createApplicationAuth } from "./application-auth";
import type { NotesPrincipal } from "./auth";
import { createNotesPlugin, type NotesQueries } from "./notes";

export function createNotesApplication<T>(options: {
  database: Plugin<T>;
  store(database: T): SessionStore<string>;
  queries(database: T): NotesQueries;
  principals: readonly NotesPrincipal[] | (() => readonly NotesPrincipal[]);
  lifetime?: SessionLifetime;
}) {
  const authentication = createApplicationAuth(options);
  const notes = createNotesPlugin({
    id: "notes",
    database: options.database,
    authentication,
    queries: options.queries,
  });
  return {
    database: options.database,
    authentication,
    notes,
    plugins: [options.database, authentication, notes],
  };
}
