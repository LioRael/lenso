import { bigint, index, integer, jsonb, pgTable, text, uniqueIndex } from "drizzle-orm/pg-core";

export const apiKeys = pgTable(
  "api_keys",
  {
    id: text("id").primaryKey(),
    namespace: text("namespace").notNull(),
    tenantId: text("tenant_id").notNull(),
    subjectId: text("subject_id").notNull(),
    requestId: text("request_id").notNull(),
    digest: text("digest").notNull(),
    previousDigest: text("previous_digest"),
    scopes: jsonb("scopes").$type<string[]>().notNull(),
    revision: integer("revision").notNull(),
    issuedAt: bigint("issued_at", { mode: "number" }).notNull(),
    expiresAt: bigint("expires_at", { mode: "number" }).notNull(),
    revokedAt: bigint("revoked_at", { mode: "number" }),
    overlapUntil: bigint("overlap_until", { mode: "number" }),
  },
  (table) => [
    uniqueIndex("api_keys_request_uq").on(table.namespace, table.tenantId, table.requestId),
    uniqueIndex("api_keys_digest_uq").on(table.digest),
    uniqueIndex("api_keys_previous_digest_uq").on(table.previousDigest),
    index("api_keys_subject_idx").on(table.namespace, table.tenantId, table.subjectId, table.id),
  ],
);
