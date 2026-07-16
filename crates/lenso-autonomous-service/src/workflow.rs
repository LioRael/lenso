use crate::ServiceRuntimeState;
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use lenso_contracts::WorkflowDefinition;
use lenso_service::ServiceTenancyMode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowInstanceState {
    Running,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStepState {
    Pending,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStepInspection {
    pub step_id: String,
    pub definition_step_name: String,
    pub position: u32,
    pub state: WorkflowStepState,
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
    pub initial_step_id: String,
    pub steps: Vec<WorkflowStepInspection>,
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
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
pub enum WorkflowErrorCode {
    #[serde(rename = "workflow_invalid_request")]
    InvalidRequest,
    #[serde(rename = "workflow_definition_not_found")]
    DefinitionNotFound,
    #[serde(rename = "workflow_definition_version_not_found")]
    DefinitionVersionNotFound,
    #[serde(rename = "workflow_tenant_required")]
    TenantRequired,
    #[serde(rename = "workflow_tenant_incompatible")]
    TenantIncompatible,
    #[serde(rename = "workflow_instance_not_found")]
    InstanceNotFound,
    #[serde(rename = "workflow_store_unavailable")]
    StoreUnavailable,
    #[serde(rename = "workflow_stored_state_invalid")]
    StoredStateInvalid,
}

impl WorkflowErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "workflow_invalid_request",
            Self::DefinitionNotFound => "workflow_definition_not_found",
            Self::DefinitionVersionNotFound => "workflow_definition_version_not_found",
            Self::TenantRequired => "workflow_tenant_required",
            Self::TenantIncompatible => "workflow_tenant_incompatible",
            Self::InstanceNotFound => "workflow_instance_not_found",
            Self::StoreUnavailable => "workflow_store_unavailable",
            Self::StoredStateInvalid => "workflow_stored_state_invalid",
        }
    }
}

#[derive(Debug)]
struct WorkflowApiError {
    code: WorkflowErrorCode,
    message: String,
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

    fn store(message: impl Into<String>) -> Self {
        Self {
            code: WorkflowErrorCode::StoreUnavailable,
            message: message.into(),
            next_actions: vec![
                "restore_service_store".to_owned(),
                "retry_workflow_request".to_owned(),
            ],
        }
    }

    fn stored_state(message: impl Into<String>) -> Self {
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
            | WorkflowErrorCode::TenantIncompatible => StatusCode::BAD_REQUEST,
            WorkflowErrorCode::DefinitionNotFound
            | WorkflowErrorCode::DefinitionVersionNotFound
            | WorkflowErrorCode::InstanceNotFound => StatusCode::NOT_FOUND,
            WorkflowErrorCode::StoreUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            WorkflowErrorCode::StoredStateInvalid => StatusCode::INTERNAL_SERVER_ERROR,
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
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

pub(crate) fn workflow_router() -> OpenApiRouter<ServiceRuntimeState> {
    OpenApiRouter::new()
        .routes(routes!(start_workflow))
        .routes(routes!(inspect_workflow))
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
    let first_step_name = definition.steps[0].name.clone();
    let instance_id = format!("workflow_{}", Uuid::now_v7());
    let initial_step_id = format!("workflow_step_{}", Uuid::now_v7());
    let now = Utc::now();
    let now = DateTime::<Utc>::from_timestamp_micros(now.timestamp_micros())
        .expect("current UTC timestamp must fit PostgreSQL microsecond precision");
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
            definition_version, state, input, result, story_context,
            tenant_scope, initial_step_id, created_at, updated_at
        ) values ($1, $2, $3, $4, $5, 'running', $6, null, $7, $8, $9, $10, $10)
        "#,
    )
    .bind(&instance_id)
    .bind(&state.identity.service_id)
    .bind(&definition.owner)
    .bind(&definition.name)
    .bind(&definition.version)
    .bind(&request.input)
    .bind(story_context)
    .bind(tenant_scope)
    .bind(&initial_step_id)
    .bind(now)
    .execute(&mut *transaction)
    .await
    .map_err(|error| WorkflowApiError::store(format!("Could not persist workflow: {error}")))?;
    sqlx::query(
        r#"
        insert into platform.service_workflow_steps (
            step_id, instance_id, definition_step_name, step_position,
            state, created_at, updated_at
        ) values ($1, $2, $3, 0, 'pending', $4, $4)
        "#,
    )
    .bind(&initial_step_id)
    .bind(&instance_id)
    .bind(&first_step_name)
    .bind(now)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        WorkflowApiError::store(format!("Could not persist initial workflow step: {error}"))
    })?;
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
        initial_step_id: initial_step_id.clone(),
        steps: vec![WorkflowStepInspection {
            step_id: initial_step_id,
            definition_step_name: first_step_name,
            position: 0,
            state: WorkflowStepState::Pending,
            created_at: now,
            updated_at: now,
        }],
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

#[utoipa::path(
    get,
    path = "/runtime/workflows/instances/{instance_id}",
    params(("instance_id" = String, Path, description = "Stable Workflow Instance identity")),
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
) -> Result<Json<WorkflowInspectionResult>, WorkflowApiError> {
    if instance_id.trim().is_empty() {
        return Err(WorkflowApiError::invalid(
            WorkflowErrorCode::InvalidRequest,
            "Workflow Instance identity must not be empty",
        ));
    }
    let instance = load_instance(&state, &instance_id).await?;
    Ok(Json(WorkflowInspectionResult {
        protocol: WORKFLOW_INSPECTION_PROTOCOL.to_owned(),
        instance,
        next_actions: vec!["no_action_required".to_owned()],
    }))
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

fn resolve_definition(
    state: &ServiceRuntimeState,
    owner: &str,
    name: &str,
    version: &str,
) -> Result<WorkflowDefinition, WorkflowApiError> {
    let named = state
        .workflow_definitions
        .iter()
        .filter(|definition| definition.owner == owner && definition.name == name)
        .collect::<Vec<_>>();
    if named.is_empty() {
        return Err(WorkflowApiError {
            code: WorkflowErrorCode::DefinitionNotFound,
            message: format!("Workflow Definition `{owner}/{name}` is not registered"),
            next_actions: vec!["inspect_module_workflow_definitions".to_owned()],
        });
    }
    named
        .into_iter()
        .find(|definition| definition.version == version)
        .cloned()
        .ok_or_else(|| WorkflowApiError {
            code: WorkflowErrorCode::DefinitionVersionNotFound,
            message: format!(
                "Workflow Definition `{owner}/{name}` has no registered version `{version}`"
            ),
            next_actions: vec!["select_registered_workflow_version".to_owned()],
        })
}

async fn load_instance(
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
               tenant_scope, initial_step_id, created_at, updated_at
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
        select step_id, definition_step_name, step_position, state, created_at, updated_at
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
    workflow_from_rows(row, step_rows)
}

fn workflow_from_rows(
    row: WorkflowInstanceRow,
    step_rows: Vec<WorkflowStepRow>,
) -> Result<WorkflowInstance, WorkflowApiError> {
    let state = match row.state.as_str() {
        "running" => WorkflowInstanceState::Running,
        other => {
            return Err(WorkflowApiError::stored_state(format!(
                "Workflow Instance `{}` has unsupported state `{other}`",
                row.instance_id
            )));
        }
    };
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
    let steps = step_rows
        .into_iter()
        .map(|step| {
            let state = match step.state.as_str() {
                "pending" => WorkflowStepState::Pending,
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
            Ok(WorkflowStepInspection {
                step_id: step.step_id,
                definition_step_name: step.definition_step_name,
                position,
                state,
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
        initial_step_id: row.initial_step_id,
        steps,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}
