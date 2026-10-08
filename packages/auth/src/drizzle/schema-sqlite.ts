import {
  index,
  integer,
  primaryKey,
  sqliteTable,
  text,
  uniqueIndex,
} from "drizzle-orm/sqlite-core";

export const authSessions = sqliteTable(
  "auth_sessions",
  {
    id: text("id").notNull(),
    realmId: text("realm_id").notNull(),
    subjectId: text("subject_id").notNull(),
    kind: text("kind").notNull(),
    tokenDigest: text("token_digest").notNull(),
    revision: integer("revision").notNull(),
    issuedAt: integer("issued_at").notNull(),
    expiresAt: integer("expires_at").notNull(),
    idleTimeoutMs: integer("idle_timeout_ms").notNull(),
    renewAfterMs: integer("renew_after_ms").notNull(),
    lastActiveAt: integer("last_active_at").notNull(),
    renewedAt: integer("renewed_at").notNull(),
    authenticatedAt: integer("authenticated_at"),
    assurance: text("assurance", { mode: "json" }).$type<string[]>().notNull().default([]),
    revokedAt: integer("revoked_at"),
  },
  (table) => [
    primaryKey({ columns: [table.realmId, table.id] }),
    uniqueIndex("auth_sessions_token_digest_uq").on(table.tokenDigest),
    index("auth_sessions_realm_subject_idx").on(table.realmId, table.subjectId),
  ],
);
