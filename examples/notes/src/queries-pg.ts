import { and, asc, eq } from "drizzle-orm";
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
    async list(ownerId) {
      return db
        .select()
        .from(notes)
        .where(eq(notes.ownerId, ownerId))
        .orderBy(asc(notes.createdAt), asc(notes.id));
    },
    async read(id) {
      const [row] = await db.select().from(notes).where(eq(notes.id, id)).limit(1);
      return row ?? null;
    },
    async update(id, ownerId, input) {
      const [row] = await db
        .update(notes)
        .set(input)
        .where(and(eq(notes.id, id), eq(notes.ownerId, ownerId)))
        .returning();
      return row ?? null;
    },
    async remove(id, ownerId) {
      const rows = await db
        .delete(notes)
        .where(and(eq(notes.id, id), eq(notes.ownerId, ownerId)))
        .returning({ id: notes.id });
      return rows.length !== 0;
    },
  };
}
