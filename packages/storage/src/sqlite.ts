import { and, eq } from "drizzle-orm";
import { integer, sqliteTable, text } from "drizzle-orm/sqlite-core";
import type { BunSQLiteDatabase } from "drizzle-orm/bun-sqlite";
import type { DrizzleD1Database } from "drizzle-orm/d1";
import type { FileQueries, FileState } from "./files";

export const files = sqliteTable("lenso_files", {
  fileId: text("file_id").primaryKey(),
  storageId: text("storage_id").notNull(),
  objectKey: text("object_key").notNull(),
  filename: text("filename").notNull(),
  contentType: text("content_type").notNull(),
  ownerId: text("owner_id"),
  tenantId: text("tenant_id"),
  state: text("state").$type<FileState>().notNull(),
  revision: integer("revision").notNull(),
  size: integer("size"),
  expectedSize: integer("expected_size"),
  maxBytes: integer("max_bytes"),
  etag: text("etag"),
  uploadExpiresAt: integer("upload_expires_at"),
  createdAt: integer("created_at").notNull(),
  updatedAt: integer("updated_at").notNull(),
});
export const fileSchema = { files };

export function createSqliteFileQueries<TSchema extends Record<string, unknown>>(
  db: BunSQLiteDatabase<TSchema> | DrizzleD1Database<TSchema>,
): FileQueries {
  return {
    async insert(file) { await db.insert(files).values(file); },
    async get(fileId) {
      const rows = await db.select().from(files).where(eq(files.fileId, fileId)).limit(1);
      return rows[0] ?? null;
    },
    async transition(fileId, revision, next) {
      const rows = await db.update(files).set(next)
        .where(and(eq(files.fileId, fileId), eq(files.revision, revision)))
        .returning();
      return rows.length === 1;
    },
  };
}
