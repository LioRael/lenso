import { definePlugin, type Plugin } from "@lenso/core";
import type { ObjectStorage } from "@lenso/storage";
import {
  createFilesPlugin,
  type Files,
  type FileAction,
  type FileRecord,
  type FileQueries,
} from "@lenso/storage/files";
import type { NotesAuthentication } from "./auth";
import { notesAudiences, type NotesActor } from "./notes";

export type NotesFileAccess = NotesActor<"create" | "read" | "fileMetadata" | "fileDelete">;
export const notesFileTenant = "local-notes";
const fileAudience = {
  upload: notesAudiences.create,
  completeUpload: notesAudiences.create,
  metadata: notesAudiences.fileMetadata,
  read: notesAudiences.read,
  delete: notesAudiences.fileDelete,
  signDownload: notesAudiences.read,
} satisfies Record<FileAction, (typeof notesAudiences)[keyof typeof notesAudiences]>;

/** Borrow exact instances. This factory creates no Auth, database, bucket or migration. */
export function createPrivateNotesFiles<T>(options: {
  id?: string;
  database: Plugin<T>;
  authentication: Plugin<NotesAuthentication>;
  storages: readonly Plugin<ObjectStorage>[];
  queries(database: T): FileQueries;
}) {
  const { database, authentication, storages, queries } = options;
  const id = options.id ?? "note-files";
  return definePlugin<Files<NotesFileAccess>>({
    id,
    requires: [database, authentication, ...storages],
    setup(context) {
      const auth = context.get(authentication);
      return createFilesPlugin({
        id,
        storages,
        database,
        queries,
        async authorize({
          access,
          action,
          file,
        }: {
          access: NotesFileAccess;
          action: FileAction;
          file: Readonly<FileRecord>;
        }) {
          await auth
            .for(fileAudience[action])
            .enforce(
              access,
              file,
              ({ principal, resource }) =>
                principal.kind === "user" &&
                principal.subjectId === resource.ownerId &&
                resource.tenantId === notesFileTenant,
            );
          return true;
        },
      }).setup(context);
    },
  });
}
