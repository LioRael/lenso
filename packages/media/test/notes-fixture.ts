import { Database } from "bun:sqlite";
import { drizzle } from "drizzle-orm/bun-sqlite";
import { mkdtemp, realpath, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { definePlugin, startApp } from "@lenso/core";
import { createFilesPlugin, type FileAction, type FileRecord } from "@lenso/storage/files";
import { createLocalStoragePlugin } from "@lenso/storage/local";
import { createSqliteFileQueries, fileSchema } from "@lenso/storage/sqlite";
import { createMedia, MediaError, type Media, type MediaStore } from "../src";
import { createMediaFileJournal, createFilesMediaStorage, fileReference } from "../src/files";
import { createSqliteMediaStore } from "../src/sqlite";
import { createBunImageProcessor } from "../src/bun";
import { createMediaTask, createTasksMediaAdapter } from "../src/tasks";
import { createTaskQueue } from "@lenso/tasks";
import { createD1TaskProvider, provisionD1TaskQueue, type D1Database } from "@lenso/tasks/d1";
import { fixture } from "./native-fixtures";

export interface Actor {
  subjectId: string;
  tenantId: string;
  kind: "user" | "execute" | "cleanup";
  derivationId?: string;
}
export async function notesFixture(database: D1Database) {
  const root = await mkdtemp(join(await realpath(tmpdir()), "lenso-media-notes-"));
  const client = new Database(":memory:");
  let app: Awaited<ReturnType<typeof startApp>> | undefined;
  let queue: ReturnType<typeof createTaskQueue> | undefined;
  const close = async () => {
    try {
      await queue?.close();
    } finally {
      try {
        await app?.stop();
      } finally {
        client.close();
        await rm(root, { recursive: true, force: true });
      }
    }
  };
  try {
    client.exec(
      await Bun.file(
        new URL("../../storage/migrations/sqlite/0001_files.sql", import.meta.url),
      ).text(),
    );
    client.exec(
      await Bun.file(new URL("../migrations/sqlite/0001_media.sql", import.meta.url)).text(),
    );
    const fileDb = drizzle(client, { schema: fileSchema });
    const baseStore = createSqliteMediaStore(client);
    const controls = {
      clock: Date.now(),
      revoked: false,
      deriveDenied: false,
      downloadError: false,
      uploadError: "" as "" | "before" | "after",
      deleteError: false,
      registerError: "" as "" | "before" | "after",
      beforeRegister: undefined as (() => Promise<void>) | undefined,
      beforePut: undefined as (() => Promise<void>) | undefined,
      afterPut: undefined as (() => Promise<void>) | undefined,
      puts: 0,
      deletes: 0,
    };
    const store: MediaStore = {
      ...baseStore,
      async replace(id, revision, next) {
        if (next.state === "ready") {
          await controls.beforeRegister?.();
          if (controls.registerError === "before") {
            controls.registerError = "";
            throw new Error("register-before");
          }
        }
        const changed = await baseStore.replace(id, revision, next);
        if (changed && next.state === "ready" && controls.registerError === "after") {
          controls.registerError = "";
          throw new Error("register-after");
        }
        return changed;
      },
    };
    const journal = createMediaFileJournal({ store, queries: createSqliteFileQueries(fileDb) });
    const actors = new WeakSet<Actor>();
    const actor = (
      subjectId = "alice",
      tenantId = "local-notes",
      kind: Actor["kind"] = "user",
      derivationId?: string,
    ) => {
      const value: Actor = { subjectId, tenantId, kind, derivationId };
      actors.add(value);
      return value;
    };
    const alice = actor();
    let media: Media<Actor>;
    async function policy({
      access,
      action,
      file,
    }: {
      access: Actor;
      action: FileAction;
      file: Readonly<FileRecord>;
    }) {
      if (!actors.has(access)) return false;
      const artifact = await store.getArtifact(file.fileId);
      if (access.kind === "cleanup") {
        return action === "delete" && artifact?.derivationId === access.derivationId;
      }
      if (
        controls.revoked ||
        access.tenantId !== file.tenantId ||
        access.subjectId !== file.ownerId
      )
        return false;
      if (access.kind === "execute") {
        const record = access.derivationId ? await store.get(access.derivationId) : null;
        if (!record) return false;
        if (action === "read" || action === "metadata") return file.fileId === record.source.fileId;
        if (action === "upload") return journal.staging(file.filename)?.id === record.id;
        return false;
      }
      return !journal.staging(file.filename);
    }
    const dbPlugin = definePlugin({ id: "note-file-db", setup: () => fileDb });
    const local = createLocalStoragePlugin({ id: "local-objects", root });
    const privateFiles = definePlugin({
      id: "privateFiles",
      requires: [local],
      setup(context) {
        const objects = context.get(local);
        return {
          ...objects,
          id: "privateFiles",
          async get(...args: Parameters<typeof objects.get>) {
            if (controls.downloadError) throw new Error("download");
            return objects.get(...args);
          },
          async put(...args: Parameters<typeof objects.put>) {
            controls.puts++;
            if (controls.uploadError === "before") throw new Error("upload-before");
            await controls.beforePut?.();
            const result = await objects.put(...args);
            await controls.afterPut?.();
            if (controls.uploadError === "after") throw new Error("upload-after");
            return result;
          },
          async delete(...args: Parameters<typeof objects.delete>) {
            controls.deletes++;
            if (controls.deleteError) throw new Error("delete");
            return objects.delete(...args);
          },
        };
      },
    });
    const filesPlugin = createFilesPlugin({
      id: "note-files",
      database: dbPlugin,
      storages: [privateFiles],
      queries: () => journal.queries,
      authorize: journal.authorizer({
        authorize: policy,
        authorizeDelivery: (access, id) => media.authorizeDelivery(access, id),
      }),
    });
    app = await startApp({ plugins: [dbPlugin, local, privateFiles, filesPlugin] });
    const files = app.get(filesPlugin);
    const task = createMediaTask({ execute: (id, context) => media.execute(id, context) });
    const queueName = `notes-media-${crypto.randomUUID()}`;
    await provisionD1TaskQueue(database, queueName);
    queue = createTaskQueue({
      provider: await createD1TaskProvider({
        database,
        queueName,
        pollIntervalMs: 10,
        clock: () => controls.clock,
      }),
      tasks: [task],
    });
    const processor = createBunImageProcessor({ limits: { concurrency: 1 } });
    media = createMedia<Actor>({
      store,
      tasks: createTasksMediaAdapter(queue, task),
      storage: createFilesMediaStorage({ files, journal, storageId: privateFiles.id }),
      presets: [
        {
          name: "notes-attachment",
          version: "1",
          width: 32,
          height: 32,
          fit: "cover",
          formats: ["webp", "png", "jpeg"],
          defaultFormat: "webp",
          quality: 80,
          metadata: "strip",
          animation: "reject",
        },
      ],
      processor,
      processorVersion: processor.version,
      scope(access) {
        if (!actors.has(access) || access.kind === "cleanup") throw new MediaError("forbidden");
        return { subjectId: access.subjectId, tenantId: access.tenantId, isolationId: "notes" };
      },
      authorizeDerive: (access) =>
        actors.has(access) && !controls.revoked && !controls.deriveDenied,
      delegate: (record, purpose) =>
        actor(record.scope.subjectId, record.scope.tenantId, purpose, record.id),
    });
    const bytes = await fixture();
    const source = await files.upload(alice, {
      storageId: privateFiles.id,
      filename: "notes-attachment.jpg",
      contentType: "image/jpeg",
      ownerId: alice.subjectId,
      tenantId: alice.tenantId,
      size: bytes.length,
      body: new Blob([new Uint8Array(bytes)]).stream(),
      maxBytes: 1024 * 1024,
    });
    // Fixed Notes attachment fixture: the source already exists and is authorized.
    const note = {
      noteId: "note-1",
      ownerId: alice.subjectId,
      attachments: [fileReference(source)],
    };
    return {
      close,
      root,
      client,
      files,
      source,
      note,
      alice,
      actor,
      media,
      queue,
      queueName,
      task,
      journal,
      store,
      controls,
      objects: app.get(privateFiles),
      request: () =>
        media.request(alice, { source: note.attachments[0]!, preset: "notes-attachment" }),
      run: () => queue!.runBatch({ maxJobs: 4, concurrency: 1, timeoutMs: 20_000 }),
    };
  } catch (error) {
    await close();
    throw error;
  }
}
