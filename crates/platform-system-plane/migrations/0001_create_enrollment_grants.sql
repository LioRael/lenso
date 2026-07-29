create table if not exists platform.system_plane_enrollment_grants (
    managed_service_id text primary key,
    console_service_principal text not null,
    grant_revision bigint not null check (grant_revision > 0),
    authorization_epoch bigint not null check (authorization_epoch >= 0),
    expires_at_unix_ms bigint not null check (expires_at_unix_ms > 0),
    revoked_at_unix_ms bigint,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    check (revoked_at_unix_ms is null or revoked_at_unix_ms > 0)
);

create unique index if not exists system_plane_one_active_console_per_service_idx
    on platform.system_plane_enrollment_grants (managed_service_id)
    where revoked_at_unix_ms is null;
