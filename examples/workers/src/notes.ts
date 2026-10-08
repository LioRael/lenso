import { createD1Plugin } from "@lenso/db/d1";
import { createWebPlugin } from "@lenso/web";
import { createWorkerHandler } from "@lenso/workers";
import { createNotesPlugin } from "../../notes/src/notes";
import { createSqliteNotesQueries } from "../../notes/src/queries-sqlite";
import { createNotesRouter } from "../../notes/src/router";
import * as schema from "../../notes/src/schema-sqlite";

/** The same Notes service, schema, and queries used by the Bun SQLite example. */
export default createWorkerHandler<NotesEnv>((env) => {
  const database = createD1Plugin({ id: "notes-db", binding: env.DB, schema });
  const notes = createNotesPlugin({
    id: "notes",
    database,
    queries: createSqliteNotesQueries,
  });
  const web = createWebPlugin({
    requires: [notes],
    router: (context) => createNotesRouter(context.get(notes)),
  });
  return { plugins: [database, notes, web], web };
});
