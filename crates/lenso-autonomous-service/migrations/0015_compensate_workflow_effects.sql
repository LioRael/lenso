alter table platform.service_workflow_instances
    drop constraint if exists service_workflow_instances_state_check;

alter table platform.service_workflow_instances
    add constraint service_workflow_instances_state_check
        check (state in (
            'running',
            'completed',
            'failed',
            'compensating',
            'compensated',
            'compensation_failed'
        ));

alter table platform.service_workflow_instances
    drop constraint if exists service_workflow_instances_failure_check;

alter table platform.service_workflow_instances
    add constraint service_workflow_instances_failure_check check (
        (
            state in ('failed', 'compensation_failed')
            and failure_evidence is not null
            and terminal_transition_id is not null
        )
        or
        (
            state not in ('failed', 'compensation_failed')
            and failure_evidence is null
            and terminal_transition_id is null
        )
    );

create table if not exists platform.service_workflow_effects (
    effect_id text primary key,
    instance_id text not null
        references platform.service_workflow_instances(instance_id),
    step_id text not null references platform.service_workflow_steps(step_id),
    definition_step_name text not null,
    effect_transition_id text not null,
    effect_outgoing_work jsonb not null,
    compensation_name text not null,
    compensation_order integer not null check (compensation_order > 0),
    compensation_contract_id text not null,
    compensation_contract_version text not null,
    compensation_completion_contract_id text not null,
    compensation_completion_contract_version text not null,
    state text not null check (
        state in ('completed', 'compensated', 'compensation_failed')
    ),
    completed_at timestamptz not null,
    updated_at timestamptz not null,
    unique (instance_id, step_id),
    unique (instance_id, effect_transition_id),
    unique (instance_id, compensation_name),
    unique (instance_id, compensation_order)
);

create index if not exists service_workflow_effects_instance_idx
    on platform.service_workflow_effects (
        instance_id,
        compensation_order,
        effect_id
    );
