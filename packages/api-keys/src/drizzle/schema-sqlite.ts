import { index, integer, sqliteTable, text, uniqueIndex } from "drizzle-orm/sqlite-core";

export const apiKeys = sqliteTable(
  "api_keys",
  {
    id: text("id").primaryKey(),
    namespace: text("namespace").notNull(),
    tenantId: text("tenant_id").notNull(),
    subjectId: text("subject_id").notNull(),
    requestId: text("request_id").notNull(),
    digest: text("digest").notNull(),
    previousDigest: text("previous_digest"),
    scopes: text("scopes", { mode: "json" }).$type<string[]>().notNull(),
    revision: integer("revision").notNull(),
    issuedAt: integer("issued_at").notNull(),
    expiresAt: integer("expires_at").notNull(),
    revokedAt: integer("revoked_at"),
    overlapUntil: integer("overlap_until"),
  },
  (table) => [
    uniqueIndex("api_keys_request_uq").on(table.namespace, table.tenantId, table.requestId),
    uniqueIndex("api_keys_digest_uq").on(table.digest),
    uniqueIndex("api_keys_previous_digest_uq").on(table.previousDigest),
    index("api_keys_subject_idx").on(table.namespace, table.tenantId, table.subjectId, table.id),
  ],
);
