import { sqliteSessionStore } from "@lenso/auth/drizzle/sqlite";
import { createBunSqlitePlugin } from "@lenso/db/bun-sqlite";
import type { Plugin } from "@lenso/core";
import type { ObjectStorage } from "@lenso/storage";
import { createSqliteFileQueries, fileSchema } from "@lenso/storage/sqlite";
import { createNotesAuthPlugin, type NotesPrincipalsInput } from "../auth";
import { createPrivateNotesFiles } from "../private-files";
import { createRecipeWeb } from "./web";

/** Standalone host owner. Existing apps copy only the borrowed-instance factories instead. */
export function createRecipeApplication(options: {
  filename: string;
  principals: NotesPrincipalsInput;
  storage?: Plugin<ObjectStorage>;
  login?: boolean;
}) {
  const database = createBunSqlitePlugin({
    id: "recipe-db",
    filename: options.filename,
    schema: fileSchema,
  });
  const authentication = createNotesAuthPlugin({
    database,
    store: sqliteSessionStore,
    principals: options.principals,
  });
  const storage = options.storage;
  const files = storage
    ? createPrivateNotesFiles({
        database,
        authentication,
        storages: [storage],
        queries: createSqliteFileQueries,
      })
    : undefined;
  const web = createRecipeWeb({
    authentication,
    login: options.login,
    ...(files && storage ? { files: { plugin: files, storageId: storage.id } } : {}),
  });
  return {
    database,
    authentication,
    storage,
    files,
    web,
    plugins: [database, authentication, ...(storage && files ? [storage, files] : []), web],
  };
}
