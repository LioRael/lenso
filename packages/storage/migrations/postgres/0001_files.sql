CREATE TABLE lenso_files (
  file_id TEXT PRIMARY KEY NOT NULL,
  storage_id TEXT NOT NULL,
  object_key TEXT NOT NULL,
  filename TEXT NOT NULL,
  content_type TEXT NOT NULL,
  owner_id TEXT,
  tenant_id TEXT,
  state TEXT NOT NULL CONSTRAINT lenso_files_state_check CHECK (state IN ('pending','uploading','ready','failed','deleting','deleted')),
  revision INTEGER NOT NULL,
  size BIGINT,
  expected_size BIGINT,
  max_bytes BIGINT,
  etag TEXT,
  upload_expires_at BIGINT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  CONSTRAINT lenso_files_storage_object_unique UNIQUE (storage_id, object_key)
);
