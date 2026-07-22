create table platform.delivery_artifacts (
    record_index bigserial unique not null,
    delivery_id text not null,
    artifact_id text not null,
    protocol text not null,
    artifact_digest text not null,
    artifact_json jsonb not null,
    recorded_at timestamptz not null default now(),
    primary key (delivery_id, artifact_id, artifact_digest)
);

create index delivery_artifacts_latest_delivery_idx
    on platform.delivery_artifacts (recorded_at desc, record_index desc, delivery_id desc);
