alter table platform.service_workflow_instances
    drop constraint if exists service_workflow_instances_state_check;

alter table platform.service_workflow_instances
    add constraint service_workflow_instances_state_check
        check (state in ('running', 'completed', 'failed'));

alter table platform.service_workflow_steps
    drop constraint if exists service_workflow_steps_state_check;

alter table platform.service_workflow_steps
    add constraint service_workflow_steps_state_check
        check (state in (
            'pending',
            'waiting_for_child',
            'completed',
            'exhausted',
            'failed'
        )),
    add column if not exists attempt_count integer not null default 0
        check (attempt_count >= 0),
    add column if not exists max_attempts integer not null default 1
        check (max_attempts > 0),
    add column if not exists retry_schedule jsonb not null default '[]'::jsonb,
    add column if not exists timeout_ms bigint
        check (timeout_ms is null or timeout_ms > 0),
    add column if not exists next_attempt_at timestamptz,
    add column if not exists failure_classification text,
    add column if not exists failure_code text,
    add column if not exists failure_message text,
    add column if not exists exhausted_at timestamptz;

create table if not exists platform.service_workflow_step_attempts (
    attempt_id text primary key,
    instance_id text not null
        references platform.service_workflow_instances(instance_id),
    step_id text not null references platform.service_workflow_steps(step_id),
    attempt_number integer not null check (attempt_number > 0),
    transition_id text not null,
    state text not null check (state in ('running', 'failed', 'succeeded')),
    failure_classification text,
    failure_code text,
    failure_message text,
    scheduled_at timestamptz not null,
    started_at timestamptz not null,
    completed_at timestamptz,
    unique (step_id, attempt_number),
    unique (step_id, transition_id)
);

create index if not exists service_workflow_attempts_instance_idx
    on platform.service_workflow_step_attempts (
        instance_id,
        step_id,
        attempt_number
    );

create table if not exists platform.service_workflow_timers (
    timer_id text primary key,
    instance_id text not null
        references platform.service_workflow_instances(instance_id),
    step_id text not null references platform.service_workflow_steps(step_id),
    kind text not null check (kind in ('retry', 'step_timeout')),
    attempt_number integer not null check (attempt_number > 0),
    transition_id text not null,
    attempt_transition_id text not null,
    due_at timestamptz not null,
    state text not null check (
        state in ('pending', 'claimed', 'completed', 'cancelled')
    ),
    claimed_by text,
    claimed_at timestamptz,
    completed_at timestamptz,
    created_at timestamptz not null,
    updated_at timestamptz not null,
    unique (step_id, transition_id)
);

create index if not exists service_workflow_timers_due_idx
    on platform.service_workflow_timers (
        state,
        due_at,
        timer_id
    );

create index if not exists service_workflow_timers_instance_idx
    on platform.service_workflow_timers (
        instance_id,
        step_id,
        attempt_number,
        created_at
    );
