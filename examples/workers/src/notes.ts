import { createD1Plugin } from "@lenso/db/d1";
import { d1SessionStore } from "@lenso/auth/drizzle/d1";
import { createWorkerHandler } from "@lenso/workers";
import { createNotesApplication } from "../../notes/src/application";
import { envSource } from "@lenso/core/config/env";
import { createSqliteNotesQueries } from "../../notes/src/queries-sqlite";
import { createNotesWebPlugin } from "../../notes/src/web";
import * as schema from "../../notes/src/schema-sqlite";

interface AuthenticatedNotesEnv extends NotesEnv {
  NOTES_LOGIN_KEYS: string;
  NOTES_RENEW_AFTER_MS?: string;
}

/** The same Notes service, schema, and queries used by the Bun SQLite example. */
export default createWorkerHandler<AuthenticatedNotesEnv>((env) => {
  const database = createD1Plugin({ id: "notes-db", binding: env.DB, schema });
  const application = createNotesApplication({
    database,
    store: d1SessionStore,
    queries: createSqliteNotesQueries,
    principals: {
      sources: [
        envSource({
          id: "notes-worker-env",
          read: (name) =>
            name === "NOTES_LOGIN_KEYS" ? env.NOTES_LOGIN_KEYS : env.NOTES_RENEW_AFTER_MS,
          bindings: {
            principals: { name: "NOTES_LOGIN_KEYS", sensitive: true },
            renewAfter: { name: "NOTES_RENEW_AFTER_MS", type: "number" },
          },
        }),
      ],
    },
  });
  const { notes, authentication } = application;
  const web = createNotesWebPlugin(notes, authentication);
  return { plugins: [...application.plugins, web], web };
});
