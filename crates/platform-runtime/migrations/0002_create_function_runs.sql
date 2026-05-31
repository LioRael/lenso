create table if not exists runtime.function_runs (
    id text primary key,
    function_name text not null,
    input_json jsonb not null,
    status text not null default 'pending',
    attempts integer not null default 0,
    max_attempts integer not null default 3,
    available_at timestamptz not null default now(),
    locked_at timestamptz,
    locked_by text,
    started_at timestamptz,
    completed_at timestamptz,
    last_error text,
    correlation_id text not null,
    actor jsonb not null default '{"kind":"anonymous"}'::jsonb,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create index if not exists function_runs_pending_idx
    on runtime.function_runs (status, available_at, created_at)
    where status in ('pending', 'failed');
