create schema if not exists platform;

create table if not exists platform.local_transport_deliveries (
    delivery_id text primary key,
    consumer_id text not null,
    event_id text not null,
    envelope jsonb not null,
    status text not null,
    attempts integer not null default 0,
    last_error text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create index if not exists local_transport_available_idx
    on platform.local_transport_deliveries (consumer_id, status, created_at);

create table if not exists platform.local_transport_diagnostics (
    diagnostic_id text primary key,
    delivery_id text not null,
    event_id text not null,
    outcome text not null,
    detail jsonb not null default '{}'::jsonb,
    recorded_at timestamptz not null default now()
);
