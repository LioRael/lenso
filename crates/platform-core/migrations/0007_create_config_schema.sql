create schema if not exists config;

create table if not exists config.setting_values (
    service text not null,
    key text not null,
    value jsonb not null,
    updated_at timestamptz not null default now(),
    updated_by text,
    primary key (service, key)
);

create table if not exists config.setting_audit (
    id uuid primary key,
    service text not null,
    key text not null,
    old_value jsonb,
    new_value jsonb not null,
    actor text,
    changed_at timestamptz not null default now()
);

create index if not exists setting_audit_key_idx
    on config.setting_audit (service, key, changed_at desc);
