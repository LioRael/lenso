alter table platform.system_plane_enrollment_grants
    add column system_id text not null,
    add column managed_service_principal text not null,
    add column managed_service_revision text not null,
    add column offer_digest text not null,
    add column receipt_digest text not null,
    add column capabilities jsonb not null,
    add column policy jsonb not null,
    add constraint system_plane_offer_digest_format
        check (offer_digest ~ '^sha256:[0-9a-f]{64}$'),
    add constraint system_plane_receipt_digest_format
        check (receipt_digest ~ '^sha256:[0-9a-f]{64}$');

create table platform.system_plane_enrollment_receipts (
    receipt_digest text primary key check (receipt_digest ~ '^sha256:[0-9a-f]{64}$'),
    offer_digest text not null unique check (offer_digest ~ '^sha256:[0-9a-f]{64}$'),
    nonce text not null unique,
    managed_service_id text not null references platform.system_plane_enrollment_grants(managed_service_id),
    receipt jsonb not null,
    persisted_at timestamptz not null default now()
);

create table platform.system_plane_enrollment_audit (
    sequence bigint generated always as identity primary key,
    managed_service_id text not null,
    event_kind text not null check (event_kind in ('enrollment_accepted', 'enrollment_revoked', 'enrollment_transferred')),
    receipt_digest text,
    authorization_epoch bigint not null check (authorization_epoch >= 0),
    evidence jsonb not null,
    recorded_at timestamptz not null default now()
);
