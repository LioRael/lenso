use super::{
    WorkflowApiError, WorkflowErrorCode, WorkflowInstance, WorkflowInstanceState,
    WorkflowMutationError, WorkflowStepInspection, WorkflowStepState, WorkflowTenantScope,
    WorkflowTimerState, load_instance, load_instance_in_tx, sha256_hex,
};
use crate::{
    ServiceRuntimeState, WorkflowTransitionDisposition,
    append_persisted_workflow_story_segment_in_tx,
};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, header},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use std::{fmt, str::FromStr};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

pub const WORKFLOW_OPERATOR_PLAN_PROTOCOL: &str = "lenso.workflow-operator-plan.v1";
pub const WORKFLOW_OPERATOR_RESULT_PROTOCOL: &str = "lenso.workflow-operator-result.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowControlState {
    Active,
    Paused,
}

impl WorkflowControlState {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "paused" => Some(Self::Paused),
            _ => None,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowControlInspection {
    pub state: WorkflowControlState,
    pub revision: u64,
    pub paused_at: Option<DateTime<Utc>>,
}

impl WorkflowControlInspection {
    pub(super) const fn active() -> Self {
        Self {
            state: WorkflowControlState::Active,
            revision: 0,
            paused_at: None,
        }
    }

    pub(super) fn from_stored(
        state: &str,
        revision: i64,
        paused_at: Option<DateTime<Utc>>,
    ) -> Result<Self, WorkflowApiError> {
        let state = WorkflowControlState::parse(state).ok_or_else(|| {
            WorkflowApiError::stored_state(format!(
                "Stored Workflow control state `{state}` is invalid"
            ))
        })?;
        let revision = u64::try_from(revision).map_err(|_| {
            WorkflowApiError::stored_state("Stored Workflow control revision is invalid")
        })?;
        if (state == WorkflowControlState::Paused) != paused_at.is_some() {
            return Err(WorkflowApiError::stored_state(
                "Stored Workflow pause timestamp does not match its control state",
            ));
        }
        Ok(Self {
            state,
            revision,
            paused_at,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowOperatorAction {
    Pause,
    Resume,
    Retry,
    Cancel,
    Terminate,
    Intervene,
}

impl WorkflowOperatorAction {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Retry => "retry",
            Self::Cancel => "cancel",
            Self::Terminate => "terminate",
            Self::Intervene => "intervene",
        }
    }

    const fn required_authority(self) -> &'static str {
        match self {
            Self::Pause => "workflow_instance_pause",
            Self::Resume => "workflow_instance_resume",
            Self::Retry => "workflow_step_retry",
            Self::Cancel => "workflow_instance_cancel",
            Self::Terminate => "workflow_instance_terminate",
            Self::Intervene => "workflow_human_intervention",
        }
    }

    const fn approval_boundary(self) -> WorkflowOperatorApprovalBoundary {
        match self {
            Self::Pause | Self::Resume | Self::Retry => {
                WorkflowOperatorApprovalBoundary::WorkflowInstanceControl
            }
            Self::Cancel | Self::Terminate => {
                WorkflowOperatorApprovalBoundary::WorkflowTerminalOperation
            }
            Self::Intervene => WorkflowOperatorApprovalBoundary::WorkflowHumanIntervention,
        }
    }
}

impl FromStr for WorkflowOperatorAction {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pause" => Ok(Self::Pause),
            "resume" => Ok(Self::Resume),
            "retry" => Ok(Self::Retry),
            "cancel" => Ok(Self::Cancel),
            "terminate" => Ok(Self::Terminate),
            "intervene" => Ok(Self::Intervene),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowTerminalOperationInspection {
    pub action: WorkflowOperatorAction,
    pub plan_id: String,
    pub expected_terminal_state: WorkflowInstanceState,
    pub compensation_required: bool,
    pub cleanup_reported: bool,
    pub requested_at: DateTime<Utc>,
}

impl WorkflowTerminalOperationInspection {
    pub(super) fn from_stored(
        terminal_intent: Option<&str>,
        evidence: Option<Value>,
        state: WorkflowInstanceState,
    ) -> Result<Option<Self>, WorkflowApiError> {
        let Some(evidence) = evidence else {
            if terminal_intent.is_some()
                || matches!(
                    state,
                    WorkflowInstanceState::Cancelled | WorkflowInstanceState::Terminated
                )
            {
                return Err(WorkflowApiError::stored_state(
                    "Stored Workflow terminal state is missing terminal operation evidence",
                ));
            }
            return Ok(None);
        };
        let operation: Self = serde_json::from_value(evidence).map_err(|error| {
            WorkflowApiError::stored_state(format!(
                "Stored Workflow terminal operation evidence is invalid: {error}"
            ))
        })?;
        let valid = match operation.action {
            WorkflowOperatorAction::Cancel => {
                terminal_intent == Some("cancelled")
                    && operation.expected_terminal_state == WorkflowInstanceState::Cancelled
                    && matches!(
                        state,
                        WorkflowInstanceState::Compensating
                            | WorkflowInstanceState::Cancelled
                            | WorkflowInstanceState::CompensationFailed
                    )
                    && match state {
                        WorkflowInstanceState::Cancelled if operation.compensation_required => {
                            operation.cleanup_reported
                        }
                        WorkflowInstanceState::Compensating
                        | WorkflowInstanceState::CompensationFailed => !operation.cleanup_reported,
                        WorkflowInstanceState::Cancelled => !operation.cleanup_reported,
                        _ => false,
                    }
            }
            WorkflowOperatorAction::Terminate => {
                terminal_intent.is_none()
                    && operation.expected_terminal_state == WorkflowInstanceState::Terminated
                    && state == WorkflowInstanceState::Terminated
                    && !operation.compensation_required
                    && !operation.cleanup_reported
            }
            _ => false,
        };
        if !valid {
            return Err(WorkflowApiError::stored_state(
                "Stored Workflow terminal operation does not match its execution state",
            ));
        }
        Ok(Some(operation))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowPendingWorkKind {
    Step,
    Timer,
    ChildWorkflow,
    Compensation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowPendingWorkInspection {
    pub kind: WorkflowPendingWorkKind,
    pub resource_id: String,
    pub step_id: Option<String>,
    pub state: String,
    pub due_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowOperatorStateSnapshot {
    pub execution_state: WorkflowInstanceState,
    pub control_state: WorkflowControlState,
    pub control_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowInterventionInspection {
    pub intervention_id: String,
    pub action: WorkflowOperatorAction,
    pub plan_id: String,
    pub step_id: Option<String>,
    pub actor_id: String,
    pub authority_id: String,
    pub reason: String,
    pub tenant_scope: Option<WorkflowTenantScope>,
    pub affected_resources: WorkflowOperatorAffectedResources,
    pub approval_boundary: WorkflowOperatorApprovalBoundary,
    pub expected_terminal_state: Option<WorkflowInstanceState>,
    pub prior_state: WorkflowOperatorStateSnapshot,
    pub resulting_state: WorkflowOperatorStateSnapshot,
    pub next_action: String,
    pub attempt_transition_id: Option<String>,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Default)]
pub(super) struct WorkflowControlEvidence {
    pub interventions: Vec<WorkflowInterventionInspection>,
}

#[derive(Debug, FromRow)]
struct InterventionRow {
    intervention_id: String,
    action: String,
    plan_id: String,
    step_id: Option<String>,
    actor_id: String,
    authority_id: String,
    reason: String,
    tenant_scope: Option<Value>,
    affected_resources: Value,
    approval_boundary: String,
    expected_terminal_state: Option<String>,
    prior_state: Value,
    resulting_state: Value,
    next_action: String,
    attempt_transition_id: Option<String>,
    recorded_at: DateTime<Utc>,
}

fn intervention_from_row(
    row: InterventionRow,
) -> Result<WorkflowInterventionInspection, WorkflowApiError> {
    Ok(WorkflowInterventionInspection {
        intervention_id: row.intervention_id,
        action: row.action.parse().map_err(|()| {
            WorkflowApiError::stored_state("Stored Workflow intervention action is invalid")
        })?,
        plan_id: row.plan_id,
        step_id: row.step_id,
        actor_id: row.actor_id,
        authority_id: row.authority_id,
        reason: row.reason,
        tenant_scope: row
            .tenant_scope
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| {
                WorkflowApiError::stored_state(format!(
                    "Stored Workflow intervention tenant scope is invalid: {error}"
                ))
            })?,
        affected_resources: serde_json::from_value(row.affected_resources).map_err(|error| {
            WorkflowApiError::stored_state(format!(
                "Stored Workflow intervention affected resources are invalid: {error}"
            ))
        })?,
        approval_boundary: WorkflowOperatorApprovalBoundary::parse(&row.approval_boundary)
            .ok_or_else(|| {
                WorkflowApiError::stored_state(
                    "Stored Workflow intervention Approval Boundary is invalid",
                )
            })?,
        expected_terminal_state: match row.expected_terminal_state {
            Some(value) => Some(WorkflowInstanceState::parse(&value).ok_or_else(|| {
                WorkflowApiError::stored_state(
                    "Stored Workflow intervention expected terminal state is invalid",
                )
            })?),
            None => None,
        },
        prior_state: serde_json::from_value(row.prior_state).map_err(|error| {
            WorkflowApiError::stored_state(format!(
                "Stored Workflow intervention prior state is invalid: {error}"
            ))
        })?,
        resulting_state: serde_json::from_value(row.resulting_state).map_err(|error| {
            WorkflowApiError::stored_state(format!(
                "Stored Workflow intervention resulting state is invalid: {error}"
            ))
        })?,
        next_action: row.next_action,
        attempt_transition_id: row.attempt_transition_id,
        recorded_at: row.recorded_at,
    })
}

const INTERVENTION_SELECT: &str = r#"
    select intervention_id, action, plan_id, step_id, actor_id, authority_id,
           reason, tenant_scope, affected_resources, approval_boundary,
           expected_terminal_state, prior_state, resulting_state, next_action,
           attempt_transition_id, recorded_at
    from platform.service_workflow_interventions
    where instance_id = $1
    order by ((resulting_state ->> 'controlRevision')::bigint), intervention_id
"#;

pub(super) async fn load_control_evidence(
    pool: &PgPool,
    instance_id: &str,
) -> Result<WorkflowControlEvidence, WorkflowApiError> {
    let rows = sqlx::query_as::<_, InterventionRow>(INTERVENTION_SELECT)
        .bind(instance_id)
        .fetch_all(pool)
        .await
        .map_err(|error| {
            WorkflowApiError::store(format!(
                "Could not inspect Workflow intervention history: {error}"
            ))
        })?;
    Ok(WorkflowControlEvidence {
        interventions: rows
            .into_iter()
            .map(intervention_from_row)
            .collect::<Result<_, _>>()?,
    })
}

pub(super) async fn load_control_evidence_in_tx(
    transaction: &mut Transaction<'_, Postgres>,
    instance_id: &str,
) -> Result<WorkflowControlEvidence, WorkflowMutationError> {
    let rows = sqlx::query_as::<_, InterventionRow>(INTERVENTION_SELECT)
        .bind(instance_id)
        .fetch_all(&mut **transaction)
        .await
        .map_err(|error| {
            WorkflowMutationError::store(format!(
                "Could not inspect Workflow intervention history: {error}"
            ))
        })?;
    let interventions = rows
        .into_iter()
        .map(intervention_from_row)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| WorkflowMutationError::new(error.code, error.message))?;
    Ok(WorkflowControlEvidence { interventions })
}

pub(super) fn selected_workflow_step(
    instance: &WorkflowInstance,
    step_id: Option<&str>,
) -> Result<Option<WorkflowStepInspection>, WorkflowApiError> {
    let Some(step_id) = step_id else {
        return Ok(None);
    };
    if step_id.trim().is_empty() {
        return Err(WorkflowApiError::invalid(
            WorkflowErrorCode::InvalidRequest,
            "Selected Workflow step identity must not be empty",
        ));
    }
    instance
        .steps
        .iter()
        .find(|step| step.step_id == step_id)
        .cloned()
        .map(Some)
        .ok_or_else(|| WorkflowApiError {
            code: WorkflowErrorCode::StepNotFound,
            message: format!(
                "Workflow step `{step_id}` was not found in instance `{}`",
                instance.instance_id
            ),
            next_actions: vec!["inspect_workflow_steps".to_owned()],
        })
}

pub(super) fn workflow_pending_work(
    instance: &WorkflowInstance,
) -> Vec<WorkflowPendingWorkInspection> {
    if matches!(
        instance.state,
        WorkflowInstanceState::Cancelled | WorkflowInstanceState::Terminated
    ) {
        return Vec::new();
    }
    let mut pending = Vec::new();
    for step in &instance.steps {
        if matches!(
            step.state,
            WorkflowStepState::Pending | WorkflowStepState::WaitingForChild
        ) {
            pending.push(WorkflowPendingWorkInspection {
                kind: WorkflowPendingWorkKind::Step,
                resource_id: step.step_id.clone(),
                step_id: Some(step.step_id.clone()),
                state: match step.state {
                    WorkflowStepState::Pending => "pending",
                    WorkflowStepState::WaitingForChild => "waiting_for_child",
                    _ => unreachable!("guarded by matches"),
                }
                .to_owned(),
                due_at: step.next_attempt_at,
            });
        }
        for timer in &step.timers {
            if matches!(
                timer.state,
                WorkflowTimerState::Pending | WorkflowTimerState::Claimed
            ) {
                pending.push(WorkflowPendingWorkInspection {
                    kind: WorkflowPendingWorkKind::Timer,
                    resource_id: timer.timer_id.clone(),
                    step_id: Some(step.step_id.clone()),
                    state: match timer.state {
                        WorkflowTimerState::Pending => "pending",
                        WorkflowTimerState::Claimed => "claimed",
                        _ => unreachable!("guarded by matches"),
                    }
                    .to_owned(),
                    due_at: Some(timer.due_at),
                });
            }
        }
        if let Some(child) = &step.child_workflow
            && step.state == WorkflowStepState::WaitingForChild
            && child.state == super::WorkflowChildState::Waiting
        {
            pending.push(WorkflowPendingWorkInspection {
                kind: WorkflowPendingWorkKind::ChildWorkflow,
                resource_id: child.link_id.clone(),
                step_id: Some(step.step_id.clone()),
                state: "waiting".to_owned(),
                due_at: None,
            });
        }
    }
    for compensation in &instance.compensations {
        if matches!(
            compensation.state,
            crate::WorkflowCompensationState::Pending
                | crate::WorkflowCompensationState::Dispatched
        ) {
            pending.push(WorkflowPendingWorkInspection {
                kind: WorkflowPendingWorkKind::Compensation,
                resource_id: compensation.compensation_id.clone(),
                step_id: Some(compensation.step_id.clone()),
                state: match compensation.state {
                    crate::WorkflowCompensationState::Pending => "pending",
                    crate::WorkflowCompensationState::Dispatched => "dispatched",
                    _ => unreachable!("guarded by matches"),
                }
                .to_owned(),
                due_at: None,
            });
        }
    }
    pending
}

pub(super) fn workflow_available_actions(
    instance: &WorkflowInstance,
    selected_step: Option<&WorkflowStepInspection>,
) -> Vec<WorkflowOperatorAction> {
    let mut actions = Vec::new();
    let cancellation_in_progress = instance
        .terminal_operation
        .as_ref()
        .is_some_and(|operation| operation.action == WorkflowOperatorAction::Cancel);
    if instance.control.state == WorkflowControlState::Paused {
        actions.push(WorkflowOperatorAction::Resume);
    } else {
        if matches!(
            instance.state,
            WorkflowInstanceState::Running | WorkflowInstanceState::Compensating
        ) {
            actions.push(WorkflowOperatorAction::Pause);
        }
        if instance.state == WorkflowInstanceState::Failed
            && selected_step.is_some_and(retry_step_is_eligible)
        {
            actions.push(WorkflowOperatorAction::Retry);
        }
    }
    if !cancellation_in_progress
        && matches!(
            instance.state,
            WorkflowInstanceState::Running
                | WorkflowInstanceState::Failed
                | WorkflowInstanceState::Compensating
        )
    {
        actions.push(WorkflowOperatorAction::Cancel);
    }
    if matches!(
        instance.state,
        WorkflowInstanceState::Running
            | WorkflowInstanceState::Failed
            | WorkflowInstanceState::Compensating
            | WorkflowInstanceState::CompensationFailed
    ) {
        actions.push(WorkflowOperatorAction::Terminate);
    }
    actions.push(WorkflowOperatorAction::Intervene);
    actions
}

fn retry_step_is_eligible(step: &WorkflowStepInspection) -> bool {
    step.state == WorkflowStepState::Exhausted
        && step.latest_failure.is_some()
        && step.child_workflow.is_none()
        && !step.timers.iter().any(|timer| {
            matches!(
                timer.state,
                WorkflowTimerState::Pending | WorkflowTimerState::Claimed
            )
        })
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowOperatorPlanRequest {
    pub selected_step_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowOperatorApplyRequest {
    pub plan_id: String,
    pub selected_step_id: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowOperatorAuthorizationStatus {
    Required,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowOperatorAuthorization {
    pub status: WorkflowOperatorAuthorizationStatus,
    pub required_authority: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowOperatorApprovalBoundary {
    WorkflowInstanceControl,
    WorkflowTerminalOperation,
    WorkflowHumanIntervention,
}

impl WorkflowOperatorApprovalBoundary {
    const fn as_str(self) -> &'static str {
        match self {
            Self::WorkflowInstanceControl => "workflow_instance_control",
            Self::WorkflowTerminalOperation => "workflow_terminal_operation",
            Self::WorkflowHumanIntervention => "workflow_human_intervention",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "workflow_instance_control" => Some(Self::WorkflowInstanceControl),
            "workflow_terminal_operation" => Some(Self::WorkflowTerminalOperation),
            "workflow_human_intervention" => Some(Self::WorkflowHumanIntervention),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowIrreversibleEffectInspection {
    pub step_id: String,
    pub definition_step_name: String,
    pub transition_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowOperatorAffectedResources {
    pub instance_id: String,
    pub selected_step_id: Option<String>,
    pub affected_step_ids: Vec<String>,
    pub pending_work_ids: Vec<String>,
    pub timer_ids: Vec<String>,
    pub attempt_ids: Vec<String>,
    pub completed_step_ids: Vec<String>,
    pub child_workflow_ids: Vec<String>,
    pub compensation_ids: Vec<String>,
    pub irreversible_effects: Vec<WorkflowIrreversibleEffectInspection>,
    pub in_flight_claim_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowOperatorPlan {
    pub protocol: String,
    pub plan_id: String,
    pub mutates_state: bool,
    pub action: WorkflowOperatorAction,
    pub service_id: String,
    pub instance_id: String,
    pub selected_step_id: Option<String>,
    pub definition_version: String,
    pub prior_state: WorkflowOperatorStateSnapshot,
    pub resulting_state: WorkflowOperatorStateSnapshot,
    pub expected_terminal_state: Option<WorkflowInstanceState>,
    pub affected_resources: WorkflowOperatorAffectedResources,
    pub preserved_state: Vec<String>,
    pub authorization: WorkflowOperatorAuthorization,
    pub approval_boundary: WorkflowOperatorApprovalBoundary,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowAuthorityRequest {
    pub action: WorkflowOperatorAction,
    pub required_authority: String,
    pub plan_id: String,
    pub service_id: String,
    pub instance_id: String,
    pub approval_boundary: WorkflowOperatorApprovalBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowAuthorityGrant {
    pub actor_id: String,
    pub authority_id: String,
}

pub trait WorkflowAuthorityVerifier: fmt::Debug + Send + Sync {
    /// Verifies a deployment-owned credential for one exact deterministic
    /// plan. The returned actor and authority identities are persisted in the
    /// intervention audit trail; the credential itself is never stored.
    fn verify(
        &self,
        request: &WorkflowAuthorityRequest,
        credential: &str,
    ) -> Result<WorkflowAuthorityGrant, String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VerifiedWorkflowAuthority {
    actor_id: String,
    authority_id: String,
    request: WorkflowAuthorityRequest,
}

fn verify_workflow_authority(
    state: &ServiceRuntimeState,
    request: WorkflowAuthorityRequest,
    credential: &str,
) -> Result<VerifiedWorkflowAuthority, WorkflowApiError> {
    if credential.trim().is_empty() {
        return Err(operator_error(
            WorkflowErrorCode::AuthorityRequired,
            "Workflow operator action requires an authority credential",
            "provide_workflow_operator_authority",
        ));
    }
    let verifier = state
        .workflow_authority_verifier
        .as_deref()
        .ok_or_else(|| {
            operator_error(
                WorkflowErrorCode::AuthorityUnavailable,
                "Workflow operator authority verifier is not configured",
                "configure_workflow_authority_verifier",
            )
        })?;
    let grant = verifier.verify(&request, credential).map_err(|message| {
        operator_error(
            WorkflowErrorCode::AuthorizationDenied,
            message,
            "request_workflow_operator_authority",
        )
    })?;
    if grant.actor_id.trim().is_empty() || grant.authority_id.trim().is_empty() {
        return Err(operator_error(
            WorkflowErrorCode::AuthorizationDenied,
            "Workflow authority verifier returned an incomplete identity",
            "repair_workflow_authority_provider",
        ));
    }
    Ok(VerifiedWorkflowAuthority {
        actor_id: grant.actor_id,
        authority_id: grant.authority_id,
        request,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowOperatorResult {
    pub protocol: String,
    pub disposition: WorkflowTransitionDisposition,
    pub intervention: WorkflowInterventionInspection,
    pub next_actions: Vec<String>,
}

pub(super) fn workflow_control_router() -> OpenApiRouter<ServiceRuntimeState> {
    OpenApiRouter::new()
        .routes(routes!(dry_run_workflow_operator_action))
        .routes(routes!(apply_workflow_operator_action_http))
}

#[utoipa::path(
    post,
    path = "/runtime/workflows/instances/{instance_id}/operator-actions/{action}/dry-run",
    params(
        ("instance_id" = String, Path, description = "Stable Workflow Instance identity"),
        ("action" = String, Path, description = "pause, resume, retry, cancel, terminate, or intervene")
    ),
    request_body = WorkflowOperatorPlanRequest,
    responses(
        (status = 200, body = WorkflowOperatorPlan),
        (status = 400, body = platform_http::ErrorResponse, content_type = "application/problem+json"),
        (status = 404, body = platform_http::ErrorResponse, content_type = "application/problem+json"),
        (status = 409, body = platform_http::ErrorResponse, content_type = "application/problem+json"),
        (status = 503, body = platform_http::ErrorResponse, content_type = "application/problem+json")
    ),
    tag = "service-runtime"
)]
async fn dry_run_workflow_operator_action(
    State(state): State<ServiceRuntimeState>,
    Path((instance_id, action)): Path<(String, String)>,
    request: Result<Json<WorkflowOperatorPlanRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<WorkflowOperatorPlan>, WorkflowApiError> {
    let action = parse_action(&action)?;
    let Json(request) = request.map_err(|_| {
        WorkflowApiError::invalid(
            WorkflowErrorCode::InvalidRequest,
            "Workflow operator dry run must match the v1 request contract",
        )
    })?;
    Ok(Json(
        plan_workflow_operator_action(
            &state,
            &instance_id,
            action,
            request.selected_step_id.as_deref(),
        )
        .await?,
    ))
}

#[utoipa::path(
    post,
    path = "/runtime/workflows/instances/{instance_id}/operator-actions/{action}",
    params(
        ("instance_id" = String, Path, description = "Stable Workflow Instance identity"),
        ("action" = String, Path, description = "pause, resume, retry, cancel, terminate, or intervene"),
        ("authorization" = String, Header, description = "Bearer credential verified by the deployment-owned Workflow authority provider")
    ),
    request_body = WorkflowOperatorApplyRequest,
    responses(
        (status = 200, body = WorkflowOperatorResult),
        (status = 400, body = platform_http::ErrorResponse, content_type = "application/problem+json"),
        (status = 401, body = platform_http::ErrorResponse, content_type = "application/problem+json"),
        (status = 403, body = platform_http::ErrorResponse, content_type = "application/problem+json"),
        (status = 404, body = platform_http::ErrorResponse, content_type = "application/problem+json"),
        (status = 409, body = platform_http::ErrorResponse, content_type = "application/problem+json"),
        (status = 503, body = platform_http::ErrorResponse, content_type = "application/problem+json")
    ),
    tag = "service-runtime"
)]
async fn apply_workflow_operator_action_http(
    State(state): State<ServiceRuntimeState>,
    Path((instance_id, action)): Path<(String, String)>,
    headers: HeaderMap,
    request: Result<Json<WorkflowOperatorApplyRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<WorkflowOperatorResult>, WorkflowApiError> {
    let action = parse_action(&action)?;
    let Json(request) = request.map_err(|_| {
        WorkflowApiError::invalid(
            WorkflowErrorCode::InvalidRequest,
            "Workflow operator action must match the v1 request contract",
        )
    })?;
    if request.plan_id.trim().is_empty() || request.reason.trim().is_empty() {
        return Err(WorkflowApiError::invalid(
            WorkflowErrorCode::InvalidRequest,
            "Workflow operator plan identity and reason must not be empty",
        ));
    }
    let credential = bearer_credential(&headers)?;
    let authority_request = WorkflowAuthorityRequest {
        action,
        required_authority: action.required_authority().to_owned(),
        plan_id: request.plan_id.clone(),
        service_id: state.identity.service_id.clone(),
        instance_id: instance_id.clone(),
        approval_boundary: action.approval_boundary(),
    };
    let authority = verify_workflow_authority(&state, authority_request, credential)?;
    Ok(Json(
        apply_workflow_operator_action(
            &state,
            &instance_id,
            action,
            request.selected_step_id.as_deref(),
            &request.plan_id,
            &request.reason,
            &authority,
        )
        .await?,
    ))
}

fn parse_action(action: &str) -> Result<WorkflowOperatorAction, WorkflowApiError> {
    action.parse().map_err(|()| {
        WorkflowApiError::invalid(
            WorkflowErrorCode::InvalidRequest,
            "Workflow operator action must be `pause`, `resume`, `retry`, `cancel`, `terminate`, or `intervene`",
        )
    })
}

fn bearer_credential(headers: &HeaderMap) -> Result<&str, WorkflowApiError> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            operator_error(
                WorkflowErrorCode::AuthorityRequired,
                "Workflow operator action requires an Authorization Bearer credential",
                "provide_workflow_operator_authority",
            )
        })
}

async fn plan_workflow_operator_action(
    state: &ServiceRuntimeState,
    instance_id: &str,
    action: WorkflowOperatorAction,
    selected_step_id: Option<&str>,
) -> Result<WorkflowOperatorPlan, WorkflowApiError> {
    if instance_id.trim().is_empty() {
        return Err(WorkflowApiError::invalid(
            WorkflowErrorCode::InvalidRequest,
            "Workflow Instance identity must not be empty",
        ));
    }
    let instance = load_instance(state, instance_id).await?;
    build_operator_plan(&instance, action, selected_step_id)
}

fn build_operator_plan(
    instance: &WorkflowInstance,
    action: WorkflowOperatorAction,
    selected_step_id: Option<&str>,
) -> Result<WorkflowOperatorPlan, WorkflowApiError> {
    if !matches!(
        action,
        WorkflowOperatorAction::Retry | WorkflowOperatorAction::Intervene
    ) && selected_step_id.is_some()
    {
        return Err(WorkflowApiError::invalid(
            WorkflowErrorCode::InvalidRequest,
            "Only retry and intervention plans may select a Workflow step",
        ));
    }
    let selected_step = selected_workflow_step(instance, selected_step_id)?;
    let available = workflow_available_actions(instance, selected_step.as_ref());
    if !available.contains(&action) {
        let next_action = if instance.control.state == WorkflowControlState::Paused {
            "plan_workflow_resume"
        } else {
            "inspect_available_workflow_actions"
        };
        return Err(operator_error(
            WorkflowErrorCode::ActionNotEligible,
            format!(
                "Workflow action `{}` is not eligible for instance `{}` in its current state",
                action.as_str(),
                instance.instance_id
            ),
            next_action,
        ));
    }
    if action == WorkflowOperatorAction::Retry && selected_step.is_none() {
        return Err(WorkflowApiError::invalid(
            WorkflowErrorCode::InvalidRequest,
            "Workflow retry requires one selected failed step",
        ));
    }
    let compensation_required = action == WorkflowOperatorAction::Cancel
        && (instance.state == WorkflowInstanceState::Compensating
            || instance
                .effects
                .iter()
                .any(|effect| effect.state == crate::WorkflowEffectState::Completed));
    let prior_state = state_snapshot(instance);
    let resulting_state = WorkflowOperatorStateSnapshot {
        execution_state: match action {
            WorkflowOperatorAction::Retry => WorkflowInstanceState::Running,
            WorkflowOperatorAction::Cancel if compensation_required => {
                WorkflowInstanceState::Compensating
            }
            WorkflowOperatorAction::Cancel => WorkflowInstanceState::Cancelled,
            WorkflowOperatorAction::Terminate => WorkflowInstanceState::Terminated,
            WorkflowOperatorAction::Pause
            | WorkflowOperatorAction::Resume
            | WorkflowOperatorAction::Intervene => instance.state,
        },
        control_state: match action {
            WorkflowOperatorAction::Pause => WorkflowControlState::Paused,
            WorkflowOperatorAction::Resume
            | WorkflowOperatorAction::Retry
            | WorkflowOperatorAction::Cancel
            | WorkflowOperatorAction::Terminate => WorkflowControlState::Active,
            WorkflowOperatorAction::Intervene => instance.control.state,
        },
        control_revision: instance.control.revision + 1,
    };
    let pending_work = workflow_pending_work(instance);
    let affected_resources = WorkflowOperatorAffectedResources {
        instance_id: instance.instance_id.clone(),
        selected_step_id: selected_step.as_ref().map(|step| step.step_id.clone()),
        affected_step_ids: instance
            .steps
            .iter()
            .filter(|step| step.state != WorkflowStepState::Completed)
            .map(|step| step.step_id.clone())
            .collect(),
        pending_work_ids: pending_work
            .iter()
            .map(|work| work.resource_id.clone())
            .collect(),
        timer_ids: instance
            .steps
            .iter()
            .flat_map(|step| step.timers.iter().map(|timer| timer.timer_id.clone()))
            .collect(),
        attempt_ids: instance
            .steps
            .iter()
            .flat_map(|step| {
                step.attempts
                    .iter()
                    .map(|attempt| attempt.attempt_id.clone())
            })
            .collect(),
        completed_step_ids: instance
            .steps
            .iter()
            .filter(|step| step.state == WorkflowStepState::Completed)
            .map(|step| step.step_id.clone())
            .collect(),
        child_workflow_ids: instance
            .steps
            .iter()
            .filter_map(|step| step.child_workflow.as_ref())
            .map(|child| {
                child
                    .instance_id
                    .clone()
                    .unwrap_or_else(|| child.link_id.clone())
            })
            .collect(),
        compensation_ids: workflow_compensation_ids(instance),
        irreversible_effects: instance
            .steps
            .iter()
            .filter(|step| {
                step.state == WorkflowStepState::Completed
                    && !instance
                        .effects
                        .iter()
                        .any(|effect| effect.step_id == step.step_id)
            })
            .filter_map(|step| {
                step.transition_id.as_ref().map(|transition_id| {
                    WorkflowIrreversibleEffectInspection {
                        step_id: step.step_id.clone(),
                        definition_step_name: step.definition_step_name.clone(),
                        transition_id: transition_id.clone(),
                        reason: "no_declared_compensation".to_owned(),
                    }
                })
            })
            .collect(),
        in_flight_claim_ids: instance
            .steps
            .iter()
            .flat_map(|step| {
                step.timers
                    .iter()
                    .filter(|timer| timer.state == WorkflowTimerState::Claimed)
                    .map(|timer| timer.timer_id.clone())
            })
            .collect(),
    };
    let mut preserved_state = vec![
        "completed_steps".to_owned(),
        "workflow_step_identity".to_owned(),
        "attempt_history".to_owned(),
        "timer_history".to_owned(),
        "in_flight_claims".to_owned(),
        "story_causation_tenant_and_idempotency_context".to_owned(),
    ];
    if action == WorkflowOperatorAction::Terminate {
        preserved_state.push("completed_effects_without_cleanup_claims".to_owned());
    }
    let authorization = WorkflowOperatorAuthorization {
        status: WorkflowOperatorAuthorizationStatus::Required,
        required_authority: action.required_authority().to_owned(),
    };
    let approval_boundary = action.approval_boundary();
    let expected_terminal_state = match action {
        WorkflowOperatorAction::Cancel => Some(WorkflowInstanceState::Cancelled),
        WorkflowOperatorAction::Terminate => Some(WorkflowInstanceState::Terminated),
        _ => None,
    };
    let next_actions = vec![format!("authorize_and_apply_workflow_{}", action.as_str())];
    let selected_step_id = selected_step.map(|step| step.step_id);
    let plan_material = serde_json::json!({
        "protocol": WORKFLOW_OPERATOR_PLAN_PROTOCOL,
        "mutatesState": false,
        "action": action,
        "serviceId": instance.service_id,
        "instanceId": instance.instance_id,
        "selectedStepId": selected_step_id,
        "definitionVersion": instance.definition.version,
        "instanceUpdatedAt": instance.updated_at,
        "priorState": prior_state,
        "resultingState": resulting_state,
        "expectedTerminalState": expected_terminal_state,
        "affectedResources": affected_resources,
        "preservedState": preserved_state,
        "authorization": authorization,
        "approvalBoundary": approval_boundary,
        "nextActions": next_actions,
    });
    let bytes = serde_json::to_vec(&plan_material)
        .map_err(|error| WorkflowApiError::stored_state(error.to_string()))?;
    let plan_id = format!("workflow_operator_sha256_{}", sha256_hex(&bytes));
    Ok(WorkflowOperatorPlan {
        protocol: WORKFLOW_OPERATOR_PLAN_PROTOCOL.to_owned(),
        plan_id,
        mutates_state: false,
        action,
        service_id: instance.service_id.clone(),
        instance_id: instance.instance_id.clone(),
        selected_step_id,
        definition_version: instance.definition.version.clone(),
        prior_state,
        resulting_state,
        expected_terminal_state,
        affected_resources,
        preserved_state,
        authorization,
        approval_boundary,
        next_actions,
    })
}

fn workflow_compensation_ids(instance: &WorkflowInstance) -> Vec<String> {
    let mut compensation_ids = instance
        .compensations
        .iter()
        .map(|compensation| compensation.compensation_id.clone())
        .collect::<Vec<_>>();
    for effect in instance
        .effects
        .iter()
        .filter(|effect| effect.state == crate::WorkflowEffectState::Completed)
    {
        let compensation_id = format!(
            "{}:compensation:{}",
            effect.effect_id, effect.compensation_name
        );
        if !compensation_ids.contains(&compensation_id) {
            compensation_ids.push(compensation_id);
        }
    }
    compensation_ids
}

fn state_snapshot(instance: &WorkflowInstance) -> WorkflowOperatorStateSnapshot {
    WorkflowOperatorStateSnapshot {
        execution_state: instance.state,
        control_state: instance.control.state,
        control_revision: instance.control.revision,
    }
}

#[allow(clippy::too_many_arguments)]
async fn apply_workflow_operator_action(
    state: &ServiceRuntimeState,
    instance_id: &str,
    action: WorkflowOperatorAction,
    selected_step_id: Option<&str>,
    plan_id: &str,
    reason: &str,
    authority: &VerifiedWorkflowAuthority,
) -> Result<WorkflowOperatorResult, WorkflowApiError> {
    if authority.request.action != action
        || authority.request.plan_id != plan_id
        || authority.request.service_id != state.identity.service_id
        || authority.request.instance_id != instance_id
        || authority.request.required_authority != action.required_authority()
        || authority.request.approval_boundary != action.approval_boundary()
    {
        return Err(operator_error(
            WorkflowErrorCode::AuthorizationDenied,
            "Verified Workflow authority does not authorize this exact action plan",
            "request_workflow_operator_authority",
        ));
    }
    let pool = state
        .store()
        .map_err(|error| WorkflowApiError::store(error.public_message))?;
    let mut transaction = pool.begin().await.map_err(|error| {
        WorkflowApiError::store(format!("Could not begin Workflow operator action: {error}"))
    })?;
    if let Some(existing) =
        intervention_by_plan_in_tx(&mut transaction, instance_id, action, plan_id).await?
    {
        transaction.commit().await.map_err(|error| {
            WorkflowApiError::store(format!(
                "Could not commit duplicate Workflow operator action: {error}"
            ))
        })?;
        return Ok(result_from_intervention(
            WorkflowTransitionDisposition::Duplicate,
            existing,
        ));
    }
    sqlx::query(
        "select instance_id from platform.service_workflow_instances where service_id = $1 and instance_id = $2 for update",
    )
    .bind(&state.identity.service_id)
    .bind(instance_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| WorkflowApiError::store(format!("Could not lock Workflow Instance: {error}")))?
    .ok_or_else(|| WorkflowApiError {
        code: WorkflowErrorCode::InstanceNotFound,
        message: format!("Workflow Instance `{instance_id}` was not found in this Service Store"),
        next_actions: vec!["verify_workflow_instance_identity".to_owned()],
    })?;
    if let Some(step_id) = selected_step_id {
        sqlx::query(
            "select step_id from platform.service_workflow_steps where instance_id = $1 and step_id = $2 for update",
        )
        .bind(instance_id)
        .bind(step_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| WorkflowApiError::store(format!("Could not lock Workflow step: {error}")))?
        .ok_or_else(|| WorkflowApiError {
            code: WorkflowErrorCode::StepNotFound,
            message: format!("Workflow step `{step_id}` was not found in instance `{instance_id}`"),
            next_actions: vec!["inspect_workflow_steps".to_owned()],
        })?;
    }
    let instance = load_instance_in_tx(state, &mut transaction, instance_id)
        .await
        .map_err(WorkflowApiError::from)?;
    let current_plan = build_operator_plan(&instance, action, selected_step_id)?;
    if current_plan.plan_id != plan_id {
        return Err(operator_error(
            WorkflowErrorCode::StalePlan,
            "Workflow state changed after the operator plan was created",
            "plan_workflow_action_again",
        ));
    }
    let now = super::recovery::workflow_now(state);
    let intervention_id = stable_intervention_id(plan_id);
    let (attempt_transition_id, next_action) = match action {
        WorkflowOperatorAction::Pause => {
            update_control_state(
                &mut transaction,
                instance_id,
                WorkflowControlState::Active,
                WorkflowControlState::Paused,
                instance.control.revision,
                now,
            )
            .await?;
            (None, "review_paused_workflow_state".to_owned())
        }
        WorkflowOperatorAction::Resume => {
            update_control_state(
                &mut transaction,
                instance_id,
                WorkflowControlState::Paused,
                WorkflowControlState::Active,
                instance.control.revision,
                now,
            )
            .await?;
            (None, "continue_recorded_workflow_state".to_owned())
        }
        WorkflowOperatorAction::Retry => {
            let step = selected_workflow_step(&instance, selected_step_id)?
                .expect("retry plan validation requires a selected step");
            let transition_id = format!("{}:operator-retry:{intervention_id}", step.step_id);
            schedule_operator_retry(&mut transaction, &instance, &step, &transition_id, now)
                .await?;
            (
                Some(transition_id),
                "dispatch_selected_step_retry".to_owned(),
            )
        }
        WorkflowOperatorAction::Cancel => {
            apply_cancel(
                state,
                &mut transaction,
                &instance,
                &current_plan,
                &intervention_id,
                now,
            )
            .await?;
            let next_action = if current_plan.resulting_state.execution_state
                == WorkflowInstanceState::Compensating
            {
                "execute_next_workflow_compensation"
            } else {
                "inspect_cancelled_workflow"
            };
            (None, next_action.to_owned())
        }
        WorkflowOperatorAction::Terminate => {
            apply_terminate(&mut transaction, &instance, &current_plan, now).await?;
            (None, "inspect_terminated_workflow".to_owned())
        }
        WorkflowOperatorAction::Intervene => {
            record_human_intervention_state_change(
                &mut transaction,
                instance_id,
                instance.control.revision,
                now,
            )
            .await?;
            (None, "follow_recorded_human_intervention".to_owned())
        }
    };
    let prior_state = current_plan.prior_state;
    let resulting_state = current_plan.resulting_state;
    let intervention = WorkflowInterventionInspection {
        intervention_id,
        action,
        plan_id: plan_id.to_owned(),
        step_id: selected_step_id.map(ToOwned::to_owned),
        actor_id: authority.actor_id.clone(),
        authority_id: authority.authority_id.clone(),
        reason: reason.to_owned(),
        tenant_scope: instance.tenant_scope.clone(),
        affected_resources: current_plan.affected_resources,
        approval_boundary: current_plan.approval_boundary,
        expected_terminal_state: current_plan.expected_terminal_state,
        prior_state,
        resulting_state,
        next_action,
        attempt_transition_id,
        recorded_at: now,
    };
    insert_intervention(&mut transaction, instance_id, &intervention).await?;
    let evidence_status = match action {
        WorkflowOperatorAction::Pause => "paused",
        WorkflowOperatorAction::Resume => "running",
        WorkflowOperatorAction::Retry => "retry_scheduled",
        WorkflowOperatorAction::Cancel | WorkflowOperatorAction::Terminate => {
            workflow_instance_state_as_str(intervention.resulting_state.execution_state)
        }
        WorkflowOperatorAction::Intervene => "intervention_recorded",
    };
    let evidence_attempt = selected_step_id
        .and_then(|step_id| instance.steps.iter().find(|step| step.step_id == step_id))
        .map_or(1, |step| step.attempt_count.saturating_add(1).max(1));
    append_persisted_workflow_story_segment_in_tx(
        state,
        &mut transaction,
        instance_id,
        selected_step_id,
        None,
        Some(&intervention.intervention_id),
        &format!(
            "workflow:{instance_id}:intervention:{}",
            intervention.intervention_id
        ),
        &format!("workflow.instance.{}", action.as_str()),
        "lenso.workflow-operator-result",
        "v1",
        evidence_status,
        evidence_attempt,
        Some(plan_id),
        now,
    )
    .await
    .map_err(|error| WorkflowApiError::store(error.public_message))?;
    transaction.commit().await.map_err(|error| {
        WorkflowApiError::store(format!(
            "Could not commit Workflow operator action: {error}"
        ))
    })?;
    Ok(result_from_intervention(
        WorkflowTransitionDisposition::Applied,
        intervention,
    ))
}

async fn apply_cancel(
    state: &ServiceRuntimeState,
    transaction: &mut Transaction<'_, Postgres>,
    instance: &WorkflowInstance,
    plan: &WorkflowOperatorPlan,
    intervention_id: &str,
    now: DateTime<Utc>,
) -> Result<(), WorkflowApiError> {
    stop_future_ordinary_work(transaction, &instance.instance_id, "cancelled", now).await?;
    if instance.state != WorkflowInstanceState::Compensating {
        crate::select_workflow_compensations_for_cancel_in_tx(
            state,
            transaction,
            &instance.instance_id,
            intervention_id,
            now,
        )
        .await
        .map_err(WorkflowApiError::from)?;
    }
    let terminal_operation = WorkflowTerminalOperationInspection {
        action: WorkflowOperatorAction::Cancel,
        plan_id: plan.plan_id.clone(),
        expected_terminal_state: WorkflowInstanceState::Cancelled,
        compensation_required: plan.resulting_state.execution_state
            == WorkflowInstanceState::Compensating,
        cleanup_reported: false,
        requested_at: now,
    };
    update_terminal_instance(
        transaction,
        instance,
        plan.resulting_state.execution_state,
        Some("cancelled"),
        &terminal_operation,
        now,
    )
    .await
}

async fn apply_terminate(
    transaction: &mut Transaction<'_, Postgres>,
    instance: &WorkflowInstance,
    plan: &WorkflowOperatorPlan,
    now: DateTime<Utc>,
) -> Result<(), WorkflowApiError> {
    stop_future_ordinary_work(transaction, &instance.instance_id, "terminated", now).await?;
    let terminal_operation = WorkflowTerminalOperationInspection {
        action: WorkflowOperatorAction::Terminate,
        plan_id: plan.plan_id.clone(),
        expected_terminal_state: WorkflowInstanceState::Terminated,
        compensation_required: false,
        cleanup_reported: false,
        requested_at: now,
    };
    update_terminal_instance(
        transaction,
        instance,
        WorkflowInstanceState::Terminated,
        None,
        &terminal_operation,
        now,
    )
    .await
}

async fn stop_future_ordinary_work(
    transaction: &mut Transaction<'_, Postgres>,
    instance_id: &str,
    step_state: &str,
    now: DateTime<Utc>,
) -> Result<(), WorkflowApiError> {
    sqlx::query(
        r#"
        update platform.service_workflow_timers
        set state = 'cancelled', completed_at = $2, updated_at = $2
        where instance_id = $1 and state in ('pending', 'claimed')
        "#,
    )
    .bind(instance_id)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| WorkflowApiError::store(format!("Could not stop Workflow timers: {error}")))?;
    sqlx::query(
        r#"
        update platform.service_workflow_steps
        set state = $2, next_attempt_at = null, updated_at = $3
        where instance_id = $1 and state <> 'completed'
        "#,
    )
    .bind(instance_id)
    .bind(step_state)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| WorkflowApiError::store(format!("Could not stop Workflow steps: {error}")))?;
    Ok(())
}

async fn update_terminal_instance(
    transaction: &mut Transaction<'_, Postgres>,
    instance: &WorkflowInstance,
    state: WorkflowInstanceState,
    terminal_intent: Option<&str>,
    terminal_operation: &WorkflowTerminalOperationInspection,
    now: DateTime<Utc>,
) -> Result<(), WorkflowApiError> {
    let terminal_evidence = serde_json::to_value(terminal_operation)
        .map_err(|error| WorkflowApiError::stored_state(error.to_string()))?;
    let updated = sqlx::query(
        r#"
        update platform.service_workflow_instances
        set state = $2, control_state = 'active',
            control_revision = control_revision + 1, paused_at = null,
            terminal_intent = $3, terminal_evidence = $4,
            failure_evidence = null, terminal_transition_id = null,
            updated_at = $5
        where instance_id = $1 and control_revision = $6
        "#,
    )
    .bind(&instance.instance_id)
    .bind(workflow_instance_state_as_str(state))
    .bind(terminal_intent)
    .bind(terminal_evidence)
    .bind(now)
    .bind(i64::try_from(instance.control.revision).unwrap_or(i64::MAX))
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowApiError::store(format!("Could not apply Workflow terminal state: {error}"))
    })?;
    if updated.rows_affected() != 1 {
        return Err(operator_error(
            WorkflowErrorCode::StalePlan,
            "Workflow state changed while applying the terminal operation",
            "plan_workflow_action_again",
        ));
    }
    Ok(())
}

async fn record_human_intervention_state_change(
    transaction: &mut Transaction<'_, Postgres>,
    instance_id: &str,
    revision: u64,
    now: DateTime<Utc>,
) -> Result<(), WorkflowApiError> {
    let updated = sqlx::query(
        r#"
        update platform.service_workflow_instances
        set control_revision = control_revision + 1, updated_at = $3
        where instance_id = $1 and control_revision = $2
        "#,
    )
    .bind(instance_id)
    .bind(i64::try_from(revision).unwrap_or(i64::MAX))
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowApiError::store(format!("Could not record Workflow intervention: {error}"))
    })?;
    if updated.rows_affected() != 1 {
        return Err(operator_error(
            WorkflowErrorCode::StalePlan,
            "Workflow state changed while recording the human intervention",
            "plan_workflow_action_again",
        ));
    }
    Ok(())
}

const fn workflow_instance_state_as_str(state: WorkflowInstanceState) -> &'static str {
    match state {
        WorkflowInstanceState::Running => "running",
        WorkflowInstanceState::Completed => "completed",
        WorkflowInstanceState::Failed => "failed",
        WorkflowInstanceState::Compensating => "compensating",
        WorkflowInstanceState::Compensated => "compensated",
        WorkflowInstanceState::CompensationFailed => "compensation_failed",
        WorkflowInstanceState::Cancelled => "cancelled",
        WorkflowInstanceState::Terminated => "terminated",
    }
}

fn stable_intervention_id(plan_id: &str) -> String {
    format!("workflow_intervention_{}", sha256_hex(plan_id.as_bytes()))
}

async fn update_control_state(
    transaction: &mut Transaction<'_, Postgres>,
    instance_id: &str,
    prior: WorkflowControlState,
    resulting: WorkflowControlState,
    revision: u64,
    now: DateTime<Utc>,
) -> Result<(), WorkflowApiError> {
    let updated = sqlx::query(
        r#"
        update platform.service_workflow_instances
        set control_state = $2, control_revision = control_revision + 1,
            paused_at = case when $2 = 'paused' then $5 else null end,
            updated_at = $5
        where instance_id = $1 and control_state = $3 and control_revision = $4
        "#,
    )
    .bind(instance_id)
    .bind(resulting.as_str())
    .bind(prior.as_str())
    .bind(i64::try_from(revision).unwrap_or(i64::MAX))
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowApiError::store(format!("Could not update Workflow control state: {error}"))
    })?;
    if updated.rows_affected() != 1 {
        return Err(operator_error(
            WorkflowErrorCode::StalePlan,
            "Workflow control state changed while applying the operator plan",
            "plan_workflow_action_again",
        ));
    }
    Ok(())
}

async fn schedule_operator_retry(
    transaction: &mut Transaction<'_, Postgres>,
    instance: &WorkflowInstance,
    step: &WorkflowStepInspection,
    attempt_transition_id: &str,
    now: DateTime<Utc>,
) -> Result<(), WorkflowApiError> {
    let attempt_number = step.attempt_count.checked_add(1).ok_or_else(|| {
        WorkflowApiError::stored_state("Workflow attempt count cannot be incremented")
    })?;
    let timer_id = format!(
        "workflow_timer_operator_{}",
        sha256_hex(attempt_transition_id.as_bytes())
    );
    sqlx::query(
        r#"
        insert into platform.service_workflow_timers (
            timer_id, instance_id, step_id, kind, attempt_number,
            transition_id, attempt_transition_id, due_at, state,
            created_at, updated_at
        ) values ($1, $2, $3, 'retry', $4, $5, $5, $6, 'pending', $6, $6)
        "#,
    )
    .bind(timer_id)
    .bind(&instance.instance_id)
    .bind(&step.step_id)
    .bind(i32::try_from(attempt_number).unwrap_or(i32::MAX))
    .bind(attempt_transition_id)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowApiError::store(format!(
            "Could not schedule selected Workflow step retry: {error}"
        ))
    })?;
    let step_updated = sqlx::query(
        r#"
        update platform.service_workflow_steps
        set state = 'pending', next_attempt_at = $3, exhausted_at = null,
            updated_at = $3
        where instance_id = $1 and step_id = $2 and state = 'exhausted'
        "#,
    )
    .bind(&instance.instance_id)
    .bind(&step.step_id)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowApiError::store(format!("Could not reopen selected Workflow step: {error}"))
    })?;
    let instance_updated = sqlx::query(
        r#"
        update platform.service_workflow_instances
        set state = 'running', failure_evidence = null, terminal_transition_id = null,
            control_revision = control_revision + 1, updated_at = $2
        where instance_id = $1 and state = 'failed' and control_state = 'active'
        "#,
    )
    .bind(&instance.instance_id)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowApiError::store(format!("Could not reopen Workflow Instance: {error}"))
    })?;
    if step_updated.rows_affected() != 1 || instance_updated.rows_affected() != 1 {
        return Err(operator_error(
            WorkflowErrorCode::StalePlan,
            "Workflow failure state changed while applying the retry plan",
            "plan_workflow_action_again",
        ));
    }
    Ok(())
}

async fn insert_intervention(
    transaction: &mut Transaction<'_, Postgres>,
    instance_id: &str,
    intervention: &WorkflowInterventionInspection,
) -> Result<(), WorkflowApiError> {
    let prior_state = serde_json::to_value(&intervention.prior_state)
        .map_err(|error| WorkflowApiError::stored_state(error.to_string()))?;
    let resulting_state = serde_json::to_value(&intervention.resulting_state)
        .map_err(|error| WorkflowApiError::stored_state(error.to_string()))?;
    let tenant_scope = intervention
        .tenant_scope
        .as_ref()
        .map(serde_json::to_value)
        .transpose()
        .map_err(|error| WorkflowApiError::stored_state(error.to_string()))?;
    let affected_resources = serde_json::to_value(&intervention.affected_resources)
        .map_err(|error| WorkflowApiError::stored_state(error.to_string()))?;
    sqlx::query(
        r#"
        insert into platform.service_workflow_interventions (
            intervention_id, instance_id, step_id, action, plan_id,
            actor_id, authority_id, reason, tenant_scope, affected_resources,
            approval_boundary, expected_terminal_state, prior_state,
            resulting_state, next_action, attempt_transition_id, recorded_at
        ) values (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
            $13, $14, $15, $16, $17
        )
        "#,
    )
    .bind(&intervention.intervention_id)
    .bind(instance_id)
    .bind(&intervention.step_id)
    .bind(intervention.action.as_str())
    .bind(&intervention.plan_id)
    .bind(&intervention.actor_id)
    .bind(&intervention.authority_id)
    .bind(&intervention.reason)
    .bind(tenant_scope)
    .bind(affected_resources)
    .bind(intervention.approval_boundary.as_str())
    .bind(
        intervention
            .expected_terminal_state
            .map(workflow_instance_state_as_str),
    )
    .bind(prior_state)
    .bind(resulting_state)
    .bind(&intervention.next_action)
    .bind(&intervention.attempt_transition_id)
    .bind(intervention.recorded_at)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowApiError::store(format!("Could not record Workflow intervention: {error}"))
    })?;
    Ok(())
}

async fn intervention_by_plan_in_tx(
    transaction: &mut Transaction<'_, Postgres>,
    instance_id: &str,
    action: WorkflowOperatorAction,
    plan_id: &str,
) -> Result<Option<WorkflowInterventionInspection>, WorkflowApiError> {
    let row = sqlx::query_as::<_, InterventionRow>(
        r#"
        select intervention_id, action, plan_id, step_id, actor_id, authority_id,
               reason, tenant_scope, affected_resources, approval_boundary,
               expected_terminal_state, prior_state, resulting_state, next_action,
               attempt_transition_id, recorded_at
        from platform.service_workflow_interventions
        where instance_id = $1 and action = $2 and plan_id = $3
        "#,
    )
    .bind(instance_id)
    .bind(action.as_str())
    .bind(plan_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowApiError::store(format!("Could not inspect Workflow intervention: {error}"))
    })?;
    row.map(intervention_from_row).transpose()
}

fn result_from_intervention(
    disposition: WorkflowTransitionDisposition,
    intervention: WorkflowInterventionInspection,
) -> WorkflowOperatorResult {
    WorkflowOperatorResult {
        protocol: WORKFLOW_OPERATOR_RESULT_PROTOCOL.to_owned(),
        disposition,
        next_actions: vec![intervention.next_action.clone()],
        intervention,
    }
}

fn operator_error(
    code: WorkflowErrorCode,
    message: impl Into<String>,
    next_action: impl Into<String>,
) -> WorkflowApiError {
    WorkflowApiError {
        code,
        message: message.into(),
        next_actions: vec![next_action.into()],
    }
}
