ALTER TABLE "notes" ADD COLUMN "owner_id" text;
--> statement-breakpoint
UPDATE "notes" SET "owner_id" = '__legacy_unowned__';
--> statement-breakpoint
ALTER TABLE "notes" ALTER COLUMN "owner_id" SET NOT NULL;
--> statement-breakpoint
-- Auth-owned SQL copied from packages/auth/migrations/pg/0000_auth_sessions.sql.
CREATE TABLE auth_sessions (
  id text NOT NULL,
  realm_id text NOT NULL,
  subject_id text NOT NULL,
  kind text NOT NULL CHECK (kind IN ('user', 'guest', 'service')),
  token_digest text NOT NULL,
  revision integer NOT NULL CHECK (revision > 0),
  issued_at bigint NOT NULL,
  expires_at bigint NOT NULL,
  idle_timeout_ms bigint NOT NULL,
  renew_after_ms bigint NOT NULL,
  last_active_at bigint NOT NULL,
  renewed_at bigint NOT NULL,
  authenticated_at bigint,
  assurance jsonb NOT NULL DEFAULT '[]'::jsonb CHECK (jsonb_typeof(assurance) = 'array'),
  revoked_at bigint,
  PRIMARY KEY (realm_id, id),
  UNIQUE (token_digest)
);
--> statement-breakpoint
CREATE INDEX auth_sessions_realm_subject_idx ON auth_sessions (realm_id, subject_id);
