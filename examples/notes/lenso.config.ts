import { defineApp, type Plugin } from "@lenso/core";
import type { Operation } from "@lenso/engine/operations";
import { selectManageOperations } from "@lenso/manage";
import { resolve } from "node:path";
import { envSource } from "@lenso/core/config/env";
import { createPgNotesPlugins } from "./src/app-pg";
import { createNotesFiles } from "./src/files";
import { createNotesOperations, createNotesFileOperations } from "./src/operations";

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
const notesOperations = createNotesOperations({
  notes: definition.notes,
  authentication: definition.authentication,
});
const fileOperations = local
  ? createNotesFileOperations({
      files: local.files,
      authentication: local.authentication,
    })
  : undefined;
export const operations = [
  notesOperations.operations[0]!,
  ...selectManageOperations(notesOperations.manage, ["list", "read"]),
  notesOperations.operations[3]!,
  ...selectManageOperations(notesOperations.manage, ["remove"]),
  ...(fileOperations ? selectManageOperations(fileOperations.manage, ["metadata", "delete"]) : []),
];
export const manage = [notesOperations.manage, ...(fileOperations ? [fileOperations.manage] : [])];
export const mcpOperations = selectManageOperations(notesOperations.manage, [
  "list",
  "read",
  "remove",
]);
export const operationBinding = (_operation: Operation, _input: unknown) => ({
  context: { evidence: credential() },
});
const plugins: Plugin<unknown>[] = [
  ...definition.plugins,
  notesOperations.plugin,
  ...(fileOperations ? [fileOperations.plugin] : []),
];
export default defineApp({ plugins });
