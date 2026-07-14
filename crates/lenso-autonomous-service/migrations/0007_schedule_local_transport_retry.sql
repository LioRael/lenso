alter table platform.local_transport_deliveries
    add column if not exists available_at timestamptz not null default '-infinity',
    add column if not exists failure_reason text,
    add column if not exists reason_code text,
    add column if not exists terminal_outcome text;

drop index if exists platform.local_transport_available_idx;

create index if not exists local_transport_available_idx
    on platform.local_transport_deliveries (consumer_id, status, available_at, created_at);
