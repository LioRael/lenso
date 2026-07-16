create table if not exists platform.service_workflow_instances (
    instance_id text primary key,
    service_id text not null,
    definition_owner text not null,
    definition_name text not null,
    definition_version text not null,
    state text not null check (state in ('running')),
    input jsonb not null,
    result jsonb,
    story_context jsonb not null,
    tenant_scope jsonb,
    initial_step_id text not null,
    created_at timestamptz not null,
    updated_at timestamptz not null
);

create index if not exists service_workflow_instances_service_created_idx
    on platform.service_workflow_instances (service_id, created_at, instance_id);

create index if not exists service_workflow_instances_definition_idx
    on platform.service_workflow_instances (
        service_id,
        definition_owner,
        definition_name,
        definition_version
    );

create table if not exists platform.service_workflow_steps (
    step_id text primary key,
    instance_id text not null references platform.service_workflow_instances(instance_id),
    definition_step_name text not null,
    step_position integer not null check (step_position >= 0),
    state text not null check (state in ('pending')),
    created_at timestamptz not null,
    updated_at timestamptz not null,
    unique (instance_id, step_position),
    unique (instance_id, definition_step_name)
);

create index if not exists service_workflow_steps_instance_idx
    on platform.service_workflow_steps (instance_id, step_position, step_id);
