ALTER TABLE __LENSO_SCHEMA__.lenso_task_relation
ADD COLUMN IF NOT EXISTS trace_metadata jsonb;
