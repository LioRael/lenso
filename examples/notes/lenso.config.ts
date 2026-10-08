import { defineApp, type Plugin } from "@lenso/core";
import { resolve } from "node:path";
import { parseNotesPrincipals } from "./src/auth";
import { createPgNotesPlugins } from "./src/app-pg";
import { createNotesFiles } from "./src/files";
import { createNotesOperations, createNotesFileOperations } from "./src/operations";

const principals = () => parseNotesPrincipals(process.env.NOTES_LOGIN_KEYS);
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
const notesOperations = createNotesOperations({
  notes: definition.notes,
  authentication: definition.authentication,
  credential,
});
const fileOperations = local
  ? createNotesFileOperations({
      files: local.files,
      authentication: local.authentication,
      credential,
    })
  : undefined;
export const operations = [
  ...notesOperations.operations,
  ...(fileOperations ? fileOperations.operations : []),
];
const plugins: Plugin<unknown>[] = [
  ...definition.plugins,
  notesOperations.plugin,
  ...(fileOperations ? [fileOperations.plugin] : []),
];
export default defineApp({ plugins });
