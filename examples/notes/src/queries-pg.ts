import { asc, eq } from "drizzle-orm";
import type { BunSQLDatabase } from "drizzle-orm/bun-sql";
import type { NotesQueries } from "./notes";
import * as schema from "./schema-pg";

export function createPgNotesQueries(db: BunSQLDatabase<typeof schema>): NotesQueries {
  const { notes } = schema;
  return {
    async insert(note) {
      const [row] = await db.insert(notes).values(note).returning();
      if (!row) throw new Error("Note insert returned no row");
      return row;
    },
    async list() {
      return db.select().from(notes).orderBy(asc(notes.createdAt), asc(notes.id));
    },
    async update(id, input) {
      const [row] = await db.update(notes).set(input).where(eq(notes.id, id)).returning();
      return row ?? null;
    },
    async remove(id) {
      const rows = await db.delete(notes).where(eq(notes.id, id)).returning({ id: notes.id });
      return rows.length !== 0;
    },
  };
}
