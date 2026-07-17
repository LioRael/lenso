create table if not exists platform.service_workflow_history (
    history_id text primary key,
    instance_id text not null
        references platform.service_workflow_instances(instance_id),
    step_id text references platform.service_workflow_steps(step_id),
    compensation_id text
        references platform.service_workflow_compensations(compensation_id),
    kind text not null,
    detail jsonb not null default '{}'::jsonb,
    recorded_at timestamptz not null
);

create index if not exists service_workflow_history_instance_idx
    on platform.service_workflow_history (instance_id, recorded_at, history_id);
