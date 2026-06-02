create table if not exists platform.story_events (
    id text primary key,
    source_type text not null,
    source_id text not null,
    node_type text not null,
    name text not null,
    status text not null,
    service text not null,
    correlation_id text not null,
    causation_id text,
    started_at timestamptz not null,
    completed_at timestamptz,
    duration_ms bigint not null default 0,
    error text,
    metadata jsonb not null default '{}'::jsonb,
    trace_id text,
    span_id text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (source_type, source_id)
);

create index if not exists story_events_correlation_idx
    on platform.story_events (correlation_id, updated_at, id);

create index if not exists story_events_source_idx
    on platform.story_events (source_type, source_id);
