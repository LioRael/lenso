alter table platform.service_workflow_instances
    drop constraint if exists service_workflow_instances_state_check;

alter table platform.service_workflow_instances
    add constraint service_workflow_instances_state_check
        check (state in ('running', 'completed', 'failed')),
    add column if not exists parent_instance_id text
        references platform.service_workflow_instances(instance_id),
    add column if not exists parent_step_id text
        references platform.service_workflow_steps(step_id),
    add column if not exists causation_id text,
    add column if not exists failure_evidence jsonb,
    add column if not exists terminal_transition_id text;

alter table platform.service_workflow_instances
    drop constraint if exists service_workflow_instances_parent_link_check;

alter table platform.service_workflow_instances
    add constraint service_workflow_instances_parent_link_check check (
        (parent_instance_id is null and parent_step_id is null and causation_id is null)
        or
        (
            parent_instance_id is not null
            and parent_step_id is not null
            and causation_id is not null
            and causation_id = parent_step_id
            and parent_instance_id <> instance_id
        )
    );

alter table platform.service_workflow_instances
    drop constraint if exists service_workflow_instances_failure_check;

alter table platform.service_workflow_instances
    add constraint service_workflow_instances_failure_check check (
        (
            state = 'failed'
            and failure_evidence is not null
            and terminal_transition_id is not null
        )
        or
        (
            state <> 'failed'
            and failure_evidence is null
            and terminal_transition_id is null
        )
    );

create index if not exists service_workflow_instances_parent_idx
    on platform.service_workflow_instances (
        service_id,
        parent_instance_id,
        parent_step_id,
        instance_id
    )
    where parent_instance_id is not null;

alter table platform.service_workflow_steps
    drop constraint if exists service_workflow_steps_state_check;

alter table platform.service_workflow_steps
    add constraint service_workflow_steps_state_check
        check (state in ('pending', 'waiting_for_child', 'completed', 'failed'));

create unique index if not exists service_workflow_steps_instance_step_idx
    on platform.service_workflow_steps (instance_id, step_id);

alter table platform.service_workflow_instances
    add constraint service_workflow_instances_parent_step_fk
        foreign key (parent_instance_id, parent_step_id)
        references platform.service_workflow_steps(instance_id, step_id);

create table if not exists platform.service_workflow_child_links (
    link_id text primary key,
    start_id text not null,
    parent_instance_id text not null
        references platform.service_workflow_instances(instance_id),
    parent_step_id text not null
        references platform.service_workflow_steps(step_id),
    parent_definition_version text not null,
    child_definition_owner text not null,
    child_definition_name text not null,
    child_definition_version text not null,
    child_instance_id text unique
        references platform.service_workflow_instances(instance_id),
    state text not null check (
        state in ('waiting', 'completed', 'failed', 'unsupported_version')
    ),
    completion_delivery_id text,
    failure_evidence jsonb,
    next_action text not null,
    created_at timestamptz not null,
    updated_at timestamptz not null,
    unique (parent_instance_id, parent_step_id),
    unique (parent_instance_id, start_id),
    foreign key (parent_instance_id, parent_step_id)
        references platform.service_workflow_steps(instance_id, step_id),
    check (length(trim(next_action)) > 0),
    check (
        (state = 'unsupported_version' and child_instance_id is null)
        or
        (state <> 'unsupported_version' and child_instance_id is not null)
    ),
    check (
        (
            state = 'waiting'
            and completion_delivery_id is null
            and failure_evidence is null
        )
        or
        (
            state = 'completed'
            and completion_delivery_id is not null
            and failure_evidence is null
        )
        or
        (
            state = 'failed'
            and completion_delivery_id is not null
            and failure_evidence is not null
        )
        or
        (
            state = 'unsupported_version'
            and completion_delivery_id is null
            and failure_evidence is not null
        )
    )
);

create index if not exists service_workflow_child_links_parent_idx
    on platform.service_workflow_child_links (
        parent_instance_id,
        parent_step_id,
        created_at,
        link_id
    );
