create table if not exists platform.execution_logs (
    id text primary key,
    correlation_id text not null,
    story_id text not null,
    execution_id text not null,
    execution_type text not null,
    execution_name text not null,
    occurred_at timestamptz not null default now(),
    severity text not null,
    body text not null,
    attributes jsonb not null default '{}'::jsonb,
    trace_id text,
    span_id text,
    service_name text not null default 'lenso',
    redacted_fields text[] not null default '{}'::text[],
    created_at timestamptz not null default now()
);

create index if not exists execution_logs_execution_idx
    on platform.execution_logs (execution_id, occurred_at, id);

create index if not exists execution_logs_correlation_idx
    on platform.execution_logs (correlation_id, occurred_at, id);
