alter table platform.service_event_inbox
    add column if not exists attempt_count integer not null default 0,
    add column if not exists next_attempt_at timestamptz,
    add column if not exists failure_reason text,
    add column if not exists reason_code text,
    add column if not exists terminal_outcome text,
    add column if not exists delivery_history jsonb not null default '[]'::jsonb,
    add column if not exists original_envelope jsonb,
    add column if not exists max_attempts integer,
    add column if not exists retry_schedule jsonb;

update platform.service_event_inbox
set original_envelope = envelope
where original_envelope is null;

create table if not exists platform.service_event_dead_letters (
    dead_letter_id text primary key,
    consumer_id text not null,
    event_id text not null,
    delivery_id text not null,
    envelope jsonb not null,
    contract_id text not null,
    contract_version text not null,
    failure_reason text not null,
    reason_code text not null,
    diagnostic text not null,
    attempt_count integer not null,
    terminal_outcome text not null,
    delivery_history jsonb not null,
    max_attempts integer not null,
    retry_schedule jsonb not null,
    next_actions jsonb not null,
    dead_lettered_at timestamptz not null,
    unique (consumer_id, event_id)
);

create index if not exists service_event_dead_letters_recorded_idx
    on platform.service_event_dead_letters (dead_lettered_at, event_id);
