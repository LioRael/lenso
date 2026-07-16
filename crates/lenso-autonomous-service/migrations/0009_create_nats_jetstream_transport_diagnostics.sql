create schema if not exists platform;

create table if not exists platform.nats_jetstream_transport_diagnostics (
    diagnostic_id text primary key,
    delivery_id text not null,
    event_id text not null,
    outcome text not null,
    detail jsonb not null default '{}'::jsonb,
    recorded_at timestamptz not null default now()
);

create index if not exists nats_jetstream_transport_diagnostics_delivery_idx
    on platform.nats_jetstream_transport_diagnostics (delivery_id, recorded_at);
