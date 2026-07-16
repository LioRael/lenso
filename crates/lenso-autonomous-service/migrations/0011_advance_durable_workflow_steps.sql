alter table platform.service_workflow_instances
    drop constraint if exists service_workflow_instances_state_check;

alter table platform.service_workflow_instances
    add constraint service_workflow_instances_state_check
        check (state in ('running', 'completed')),
    add column if not exists start_trigger_kind text,
    add column if not exists start_trigger_id text,
    add column if not exists workflow_context jsonb;

create unique index if not exists service_workflow_instances_trigger_idx
    on platform.service_workflow_instances (
        service_id,
        definition_owner,
        definition_name,
        definition_version,
        start_trigger_kind,
        start_trigger_id
    )
    where start_trigger_kind is not null and start_trigger_id is not null;

alter table platform.service_workflow_steps
    drop constraint if exists service_workflow_steps_state_check;

alter table platform.service_workflow_steps
    add constraint service_workflow_steps_state_check
        check (state in ('pending', 'completed')),
    add column if not exists transition_id text,
    add column if not exists completed_at timestamptz,
    add column if not exists outgoing_work jsonb;

create unique index if not exists service_workflow_steps_transition_idx
    on platform.service_workflow_steps (instance_id, transition_id)
    where transition_id is not null;
