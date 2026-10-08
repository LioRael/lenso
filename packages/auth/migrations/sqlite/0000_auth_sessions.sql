CREATE TABLE auth_sessions (
  id TEXT NOT NULL,
  realm_id TEXT NOT NULL,
  subject_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('user', 'guest', 'service')),
  token_digest TEXT NOT NULL UNIQUE,
  revision INTEGER NOT NULL CHECK (revision > 0),
  issued_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  idle_timeout_ms INTEGER NOT NULL,
  renew_after_ms INTEGER NOT NULL,
  last_active_at INTEGER NOT NULL,
  renewed_at INTEGER NOT NULL,
  authenticated_at INTEGER,
  assurance TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(assurance) AND json_type(assurance) = 'array'),
  revoked_at INTEGER,
  PRIMARY KEY (realm_id, id)
);
CREATE INDEX auth_sessions_realm_subject_idx ON auth_sessions (realm_id, subject_id);
