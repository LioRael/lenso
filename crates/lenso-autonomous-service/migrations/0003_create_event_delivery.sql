create table if not exists platform.service_event_outbox (
    event_id text primary key,
    consumer_id text not null,
    envelope jsonb not null,
    status text not null default 'pending',
    attempts integer not null default 0,
    transport_message_id text,
    last_error text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    published_at timestamptz
);

create table if not exists platform.service_event_inbox (
    delivery_id text primary key,
    consumer_id text not null,
    event_id text not null,
    envelope jsonb not null,
    status text not null,
    last_error text,
    received_at timestamptz not null default now(),
    completed_at timestamptz
);

create table if not exists platform.service_event_delivery_evidence (
    evidence_id text primary key,
    stage text not null,
    outcome text not null,
    event_id text not null,
    delivery_id text,
    detail jsonb not null default '{}'::jsonb,
    recorded_at timestamptz not null default now()
);

create index if not exists service_event_delivery_evidence_event_idx
    on platform.service_event_delivery_evidence (event_id, recorded_at);
