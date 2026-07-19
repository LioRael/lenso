create table platform.extraction_artifacts (
    plan_id text not null,
    artifact_id text not null,
    protocol text not null,
    artifact_digest text not null,
    artifact_json jsonb not null,
    recorded_at timestamptz not null default now(),
    primary key (plan_id, artifact_id, artifact_digest)
);

create index extraction_artifacts_latest_plan_idx
    on platform.extraction_artifacts (recorded_at desc, plan_id desc);
