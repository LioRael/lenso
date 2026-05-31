alter table platform.outbox
    add column if not exists max_attempts integer not null default 3,
    add column if not exists locked_by text,
    add column if not exists published_at timestamptz,
    add column if not exists last_error text;

alter table platform.outbox
    drop column if exists processed_at;

