CREATE TABLE IF NOT EXISTS __LENSO_SCHEMA__.lenso_task_relation (
  queue_name text NOT NULL,
  job_id uuid NOT NULL,
  task text NOT NULL,
  input jsonb NOT NULL,
  deduplication_key text,
  cancel_requested boolean NOT NULL DEFAULT false,
  PRIMARY KEY (queue_name, job_id),
  CONSTRAINT lenso_task_relation_queue_key UNIQUE (queue_name, deduplication_key)
);
