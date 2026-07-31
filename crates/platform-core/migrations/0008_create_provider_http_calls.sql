create table if not exists platform.provider_http_calls (
    id text primary key,
    module_name text not null,
    method text not null,
    declared_path text not null,
    provider_path text not null,
    capability text,
    provider_status integer,
    duration_ms bigint not null,
    success boolean not null,
    error_code text,
    retryable boolean not null default false,
    request_id text not null,
    correlation_id text not null,
    trace_id text,
    span_id text,
    path_params jsonb not null default '{}'::jsonb,
    error_details jsonb not null default '[]'::jsonb,
    occurred_at timestamptz not null default now(),
    created_at timestamptz not null default now()
);

create index if not exists provider_http_calls_correlation_idx
    on platform.provider_http_calls (correlation_id, occurred_at desc, id desc);

create index if not exists provider_http_calls_module_idx
    on platform.provider_http_calls (module_name, occurred_at desc, id desc);

create index if not exists provider_http_calls_success_idx
    on platform.provider_http_calls (success, occurred_at desc, id desc);
