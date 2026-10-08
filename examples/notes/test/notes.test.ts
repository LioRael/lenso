import { expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createBunSqlitePlugin } from "@lenso/db/bun-sqlite";
import { startApp } from "lenso";
import { runNotesCommand } from "../src/cli";
import { migrateSqlite } from "../src/migrate-sqlite";
import { createNotesPlugin, NoteInputError } from "../src/notes";
import { createSqliteNotesQueries } from "../src/queries-sqlite";
import * as schema from "../src/schema-sqlite";

test("SQLite notes persist across lifecycle restarts with explicit multi-instance binding", async () => {
  const directory = await mkdtemp(join(tmpdir(), "lenso-notes-"));
  const filename = join(directory, "notes.sqlite");
  const archive = join(directory, "archive.sqlite");
  const database = createBunSqlitePlugin({ id: "notes-db", filename, schema });
  const archiveDatabase = createBunSqlitePlugin({ id: "archive-db", filename: archive, schema });
  const notes = createNotesPlugin({ id: "notes", database, queries: createSqliteNotesQueries });
  const archiveNotes = createNotesPlugin({
    id: "archive",
    database: archiveDatabase,
    queries: createSqliteNotesQueries,
  });
  const definition = { plugins: [notes, database, archiveNotes, archiveDatabase] };
  try {
    migrateSqlite(filename);
    migrateSqlite(filename); // rerunning an explicit migration is safe
    migrateSqlite(archive);
    const app = await startApp(definition);
    let id = "";
    try {
      const service = app.get(notes);
      await expect(service.create({ title: " " })).rejects.toBeInstanceOf(NoteInputError);
      await runNotesCommand(service, ["create", "  Persistent '); DROP TABLE notes; --  ", "body"]);
      const [row] = await service.list();
      id = row.id;
      expect(row.title).toBe("Persistent '); DROP TABLE notes; --");
      expect(await app.get(archiveNotes).list()).toEqual([]);
      await app.get(archiveNotes).create({ title: "Separate archive" });
    } finally {
      await app.stop();
    }
    const restarted = await startApp(definition);
    try {
      const service = restarted.get(notes);
      expect((await service.list())[0].id).toBe(id);
      expect((await restarted.get(archiveNotes).list())[0].title).toBe("Separate archive");
      const updated = await service.update(id, { title: "Updated", body: "new body" });
      expect(updated?.body).toBe("new body");
      expect(await service.remove(id)).toBe(true);
      expect(await service.remove(id)).toBe(false);
      expect(await service.update(id, { title: "Missing" })).toBeNull();
      expect(await service.list()).toEqual([]);
    } finally {
      await restarted.stop();
    }
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
