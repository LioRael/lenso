create table if not exists platform.provider_host_effect_commits (
    invocation_id text primary key,
    outcome_digest text not null,
    effects_digest text not null,
    service_release_digest text not null,
    module_release_digest text not null,
    export_key text not null,
    committed_at timestamptz not null default now(),
    acknowledged_at timestamptz,
    check (outcome_digest ~ '^sha256:[0-9a-f]{64}$'),
    check (effects_digest ~ '^sha256:[0-9a-f]{64}$')
);
