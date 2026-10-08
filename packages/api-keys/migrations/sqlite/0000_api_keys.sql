CREATE TABLE api_keys (
  id TEXT PRIMARY KEY NOT NULL,
  namespace TEXT NOT NULL,
  tenant_id TEXT NOT NULL,
  subject_id TEXT NOT NULL,
  request_id TEXT NOT NULL,
  digest TEXT NOT NULL,
  previous_digest TEXT,
  scopes TEXT NOT NULL,
  revision INTEGER NOT NULL,
  issued_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  revoked_at INTEGER,
  overlap_until INTEGER
);
CREATE UNIQUE INDEX api_keys_request_uq ON api_keys(namespace, tenant_id, request_id);
CREATE UNIQUE INDEX api_keys_digest_uq ON api_keys(digest);
CREATE UNIQUE INDEX api_keys_previous_digest_uq ON api_keys(previous_digest);
CREATE INDEX api_keys_subject_idx ON api_keys(namespace, tenant_id, subject_id, id);
