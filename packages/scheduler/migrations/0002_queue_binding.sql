CREATE TABLE public.lenso_schedule_queue_binding (
  namespace text NOT NULL,
  tenant_id text NOT NULL,
  queue_kind text NOT NULL CHECK (queue_kind IN ('postgres', 'd1')),
  queue_id text NOT NULL,
  CONSTRAINT lenso_schedule_queue_binding_pk PRIMARY KEY (namespace, tenant_id)
);
