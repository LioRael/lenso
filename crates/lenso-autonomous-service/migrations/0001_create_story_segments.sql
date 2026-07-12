create schema if not exists platform;

create table if not exists platform.service_store_ownership (
    store_id text primary key,
    service_id text not null
);

create table if not exists platform.service_story_segments (
    segment_id text primary key,
    service_id text not null,
    workload_id text not null,
    operation text not null,
    status text not null,
    started_at timestamptz not null,
    completed_at timestamptz not null
);

create index if not exists service_story_segments_service_completed_idx
    on platform.service_story_segments (service_id, completed_at desc);
