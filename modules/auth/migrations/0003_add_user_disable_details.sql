alter table auth.users
    add column if not exists disabled_reason text,
    add column if not exists disabled_until timestamptz;
