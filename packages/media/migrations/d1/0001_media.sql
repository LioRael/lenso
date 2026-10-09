CREATE TABLE media (
  id TEXT PRIMARY KEY NOT NULL,
  revision INTEGER NOT NULL,
  record TEXT NOT NULL
);

CREATE TABLE media_artifacts (
  file_id TEXT PRIMARY KEY NOT NULL,
  derivation_id TEXT NOT NULL REFERENCES media(id),
  revision INTEGER NOT NULL,
  record TEXT NOT NULL
);

CREATE INDEX media_artifacts_derivation_id ON media_artifacts (derivation_id, file_id);
