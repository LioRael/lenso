create table if not exists platform.idempotency_claims (
    scope text not null,
    key text not null,
    claimed_at timestamptz not null default now(),
    primary key (scope, key),
    check (length(trim(scope)) > 0),
    check (length(trim(key)) > 0)
);
