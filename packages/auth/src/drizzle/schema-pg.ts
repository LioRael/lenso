import {
  bigint,
  index,
  integer,
  jsonb,
  pgTable,
  primaryKey,
  text,
  uniqueIndex,
} from "drizzle-orm/pg-core";

export const authSessions = pgTable(
  "auth_sessions",
  {
    id: text("id").notNull(),
    realmId: text("realm_id").notNull(),
    subjectId: text("subject_id").notNull(),
    kind: text("kind").notNull(),
    tokenDigest: text("token_digest").notNull(),
    revision: integer("revision").notNull(),
    issuedAt: bigint("issued_at", { mode: "number" }).notNull(),
    expiresAt: bigint("expires_at", { mode: "number" }).notNull(),
    idleTimeoutMs: bigint("idle_timeout_ms", { mode: "number" }).notNull(),
    renewAfterMs: bigint("renew_after_ms", { mode: "number" }).notNull(),
    lastActiveAt: bigint("last_active_at", { mode: "number" }).notNull(),
    renewedAt: bigint("renewed_at", { mode: "number" }).notNull(),
    authenticatedAt: bigint("authenticated_at", { mode: "number" }),
    assurance: jsonb("assurance").$type<string[]>().notNull().default([]),
    revokedAt: bigint("revoked_at", { mode: "number" }),
  },
  (table) => [
    primaryKey({ columns: [table.realmId, table.id] }),
    uniqueIndex("auth_sessions_token_digest_uq").on(table.tokenDigest),
    index("auth_sessions_realm_subject_idx").on(table.realmId, table.subjectId),
  ],
);
