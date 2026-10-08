CREATE TABLE IF NOT EXISTS __LENSO_SCHEMA__.lenso_task_queue_identity (
  queue_name text PRIMARY KEY,
  queue_id uuid NOT NULL UNIQUE
);
