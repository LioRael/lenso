create table if not exists platform.federated_story_reliability (
    source_service_id text not null,
    tenant_scope text not null,
    status text not null check (status in (
        'available',
        'unavailable',
        'not_declared'
    )),
    report jsonb,
    detail text,
    next_action text,
    observed_at timestamptz not null,
    primary key (source_service_id, tenant_scope),
    check (
        (status = 'available' and report is not null and detail is null)
        or (status = 'not_declared' and report is null)
        or (status = 'unavailable' and report is null and detail is not null)
    )
);

comment on table platform.federated_story_reliability is
    'Latest report-only Reliability Contract evidence collected beside each Story Segment source.';
