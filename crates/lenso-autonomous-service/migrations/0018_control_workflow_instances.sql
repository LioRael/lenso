alter table platform.service_workflow_instances
    add column if not exists control_state text not null default 'active',
    add column if not exists control_revision bigint not null default 0,
    add column if not exists paused_at timestamptz;

alter table platform.service_workflow_instances
    drop constraint if exists service_workflow_instances_control_state_check;

alter table platform.service_workflow_instances
    add constraint service_workflow_instances_control_state_check
        check (control_state in ('active', 'paused')),
    drop constraint if exists service_workflow_instances_control_revision_check,
    add constraint service_workflow_instances_control_revision_check
        check (control_revision >= 0),
    drop constraint if exists service_workflow_instances_paused_at_check,
    add constraint service_workflow_instances_paused_at_check check (
        (control_state = 'active' and paused_at is null)
        or (control_state = 'paused' and paused_at is not null)
    );

comment on column platform.service_workflow_instances.control_state is
    'Operator dispatch gate. Pausing preserves execution state, timers, attempts, and claims.';

comment on column platform.service_workflow_instances.control_revision is
    'Monotonic revision included in deterministic operator plans for stale-plan detection.';

create table if not exists platform.service_workflow_interventions (
    intervention_id text primary key,
    instance_id text not null
        references platform.service_workflow_instances(instance_id),
    step_id text references platform.service_workflow_steps(step_id),
    action text not null check (action in ('pause', 'resume', 'retry')),
    plan_id text not null unique,
    actor_id text not null check (length(trim(actor_id)) > 0),
    authority_id text not null check (length(trim(authority_id)) > 0),
    reason text not null check (length(trim(reason)) > 0),
    prior_state jsonb not null,
    resulting_state jsonb not null,
    next_action text not null check (length(trim(next_action)) > 0),
    attempt_transition_id text,
    recorded_at timestamptz not null,
    foreign key (instance_id, step_id)
        references platform.service_workflow_steps(instance_id, step_id),
    check (
        (action = 'retry' and step_id is not null and attempt_transition_id is not null)
        or (action <> 'retry' and step_id is null and attempt_transition_id is null)
    )
);

create index if not exists service_workflow_interventions_instance_idx
    on platform.service_workflow_interventions (
        instance_id,
        recorded_at,
        intervention_id
    );
