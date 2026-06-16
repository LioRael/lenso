create table if not exists auth.users (
    id text primary key,
    created_at timestamptz not null,
    disabled_at timestamptz
);

create table if not exists auth.identities (
    id text primary key,
    user_id text not null references auth.users(id) on delete cascade,
    provider text not null,
    provider_subject text not null,
    created_at timestamptz not null,
    updated_at timestamptz not null,
    constraint identities_provider_subject_key unique (provider, provider_subject),
    constraint identities_provider_not_empty check (length(provider) > 0),
    constraint identities_provider_subject_not_empty check (length(provider_subject) > 0)
);

create index if not exists identities_user_id_idx on auth.identities (user_id);

create table if not exists auth.sessions (
    id text primary key,
    user_id text not null references auth.users(id) on delete cascade,
    token_hash text not null,
    created_at timestamptz not null,
    expires_at timestamptz not null,
    revoked_at timestamptz,
    constraint sessions_token_hash_key unique (token_hash)
);

create index if not exists sessions_user_id_idx on auth.sessions (user_id);
create index if not exists sessions_expires_at_idx on auth.sessions (expires_at);
