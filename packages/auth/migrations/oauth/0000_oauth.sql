-- Portable PostgreSQL/SQLite/D1 schema; apply explicitly, never at module import.
CREATE TABLE auth_oauth_records (
 namespace TEXT NOT NULL,
 kind TEXT NOT NULL,
 id TEXT NOT NULL,
 subject_id TEXT NOT NULL,
 expires_at BIGINT NOT NULL,
 revision BIGINT NOT NULL,
 document TEXT NOT NULL,
 PRIMARY KEY(namespace,kind,id)
);
CREATE INDEX auth_oauth_subject ON auth_oauth_records(namespace,kind,subject_id);
