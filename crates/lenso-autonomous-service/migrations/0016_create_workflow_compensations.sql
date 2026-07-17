create table if not exists platform.service_workflow_compensations (
    compensation_id text primary key,
    effect_id text not null unique
        references platform.service_workflow_effects(effect_id),
    instance_id text not null
        references platform.service_workflow_instances(instance_id),
    step_id text not null references platform.service_workflow_steps(step_id),
    name text not null,
    execution_order integer not null check (execution_order > 0),
    contract_id text not null,
    contract_version text not null,
    completion_contract_id text not null,
    completion_contract_version text not null,
    state text not null check (state in ('pending', 'dispatched', 'compensated', 'failed')),
    attempt_count integer not null default 0 check (attempt_count >= 0),
    transition_id text,
    outgoing_work jsonb,
    failure_evidence jsonb,
    selected_by_timeout_transition_id text not null,
    selected_at timestamptz not null,
    completed_at timestamptz,
    updated_at timestamptz not null,
    unique (instance_id, execution_order),
    unique (instance_id, name),
    check (
        (
            state = 'pending'
            and transition_id is null
            and outgoing_work is null
            and failure_evidence is null
            and completed_at is null
        )
        or
        (
            state = 'dispatched'
            and transition_id is not null
            and outgoing_work is not null
            and failure_evidence is null
            and completed_at is null
        )
        or
        (
            state = 'compensated'
            and transition_id is not null
            and outgoing_work is not null
            and failure_evidence is null
            and completed_at is not null
        )
        or
        (
            state = 'failed'
            and transition_id is not null
            and failure_evidence is not null
            and completed_at is not null
        )
    )
);

create index if not exists service_workflow_compensations_next_idx
    on platform.service_workflow_compensations (
        instance_id,
        state,
        execution_order,
        compensation_id
    );

create table if not exists platform.service_workflow_compensation_attempts (
    attempt_id text primary key,
    compensation_id text not null
        references platform.service_workflow_compensations(compensation_id),
    instance_id text not null
        references platform.service_workflow_instances(instance_id),
    attempt_number integer not null check (attempt_number > 0),
    transition_id text not null,
    state text not null check (state in ('dispatched', 'succeeded', 'failed')),
    failure_evidence jsonb,
    started_at timestamptz not null,
    completed_at timestamptz,
    unique (compensation_id, attempt_number),
    unique (compensation_id, transition_id),
    check (
        (state = 'dispatched' and failure_evidence is null and completed_at is null)
        or
        (state = 'succeeded' and failure_evidence is null and completed_at is not null)
        or
        (state = 'failed' and failure_evidence is not null and completed_at is not null)
    )
);
