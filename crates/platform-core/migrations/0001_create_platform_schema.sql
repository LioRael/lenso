create schema if not exists platform;

create table if not exists platform.schema_migrations (
    name text primary key,
    applied_at timestamptz not null default now()
);

