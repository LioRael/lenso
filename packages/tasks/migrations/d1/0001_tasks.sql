CREATE TABLE lenso_d1_task_queue (
  queue_name TEXT PRIMARY KEY NOT NULL,
  queue_id TEXT NOT NULL UNIQUE
);

CREATE TABLE lenso_d1_task_job (
  queue_name TEXT NOT NULL REFERENCES lenso_d1_task_queue(queue_name),
  id TEXT NOT NULL,
  task TEXT NOT NULL,
  input TEXT NOT NULL CHECK (json_valid(input)),
  trace_metadata TEXT CHECK (trace_metadata IS NULL OR json_valid(trace_metadata)),
  dedup_key TEXT,
  state TEXT NOT NULL DEFAULT 'pending'
    CHECK (state IN ('pending', 'running', 'succeeded', 'failed', 'cancelled')),
  attempt INTEGER NOT NULL DEFAULT 0 CHECK (attempt >= 0),
  max_attempts INTEGER NOT NULL CHECK (max_attempts >= 1),
  cancel_requested INTEGER NOT NULL DEFAULT 0 CHECK (cancel_requested IN (0, 1)),
  run_at INTEGER NOT NULL,
  lease_until INTEGER,
  expires_at INTEGER,
  retry_delay_seconds INTEGER NOT NULL DEFAULT 0 CHECK (retry_delay_seconds >= 0),
  retry_backoff INTEGER NOT NULL DEFAULT 0 CHECK (retry_backoff IN (0, 1)),
  retry_max_delay_seconds INTEGER CHECK (retry_max_delay_seconds >= 0),
  result TEXT CHECK (result IS NULL OR json_valid(result)),
  error TEXT CHECK (error IS NULL OR error IN ('handler-failed', 'invalid-input', 'invalid-result', 'aborted')),
  PRIMARY KEY (queue_name, id),
  UNIQUE (queue_name, dedup_key)
);

CREATE INDEX lenso_d1_task_job_due ON lenso_d1_task_job(queue_name, state, run_at);
CREATE INDEX lenso_d1_task_job_lease ON lenso_d1_task_job(queue_name, state, lease_until);
