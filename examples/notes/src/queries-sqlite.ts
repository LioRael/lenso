import { and, asc, eq } from "drizzle-orm";
import type { BunSQLiteDatabase } from "drizzle-orm/bun-sqlite";
import type { DrizzleD1Database } from "drizzle-orm/d1";
import type { NotesQueries } from "./notes";
import * as schema from "./schema-sqlite";

/** These single-statement SQLite queries also use the real asynchronous D1 driver. */
export function createSqliteNotesQueries(
  db: BunSQLiteDatabase<typeof schema> | DrizzleD1Database<typeof schema>,
): NotesQueries {
  const { notes } = schema;
  return {
    async insert(note) {
      const [row] = await db.insert(notes).values(note).returning().all();
      if (!row) throw new Error("Note insert returned no row");
      return row;
    },
    async list(ownerId) {
      return db
        .select()
        .from(notes)
        .where(eq(notes.ownerId, ownerId))
        .orderBy(asc(notes.createdAt), asc(notes.id))
        .all();
    },
    async read(id) {
      const [row] = await db.select().from(notes).where(eq(notes.id, id)).limit(1).all();
      return row ?? null;
    },
    async update(id, ownerId, input) {
      const [row] = await db
        .update(notes)
        .set(input)
        .where(and(eq(notes.id, id), eq(notes.ownerId, ownerId)))
        .returning()
        .all();
      return row ?? null;
    },
    async remove(id, ownerId) {
      const rows = await db
        .delete(notes)
        .where(and(eq(notes.id, id), eq(notes.ownerId, ownerId)))
        .returning()
        .all();
      return rows.length !== 0;
    },
  };
}
