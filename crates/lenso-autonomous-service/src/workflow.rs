use crate::{
    ServiceEventPublisher, ServiceRuntimeState, StorySegmentRecord, StorySegmentWorkflow,
    append_persisted_workflow_story_segment_in_tx, append_story_segment_in_tx,
    append_worker_story_segment_in_tx,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use lenso_contracts::WorkflowDefinition;
use lenso_service::{
    CausationContext, CommonContextRequirement, EventContent, EventContext, EventContractArtifact,
    EventEnvelope, ServicePrincipal, ServiceTenancyMode,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use sqlx::{FromRow, Postgres, Transaction};
use std::fmt::Write as _;
use thiserror::Error;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

mod control;
mod evolution;
pub(crate) mod recovery;
pub use control::*;
pub use evolution::*;
pub use recovery::*;

pub const WORKFLOW_START_RESULT_PROTOCOL: &str = "lenso.workflow-start-result.v1";
pub const WORKFLOW_INSPECTION_PROTOCOL: &str = "lenso.workflow-inspection.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowTenantScope {
    pub tenant_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowStoryContext {
    pub story_id: String,
    pub segment_id: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowStartRequest {
    pub definition_version: String,
    pub input: Value,
    pub story_context: WorkflowStoryContext,
    #[serde(default)]
    pub tenant_scope: Option<WorkflowTenantScope>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDefinitionIdentity {
    pub owner: String,
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowFailureEvidence {
    pub code: String,
    pub message: String,
    pub next_action: String,
}

impl WorkflowFailureEvidence {
    #[must_use]
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        next_action: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            next_action: next_action.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowParentInspection {
    pub instance_id: String,
    pub step_id: String,
    pub causation_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowChildState {
    Waiting,
    Completed,
    Failed,
    UnsupportedVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowChildInspection {
    pub link_id: String,
    pub start_id: String,
    pub definition: WorkflowDefinitionIdentity,
    pub instance_id: Option<String>,
    pub state: WorkflowChildState,
    pub completion_delivery_id: Option<String>,
    pub failure: Option<WorkflowFailureEvidence>,
    pub next_actions: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowInstanceState {
    Running,
    Completed,
    Failed,
    Compensating,
    Compensated,
    CompensationFailed,
}

impl WorkflowInstanceState {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "compensating" => Some(Self::Compensating),
            "compensated" => Some(Self::Compensated),
            "compensation_failed" => Some(Self::CompensationFailed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStepState {
    Pending,
    WaitingForChild,
    Completed,
    Exhausted,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowOutgoingWorkInspection {
    pub kind: String,
    pub consumer_id: String,
    pub event_id: String,
    pub contract_id: String,
    pub contract_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStepInspection {
    pub step_id: String,
    pub definition_step_name: String,
    pub position: u32,
    pub state: WorkflowStepState,
    pub transition_id: Option<String>,
    pub completed_at: Option<DateTime<Utc>>,
    pub outgoing_work: Option<WorkflowOutgoingWorkInspection>,
    pub attempt_count: u32,
    pub max_attempts: u32,
    pub retry_schedule_ms: Vec<u64>,
    pub next_attempt_at: Option<DateTime<Utc>>,
    pub latest_failure: Option<WorkflowStepFailureInspection>,
    pub exhausted_at: Option<DateTime<Utc>>,
    pub attempts: Vec<WorkflowStepAttemptInspection>,
    pub timers: Vec<WorkflowTimerInspection>,
    pub child_workflow: Option<WorkflowChildInspection>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowInstance {
    pub instance_id: String,
    pub service_id: String,
    pub definition: WorkflowDefinitionIdentity,
    pub state: WorkflowInstanceState,
    pub input: Value,
    pub result: Option<Value>,
    pub story_context: WorkflowStoryContext,
    pub tenant_scope: Option<WorkflowTenantScope>,
    pub parent: Option<WorkflowParentInspection>,
    pub failure: Option<WorkflowFailureEvidence>,
    pub control: WorkflowControlInspection,
    pub initial_step_id: String,
    pub steps: Vec<WorkflowStepInspection>,
    pub effects: Vec<crate::WorkflowEffectInspection>,
    pub compensations: Vec<crate::WorkflowCompensationInspection>,
    pub history: Vec<crate::WorkflowHistoryEntry>,
    pub interventions: Vec<WorkflowInterventionInspection>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStartResult {
    pub protocol: String,
    pub instance: WorkflowInstance,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowInspectionResult {
    pub protocol: String,
    pub instance: WorkflowInstance,
    pub selected_step: Option<WorkflowStepInspection>,
    pub pending_work: Vec<WorkflowPendingWorkInspection>,
    pub available_actions: Vec<WorkflowOperatorAction>,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowInspectionQuery {
    /// Selects one stable step identity for focused attempts, timers, and
    /// operator-action eligibility.
    pub step_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
pub enum WorkflowErrorCode {
    #[serde(rename = "workflow_invalid_request")]
    InvalidRequest,
    #[serde(rename = "workflow_definition_not_found")]
    DefinitionNotFound,
    #[serde(rename = "workflow_definition_version_not_found")]
    DefinitionVersionNotFound,
    #[serde(rename = "workflow_definition_version_unsupported")]
    DefinitionVersionUnsupported,
    #[serde(rename = "workflow_tenant_required")]
    TenantRequired,
    #[serde(rename = "workflow_tenant_incompatible")]
    TenantIncompatible,
    #[serde(rename = "workflow_instance_not_found")]
    InstanceNotFound,
    #[serde(rename = "workflow_context_required")]
    ContextRequired,
    #[serde(rename = "workflow_step_not_found")]
    StepNotFound,
    #[serde(rename = "workflow_transition_conflict")]
    TransitionConflict,
    #[serde(rename = "workflow_event_contract_not_declared")]
    EventContractNotDeclared,
    #[serde(rename = "workflow_child_link_not_found")]
    ChildLinkNotFound,
    #[serde(rename = "workflow_child_not_terminal")]
    ChildNotTerminal,
    #[serde(rename = "workflow_store_unavailable")]
    StoreUnavailable,
    #[serde(rename = "workflow_stored_state_invalid")]
    StoredStateInvalid,
    #[serde(rename = "workflow_action_not_eligible")]
    ActionNotEligible,
    #[serde(rename = "workflow_stale_plan")]
    StalePlan,
    #[serde(rename = "workflow_authority_required")]
    AuthorityRequired,
    #[serde(rename = "workflow_authority_unavailable")]
    AuthorityUnavailable,
    #[serde(rename = "workflow_authorization_denied")]
    AuthorizationDenied,
}

impl WorkflowErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "workflow_invalid_request",
            Self::DefinitionNotFound => "workflow_definition_not_found",
            Self::DefinitionVersionNotFound => "workflow_definition_version_not_found",
            Self::DefinitionVersionUnsupported => "workflow_definition_version_unsupported",
            Self::TenantRequired => "workflow_tenant_required",
            Self::TenantIncompatible => "workflow_tenant_incompatible",
            Self::InstanceNotFound => "workflow_instance_not_found",
            Self::ContextRequired => "workflow_context_required",
            Self::StepNotFound => "workflow_step_not_found",
            Self::TransitionConflict => "workflow_transition_conflict",
            Self::EventContractNotDeclared => "workflow_event_contract_not_declared",
            Self::ChildLinkNotFound => "workflow_child_link_not_found",
            Self::ChildNotTerminal => "workflow_child_not_terminal",
            Self::StoreUnavailable => "workflow_store_unavailable",
            Self::StoredStateInvalid => "workflow_stored_state_invalid",
            Self::ActionNotEligible => "workflow_action_not_eligible",
            Self::StalePlan => "workflow_stale_plan",
            Self::AuthorityRequired => "workflow_authority_required",
            Self::AuthorityUnavailable => "workflow_authority_unavailable",
            Self::AuthorizationDenied => "workflow_authorization_denied",
        }
    }
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct WorkflowMutationError {
    pub code: WorkflowErrorCode,
    pub message: String,
}

impl WorkflowMutationError {
    pub(crate) fn new(code: WorkflowErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub(crate) fn store(message: impl Into<String>) -> Self {
        Self::new(WorkflowErrorCode::StoreUnavailable, message)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowEventPublication {
    pub consumer_id: String,
    pub event_id: String,
    pub contract_id: String,
    pub contract_version: String,
    pub occurred_at: String,
    pub service_principal: ServicePrincipal,
    pub data: Value,
}

impl WorkflowEventPublication {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        consumer_id: impl Into<String>,
        event_id: impl Into<String>,
        contract_id: impl Into<String>,
        contract_version: impl Into<String>,
        occurred_at: impl Into<String>,
        service_principal: ServicePrincipal,
        data: Value,
    ) -> Self {
        Self {
            consumer_id: consumer_id.into(),
            event_id: event_id.into(),
            contract_id: contract_id.into(),
            contract_version: contract_version.into(),
            occurred_at: occurred_at.into(),
            service_principal,
            data,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowTransitionDisposition {
    Applied,
    Duplicate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowStepTransitionResult {
    pub disposition: WorkflowTransitionDisposition,
    pub instance_id: String,
    pub completed_step_id: String,
    pub transition_id: String,
    pub next_step_id: Option<String>,
    pub outgoing_event_id: String,
}

#[derive(Debug)]
pub(crate) struct WorkflowApiError {
    pub(crate) code: WorkflowErrorCode,
    pub(crate) message: String,
    next_actions: Vec<String>,
}

impl WorkflowApiError {
    fn invalid(code: WorkflowErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            next_actions: vec!["correct_workflow_request".to_owned()],
        }
    }

    pub(crate) fn store(message: impl Into<String>) -> Self {
        Self {
            code: WorkflowErrorCode::StoreUnavailable,
            message: message.into(),
            next_actions: vec![
                "restore_service_store".to_owned(),
                "retry_workflow_request".to_owned(),
            ],
        }
    }

    pub(crate) fn stored_state(message: impl Into<String>) -> Self {
        Self {
            code: WorkflowErrorCode::StoredStateInvalid,
            message: message.into(),
            next_actions: vec!["inspect_service_store_migrations".to_owned()],
        }
    }

    const fn status(&self) -> StatusCode {
        match self.code {
            WorkflowErrorCode::InvalidRequest
            | WorkflowErrorCode::TenantRequired
            | WorkflowErrorCode::TenantIncompatible
            | WorkflowErrorCode::ContextRequired
            | WorkflowErrorCode::EventContractNotDeclared => StatusCode::BAD_REQUEST,
            WorkflowErrorCode::DefinitionNotFound
            | WorkflowErrorCode::DefinitionVersionNotFound
            | WorkflowErrorCode::InstanceNotFound
            | WorkflowErrorCode::StepNotFound
            | WorkflowErrorCode::ChildLinkNotFound => StatusCode::NOT_FOUND,
            WorkflowErrorCode::DefinitionVersionUnsupported
            | WorkflowErrorCode::TransitionConflict
            | WorkflowErrorCode::ChildNotTerminal
            | WorkflowErrorCode::ActionNotEligible
            | WorkflowErrorCode::StalePlan => StatusCode::CONFLICT,
            WorkflowErrorCode::AuthorityRequired => StatusCode::UNAUTHORIZED,
            WorkflowErrorCode::AuthorizationDenied => StatusCode::FORBIDDEN,
            WorkflowErrorCode::StoreUnavailable | WorkflowErrorCode::AuthorityUnavailable => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            WorkflowErrorCode::StoredStateInvalid => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<WorkflowMutationError> for WorkflowApiError {
    fn from(error: WorkflowMutationError) -> Self {
        let next_action = match error.code {
            WorkflowErrorCode::ContextRequired => "start_workflow_from_declared_event_context",
            WorkflowErrorCode::StepNotFound => "inspect_workflow_steps",
            WorkflowErrorCode::TransitionConflict => "inspect_committed_workflow_transition",
            WorkflowErrorCode::EventContractNotDeclared => "declare_service_event_contract",
            WorkflowErrorCode::ChildLinkNotFound => "inspect_parent_workflow_child_evidence",
            WorkflowErrorCode::ChildNotTerminal => "wait_for_child_workflow",
            WorkflowErrorCode::DefinitionNotFound => "inspect_module_workflow_definitions",
            WorkflowErrorCode::DefinitionVersionNotFound => "select_registered_workflow_version",
            WorkflowErrorCode::DefinitionVersionUnsupported => {
                "deploy_worker_supporting_pinned_workflow_definition"
            }
            WorkflowErrorCode::StoreUnavailable => "restore_service_store",
            WorkflowErrorCode::ActionNotEligible => "inspect_available_workflow_actions",
            WorkflowErrorCode::StalePlan => "plan_workflow_action_again",
            WorkflowErrorCode::AuthorityRequired => "provide_workflow_operator_authority",
            WorkflowErrorCode::AuthorityUnavailable => "configure_workflow_authority_verifier",
            WorkflowErrorCode::AuthorizationDenied => "request_workflow_operator_authority",
            _ => "inspect_workflow_state",
        };
        Self {
            code: error.code,
            message: error.message,
            next_actions: vec![next_action.to_owned()],
        }
    }
}

impl IntoResponse for WorkflowApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let code = self.code.as_str();
        let body = platform_http::ProblemDetails {
            problem_type: format!("https://lenso.dev/problems/workflow/v1/{code}"),
            title: "Durable Workflow request failed".to_owned(),
            status: status.as_u16(),
            detail: self.message,
            code: code.to_owned(),
            request_id: None,
            correlation_id: None,
            errors: Vec::new(),
            next_actions: Some(self.next_actions),
        };
        let mut response = (status, Json(body)).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        response.headers_mut().insert(
            "x-lenso-error-code",
            HeaderValue::from_static(self.code.as_str()),
        );
        response
    }
}

#[derive(Debug, FromRow)]
struct WorkflowInstanceRow {
    instance_id: String,
    service_id: String,
    definition_owner: String,
    definition_name: String,
    definition_version: String,
    state: String,
    input: Value,
    result: Option<Value>,
    story_context: Value,
    tenant_scope: Option<Value>,
    parent_instance_id: Option<String>,
    parent_step_id: Option<String>,
    causation_id: Option<String>,
    failure_evidence: Option<Value>,
    control_state: String,
    control_revision: i64,
    paused_at: Option<DateTime<Utc>>,
    initial_step_id: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct WorkflowStepRow {
    step_id: String,
    definition_step_name: String,
    step_position: i32,
    state: String,
    transition_id: Option<String>,
    completed_at: Option<DateTime<Utc>>,
    outgoing_work: Option<Value>,
    attempt_count: i32,
    max_attempts: i32,
    retry_schedule: Value,
    next_attempt_at: Option<DateTime<Utc>>,
    failure_classification: Option<String>,
    failure_code: Option<String>,
    failure_message: Option<String>,
    exhausted_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct WorkflowChildRow {
    link_id: String,
    start_id: String,
    parent_step_id: String,
    child_definition_owner: String,
    child_definition_name: String,
    child_definition_version: String,
    child_instance_id: Option<String>,
    state: String,
    completion_delivery_id: Option<String>,
    failure_evidence: Option<Value>,
    next_action: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct WorkflowTransitionRow {
    definition_owner: String,
    definition_name: String,
    definition_version: String,
    definition_artifact: Option<Value>,
    definition_digest: Option<String>,
    instance_state: String,
    control_state: String,
    workflow_context: Option<Value>,
    definition_step_name: String,
    step_position: i32,
    step_state: String,
    transition_id: Option<String>,
    outgoing_work: Option<Value>,
    attempt_count: i32,
}

pub(crate) fn workflow_router() -> OpenApiRouter<ServiceRuntimeState> {
    OpenApiRouter::new()
        .routes(routes!(start_workflow))
        .routes(routes!(inspect_workflow))
        .merge(control::workflow_control_router())
        .merge(evolution::workflow_evolution_router())
}

#[utoipa::path(
    post,
    path = "/runtime/workflows/{owner}/{name}/instances",
    params(
        ("owner" = String, Path, description = "Owning Module identity"),
        ("name" = String, Path, description = "Stable Workflow Definition name")
    ),
    request_body = WorkflowStartRequest,
    responses(
        (status = 201, body = WorkflowStartResult),
        (status = 400, body = platform_http::ErrorResponse, content_type = "application/problem+json"),
        (status = 404, body = platform_http::ErrorResponse, content_type = "application/problem+json"),
        (status = 503, body = platform_http::ErrorResponse, content_type = "application/problem+json"),
        (status = 500, body = platform_http::ErrorResponse, content_type = "application/problem+json")
    ),
    tag = "service-runtime"
)]
async fn start_workflow(
    State(state): State<ServiceRuntimeState>,
    Path((owner, name)): Path<(String, String)>,
    request: Result<Json<WorkflowStartRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<(StatusCode, Json<WorkflowStartResult>), WorkflowApiError> {
    let Json(request) = request.map_err(|_| {
        WorkflowApiError::invalid(
            WorkflowErrorCode::InvalidRequest,
            "Workflow start request must contain valid JSON matching the v1 contract",
        )
    })?;
    validate_start_request(&state, &request)?;
    let definition = resolve_definition(&state, &owner, &name, &request.definition_version)?;
    let (definition_artifact, definition_digest) = encode_pinned_definition(&definition)?;
    let first_step = definition.steps[0].clone();
    let instance_id = format!("workflow_{}", Uuid::now_v7());
    let initial_step_id = format!("workflow_step_{}", Uuid::now_v7());
    let now = recovery::workflow_now(&state);
    let story_context = serde_json::to_value(&request.story_context)
        .map_err(|error| WorkflowApiError::stored_state(error.to_string()))?;
    let tenant_scope = request
        .tenant_scope
        .as_ref()
        .map(serde_json::to_value)
        .transpose()
        .map_err(|error| WorkflowApiError::stored_state(error.to_string()))?;
    let pool = state
        .store()
        .map_err(|error| WorkflowApiError::store(error.public_message))?;
    let mut transaction = pool
        .begin()
        .await
        .map_err(|error| WorkflowApiError::store(format!("Could not start workflow: {error}")))?;
    sqlx::query(
        r#"
        insert into platform.service_workflow_instances (
            instance_id, service_id, definition_owner, definition_name,
            definition_version, definition_artifact, definition_digest,
            state, input, result, story_context,
            tenant_scope, initial_step_id, created_at, updated_at
        ) values ($1, $2, $3, $4, $5, $6, $7, 'running', $8, null,
                  $9, $10, $11, $12, $12)
        "#,
    )
    .bind(&instance_id)
    .bind(&state.identity.service_id)
    .bind(&definition.owner)
    .bind(&definition.name)
    .bind(&definition.version)
    .bind(definition_artifact)
    .bind(definition_digest)
    .bind(&request.input)
    .bind(story_context)
    .bind(tenant_scope)
    .bind(&initial_step_id)
    .bind(now)
    .execute(&mut *transaction)
    .await
    .map_err(|error| WorkflowApiError::store(format!("Could not persist workflow: {error}")))?;
    let timers = recovery::persist_workflow_step_in_tx(
        &mut transaction,
        &instance_id,
        &initial_step_id,
        &first_step,
        0,
        now,
    )
    .await
    .map_err(WorkflowApiError::from)?;
    let mut segment = StorySegmentRecord::new(
        &request.story_context.story_id,
        format!("workflow:{instance_id}:started"),
        "durable_workflow",
        format!("workflow.{owner}.{name}.start"),
        &definition.input_contract.contract_id,
        &definition.input_contract.version,
        "started",
        now,
    )
    .with_parent_segment(&request.story_context.segment_id)
    .with_workflow(StorySegmentWorkflow {
        instance_id: instance_id.clone(),
        definition_owner: definition.owner.clone(),
        definition_name: definition.name.clone(),
        definition_version: definition.version.clone(),
        step_id: Some(initial_step_id.clone()),
        parent_instance_id: None,
        compensation_id: None,
        intervention_id: None,
    });
    if let Some(tenant_scope) = &request.tenant_scope {
        segment = segment.with_tenant(&tenant_scope.tenant_id);
    }
    append_story_segment_in_tx(&state, &mut transaction, &segment)
        .await
        .map_err(|error| WorkflowApiError::store(error.public_message))?;
    transaction.commit().await.map_err(|error| {
        WorkflowApiError::store(format!("Could not commit workflow start: {error}"))
    })?;

    let instance = WorkflowInstance {
        instance_id,
        service_id: state.identity.service_id.clone(),
        definition: WorkflowDefinitionIdentity {
            owner: definition.owner,
            name: definition.name,
            version: definition.version,
        },
        state: WorkflowInstanceState::Running,
        input: request.input,
        result: None,
        story_context: request.story_context,
        tenant_scope: request.tenant_scope,
        parent: None,
        failure: None,
        control: WorkflowControlInspection::active(),
        initial_step_id: initial_step_id.clone(),
        steps: vec![recovery::pending_step_inspection(
            initial_step_id,
            &first_step,
            0,
            timers,
            now,
        )],
        effects: Vec::new(),
        compensations: Vec::new(),
        history: Vec::new(),
        interventions: Vec::new(),
        created_at: now,
        updated_at: now,
    };
    Ok((
        StatusCode::CREATED,
        Json(WorkflowStartResult {
            protocol: WORKFLOW_START_RESULT_PROTOCOL.to_owned(),
            instance,
            next_actions: vec!["inspect_workflow".to_owned()],
        }),
    ))
}

/// Starts one declared Workflow from an Event Contract delivery inside the
/// caller's Service Inbox transaction. The Event identity is the durable start
/// trigger, so redelivery resolves to the already-started instance.
#[allow(clippy::too_many_lines)]
pub async fn start_workflow_from_event_in_tx(
    state: &ServiceRuntimeState,
    transaction: &mut Transaction<'_, Postgres>,
    owner: &str,
    name: &str,
    version: &str,
    envelope: &EventEnvelope,
) -> Result<WorkflowInstance, WorkflowMutationError> {
    let definition = resolve_definition(state, owner, name, version)?;
    let (definition_artifact, definition_digest) = encode_pinned_definition(&definition)?;
    let story = envelope.context.story.as_ref().ok_or_else(|| {
        WorkflowMutationError::new(
            WorkflowErrorCode::ContextRequired,
            "Event-started workflow requires Story Context",
        )
    })?;
    let tenant_scope = envelope
        .context
        .tenant
        .as_ref()
        .map(|tenant| WorkflowTenantScope {
            tenant_id: tenant.tenant_id.clone(),
        });
    match (&state.identity.tenancy_mode, &tenant_scope) {
        (ServiceTenancyMode::Required, None) => {
            return Err(WorkflowMutationError::new(
                WorkflowErrorCode::TenantRequired,
                "This Service requires tenant context for event-started workflows",
            ));
        }
        (ServiceTenancyMode::None, Some(_)) => {
            return Err(WorkflowMutationError::new(
                WorkflowErrorCode::TenantIncompatible,
                "This Service does not accept tenant-scoped workflow starts",
            ));
        }
        _ => {}
    }

    let trigger_lock = format!(
        "{}:{owner}:{name}:{version}:event:{}",
        state.identity.service_id, envelope.event_id
    );
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(trigger_lock)
        .execute(&mut **transaction)
        .await
        .map_err(|error| {
            WorkflowMutationError::store(format!("Could not lock workflow start identity: {error}"))
        })?;
    let existing_instance_id = sqlx::query_scalar::<_, String>(
        r#"
        select instance_id
        from platform.service_workflow_instances
        where service_id = $1 and definition_owner = $2 and definition_name = $3
          and definition_version = $4 and start_trigger_kind = 'event'
          and start_trigger_id = $5
        "#,
    )
    .bind(&state.identity.service_id)
    .bind(owner)
    .bind(name)
    .bind(version)
    .bind(&envelope.event_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not inspect workflow start: {error}"))
    })?;
    if let Some(instance_id) = existing_instance_id {
        return load_instance_in_tx(state, transaction, &instance_id).await;
    }

    let instance_id = format!("workflow_{}", Uuid::now_v7());
    let initial_step_id = format!("workflow_step_{}", Uuid::now_v7());
    let now = recovery::workflow_now(state);
    let story_context = WorkflowStoryContext {
        story_id: story.story_id.clone(),
        segment_id: story.segment_id.clone(),
    };
    let story_json = serde_json::to_value(&story_context).map_err(|error| {
        WorkflowMutationError::new(
            WorkflowErrorCode::StoredStateInvalid,
            format!("Could not encode workflow Story Context: {error}"),
        )
    })?;
    let tenant_json = tenant_scope
        .as_ref()
        .map(serde_json::to_value)
        .transpose()
        .map_err(|error| {
            WorkflowMutationError::new(
                WorkflowErrorCode::StoredStateInvalid,
                format!("Could not encode workflow tenant scope: {error}"),
            )
        })?;
    let workflow_context = serde_json::to_value(&envelope.context).map_err(|error| {
        WorkflowMutationError::new(
            WorkflowErrorCode::StoredStateInvalid,
            format!("Could not encode workflow execution context: {error}"),
        )
    })?;
    sqlx::query(
        r#"
        insert into platform.service_workflow_instances (
            instance_id, service_id, definition_owner, definition_name,
            definition_version, definition_artifact, definition_digest,
            state, input, result, story_context,
            tenant_scope, initial_step_id, start_trigger_kind, start_trigger_id,
            workflow_context, created_at, updated_at
        ) values ($1, $2, $3, $4, $5, $6, $7, 'running', $8, null, $9,
                  $10, $11, 'event', $12, $13, $14, $14)
        "#,
    )
    .bind(&instance_id)
    .bind(&state.identity.service_id)
    .bind(&definition.owner)
    .bind(&definition.name)
    .bind(&definition.version)
    .bind(definition_artifact)
    .bind(definition_digest)
    .bind(&envelope.content.data)
    .bind(story_json)
    .bind(tenant_json)
    .bind(&initial_step_id)
    .bind(&envelope.event_id)
    .bind(workflow_context)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not persist event-started workflow: {error}"))
    })?;
    let timers = recovery::persist_workflow_step_in_tx(
        transaction,
        &instance_id,
        &initial_step_id,
        &definition.steps[0],
        0,
        now,
    )
    .await?;
    let mut segment = StorySegmentRecord::new(
        &story_context.story_id,
        format!("workflow:{instance_id}:started"),
        "durable_workflow",
        format!("workflow.{owner}.{name}.start"),
        &definition.input_contract.contract_id,
        &definition.input_contract.version,
        "started",
        now,
    )
    .with_parent_segment(&story_context.segment_id)
    .with_causation(&envelope.event_id)
    .with_workflow(StorySegmentWorkflow {
        instance_id: instance_id.clone(),
        definition_owner: definition.owner.clone(),
        definition_name: definition.name.clone(),
        definition_version: definition.version.clone(),
        step_id: Some(initial_step_id.clone()),
        parent_instance_id: None,
        compensation_id: None,
        intervention_id: None,
    });
    if let Some(tenant_scope) = &tenant_scope {
        segment = segment.with_tenant(&tenant_scope.tenant_id);
    }
    append_worker_story_segment_in_tx(state, transaction, &segment)
        .await
        .map_err(|error| WorkflowMutationError::store(error.public_message))?;

    Ok(WorkflowInstance {
        instance_id,
        service_id: state.identity.service_id.clone(),
        definition: WorkflowDefinitionIdentity {
            owner: definition.owner,
            name: definition.name,
            version: definition.version,
        },
        state: WorkflowInstanceState::Running,
        input: envelope.content.data.clone(),
        result: None,
        story_context,
        tenant_scope,
        parent: None,
        failure: None,
        control: WorkflowControlInspection::active(),
        initial_step_id: initial_step_id.clone(),
        steps: vec![recovery::pending_step_inspection(
            initial_step_id,
            &definition.steps[0],
            0,
            timers,
            now,
        )],
        effects: Vec::new(),
        compensations: Vec::new(),
        history: Vec::new(),
        interventions: Vec::new(),
        created_at: now,
        updated_at: now,
    })
}

/// Completes one durable step and records its outgoing Event Contract work in
/// the same Service Store transaction. Reusing the same transition identity is
/// idempotent and returns the previously committed outcome.
#[allow(clippy::too_many_lines)]
pub async fn advance_workflow_step_with_event_in_tx(
    state: &ServiceRuntimeState,
    transaction: &mut Transaction<'_, Postgres>,
    instance_id: &str,
    step_id: &str,
    transition_id: &str,
    publication: WorkflowEventPublication,
) -> Result<WorkflowStepTransitionResult, WorkflowMutationError> {
    if [
        instance_id,
        step_id,
        transition_id,
        &publication.consumer_id,
        &publication.event_id,
        &publication.contract_id,
        &publication.contract_version,
        &publication.occurred_at,
    ]
    .iter()
    .any(|value| value.trim().is_empty())
    {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::InvalidRequest,
            "Workflow transition identities and outgoing Event identity must not be empty",
        ));
    }
    let row = sqlx::query_as::<_, WorkflowTransitionRow>(
        r#"
        select instance.definition_owner, instance.definition_name,
               instance.definition_version, instance.definition_artifact,
               instance.definition_digest, instance.state as instance_state,
               instance.control_state,
               instance.workflow_context, step.definition_step_name,
               step.step_position, step.state as step_state,
               step.transition_id, step.outgoing_work, step.attempt_count
        from platform.service_workflow_instances instance
        join platform.service_workflow_steps step on step.instance_id = instance.instance_id
        where instance.service_id = $1 and instance.instance_id = $2 and step.step_id = $3
        for update of instance, step
        "#,
    )
    .bind(&state.identity.service_id)
    .bind(instance_id)
    .bind(step_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not lock workflow step: {error}"))
    })?
    .ok_or_else(|| {
        WorkflowMutationError::new(
            WorkflowErrorCode::StepNotFound,
            format!("Workflow step `{step_id}` was not found in instance `{instance_id}`"),
        )
    })?;

    if row.step_state == "completed" {
        let outgoing_work = row
            .outgoing_work
            .map(serde_json::from_value::<WorkflowOutgoingWorkInspection>)
            .transpose()
            .map_err(|error| {
                WorkflowMutationError::new(
                    WorkflowErrorCode::StoredStateInvalid,
                    format!("Stored workflow outgoing work is invalid: {error}"),
                )
            })?
            .ok_or_else(|| {
                WorkflowMutationError::new(
                    WorkflowErrorCode::StoredStateInvalid,
                    "Completed workflow step is missing outgoing work evidence",
                )
            })?;
        if row.transition_id.as_deref() != Some(transition_id)
            || outgoing_work.event_id != publication.event_id
            || outgoing_work.consumer_id != publication.consumer_id
            || outgoing_work.contract_id != publication.contract_id
            || outgoing_work.contract_version != publication.contract_version
        {
            return Err(WorkflowMutationError::new(
                WorkflowErrorCode::TransitionConflict,
                format!("Workflow step `{step_id}` already completed through another transition"),
            ));
        }
        let next_step_id = sqlx::query_scalar::<_, String>(
            r#"
            select step_id from platform.service_workflow_steps
            where instance_id = $1 and step_position = $2
            "#,
        )
        .bind(instance_id)
        .bind(row.step_position + 1)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|error| {
            WorkflowMutationError::store(format!("Could not inspect next workflow step: {error}"))
        })?;
        return Ok(WorkflowStepTransitionResult {
            disposition: WorkflowTransitionDisposition::Duplicate,
            instance_id: instance_id.to_owned(),
            completed_step_id: step_id.to_owned(),
            transition_id: transition_id.to_owned(),
            next_step_id,
            outgoing_event_id: outgoing_work.event_id,
        });
    }
    if row.step_state != "pending" || row.instance_state != "running" {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::TransitionConflict,
            format!("Workflow step `{step_id}` is not pending in a running instance"),
        ));
    }
    if row.control_state != "active" {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::TransitionConflict,
            format!(
                "Workflow Instance `{instance_id}` is paused; resume it before dispatching new step work"
            ),
        ));
    }
    let definition = resolve_pinned_definition(
        state,
        &row.definition_owner,
        &row.definition_name,
        &row.definition_version,
        row.definition_artifact.as_ref(),
        row.definition_digest.as_deref(),
    )?;
    let position = usize::try_from(row.step_position).map_err(|_| {
        WorkflowMutationError::new(
            WorkflowErrorCode::StoredStateInvalid,
            format!("Workflow step `{step_id}` has an invalid position"),
        )
    })?;
    let declared_step = definition.steps.get(position).ok_or_else(|| {
        WorkflowMutationError::new(
            WorkflowErrorCode::StoredStateInvalid,
            format!("Workflow step `{step_id}` has no pinned declaration"),
        )
    })?;
    if declared_step.name != row.definition_step_name {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::StoredStateInvalid,
            format!("Workflow step `{step_id}` does not match its pinned definition"),
        ));
    }
    let contract = state
        .event_contracts
        .iter()
        .find(|contract| {
            contract.contract_id == publication.contract_id
                && contract.version == publication.contract_version
        })
        .ok_or_else(|| {
            WorkflowMutationError::new(
                WorkflowErrorCode::EventContractNotDeclared,
                format!(
                    "Outgoing Event Contract `{}` version `{}` is not declared by Service `{}`",
                    publication.contract_id,
                    publication.contract_version,
                    state.identity.service_id
                ),
            )
        })?;
    let event_type = event_type_for_contract(contract)?;
    let mut context: EventContext = row
        .workflow_context
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| {
            WorkflowMutationError::new(
                WorkflowErrorCode::StoredStateInvalid,
                format!("Stored workflow execution context is invalid: {error}"),
            )
        })?
        .ok_or_else(|| {
            WorkflowMutationError::new(
                WorkflowErrorCode::ContextRequired,
                "Cross-Service workflow step requires persisted Event Context",
            )
        })?;
    context.service_principal = Some(publication.service_principal);
    context.causation = Some(CausationContext {
        causation_id: step_id.to_owned(),
        correlation_id: context
            .causation
            .as_ref()
            .and_then(|causation| causation.correlation_id.clone()),
    });
    validate_outgoing_context(contract, &context)?;
    let envelope = EventEnvelope {
        protocol: lenso_service::EVENT_ENVELOPE_PROTOCOL.to_owned(),
        event_id: publication.event_id.clone(),
        event_type,
        contract_id: contract.contract_id.clone(),
        contract_version: contract.version.clone(),
        producer_service_id: state.identity.service_id.clone(),
        module_id: contract.module_id.clone(),
        occurred_at: publication.occurred_at,
        tenancy_mode: contract.tenancy_mode.clone(),
        context,
        content: EventContent {
            content_type: "application/json".to_owned(),
            schema: contract.artifact.path.clone(),
            data: publication.data,
        },
    };
    let outgoing_work = WorkflowOutgoingWorkInspection {
        kind: "event_contract".to_owned(),
        consumer_id: publication.consumer_id.clone(),
        event_id: envelope.event_id.clone(),
        contract_id: envelope.contract_id.clone(),
        contract_version: envelope.contract_version.clone(),
    };
    let now = recovery::workflow_now(state);
    let attempt_number = u32::try_from(row.attempt_count).map_err(|_| {
        WorkflowMutationError::new(
            WorkflowErrorCode::StoredStateInvalid,
            format!("Workflow step `{step_id}` has an invalid attempt count"),
        )
    })? + 1;
    recovery::record_workflow_step_success_in_tx(
        transaction,
        instance_id,
        step_id,
        attempt_number,
        transition_id,
        now,
    )
    .await?;
    ServiceEventPublisher
        .publish_in_tx(transaction, &publication.consumer_id, &envelope)
        .await
        .map_err(|error| WorkflowMutationError::store(error.message))?;
    let outgoing_work_json = serde_json::to_value(&outgoing_work).map_err(|error| {
        WorkflowMutationError::new(
            WorkflowErrorCode::StoredStateInvalid,
            format!("Could not encode workflow outgoing work evidence: {error}"),
        )
    })?;
    if let Some(compensation) = &declared_step.compensation {
        crate::record_compensatable_effect_in_tx(
            state,
            transaction,
            instance_id,
            step_id,
            &row.definition_step_name,
            transition_id,
            &outgoing_work,
            compensation,
            now,
        )
        .await?;
    }
    let updated = sqlx::query(
        r#"
        update platform.service_workflow_steps
        set state = 'completed', transition_id = $3, completed_at = $4,
            outgoing_work = $5, attempt_count = $6, next_attempt_at = null,
            updated_at = $4
        where instance_id = $1 and step_id = $2 and state = 'pending'
        "#,
    )
    .bind(instance_id)
    .bind(step_id)
    .bind(transition_id)
    .bind(now)
    .bind(outgoing_work_json)
    .bind(i32::try_from(attempt_number).unwrap_or(i32::MAX))
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not complete workflow step: {error}"))
    })?;
    if updated.rows_affected() != 1 {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::TransitionConflict,
            format!("Workflow step `{step_id}` lost its pending transition"),
        ));
    }

    let next_step_id = if let Some(next_step) = definition.steps.get(position + 1) {
        let next_step_id = format!("workflow_step_{}", Uuid::now_v7());
        recovery::persist_workflow_step_in_tx(
            transaction,
            instance_id,
            &next_step_id,
            next_step,
            row.step_position + 1,
            now,
        )
        .await?;
        Some(next_step_id)
    } else {
        None
    };
    sqlx::query(
        r#"
        update platform.service_workflow_instances
        set state = $2, updated_at = $3
        where instance_id = $1 and state = 'running'
        "#,
    )
    .bind(instance_id)
    .bind(if next_step_id.is_some() {
        "running"
    } else {
        "completed"
    })
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not advance workflow instance: {error}"))
    })?;

    append_persisted_workflow_story_segment_in_tx(
        state,
        transaction,
        instance_id,
        Some(step_id),
        None,
        None,
        &format!("workflow:{instance_id}:step:{step_id}"),
        &format!("workflow.step.{}", row.definition_step_name),
        &envelope.contract_id,
        &envelope.contract_version,
        "completed",
        attempt_number,
        Some(transition_id),
        now,
    )
    .await
    .map_err(|error| WorkflowMutationError::store(error.public_message))?;

    Ok(WorkflowStepTransitionResult {
        disposition: WorkflowTransitionDisposition::Applied,
        instance_id: instance_id.to_owned(),
        completed_step_id: step_id.to_owned(),
        transition_id: transition_id.to_owned(),
        next_step_id,
        outgoing_event_id: envelope.event_id,
    })
}

pub(crate) fn validate_outgoing_context(
    contract: &EventContractArtifact,
    context: &EventContext,
) -> Result<(), WorkflowMutationError> {
    if context.protocol != lenso_service::COMMON_CONTEXT_PROTOCOL {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::ContextRequired,
            "Outgoing workflow Event Context uses an unsupported protocol",
        ));
    }
    let present = |requirement| match requirement {
        CommonContextRequirement::Story => context.story.is_some(),
        CommonContextRequirement::Trace => context.trace.is_some(),
        CommonContextRequirement::ServicePrincipal => context.service_principal.is_some(),
        CommonContextRequirement::DelegatedActor => context.delegated_actor.is_some(),
        CommonContextRequirement::Tenant => context.tenant.is_some(),
        CommonContextRequirement::Deadline => context.deadline.is_some(),
        CommonContextRequirement::IdempotencyKey => context.idempotency_key.is_some(),
        CommonContextRequirement::Causation => context.causation.is_some(),
        CommonContextRequirement::Region => context.region.is_some(),
    };
    if let Some(missing) = contract
        .context
        .required
        .iter()
        .copied()
        .find(|requirement| !present(*requirement))
    {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::ContextRequired,
            format!("Outgoing workflow Event Contract requires `{missing:?}` context"),
        ));
    }
    match (&contract.tenancy_mode, &context.tenant) {
        (ServiceTenancyMode::Required, None) => Err(WorkflowMutationError::new(
            WorkflowErrorCode::TenantRequired,
            "Outgoing workflow Event Contract requires Tenant Context",
        )),
        (ServiceTenancyMode::None, Some(_)) => Err(WorkflowMutationError::new(
            WorkflowErrorCode::TenantIncompatible,
            "Outgoing workflow Event Contract does not accept Tenant Context",
        )),
        _ => Ok(()),
    }
}

pub(crate) fn event_type_for_contract(
    contract: &EventContractArtifact,
) -> Result<String, WorkflowMutationError> {
    contract
        .artifact
        .path
        .rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".schema.json"))
        .filter(|name| name.ends_with(&format!(".{}.{}", contract.contract_id, contract.version)))
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            WorkflowMutationError::new(
                WorkflowErrorCode::EventContractNotDeclared,
                "Declared Event Contract has an invalid artifact identity",
            )
        })
}

pub(crate) fn postgres_now() -> DateTime<Utc> {
    let now = Utc::now();
    DateTime::<Utc>::from_timestamp_micros(now.timestamp_micros())
        .expect("current UTC timestamp must fit PostgreSQL microsecond precision")
}

#[utoipa::path(
    get,
    path = "/runtime/workflows/instances/{instance_id}",
    params(
        ("instance_id" = String, Path, description = "Stable Workflow Instance identity"),
        WorkflowInspectionQuery
    ),
    responses(
        (status = 200, body = WorkflowInspectionResult),
        (status = 404, body = platform_http::ErrorResponse, content_type = "application/problem+json"),
        (status = 503, body = platform_http::ErrorResponse, content_type = "application/problem+json"),
        (status = 500, body = platform_http::ErrorResponse, content_type = "application/problem+json")
    ),
    tag = "service-runtime"
)]
async fn inspect_workflow(
    State(state): State<ServiceRuntimeState>,
    Path(instance_id): Path<String>,
    Query(query): Query<WorkflowInspectionQuery>,
) -> Result<Json<WorkflowInspectionResult>, WorkflowApiError> {
    if instance_id.trim().is_empty() {
        return Err(WorkflowApiError::invalid(
            WorkflowErrorCode::InvalidRequest,
            "Workflow Instance identity must not be empty",
        ));
    }
    let instance = load_instance(&state, &instance_id).await?;
    let selected_step = selected_workflow_step(&instance, query.step_id.as_deref())?;
    let pending_work = workflow_pending_work(&instance);
    let available_actions = workflow_available_actions(&instance, selected_step.as_ref());
    let next_actions = workflow_next_actions(&instance, &available_actions);
    Ok(Json(WorkflowInspectionResult {
        protocol: WORKFLOW_INSPECTION_PROTOCOL.to_owned(),
        instance,
        selected_step,
        pending_work,
        available_actions,
        next_actions,
    }))
}

fn workflow_next_actions(
    instance: &WorkflowInstance,
    available_actions: &[WorkflowOperatorAction],
) -> Vec<String> {
    if instance.control.state == WorkflowControlState::Paused {
        return vec!["plan_workflow_resume".to_owned()];
    }
    if available_actions.contains(&WorkflowOperatorAction::Retry) {
        return vec!["plan_selected_step_retry".to_owned()];
    }
    if let Some(failure) = &instance.failure {
        return vec![failure.next_action.clone()];
    }
    if instance.state == WorkflowInstanceState::Compensating {
        return vec!["execute_next_workflow_compensation".to_owned()];
    }
    if instance.steps.iter().any(|step| {
        step.child_workflow
            .as_ref()
            .is_some_and(|child| child.state == WorkflowChildState::Waiting)
    }) {
        return vec!["wait_for_child_workflow".to_owned()];
    }
    vec!["no_action_required".to_owned()]
}

fn validate_start_request(
    state: &ServiceRuntimeState,
    request: &WorkflowStartRequest,
) -> Result<(), WorkflowApiError> {
    if request.definition_version.trim().is_empty()
        || request.story_context.story_id.trim().is_empty()
        || request.story_context.segment_id.trim().is_empty()
        || request
            .tenant_scope
            .as_ref()
            .is_some_and(|scope| scope.tenant_id.trim().is_empty())
    {
        return Err(WorkflowApiError::invalid(
            WorkflowErrorCode::InvalidRequest,
            "Workflow version, Story Context, and explicit tenant identity must not be empty",
        ));
    }
    match (&state.identity.tenancy_mode, &request.tenant_scope) {
        (ServiceTenancyMode::Required, None) => Err(WorkflowApiError {
            code: WorkflowErrorCode::TenantRequired,
            message: "This Service requires an explicit tenant scope for workflow starts"
                .to_owned(),
            next_actions: vec!["provide_tenant_scope".to_owned()],
        }),
        (ServiceTenancyMode::None, Some(_)) => Err(WorkflowApiError {
            code: WorkflowErrorCode::TenantIncompatible,
            message: "This Service does not accept tenant-scoped workflow starts".to_owned(),
            next_actions: vec!["remove_tenant_scope".to_owned()],
        }),
        _ => Ok(()),
    }
}

pub(crate) fn resolve_definition(
    state: &ServiceRuntimeState,
    owner: &str,
    name: &str,
    version: &str,
) -> Result<WorkflowDefinition, WorkflowMutationError> {
    let named = state
        .workflow_definitions
        .iter()
        .filter(|definition| definition.owner == owner && definition.name == name)
        .collect::<Vec<_>>();
    if named.is_empty() {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::DefinitionNotFound,
            format!("Workflow Definition `{owner}/{name}` is not registered"),
        ));
    }
    named
        .into_iter()
        .find(|definition| definition.version == version)
        .cloned()
        .ok_or_else(|| {
            WorkflowMutationError::new(
                WorkflowErrorCode::DefinitionVersionNotFound,
                format!(
                    "Workflow Definition `{owner}/{name}` has no registered version `{version}`"
                ),
            )
        })
}

pub(crate) fn encode_pinned_definition(
    definition: &WorkflowDefinition,
) -> Result<(Value, String), WorkflowMutationError> {
    let bytes = serde_json::to_vec(definition).map_err(|error| {
        WorkflowMutationError::new(
            WorkflowErrorCode::StoredStateInvalid,
            format!("Could not encode pinned Workflow Definition: {error}"),
        )
    })?;
    let artifact = serde_json::from_slice(&bytes).map_err(|error| {
        WorkflowMutationError::new(
            WorkflowErrorCode::StoredStateInvalid,
            format!("Could not materialize pinned Workflow Definition: {error}"),
        )
    })?;
    let digest = format!("sha256:{}", sha256_hex(&bytes));
    Ok((artifact, digest))
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

pub(crate) fn resolve_pinned_definition(
    state: &ServiceRuntimeState,
    owner: &str,
    name: &str,
    version: &str,
    artifact: Option<&Value>,
    digest: Option<&str>,
) -> Result<WorkflowDefinition, WorkflowMutationError> {
    let artifact = artifact.ok_or_else(|| {
        WorkflowMutationError::new(
            WorkflowErrorCode::DefinitionVersionUnsupported,
            format!(
                "Workflow Definition `{owner}/{name}` version `{version}` has no pinned artifact and cannot be interpreted safely"
            ),
        )
    })?;
    let pinned: WorkflowDefinition = serde_json::from_value(artifact.clone()).map_err(|error| {
        WorkflowMutationError::new(
            WorkflowErrorCode::StoredStateInvalid,
            format!("Pinned Workflow Definition artifact is invalid: {error}"),
        )
    })?;
    if pinned.owner != owner || pinned.name != name || pinned.version != version {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::StoredStateInvalid,
            "Pinned Workflow Definition artifact does not match the stored instance identity",
        ));
    }
    let (_, expected_digest) = encode_pinned_definition(&pinned)?;
    if digest != Some(expected_digest.as_str()) {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::StoredStateInvalid,
            "Pinned Workflow Definition digest does not match its stored artifact",
        ));
    }
    state
        .workflow_definitions
        .iter()
        .find(|candidate| **candidate == pinned)
        .cloned()
        .ok_or_else(|| {
            WorkflowMutationError::new(
                WorkflowErrorCode::DefinitionVersionUnsupported,
                format!(
                    "This worker does not support the exact pinned Workflow Definition `{owner}/{name}` version `{version}`"
                ),
            )
        })
}

pub(super) async fn load_instance(
    state: &ServiceRuntimeState,
    instance_id: &str,
) -> Result<WorkflowInstance, WorkflowApiError> {
    let pool = state
        .store()
        .map_err(|error| WorkflowApiError::store(error.public_message))?;
    let row = sqlx::query_as::<_, WorkflowInstanceRow>(
        r#"
        select instance_id, service_id, definition_owner, definition_name,
               definition_version, state, input, result, story_context,
               tenant_scope, parent_instance_id, parent_step_id, causation_id,
               failure_evidence, control_state, control_revision, paused_at,
               initial_step_id, created_at, updated_at
        from platform.service_workflow_instances
        where service_id = $1 and instance_id = $2
        "#,
    )
    .bind(&state.identity.service_id)
    .bind(instance_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| WorkflowApiError::store(format!("Could not inspect workflow: {error}")))?
    .ok_or_else(|| WorkflowApiError {
        code: WorkflowErrorCode::InstanceNotFound,
        message: format!("Workflow Instance `{instance_id}` was not found in this Service Store"),
        next_actions: vec!["verify_workflow_instance_identity".to_owned()],
    })?;
    let step_rows = sqlx::query_as::<_, WorkflowStepRow>(
        r#"
        select step_id, definition_step_name, step_position, state, transition_id,
               completed_at, outgoing_work, attempt_count, max_attempts,
               retry_schedule, next_attempt_at, failure_classification,
               failure_code, failure_message, exhausted_at, created_at, updated_at
        from platform.service_workflow_steps
        where instance_id = $1
        order by step_position, step_id
        "#,
    )
    .bind(instance_id)
    .fetch_all(pool)
    .await
    .map_err(|error| {
        WorkflowApiError::store(format!("Could not inspect workflow steps: {error}"))
    })?;
    let recovery = recovery::load_recovery(pool, instance_id).await?;
    let control_evidence = control::load_control_evidence(pool, instance_id).await?;
    let compensation_evidence = crate::load_compensation_evidence(pool, instance_id).await?;
    let child_rows = sqlx::query_as::<_, WorkflowChildRow>(
        r#"
        select link_id, start_id, parent_step_id, child_definition_owner,
               child_definition_name, child_definition_version, child_instance_id,
               state, completion_delivery_id, failure_evidence, next_action,
               created_at, updated_at
        from platform.service_workflow_child_links
        where parent_instance_id = $1
        order by created_at, link_id
        "#,
    )
    .bind(instance_id)
    .fetch_all(pool)
    .await
    .map_err(|error| {
        WorkflowApiError::store(format!("Could not inspect child workflow links: {error}"))
    })?;
    workflow_from_rows(
        row,
        step_rows,
        recovery,
        child_rows,
        compensation_evidence,
        control_evidence,
    )
}

pub(super) async fn load_instance_in_tx(
    state: &ServiceRuntimeState,
    transaction: &mut Transaction<'_, Postgres>,
    instance_id: &str,
) -> Result<WorkflowInstance, WorkflowMutationError> {
    let row = sqlx::query_as::<_, WorkflowInstanceRow>(
        r#"
        select instance_id, service_id, definition_owner, definition_name,
               definition_version, state, input, result, story_context,
               tenant_scope, parent_instance_id, parent_step_id, causation_id,
               failure_evidence, control_state, control_revision, paused_at,
               initial_step_id, created_at, updated_at
        from platform.service_workflow_instances
        where service_id = $1 and instance_id = $2
        "#,
    )
    .bind(&state.identity.service_id)
    .bind(instance_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| WorkflowMutationError::store(format!("Could not inspect workflow: {error}")))?
    .ok_or_else(|| {
        WorkflowMutationError::new(
            WorkflowErrorCode::InstanceNotFound,
            format!("Workflow Instance `{instance_id}` was not found in this Service Store"),
        )
    })?;
    let step_rows = sqlx::query_as::<_, WorkflowStepRow>(
        r#"
        select step_id, definition_step_name, step_position, state, transition_id,
               completed_at, outgoing_work, attempt_count, max_attempts,
               retry_schedule, next_attempt_at, failure_classification,
               failure_code, failure_message, exhausted_at, created_at, updated_at
        from platform.service_workflow_steps
        where instance_id = $1
        order by step_position, step_id
        "#,
    )
    .bind(instance_id)
    .fetch_all(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not inspect workflow steps: {error}"))
    })?;
    let recovery = recovery::load_recovery_in_tx(transaction, instance_id).await?;
    let control_evidence = control::load_control_evidence_in_tx(transaction, instance_id).await?;
    let compensation_evidence =
        crate::load_compensation_evidence_in_tx(transaction, instance_id).await?;
    let child_rows = sqlx::query_as::<_, WorkflowChildRow>(
        r#"
        select link_id, start_id, parent_step_id, child_definition_owner,
               child_definition_name, child_definition_version, child_instance_id,
               state, completion_delivery_id, failure_evidence, next_action,
               created_at, updated_at
        from platform.service_workflow_child_links
        where parent_instance_id = $1
        order by created_at, link_id
        "#,
    )
    .bind(instance_id)
    .fetch_all(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not inspect child workflow links: {error}"))
    })?;
    workflow_from_rows(
        row,
        step_rows,
        recovery,
        child_rows,
        compensation_evidence,
        control_evidence,
    )
    .map_err(|error| WorkflowMutationError::new(error.code, error.message))
}

fn workflow_from_rows(
    row: WorkflowInstanceRow,
    step_rows: Vec<WorkflowStepRow>,
    mut recovery_by_step: recovery::WorkflowRecoveryByStep,
    mut child_rows: Vec<WorkflowChildRow>,
    compensation_evidence: crate::WorkflowCompensationEvidence,
    control_evidence: WorkflowControlEvidence,
) -> Result<WorkflowInstance, WorkflowApiError> {
    let state = WorkflowInstanceState::parse(&row.state).ok_or_else(|| {
        WorkflowApiError::stored_state(format!(
            "Workflow Instance `{}` has unsupported state `{}`",
            row.instance_id, row.state
        ))
    })?;
    let story_context = serde_json::from_value(row.story_context).map_err(|error| {
        WorkflowApiError::stored_state(format!("Stored Story Context is invalid: {error}"))
    })?;
    let tenant_scope = row
        .tenant_scope
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| {
            WorkflowApiError::stored_state(format!("Stored tenant scope is invalid: {error}"))
        })?;
    let parent = match (row.parent_instance_id, row.parent_step_id, row.causation_id) {
        (None, None, None) => None,
        (Some(instance_id), Some(step_id), Some(causation_id)) => Some(WorkflowParentInspection {
            instance_id,
            step_id,
            causation_id,
        }),
        _ => {
            return Err(WorkflowApiError::stored_state(format!(
                "Workflow Instance `{}` has an incomplete parent link",
                row.instance_id
            )));
        }
    };
    let failure = row
        .failure_evidence
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| {
            WorkflowApiError::stored_state(format!("Stored workflow failure is invalid: {error}"))
        })?;
    let control = WorkflowControlInspection::from_stored(
        &row.control_state,
        row.control_revision,
        row.paused_at,
    )?;
    let steps = step_rows
        .into_iter()
        .map(|step| {
            let state = match step.state.as_str() {
                "pending" => WorkflowStepState::Pending,
                "waiting_for_child" => WorkflowStepState::WaitingForChild,
                "completed" => WorkflowStepState::Completed,
                "exhausted" => WorkflowStepState::Exhausted,
                "failed" => WorkflowStepState::Failed,
                other => {
                    return Err(WorkflowApiError::stored_state(format!(
                        "Workflow step `{}` has unsupported state `{other}`",
                        step.step_id
                    )));
                }
            };
            let position = u32::try_from(step.step_position).map_err(|_| {
                WorkflowApiError::stored_state(format!(
                    "Workflow step `{}` has an invalid position",
                    step.step_id
                ))
            })?;
            let attempt_count = u32::try_from(step.attempt_count).map_err(|_| {
                WorkflowApiError::stored_state(format!(
                    "Workflow step `{}` has an invalid attempt count",
                    step.step_id
                ))
            })?;
            let max_attempts = u32::try_from(step.max_attempts).map_err(|_| {
                WorkflowApiError::stored_state(format!(
                    "Workflow step `{}` has an invalid maximum attempt count",
                    step.step_id
                ))
            })?;
            let latest_failure = recovery::latest_failure(
                step.failure_classification.as_deref(),
                step.failure_code,
                step.failure_message,
            )?;
            let recovery = recovery_by_step.remove(&step.step_id).unwrap_or_default();
            let child_workflow = child_rows
                .iter()
                .position(|child| child.parent_step_id == step.step_id)
                .map(|index| child_rows.swap_remove(index))
                .map(child_inspection_from_row)
                .transpose()?;
            Ok(WorkflowStepInspection {
                step_id: step.step_id,
                definition_step_name: step.definition_step_name,
                position,
                state,
                transition_id: step.transition_id,
                completed_at: step.completed_at,
                outgoing_work: step
                    .outgoing_work
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(|error| {
                        WorkflowApiError::stored_state(format!(
                            "Stored workflow outgoing work is invalid: {error}"
                        ))
                    })?,
                attempt_count,
                max_attempts,
                retry_schedule_ms: recovery::retry_schedule(step.retry_schedule)?,
                next_attempt_at: step.next_attempt_at,
                latest_failure,
                exhausted_at: step.exhausted_at,
                attempts: recovery.attempts,
                timers: recovery.timers,
                child_workflow,
                created_at: step.created_at,
                updated_at: step.updated_at,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if steps.first().map(|step| step.step_id.as_str()) != Some(row.initial_step_id.as_str()) {
        return Err(WorkflowApiError::stored_state(format!(
            "Workflow Instance `{}` has inconsistent initial step identity",
            row.instance_id
        )));
    }
    if !child_rows.is_empty() {
        return Err(WorkflowApiError::stored_state(format!(
            "Workflow Instance `{}` has child evidence for an unknown step",
            row.instance_id
        )));
    }
    Ok(WorkflowInstance {
        instance_id: row.instance_id,
        service_id: row.service_id,
        definition: WorkflowDefinitionIdentity {
            owner: row.definition_owner,
            name: row.definition_name,
            version: row.definition_version,
        },
        state,
        input: row.input,
        result: row.result,
        story_context,
        tenant_scope,
        parent,
        failure,
        control,
        initial_step_id: row.initial_step_id,
        steps,
        effects: compensation_evidence.effects,
        compensations: compensation_evidence.compensations,
        history: compensation_evidence.history,
        interventions: control_evidence.interventions,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn child_inspection_from_row(
    row: WorkflowChildRow,
) -> Result<WorkflowChildInspection, WorkflowApiError> {
    let state = match row.state.as_str() {
        "waiting" => WorkflowChildState::Waiting,
        "completed" => WorkflowChildState::Completed,
        "failed" => WorkflowChildState::Failed,
        "unsupported_version" => WorkflowChildState::UnsupportedVersion,
        other => {
            return Err(WorkflowApiError::stored_state(format!(
                "Child workflow link `{}` has unsupported state `{other}`",
                row.link_id
            )));
        }
    };
    let failure = row
        .failure_evidence
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| {
            WorkflowApiError::stored_state(format!(
                "Stored child workflow failure is invalid: {error}"
            ))
        })?;
    Ok(WorkflowChildInspection {
        link_id: row.link_id,
        start_id: row.start_id,
        definition: WorkflowDefinitionIdentity {
            owner: row.child_definition_owner,
            name: row.child_definition_name,
            version: row.child_definition_version,
        },
        instance_id: row.child_instance_id,
        state,
        completion_delivery_id: row.completion_delivery_id,
        failure,
        next_actions: vec![row.next_action],
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}
