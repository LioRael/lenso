CREATE TABLE public.lenso_schedule (
  namespace text NOT NULL,
  tenant_id text NOT NULL,
  id text NOT NULL,
  revision integer NOT NULL,
  state text NOT NULL CHECK (state IN ('active', 'paused', 'cancelled', 'completed')),
  task text NOT NULL,
  input jsonb NOT NULL,
  rule jsonb NOT NULL,
  misfire text NOT NULL CHECK (misfire IN ('skip', 'coalesce')),
  grace_ms bigint NOT NULL,
  next_at bigint,
  initiator jsonb NOT NULL,
  CONSTRAINT lenso_schedule_pk PRIMARY KEY (namespace, tenant_id, id),
  CONSTRAINT lenso_schedule_revision_check CHECK (revision >= 0),
  CONSTRAINT lenso_schedule_grace_check CHECK (grace_ms >= 0)
);

CREATE INDEX lenso_schedule_due_idx
  ON public.lenso_schedule (namespace, tenant_id, next_at, id)
  WHERE state = 'active';

CREATE TABLE public.lenso_schedule_occurrence (
  namespace text NOT NULL,
  tenant_id text NOT NULL,
  id text NOT NULL,
  schedule_id text NOT NULL,
  revision integer NOT NULL,
  scheduled_at bigint NOT NULL,
  source text NOT NULL CHECK (source IN ('timer', 'manual')),
  task text NOT NULL,
  input jsonb NOT NULL,
  initiator jsonb NOT NULL,
  state text NOT NULL CHECK (state IN ('pending', 'enqueued', 'blocked')),
  job_id text,
  error text CHECK (error IN ('dispatch-failed', 'execution-denied', 'job-expired', 'dispatch-invalid')),
  lease_token text,
  lease_until bigint,
  CONSTRAINT lenso_schedule_occurrence_pk PRIMARY KEY (namespace, tenant_id, id),
  CONSTRAINT lenso_schedule_occurrence_revision_check CHECK (revision >= 0),
  CONSTRAINT lenso_schedule_occurrence_schedule_fk
    FOREIGN KEY (namespace, tenant_id, schedule_id)
    REFERENCES public.lenso_schedule (namespace, tenant_id, id),
  CONSTRAINT lenso_schedule_occurrence_lifecycle_check CHECK (
    (state = 'pending' AND job_id IS NULL AND (error IS NULL OR error = 'dispatch-failed')) OR
    (state = 'enqueued' AND job_id IS NOT NULL AND error IS NULL AND lease_token IS NULL AND lease_until IS NULL) OR
    (state = 'blocked' AND job_id IS NULL AND error IS NOT NULL AND error IN ('execution-denied', 'job-expired', 'dispatch-invalid') AND lease_token IS NULL AND lease_until IS NULL)
  )
);

CREATE INDEX lenso_schedule_occurrence_claim_idx
  ON public.lenso_schedule_occurrence (namespace, tenant_id, scheduled_at, id)
  WHERE state = 'pending';

CREATE INDEX lenso_schedule_occurrence_schedule_idx
  ON public.lenso_schedule_occurrence (namespace, tenant_id, schedule_id, scheduled_at, id);
