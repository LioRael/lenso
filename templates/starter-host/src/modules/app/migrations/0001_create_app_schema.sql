create schema if not exists app;

create table if not exists app.items (
    id bigserial primary key,
    title text not null check (length(trim(title)) > 0),
    created_at timestamptz not null default now()
);
