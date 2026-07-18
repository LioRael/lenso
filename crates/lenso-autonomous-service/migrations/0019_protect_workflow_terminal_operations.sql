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
            'compensation_failed',
            'cancelled',
            'terminated'
        )),
    add column if not exists terminal_intent text,
    add column if not exists terminal_evidence jsonb;

alter table platform.service_workflow_instances
    drop constraint if exists service_workflow_instances_terminal_intent_check;

alter table platform.service_workflow_instances
    add constraint service_workflow_instances_terminal_intent_check check (
        (
            terminal_intent = 'cancelled'
            and state in ('compensating', 'cancelled', 'compensation_failed')
            and terminal_evidence is not null
        )
        or
        (
            terminal_intent is null
            and (
                (state = 'terminated' and terminal_evidence is not null)
                or (state <> 'terminated' and terminal_evidence is null)
            )
        )
    );

comment on column platform.service_workflow_instances.terminal_intent is
    'Protected terminal outcome requested while cooperative compensation is still running.';

comment on column platform.service_workflow_instances.terminal_evidence is
    'Durable terminal operation evidence. It never implies compensation or cleanup for terminate.';

alter table platform.service_workflow_steps
    drop constraint if exists service_workflow_steps_state_check;

alter table platform.service_workflow_steps
    add constraint service_workflow_steps_state_check
        check (state in (
            'pending',
            'waiting_for_child',
            'completed',
            'exhausted',
            'failed',
            'cancelled',
            'terminated'
        ));

alter table platform.service_workflow_compensations
    add column if not exists selection_kind text not null default 'timeout';

alter table platform.service_workflow_compensations
    drop constraint if exists service_workflow_compensations_selection_kind_check;

alter table platform.service_workflow_compensations
    add constraint service_workflow_compensations_selection_kind_check
        check (selection_kind in ('timeout', 'cancel'));

comment on column platform.service_workflow_compensations.selection_kind is
    'Whether durable compensation was selected by a timeout or cooperative cancel operation.';

alter table platform.service_workflow_interventions
    drop constraint if exists service_workflow_interventions_action_check,
    drop constraint if exists service_workflow_interventions_check;

alter table platform.service_workflow_interventions
    add constraint service_workflow_interventions_action_check check (
        action in ('pause', 'resume', 'retry', 'cancel', 'terminate', 'intervene')
    ),
    add column if not exists tenant_scope jsonb,
    add column if not exists affected_resources jsonb not null default '{}'::jsonb,
    add column if not exists approval_boundary text not null default 'workflow_instance_control',
    add column if not exists expected_terminal_state text;

update platform.service_workflow_interventions intervention
set tenant_scope = instance.tenant_scope,
    affected_resources = jsonb_build_object(
        'instanceId', intervention.instance_id,
        'selectedStepId', intervention.step_id,
        'affectedStepIds', case
            when intervention.step_id is null then '[]'::jsonb
            else jsonb_build_array(intervention.step_id)
        end,
        'pendingWorkIds', '[]'::jsonb,
        'timerIds', '[]'::jsonb,
        'attemptIds', '[]'::jsonb,
        'completedStepIds', '[]'::jsonb,
        'childWorkflowIds', '[]'::jsonb,
        'compensationIds', '[]'::jsonb,
        'irreversibleEffects', '[]'::jsonb,
        'inFlightClaimIds', '[]'::jsonb
    )
from platform.service_workflow_instances instance
where instance.instance_id = intervention.instance_id
  and intervention.affected_resources = '{}'::jsonb;

alter table platform.service_workflow_interventions
    alter column affected_resources drop default,
    alter column approval_boundary drop default;

alter table platform.service_workflow_interventions
    add constraint service_workflow_interventions_shape_check check (
        (
            action = 'retry'
            and step_id is not null
            and attempt_transition_id is not null
        )
        or
        (
            action = 'intervene'
            and attempt_transition_id is null
        )
        or
        (
            action in ('pause', 'resume', 'cancel', 'terminate')
            and step_id is null
            and attempt_transition_id is null
        )
    );

comment on column platform.service_workflow_interventions.tenant_scope is
    'Tenant scope copied from the exact Workflow state authorized by the Approval Boundary.';

comment on column platform.service_workflow_interventions.affected_resources is
    'Deterministic affected steps, timers, children, compensations, claims, and irreversible effects.';

comment on column platform.service_workflow_interventions.approval_boundary is
    'Explicit Approval Boundary verified before the protected mutation was applied.';
