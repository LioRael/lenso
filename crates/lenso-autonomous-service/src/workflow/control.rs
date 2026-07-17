use super::{
    WorkflowApiError, WorkflowErrorCode, WorkflowInstance, WorkflowInstanceState,
    WorkflowMutationError, WorkflowStepInspection, WorkflowStepState, WorkflowTimerState,
    load_instance, load_instance_in_tx, sha256_hex,
};
use crate::{ServiceRuntimeState, WorkflowTransitionDisposition};
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
}

impl WorkflowOperatorAction {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Retry => "retry",
        }
    }

    const fn required_authority(self) -> &'static str {
        match self {
            Self::Pause => "workflow_instance_pause",
            Self::Resume => "workflow_instance_resume",
            Self::Retry => "workflow_step_retry",
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
            _ => Err(()),
        }
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
           reason, prior_state, resulting_state, next_action,
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
    if instance.control.state == WorkflowControlState::Paused {
        return vec![WorkflowOperatorAction::Resume];
    }
    let mut actions = Vec::new();
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowOperatorApprovalBoundary {
    WorkflowInstanceControl,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowOperatorAffectedResources {
    pub instance_id: String,
    pub selected_step_id: Option<String>,
    pub pending_work_ids: Vec<String>,
    pub timer_ids: Vec<String>,
    pub attempt_ids: Vec<String>,
    pub completed_step_ids: Vec<String>,
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
        ("action" = String, Path, description = "pause, resume, or retry")
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
        ("action" = String, Path, description = "pause, resume, or retry"),
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
            "Workflow operator action must be `pause`, `resume`, or `retry`",
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
    if action != WorkflowOperatorAction::Retry && selected_step_id.is_some() {
        return Err(WorkflowApiError::invalid(
            WorkflowErrorCode::InvalidRequest,
            "Pause and resume plans do not select a Workflow step",
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
    let prior_state = state_snapshot(instance);
    let resulting_state = WorkflowOperatorStateSnapshot {
        execution_state: if action == WorkflowOperatorAction::Retry {
            WorkflowInstanceState::Running
        } else {
            instance.state
        },
        control_state: match action {
            WorkflowOperatorAction::Pause => WorkflowControlState::Paused,
            WorkflowOperatorAction::Resume | WorkflowOperatorAction::Retry => {
                WorkflowControlState::Active
            }
        },
        control_revision: instance.control.revision + 1,
    };
    let pending_work = workflow_pending_work(instance);
    let affected_resources = WorkflowOperatorAffectedResources {
        instance_id: instance.instance_id.clone(),
        selected_step_id: selected_step.as_ref().map(|step| step.step_id.clone()),
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
    let preserved_state = vec![
        "completed_steps".to_owned(),
        "workflow_step_identity".to_owned(),
        "attempt_history".to_owned(),
        "timer_history".to_owned(),
        "in_flight_claims".to_owned(),
        "story_causation_tenant_and_idempotency_context".to_owned(),
    ];
    let authorization = WorkflowOperatorAuthorization {
        status: WorkflowOperatorAuthorizationStatus::Required,
        required_authority: action.required_authority().to_owned(),
    };
    let approval_boundary = WorkflowOperatorApprovalBoundary::WorkflowInstanceControl;
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
        affected_resources,
        preserved_state,
        authorization,
        approval_boundary,
        next_actions,
    })
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
        prior_state,
        resulting_state,
        next_action,
        attempt_transition_id,
        recorded_at: now,
    };
    insert_intervention(&mut transaction, instance_id, &intervention).await?;
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
    sqlx::query(
        r#"
        insert into platform.service_workflow_interventions (
            intervention_id, instance_id, step_id, action, plan_id,
            actor_id, authority_id, reason, prior_state, resulting_state,
            next_action, attempt_transition_id, recorded_at
        ) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
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
               reason, prior_state, resulting_state, next_action,
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
