create table if not exists platform.outbox (
    id text primary key,
    event_name text not null,
    event_version integer not null,
    source_module text not null,
    aggregate_type text not null,
    aggregate_id text not null,
    correlation_id text not null,
    causation_id text,
    occurred_at timestamptz not null,
    payload jsonb not null,
    headers jsonb not null default '{}'::jsonb,
    status text not null default 'pending',
    attempts integer not null default 0,
    max_attempts integer not null default 3,
    available_at timestamptz not null default now(),
    locked_at timestamptz,
    locked_by text,
    published_at timestamptz,
    last_error text,
    created_at timestamptz not null default now()
);

create index if not exists outbox_pending_idx
    on platform.outbox (status, available_at, created_at)
    where status = 'pending';
