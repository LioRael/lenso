create table if not exists platform.service_worker_health (
    service_id text not null,
    workload_id text not null,
    phase text not null,
    updated_at timestamptz not null default now(),
    primary key (service_id, workload_id)
);

create unique index if not exists service_story_segments_background_outcome_idx
    on platform.service_story_segments (segment_id);
