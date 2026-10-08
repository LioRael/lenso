import { expect, test } from "bun:test";
import { mkdtemp, realpath, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createFileDownloadHandler } from "@lenso/storage/fetch";
import { startApp } from "lenso";
import { createNotesFiles, migrateFiles, notesFileTenant } from "../src/files";
import { notesAudiences } from "../src/notes";

test("existing Notes example streams private attachments with persistent IDs and two instances", async () => {
  const root = await mkdtemp(join(await realpath(tmpdir()), "lenso-note-files-"));
  const filename = join(root, "notes.sqlite");
  const ownerId = "alice";
  const key = Array.from(crypto.getRandomValues(new Uint8Array(32)), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
  const definition = createNotesFiles({
    filename,
    root: join(root, "objects"),
    principals: [
      { subjectId: ownerId, key },
      { subjectId: "bob", key: "ab".repeat(32) },
    ],
  });
  try {
    await migrateFiles(filename); // Never performed by createNotesFiles/startApp.
    const app = await startApp({ plugins: definition.plugins });
    let fileId: string;
    try {
      const files = app.get(definition.files);
      const auth = app.get(definition.authentication);
      const credential = (await auth.issue(key)).credential;
      const otherCredential = (await auth.issue("ab".repeat(32))).credential;
      const actor = await auth.for(notesAudiences.create).required(credential);
      let chunks = 0;
      const record = await files.upload(actor, {
        storageId: definition.privateFiles.id,
        filename: "../../original-name.bin",
        contentType: "application/octet-stream",
        ownerId,
        tenantId: notesFileTenant,
        maxBytes: 2 * 1024 * 1024,
        body: new ReadableStream(
          {
            pull(controller) {
              if (chunks++ === 32) controller.close();
              else controller.enqueue(new Uint8Array(64 * 1024).fill(17));
            },
          },
          { highWaterMark: 0 },
        ),
      });
      fileId = record.fileId;
      expect(record.state).toBe("ready");
      expect(record.size).toBe(2 * 1024 * 1024);
      expect(record.objectKey).not.toContain(record.filename);
      expect(await app.get(definition.publicAssets).list()).toEqual({ objects: [] });

      const raw = createFileDownloadHandler({
        files,
        authenticate: (request) =>
          auth.for(notesAudiences.read).required(request.headers.get("x-test-session")),
        fileId: () => fileId,
      });
      await expect(
        raw({
          request: new Request(`http://example.test/files/${fileId}`, {
            headers: { "x-test-session": otherCredential },
          }),
        }),
      ).rejects.toMatchObject({ code: "FORBIDDEN" });
      const response = await raw({
        request: new Request(`http://example.test/files/${fileId}`, {
          headers: { "x-test-session": credential },
        }),
      });
      expect(response?.status).toBe(200);
      expect(response?.headers.get("cache-control")).toBe("private, no-store");
      expect(response?.headers.get("content-disposition")).toBe("attachment");
      expect(response?.headers.get("x-content-type-options")).toBe("nosniff");
      const reader = response!.body!.getReader();
      let bytes = 0;
      for (;;) {
        const chunk = await reader.read();
        if (chunk.done) break;
        bytes += chunk.value.byteLength;
        expect(chunk.value.every((value) => value === 17)).toBe(true);
      }
      expect(bytes).toBe(record.size!);
    } finally {
      await app.stop();
    }
    const restarted = await startApp({ plugins: definition.plugins });
    try {
      const files = restarted.get(definition.files);
      const auth = restarted.get(definition.authentication);
      const credential = (await auth.issue(key)).credential;
      expect(
        (
          await files.metadata(
            await auth.for(notesAudiences.fileMetadata).required(credential),
            fileId!,
          )
        ).state,
      ).toBe("ready");
      expect(
        (
          await files.delete(
            await auth.for(notesAudiences.fileDelete).required(credential),
            fileId!,
          )
        ).state,
      ).toBe("deleted");
      expect(
        (
          await files.delete(
            await auth.for(notesAudiences.fileDelete).required(credential),
            fileId!,
          )
        ).state,
      ).toBe("deleted");
      await expect(
        files.read(await auth.for(notesAudiences.read).required(credential), fileId!),
      ).rejects.toMatchObject({ code: "conflict" });
      expect(await restarted.get(definition.privateFiles).list()).toEqual({ objects: [] });
    } finally {
      await restarted.stop();
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
