import { defineApp, type Plugin } from "@lenso/core";
import { resolve } from "node:path";
import { envSource } from "@lenso/core/config/env";
import { createPgNotesPlugins } from "./src/app-pg";
import { createNotesFiles } from "./src/files";
import {
  createNotesOperationsPlugin,
  createNotesFileOperationsPlugin,
  declareNotesOperations,
  declareNotesFileOperations,
} from "./src/operations";

const principals = {
  sources: [
    envSource({
      id: "notes-env",
      read: (name) => process.env[name],
      bindings: { principals: { name: "NOTES_LOGIN_KEYS", sensitive: true } },
    }),
  ],
};
const credential = () => process.env.NOTES_SESSION ?? null;
const local =
  process.env.DATABASE_URL && !process.env.SQLITE_PATH
    ? undefined
    : createNotesFiles({
        filename: resolve(import.meta.dir, process.env.SQLITE_PATH ?? "output/notes.sqlite"),
        root: resolve(import.meta.dir, process.env.STORAGE_ROOT ?? "output/notes-files"),
        principals,
      });
export const definition = local ?? createPgNotesPlugins(process.env.DATABASE_URL!, principals);
export const notesOperations = createNotesOperationsPlugin({
  notes: definition.notes,
  authentication: definition.authentication,
  credential,
});
const fileOperations = local
  ? createNotesFileOperationsPlugin({
      files: local.files,
      authentication: local.authentication,
      credential,
    })
  : undefined;
export const operations = [
  ...declareNotesOperations(notesOperations),
  ...(fileOperations ? declareNotesFileOperations(fileOperations) : []),
];
const plugins: Plugin<unknown>[] = [
  ...definition.plugins,
  notesOperations,
  ...(fileOperations ? [fileOperations] : []),
];
export default defineApp({ plugins });
