use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use platform_core::{AppContext, AppError, ErrorCode};
use platform_http::responses::{json, DataResponse};
use platform_http::{AdminActor, ApiErrorResponse, HttpRequestContext};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 100;

pub fn router() -> Router<AppContext> {
    Router::new()
        .route("/admin/runtime/outbox", get(list_outbox))
        .route("/admin/runtime/outbox/:id/retry", post(retry_outbox_event))
        .route("/admin/runtime/functions", get(list_function_runs))
        .route("/admin/runtime/functions/:id", get(get_function_run))
        .route(
            "/admin/runtime/functions/:id/retry",
            post(retry_function_run),
        )
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct OutboxQuery {
    pub status: Option<String>,
    pub event_name: Option<String>,
    pub limit: Option<i64>,
    pub created_before: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct FunctionRunQuery {
    pub status: Option<String>,
    pub function_name: Option<String>,
    pub limit: Option<i64>,
    pub created_before: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PageInfo {
    pub limit: i64,
    pub next_created_before: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminOutboxListResponse)]
pub struct AdminOutboxListResponse {
    pub data: Vec<AdminOutboxEvent>,
    pub page: PageInfo,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminFunctionRunListResponse)]
pub struct AdminFunctionRunListResponse {
    pub data: Vec<AdminFunctionRun>,
    pub page: PageInfo,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminFunctionRunResponse)]
pub struct AdminFunctionRunResponse {
    pub data: AdminFunctionRun,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminOutboxEvent {
    pub id: String,
    pub event_name: String,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub available_at: DateTime<Utc>,
    pub locked_by: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub correlation_id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminFunctionRun {
    pub id: String,
    pub function_name: String,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub available_at: DateTime<Utc>,
    pub locked_by: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub correlation_id: String,
    pub created_at: DateTime<Utc>,
}

async fn list_outbox(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Query(query): Query<OutboxQuery>,
) -> Result<Json<AdminOutboxListResponse>, ApiErrorResponse> {
    let limit = normalized_limit(query.limit);
    let rows = sqlx::query_as::<_, OutboxAdminRow>(
        r#"
        select
            id,
            event_name,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            published_at,
            last_error,
            correlation_id,
            created_at
        from platform.outbox
        where ($1::text is null or status = $1)
          and ($2::text is null or event_name = $2)
          and ($3::timestamptz is null or created_at < $3)
        order by created_at desc, id desc
        limit $4
        "#,
    )
    .bind(query.status)
    .bind(query.event_name)
    .bind(query.created_before)
    .bind(limit)
    .fetch_all(&ctx.db)
    .await
    .map_err(|source| {
        ApiErrorResponse::with_context(
            AppError::new(ErrorCode::Internal, "Runtime console query failed").with_source(source),
            &request_ctx,
        )
    })?;

    let data: Vec<AdminOutboxEvent> = rows.into_iter().map(Into::into).collect();
    Ok(Json(AdminOutboxListResponse {
        page: page_info(limit, data.last().map(|event| event.created_at)),
        data,
    }))
}

async fn list_function_runs(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Query(query): Query<FunctionRunQuery>,
) -> Result<Json<AdminFunctionRunListResponse>, ApiErrorResponse> {
    let limit = normalized_limit(query.limit);
    let rows = sqlx::query_as::<_, FunctionRunAdminRow>(
        r#"
        select
            id,
            function_name,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            started_at,
            completed_at,
            last_error,
            correlation_id,
            created_at
        from runtime.function_runs
        where ($1::text is null or status = $1)
          and ($2::text is null or function_name = $2)
          and ($3::timestamptz is null or created_at < $3)
        order by created_at desc, id desc
        limit $4
        "#,
    )
    .bind(query.status)
    .bind(query.function_name)
    .bind(query.created_before)
    .bind(limit)
    .fetch_all(&ctx.db)
    .await
    .map_err(|source| {
        ApiErrorResponse::with_context(
            AppError::new(ErrorCode::Internal, "Runtime console query failed").with_source(source),
            &request_ctx,
        )
    })?;

    let data: Vec<AdminFunctionRun> = rows.into_iter().map(Into::into).collect();
    Ok(Json(AdminFunctionRunListResponse {
        page: page_info(limit, data.last().map(|run| run.created_at)),
        data,
    }))
}

async fn get_function_run(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path(id): Path<String>,
) -> Result<Json<DataResponse<AdminFunctionRun>>, ApiErrorResponse> {
    let row = sqlx::query_as::<_, FunctionRunAdminRow>(
        r#"
        select
            id,
            function_name,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            started_at,
            completed_at,
            last_error,
            correlation_id,
            created_at
        from runtime.function_runs
        where id = $1
        "#,
    )
    .bind(&id)
    .fetch_optional(&ctx.db)
    .await
    .map_err(|source| {
        ApiErrorResponse::with_context(
            AppError::new(ErrorCode::Internal, "Runtime console query failed").with_source(source),
            &request_ctx,
        )
    })?
    .ok_or_else(|| {
        ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::NotFound,
                format!("Function run {id} was not found"),
            ),
            &request_ctx,
        )
    })?;

    Ok(json(row.into()))
}

async fn retry_outbox_event(
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path(id): Path<String>,
) -> Result<Json<DataResponse<AdminOutboxEvent>>, ApiErrorResponse> {
    let current = fetch_outbox_event(&ctx, &request_ctx, &id).await?;
    ensure_retryable_status("outbox event", &id, &current.status, &request_ctx)?;

    let row = sqlx::query_as::<_, OutboxAdminRow>(
        r#"
        update platform.outbox
        set status = 'pending',
            available_at = now(),
            locked_at = null,
            locked_by = null,
            last_error = null
        where id = $1
        returning
            id,
            event_name,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            published_at,
            last_error,
            correlation_id,
            created_at
        "#,
    )
    .bind(&id)
    .fetch_one(&ctx.db)
    .await
    .map_err(|source| query_error(source, &request_ctx))?;

    tracing::info!(
        actor = %admin_audit_label(&admin),
        target_type = "outbox",
        target_id = %id,
        previous_status = %current.status,
        new_status = "pending",
        correlation_id = %request_ctx.correlation_id.0,
        "runtime console retry requested"
    );

    Ok(json(row.into()))
}

async fn retry_function_run(
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path(id): Path<String>,
) -> Result<Json<DataResponse<AdminFunctionRun>>, ApiErrorResponse> {
    let current = fetch_function_run(&ctx, &request_ctx, &id).await?;
    ensure_retryable_status("function run", &id, &current.status, &request_ctx)?;

    let row = sqlx::query_as::<_, FunctionRunAdminRow>(
        r#"
        update runtime.function_runs
        set status = 'pending',
            available_at = now(),
            locked_at = null,
            locked_by = null,
            last_error = null,
            updated_at = now()
        where id = $1
        returning
            id,
            function_name,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            started_at,
            completed_at,
            last_error,
            correlation_id,
            created_at
        "#,
    )
    .bind(&id)
    .fetch_one(&ctx.db)
    .await
    .map_err(|source| query_error(source, &request_ctx))?;

    tracing::info!(
        actor = %admin_audit_label(&admin),
        target_type = "function_run",
        target_id = %id,
        previous_status = %current.status,
        new_status = "pending",
        correlation_id = %request_ctx.correlation_id.0,
        "runtime console retry requested"
    );

    Ok(json(row.into()))
}

type OutboxAdminRow = (
    String,
    String,
    String,
    i32,
    i32,
    DateTime<Utc>,
    Option<String>,
    Option<DateTime<Utc>>,
    Option<String>,
    String,
    DateTime<Utc>,
);

type FunctionRunAdminRow = (
    String,
    String,
    String,
    i32,
    i32,
    DateTime<Utc>,
    Option<String>,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    Option<String>,
    String,
    DateTime<Utc>,
);

impl From<OutboxAdminRow> for AdminOutboxEvent {
    fn from(row: OutboxAdminRow) -> Self {
        let (
            id,
            event_name,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            published_at,
            last_error,
            correlation_id,
            created_at,
        ) = row;

        Self {
            id,
            event_name,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            published_at,
            last_error,
            correlation_id,
            created_at,
        }
    }
}

impl From<FunctionRunAdminRow> for AdminFunctionRun {
    fn from(row: FunctionRunAdminRow) -> Self {
        let (
            id,
            function_name,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            started_at,
            completed_at,
            last_error,
            correlation_id,
            created_at,
        ) = row;

        Self {
            id,
            function_name,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            started_at,
            completed_at,
            last_error,
            correlation_id,
            created_at,
        }
    }
}

async fn fetch_outbox_event(
    ctx: &AppContext,
    request_ctx: &platform_core::RequestContext,
    id: &str,
) -> Result<AdminOutboxEvent, ApiErrorResponse> {
    let row = sqlx::query_as::<_, OutboxAdminRow>(
        r#"
        select
            id,
            event_name,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            published_at,
            last_error,
            correlation_id,
            created_at
        from platform.outbox
        where id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&ctx.db)
    .await
    .map_err(|source| query_error(source, request_ctx))?
    .ok_or_else(|| {
        ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::NotFound,
                format!("Outbox event {id} was not found"),
            ),
            request_ctx,
        )
    })?;

    Ok(row.into())
}

async fn fetch_function_run(
    ctx: &AppContext,
    request_ctx: &platform_core::RequestContext,
    id: &str,
) -> Result<AdminFunctionRun, ApiErrorResponse> {
    let row = sqlx::query_as::<_, FunctionRunAdminRow>(
        r#"
        select
            id,
            function_name,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            started_at,
            completed_at,
            last_error,
            correlation_id,
            created_at
        from runtime.function_runs
        where id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&ctx.db)
    .await
    .map_err(|source| query_error(source, request_ctx))?
    .ok_or_else(|| {
        ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::NotFound,
                format!("Function run {id} was not found"),
            ),
            request_ctx,
        )
    })?;

    Ok(row.into())
}

fn ensure_retryable_status(
    target_type: &str,
    id: &str,
    status: &str,
    request_ctx: &platform_core::RequestContext,
) -> Result<(), ApiErrorResponse> {
    if matches!(status, "failed" | "dead") {
        return Ok(());
    }

    Err(ApiErrorResponse::with_context(
        AppError::new(
            ErrorCode::Conflict,
            format!("{target_type} {id} cannot be retried from status {status}"),
        ),
        request_ctx,
    ))
}

fn query_error(
    source: sqlx::Error,
    request_ctx: &platform_core::RequestContext,
) -> ApiErrorResponse {
    ApiErrorResponse::with_context(
        AppError::new(ErrorCode::Internal, "Runtime console query failed").with_source(source),
        request_ctx,
    )
}

fn admin_audit_label(actor: &AdminActor) -> String {
    match actor {
        AdminActor::Service { service_id, .. } => format!("service:{service_id}"),
        AdminActor::System => "system".to_owned(),
    }
}

fn normalized_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

fn page_info(limit: i64, next_created_before: Option<DateTime<Utc>>) -> PageInfo {
    PageInfo {
        limit,
        next_created_before,
    }
}
