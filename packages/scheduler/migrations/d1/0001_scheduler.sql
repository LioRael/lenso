CREATE TABLE lenso_schedule (
  namespace TEXT NOT NULL,
  tenant_id TEXT NOT NULL,
  id TEXT NOT NULL,
  revision INTEGER NOT NULL CHECK (revision >= 0),
  state TEXT NOT NULL CHECK (state IN ('active', 'paused', 'cancelled', 'completed')),
  task TEXT NOT NULL,
  input TEXT NOT NULL,
  rule TEXT NOT NULL,
  misfire TEXT NOT NULL CHECK (misfire IN ('skip', 'coalesce')),
  grace_ms INTEGER NOT NULL CHECK (grace_ms >= 0),
  next_at INTEGER,
  initiator TEXT NOT NULL,
  write_token TEXT,
  CONSTRAINT lenso_schedule_pk PRIMARY KEY (namespace, tenant_id, id)
);

CREATE INDEX lenso_schedule_due_idx
  ON lenso_schedule (namespace, tenant_id, next_at, id)
  WHERE state = 'active';

CREATE TABLE lenso_schedule_occurrence (
  namespace TEXT NOT NULL,
  tenant_id TEXT NOT NULL,
  id TEXT NOT NULL,
  schedule_id TEXT NOT NULL,
  revision INTEGER NOT NULL CHECK (revision >= 0),
  scheduled_at INTEGER NOT NULL,
  source TEXT NOT NULL CHECK (source IN ('timer', 'manual')),
  task TEXT NOT NULL,
  input TEXT NOT NULL,
  initiator TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('pending', 'enqueued', 'blocked')),
  job_id TEXT,
  error TEXT CHECK (error IN ('dispatch-failed', 'execution-denied', 'job-expired', 'dispatch-invalid')),
  lease_token TEXT,
  lease_until INTEGER,
  CONSTRAINT lenso_schedule_occurrence_pk PRIMARY KEY (namespace, tenant_id, id),
  CONSTRAINT lenso_schedule_occurrence_schedule_fk
    FOREIGN KEY (namespace, tenant_id, schedule_id)
    REFERENCES lenso_schedule (namespace, tenant_id, id),
  CONSTRAINT lenso_schedule_occurrence_lifecycle_check CHECK (
    (state = 'pending' AND job_id IS NULL AND (error IS NULL OR error = 'dispatch-failed')) OR
    (state = 'enqueued' AND job_id IS NOT NULL AND error IS NULL AND lease_token IS NULL AND lease_until IS NULL) OR
    (state = 'blocked' AND job_id IS NULL AND error IS NOT NULL AND error IN ('execution-denied', 'job-expired', 'dispatch-invalid') AND lease_token IS NULL AND lease_until IS NULL)
  )
);

CREATE INDEX lenso_schedule_occurrence_claim_idx
  ON lenso_schedule_occurrence (namespace, tenant_id, scheduled_at, id)
  WHERE state = 'pending';

CREATE INDEX lenso_schedule_occurrence_schedule_idx
  ON lenso_schedule_occurrence (namespace, tenant_id, schedule_id, scheduled_at, id);

CREATE TABLE lenso_schedule_queue_binding (
  namespace TEXT NOT NULL,
  tenant_id TEXT NOT NULL,
  queue_kind TEXT NOT NULL CHECK (queue_kind IN ('postgres', 'd1')),
  queue_id TEXT NOT NULL,
  CONSTRAINT lenso_schedule_queue_binding_pk PRIMARY KEY (namespace, tenant_id)
);
