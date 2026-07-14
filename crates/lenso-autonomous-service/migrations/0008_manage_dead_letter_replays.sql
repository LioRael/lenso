alter table platform.service_event_dead_letters
    add column if not exists status text not null default 'dead_lettered',
    add column if not exists retained_until timestamptz,
    add column if not exists resolved_at timestamptz;

alter table platform.service_event_dead_letters
    add constraint service_event_dead_letters_status_check
        check (status in ('dead_lettered', 'replay_active', 'resolved')),
    add constraint service_event_dead_letters_retention_check
        check (retained_until is null or retained_until >= dead_lettered_at);

create table if not exists platform.service_event_replays (
    replay_id text primary key,
    dead_letter_id text not null,
    consumer_id text not null,
    event_id text not null,
    original_delivery_id text not null,
    replay_delivery_id text,
    environment text not null,
    approval_id text,
    plan_id text not null,
    status text not null check (
        status in ('preparing', 'published', 'failed', 'completed', 'duplicate_completed')
    ),
    check (environment in ('local_sandbox', 'production')),
    created_at timestamptz not null,
    completed_at timestamptz
);

-- Replay audit intentionally has no dead-letter foreign key: cleanup removes
-- only the resolved dead-letter record and preserves this evidence.

create unique index if not exists service_event_replays_active_idx
    on platform.service_event_replays (dead_letter_id)
    where status in ('preparing', 'published');

create index if not exists service_event_replays_recorded_idx
    on platform.service_event_replays (created_at, replay_id);

create index if not exists service_event_dead_letters_cleanup_idx
    on platform.service_event_dead_letters (status, resolved_at, dead_lettered_at);
