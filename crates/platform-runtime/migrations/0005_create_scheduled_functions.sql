create table if not exists runtime.scheduled_functions (
    schedule_key text primary key,
    module_name text not null,
    schedule_name text not null,
    function_name text not null,
    cron_expression text not null,
    input_json jsonb not null default '{}'::jsonb,
    max_attempts integer not null default 3,
    next_run_at timestamptz not null default now(),
    last_enqueued_at timestamptz,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create index if not exists scheduled_functions_due_idx
    on runtime.scheduled_functions (next_run_at, schedule_key);
