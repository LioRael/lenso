import { asc, eq } from "drizzle-orm";
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
    async list() {
      return db.select().from(notes).orderBy(asc(notes.createdAt), asc(notes.id)).all();
    },
    async update(id, input) {
      const [row] = await db.update(notes).set(input).where(eq(notes.id, id)).returning().all();
      return row ?? null;
    },
    async remove(id) {
      const rows = await db.delete(notes).where(eq(notes.id, id)).returning().all();
      return rows.length !== 0;
    },
  };
}
