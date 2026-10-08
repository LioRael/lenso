import { and, eq } from "drizzle-orm";
import { bigint, integer, pgTable, text } from "drizzle-orm/pg-core";
import type { BunSQLDatabase } from "drizzle-orm/bun-sql";
import type { FileQueries, FileState } from "./files";

export const files = pgTable("lenso_files", {
  fileId: text("file_id").primaryKey(),
  storageId: text("storage_id").notNull(),
  objectKey: text("object_key").notNull(),
  filename: text("filename").notNull(),
  contentType: text("content_type").notNull(),
  ownerId: text("owner_id"),
  tenantId: text("tenant_id"),
  state: text("state").$type<FileState>().notNull(),
  revision: integer("revision").notNull(),
  size: bigint("size", { mode: "number" }),
  expectedSize: bigint("expected_size", { mode: "number" }),
  maxBytes: bigint("max_bytes", { mode: "number" }),
  etag: text("etag"),
  uploadExpiresAt: bigint("upload_expires_at", { mode: "number" }),
  createdAt: bigint("created_at", { mode: "number" }).notNull(),
  updatedAt: bigint("updated_at", { mode: "number" }).notNull(),
});
export const fileSchema = { files };

export function createPostgresFileQueries<TSchema extends Record<string, unknown>>(
  db: BunSQLDatabase<TSchema>,
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
        .returning({ fileId: files.fileId });
      return rows.length === 1;
    },
  };
}
