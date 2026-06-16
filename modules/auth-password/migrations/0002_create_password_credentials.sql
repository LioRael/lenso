create table if not exists auth_password.credentials (
    identity_id text primary key,
    password_hash text not null,
    created_at timestamptz not null,
    updated_at timestamptz not null,
    constraint credentials_password_hash_not_empty check (length(password_hash) > 0)
);
