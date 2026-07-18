create table if not exists platform.federated_story_source_state (
    source_service_id text not null,
    tenant_scope text not null,
    cursor text,
    last_successful_at timestamptz,
    last_source_as_of timestamptz,
    updated_at timestamptz not null,
    primary key (source_service_id, tenant_scope)
);

create table if not exists platform.federated_story_segments (
    source_service_id text not null,
    segment_id text not null,
    evidence_revision integer not null check (evidence_revision > 0),
    story_id text not null,
    tenant_scope text not null,
    segment jsonb not null,
    collected_at timestamptz not null,
    primary key (source_service_id, segment_id, evidence_revision)
);

create index if not exists federated_story_segments_story_idx
    on platform.federated_story_segments (
        story_id,
        tenant_scope,
        collected_at,
        source_service_id,
        segment_id
    );

create table if not exists platform.federated_story_gaps (
    source_service_id text not null,
    tenant_scope text not null,
    kind text not null check (kind in (
        'unreachable',
        'stale',
        'unauthorized',
        'truncated',
        'retention_expired'
    )),
    detected_at timestamptz not null,
    last_observed_at timestamptz not null,
    resolved_at timestamptz,
    detail text not null,
    next_action text not null,
    primary key (source_service_id, tenant_scope, kind, detected_at)
);

create unique index if not exists federated_story_gaps_active_idx
    on platform.federated_story_gaps (source_service_id, tenant_scope, kind)
    where resolved_at is null;

create or replace function platform.reject_federated_story_segment_mutation()
returns trigger
language plpgsql
as $$
begin
    raise exception 'Federated Runtime Story Segment revisions are append-only';
end;
$$;

drop trigger if exists federated_story_segments_no_update
    on platform.federated_story_segments;
create trigger federated_story_segments_no_update
before update on platform.federated_story_segments
for each row execute function platform.reject_federated_story_segment_mutation();

drop trigger if exists federated_story_segments_no_delete
    on platform.federated_story_segments;
create trigger federated_story_segments_no_delete
before delete on platform.federated_story_segments
for each row execute function platform.reject_federated_story_segment_mutation();

comment on table platform.federated_story_source_state is
    'Observability-plane per-source Story Segment Feed cursors; never read by Service workflow execution.';
comment on table platform.federated_story_segments is
    'Idempotently collected Service-owned Story Segment revisions for Federated Runtime Stories.';
comment on table platform.federated_story_gaps is
    'Explicit source coverage gaps retained independently from business workflow state.';
