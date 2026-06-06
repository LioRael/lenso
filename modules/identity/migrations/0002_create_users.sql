create table if not exists identity.users (
    id text primary key,
    email text not null,
    display_name text,
    created_at timestamptz not null,
    updated_at timestamptz not null,
    constraint users_email_key unique (email)
);

