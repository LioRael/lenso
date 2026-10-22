import assert from "node:assert/strict";
import { mkdtemp, realpath, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Database } from "bun:sqlite";
import { drizzle } from "drizzle-orm/bun-sqlite";
import { asc, eq, sql } from "drizzle-orm";
import { definePlugin, startApp } from "@lenso/core";
import { createFilesPlugin, type FileAction, type FileRecord } from "@lenso/storage/files";
import { createFileDownloadHandler, createFileUploadHandler } from "@lenso/storage/fetch";
import { createLocalStoragePlugin } from "@lenso/storage/local";
import { createSqliteFileQueries, fileSchema } from "@lenso/storage/sqlite";
import { StorageError, type ObjectStorage } from "@lenso/storage";

/** Local-only application exercise. No listener, credentials or external services. */
export async function runFileRecovery() {
  const root = await mkdtemp(join(await realpath(tmpdir()), "lenso-file-recovery-"));
  const database = definePlugin({
    id: "files-db",
    setup(context) {
      const client = new Database(join(root, "files.sqlite"));
      context.onCleanup(() => client.close());
      return drizzle(client, { schema: fileSchema });
    },
  });
  const local = createLocalStoragePlugin({ id: "local", root: join(root, "objects") });
  let failNextRead = false;
  let providerReads = 0;
  const storage = definePlugin<ObjectStorage>({
    id: "private-files",
    requires: [local],
    setup(context) {
      const objects = context.get(local);
      return {
        ...objects,
        id: "private-files",
        async get(key, options) {
          providerReads++;
          if (failNextRead) {
            failNextRead = false;
            throw new StorageError("provider", "Injected transient read failure");
          }
          return objects.get(key, options);
        },
      };
    },
  });
  let revoked = false;
  const filePlugin = createFilesPlugin({
    id: "files",
    database,
    storages: [storage],
    queries: createSqliteFileQueries,
    authorize: ({
      access,
      action,
      file,
    }: {
      access: string;
      action: FileAction;
      file: Readonly<FileRecord>;
    }) => action === "metadata" || (access === file.ownerId && !revoked),
  });
  const app = await startApp({ plugins: [database, local, storage, filePlugin] });
  const checks: Record<string, unknown> = {};
  try {
    const db = app.get(database);
    // Setup is read-only with respect to schema. Migration failure is visible.
    assert.throws(() => db.select().from(fileSchema.files).all(), /no such table/);
    const migration = await Bun.file(
      new URL("../migrations/sqlite/0001_files.sql", import.meta.url),
    ).text();
    db.run(sql.raw(migration));
    checks.migration = "missing table before explicit local migration; typed query succeeds after";

    const files = app.get(filePlugin);
    const authenticate = (request: Request) => request.headers.get("authorization") ?? "anonymous";
    const upload = createFileUploadHandler({
      files,
      authenticate,
      input: (_request, access) => ({
        storageId: storage.id,
        filename: "attachment.txt",
        contentType: "text/plain",
        ownerId: access,
        maxBytes: 64,
      }),
    });
    const uploaded = await upload({
      request: new Request("https://local.invalid/upload", {
        method: "PUT",
        headers: { authorization: "alice" },
        body: "Private attachment",
      }),
    });
    assert.equal(uploaded?.status, 201);
    const file = (await uploaded!.json()) as FileRecord;
    const typedFile = await db.query.files.findFirst({
      where: eq(fileSchema.files.fileId, file.fileId),
    });
    assert.equal(typedFile?.state, "ready");
    checks.upload = { status: uploaded!.status, state: typedFile!.state, bytes: typedFile!.size };

    // Drizzle keeps its native sync transaction contract: failures roll back.
    assert.throws(
      () =>
        db.transaction((tx) => {
          tx.update(fileSchema.files)
            .set({ filename: "must-roll-back.txt" })
            .where(eq(fileSchema.files.fileId, file.fileId))
            .run();
          throw new Error("application transaction failed");
        }),
      /application transaction failed/,
    );
    assert.equal((await files.metadata("alice", file.fileId)).filename, "attachment.txt");
    checks.rollback = "original filename preserved after transaction throws";

    const download = createFileDownloadHandler({ files, authenticate, fileId: () => file.fileId });
    const request = async (method: string, actor = "alice") => {
      const response = await download({
        request: new Request("https://local.invalid/files/attachment", {
          method,
          headers: { authorization: actor },
        }),
      });
      assert(response);
      return response;
    };
    assert.equal((await files.metadata("catalog-reader", file.fileId)).state, "ready");
    const readsBeforeDenial = providerReads;
    for (const method of ["GET", "HEAD"])
      assert.equal((await request(method, "catalog-reader")).status, 403);
    assert.equal(providerReads, readsBeforeDenial);
    checks.permission = "metadata visible; GET and HEAD 403 before provider access";

    const head = await request("HEAD");
    assert.equal(head.status, 200);
    assert.equal(head.body, null);
    assert.equal(head.headers.get("content-length"), String(file.size));
    failNextRead = true;
    const failed = await request("GET");
    assert.equal(failed.status, 502);
    assert.deepEqual(await failed.json(), { error: "provider" });
    assert.equal((await files.metadata("alice", file.fileId)).state, "ready");
    const retry = await request("GET");
    assert.equal(retry.status, 200);
    assert.equal(await retry.text(), "Private attachment");
    checks.download = {
      head: head.status,
      transientFailure: failed.status,
      retry: retry.status,
      state: "ready",
    };

    revoked = true;
    for (const method of ["HEAD", "GET"]) assert.equal((await request(method)).status, 403);
    revoked = false;
    checks.revocation = "each download request rechecks current application permission";

    const second = await files.upload("alice", {
      storageId: storage.id,
      ownerId: "alice",
      filename: "z-last.txt",
      contentType: "text/plain",
      maxBytes: 64,
      body: new Response("Second attachment").body!,
    });

    const oversized = await upload({
      request: new Request("https://local.invalid/upload", {
        method: "PUT",
        headers: { authorization: "alice" },
        body: "x".repeat(65),
      }),
    });
    assert.equal(oversized!.status, 413);
    const failedFiles = await db
      .select()
      .from(fileSchema.files)
      .where(eq(fileSchema.files.state, "failed"))
      .orderBy(asc(fileSchema.files.fileId))
      .limit(1)
      .offset(0);
    assert.equal(failedFiles.length, 1);
    assert.equal(await app.get(local).head(failedFiles[0]!.objectKey), null);
    const page = (offset: number) =>
      db
        .select()
        .from(fileSchema.files)
        .where(eq(fileSchema.files.state, "ready"))
        .orderBy(asc(fileSchema.files.filename), asc(fileSchema.files.fileId))
        .limit(1)
        .offset(offset);
    const firstPage = await page(0);
    const nextPage = await page(1);
    assert.equal(firstPage[0]?.fileId, file.fileId);
    assert.equal(nextPage[0]?.fileId, second.fileId);
    assert.equal((await page(2)).length, 0);
    checks.failedUpload = { status: oversized!.status, state: "failed", object: "absent" };
    checks.pagination = "ready-only, filename/id ascending; two one-row pages, then empty";

    assert.equal((await request("DELETE")).status, 204);
    assert.equal((await request("DELETE")).status, 204);
    assert.equal((await files.metadata("alice", file.fileId)).state, "deleted");
    assert.equal((await request("GET")).status, 409);
    assert.equal(await app.get(local).head(file.objectKey), null);
    assert.equal((await files.delete("alice", second.fileId)).state, "deleted");
    checks.deletion = "204 on delete and retry; tombstone retained; payload absent";
    return checks;
  } finally {
    try {
      await app.stop();
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  }
}

if (import.meta.main) console.log(JSON.stringify(await runFileRecovery(), null, 2));
