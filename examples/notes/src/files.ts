import { Database } from "bun:sqlite";
import { join, resolve } from "node:path";
import { createBunSqlitePlugin } from "@lenso/db/bun-sqlite";
import {
  createFilesPlugin,
  type Files,
  type FileAction,
  type FileRecord,
} from "@lenso/storage/files";
import { createLocalStoragePlugin } from "@lenso/storage/local";
import { createSqliteFileQueries, fileSchema } from "@lenso/storage/sqlite";
import { sqliteSessionStore } from "@lenso/auth/drizzle/sqlite";
import { definePlugin, startApp } from "@lenso/core";
import type { NotesPrincipalsInput } from "./auth";
import { createApplicationAuth } from "./application-auth";
import { migrateSqlite } from "./migrate-sqlite";
import { createNotesPlugin, notesAudiences, type NotesActor } from "./notes";
import { createSqliteNotesQueries } from "./queries-sqlite";
import * as notesSchema from "./schema-sqlite";

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

/** Callers authenticate first; neither fileId nor a storage instance grants access. */
export function createNotesFiles(options: {
  filename: string;
  root: string;
  principals: NotesPrincipalsInput;
}) {
  const database = createBunSqlitePlugin({
    id: "notes-db",
    filename: options.filename,
    schema: { ...notesSchema, ...fileSchema },
  });
  const authentication = createApplicationAuth({
    database,
    store: sqliteSessionStore,
    principals: options.principals,
  });
  const notes = createNotesPlugin({
    id: "notes",
    database,
    authentication,
    queries: createSqliteNotesQueries,
  });
  // These names select directories, not public bucket ACLs.
  const publicAssets = createLocalStoragePlugin({
    id: "publicAssets",
    root: join(options.root, "assets"),
  });
  const privateFiles = createLocalStoragePlugin({
    id: "privateFiles",
    root: join(options.root, "private"),
  });
  const files = definePlugin<Files<NotesFileAccess>>({
    id: "note-files",
    requires: [database, authentication, publicAssets, privateFiles],
    setup(context) {
      const auth = context.get(authentication);
      return createFilesPlugin({
        id: "note-files",
        storages: [publicAssets, privateFiles],
        database,
        queries: createSqliteFileQueries,
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
  return {
    database,
    authentication,
    notes,
    publicAssets,
    privateFiles,
    files,
    plugins: [database, authentication, notes, publicAssets, privateFiles, files],
  };
}

/** Explicit command only. Plugin setup never executes this SQL. */
export async function migrateFiles(filename: string) {
  migrateSqlite(filename);
  const migration = await Bun.file(
    new URL("../../../packages/storage/migrations/sqlite/0001_files.sql", import.meta.url),
  ).text();
  const client = new Database(filename);
  try {
    client.exec(migration);
  } finally {
    client.close();
  }
}

async function demo(filename: string, root: string) {
  const principals = () => process.env.NOTES_LOGIN_KEYS;
  const definition = createNotesFiles({ filename, root, principals });
  const app = await startApp({ plugins: definition.plugins });
  try {
    const authentication = app.get(definition.authentication);
    const key = process.env.NOTES_LOGIN_KEY;
    if (!key) throw new Error("Set NOTES_LOGIN_KEY to a configured Notes login key");
    const session = await authentication.issue(key);
    const actor = (operation: "create" | "read" | "fileDelete") =>
      authentication.for(notesAudiences[operation]).required(session.credential);
    const access = await actor("create");
    const files = app.get(definition.files);
    const record = await files.upload(access, {
      storageId: definition.privateFiles.id,
      filename: "private-note.txt",
      contentType: "text/plain",
      ownerId: access.subjectId,
      tenantId: notesFileTenant,
      body: new Blob(["A private attachment stored with its file record.\n"]).stream(),
      maxBytes: 1024,
    });
    console.log({ fileId: record.fileId, storageId: record.storageId, size: record.size });
    const download = await files.read(await actor("read"), record.fileId);
    const reader = download.body.getReader();
    // The console is the destination here; production uses Response(body) or a streamed file sink.
    const decoder = new TextDecoder();
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      process.stdout.write(decoder.decode(value, { stream: true }));
    }
    process.stdout.write(decoder.decode());
    await files.delete(await actor("fileDelete"), record.fileId);
    await files.delete(await actor("fileDelete"), record.fileId);
  } finally {
    await app.stop();
  }
}

if (import.meta.main) {
  const filename = process.env.SQLITE_PATH;
  if (!filename) throw new Error("Set SQLITE_PATH; run files:migrate explicitly before files:demo");
  const command = process.argv[2];
  if (command === "migrate") await migrateFiles(filename);
  else if (command === "demo") {
    await demo(filename, resolve(process.env.STORAGE_ROOT ?? "../../output/notes-files"));
  } else throw new Error("Use migrate or demo");
}
