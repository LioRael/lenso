do $$
begin
    if exists (
        select 1
        from platform.service_workflow_instances
        where state = 'running'
    ) then
        raise exception using
            errcode = 'check_violation',
            message = 'Cannot enable pinned Workflow Definition artifacts while legacy Workflow Instances are still running. Drain them with the previous Service release before retrying this migration.';
    end if;
end
$$;

alter table platform.service_workflow_instances
    add column if not exists definition_artifact jsonb,
    add column if not exists definition_digest text;

comment on column platform.service_workflow_instances.definition_artifact is
    'Exact immutable Workflow Definition selected when this instance started.';

comment on column platform.service_workflow_instances.definition_digest is
    'sha256:<lowercase-hex> digest of the canonical serialized Workflow Definition.';
