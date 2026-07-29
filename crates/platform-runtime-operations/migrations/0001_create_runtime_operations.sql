create table if not exists platform.system_plane_runtime_operations (
    operation_id text primary key,
    service_id text not null,
    idempotency_key text not null,
    request_digest text not null check (request_digest ~ '^sha256:[0-9a-f]{64}$'),
    intent_digest text not null check (intent_digest ~ '^sha256:[0-9a-f]{64}$'),
    plan_digest text not null check (plan_digest ~ '^sha256:[0-9a-f]{64}$'),
    target_kind text not null,
    target_id text not null,
    target_revision_before text not null check (target_revision_before ~ '^sha256:[0-9a-f]{64}$'),
    target_revision_after text check (target_revision_after is null or target_revision_after ~ '^sha256:[0-9a-f]{64}$'),
    state text not null check (state in ('accepted', 'succeeded', 'rejected', 'failed')),
    authorization_evidence jsonb not null,
    acknowledgement jsonb not null,
    accepted_at_unix_ms bigint not null check (accepted_at_unix_ms > 0),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (service_id, idempotency_key)
);

create table if not exists platform.system_plane_runtime_operation_evidence (
    operation_id text not null references platform.system_plane_runtime_operations(operation_id),
    sequence bigint not null check (sequence > 0),
    state text not null check (state in ('accepted', 'succeeded', 'rejected', 'failed')),
    evidence jsonb not null,
    recorded_at timestamptz not null default now(),
    primary key (operation_id, sequence)
);
