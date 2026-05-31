use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use platform_core::{AppContext, AppError, ErrorCode};
use platform_http::responses::{json, DataResponse};
use platform_http::{AdminActor, ApiErrorResponse, HttpRequestContext};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use utoipa::{IntoParams, ToSchema};

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 100;

pub fn router() -> Router<AppContext> {
    Router::new()
        .route("/admin/runtime/summary", get(get_summary))
        .route("/admin/runtime/timeline/:correlation_id", get(get_timeline))
        .route("/admin/runtime/outbox", get(list_outbox))
        .route("/admin/runtime/outbox/:id", get(get_outbox_event))
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

#[derive(Debug, Deserialize, IntoParams)]
pub struct TimelineQuery {
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
#[schema(as = AdminOutboxEventDetailResponse)]
pub struct AdminOutboxEventDetailResponse {
    pub data: AdminOutboxEventDetail,
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
    pub data: AdminFunctionRunDetail,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminRuntimeTimelineResponse)]
pub struct AdminRuntimeTimelineResponse {
    pub data: Vec<AdminRuntimeTimelineItem>,
    pub page: PageInfo,
    pub order: &'static str,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminRuntimeSummaryResponse)]
pub struct AdminRuntimeSummaryResponse {
    pub status: String,
    pub outbox: AdminRuntimeOutboxSummary,
    pub functions: AdminRuntimeFunctionSummary,
    pub recent_activity: Vec<AdminRuntimeSummaryItem>,
    pub recent_failures: Vec<AdminRuntimeSummaryItem>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminRuntimeOutboxSummary {
    pub pending: i64,
    pub processing: i64,
    pub published: i64,
    pub failed: i64,
    pub dead: i64,
    pub oldest_pending_age_seconds: Option<i64>,
    pub oldest_failed_age_seconds: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminRuntimeFunctionSummary {
    pub pending: i64,
    pub running: i64,
    pub completed: i64,
    pub failed: i64,
    pub dead: i64,
    pub oldest_pending_age_seconds: Option<i64>,
    pub oldest_failed_age_seconds: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminRuntimeSummaryItem {
    #[serde(rename = "type")]
    pub item_type: String,
    pub id: String,
    pub name: String,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub correlation_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_error: Option<String>,
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
pub struct AdminOutboxEventDetail {
    pub id: String,
    pub event_name: String,
    pub event_version: i32,
    pub source_module: String,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub available_at: DateTime<Utc>,
    pub locked_by: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub correlation_id: String,
    pub causation_id: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub payload: Value,
    pub actor: Value,
    pub trace: Value,
    pub headers: Value,
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

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminFunctionRunDetail {
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
    pub input_json: Value,
    pub actor: Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminRuntimeTimelineItem {
    #[serde(rename = "type")]
    pub item_type: String,
    pub id: String,
    pub name: String,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub correlation_id: String,
}

async fn get_summary(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
) -> Result<Json<AdminRuntimeSummaryResponse>, ApiErrorResponse> {
    let outbox_row = sqlx::query_as::<_, SummaryCountRow>(
        r#"
        select
            count(*) filter (where status = 'pending')::bigint as pending,
            count(*) filter (where status = 'processing')::bigint as processing,
            count(*) filter (where status = 'published')::bigint as published,
            count(*) filter (where status = 'failed')::bigint as failed,
            count(*) filter (where status = 'dead')::bigint as dead,
            extract(epoch from now() - min(created_at) filter (where status = 'pending'))::bigint
                as oldest_pending_age_seconds,
            extract(epoch from now() - min(created_at) filter (where status in ('failed', 'dead')))::bigint
                as oldest_failed_age_seconds
        from platform.outbox
        "#,
    )
    .fetch_one(&ctx.db)
    .await
    .map_err(|source| query_error(source, &request_ctx))?;

    let function_row = sqlx::query_as::<_, SummaryCountRow>(
        r#"
        select
            count(*) filter (where status = 'pending')::bigint as pending,
            count(*) filter (where status in ('processing', 'running'))::bigint as running,
            count(*) filter (where status = 'completed')::bigint as completed,
            count(*) filter (where status = 'failed')::bigint as failed,
            count(*) filter (where status = 'dead')::bigint as dead,
            extract(epoch from now() - min(created_at) filter (where status = 'pending'))::bigint
                as oldest_pending_age_seconds,
            extract(epoch from now() - min(created_at) filter (where status in ('failed', 'dead')))::bigint
                as oldest_failed_age_seconds
        from runtime.function_runs
        "#,
    )
    .fetch_one(&ctx.db)
    .await
    .map_err(|source| query_error(source, &request_ctx))?;

    let outbox = AdminRuntimeOutboxSummary::from(outbox_row);
    let functions = AdminRuntimeFunctionSummary::from(function_row);
    let recent_activity = fetch_summary_items(&ctx, &request_ctx, false).await?;
    let recent_failures = fetch_summary_items(&ctx, &request_ctx, true).await?;
    let status = runtime_status(&outbox, &functions).to_owned();

    Ok(Json(AdminRuntimeSummaryResponse {
        status,
        outbox,
        functions,
        recent_activity,
        recent_failures,
    }))
}

async fn get_timeline(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path(correlation_id): Path<String>,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<AdminRuntimeTimelineResponse>, ApiErrorResponse> {
    let limit = normalized_limit(query.limit);
    let rows = sqlx::query_as::<_, TimelineRow>(
        r#"
        select *
        from (
            select
                'outbox_event'::text as item_type,
                id,
                event_name as name,
                status,
                attempts,
                max_attempts,
                created_at,
                locked_at as started_at,
                published_at as completed_at,
                last_error,
                correlation_id
            from platform.outbox
            where correlation_id = $1
              and ($2::timestamptz is null or created_at < $2)

            union all

            select
                'function_run'::text as item_type,
                id,
                function_name as name,
                status,
                attempts,
                max_attempts,
                created_at,
                coalesce(started_at, locked_at) as started_at,
                completed_at,
                last_error,
                correlation_id
            from runtime.function_runs
            where correlation_id = $1
              and ($2::timestamptz is null or created_at < $2)
        ) timeline
        order by created_at asc, item_type asc, id asc
        limit $3
        "#,
    )
    .bind(&correlation_id)
    .bind(query.created_before)
    .bind(limit)
    .fetch_all(&ctx.db)
    .await
    .map_err(|source| query_error(source, &request_ctx))?;

    let data: Vec<AdminRuntimeTimelineItem> = rows.into_iter().map(Into::into).collect();
    Ok(Json(AdminRuntimeTimelineResponse {
        page: page_info(limit, data.last().map(|item| item.created_at)),
        data,
        order: "created_at_asc",
    }))
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

async fn get_outbox_event(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path(id): Path<String>,
) -> Result<Json<DataResponse<AdminOutboxEventDetail>>, ApiErrorResponse> {
    let row = fetch_outbox_event_detail(&ctx, &request_ctx, &id).await?;
    Ok(json(row))
}

async fn get_function_run(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path(id): Path<String>,
) -> Result<Json<DataResponse<AdminFunctionRunDetail>>, ApiErrorResponse> {
    let row = sqlx::query_as::<_, FunctionRunDetailRow>(
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
            created_at,
            input_json,
            actor
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

type OutboxDetailRow = (
    String,
    String,
    i32,
    String,
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
    Option<String>,
    DateTime<Utc>,
    DateTime<Utc>,
    Value,
    Value,
);

type FunctionRunDetailRow = (
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
    Value,
    Value,
);

type TimelineRow = (
    String,
    String,
    String,
    String,
    i32,
    i32,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    Option<String>,
    String,
);

type SummaryCountRow = (i64, i64, i64, i64, i64, Option<i64>, Option<i64>);

type SummaryItemRow = (
    String,
    String,
    String,
    String,
    i32,
    i32,
    Option<String>,
    DateTime<Utc>,
    Option<String>,
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

impl From<SummaryCountRow> for AdminRuntimeOutboxSummary {
    fn from(row: SummaryCountRow) -> Self {
        let (
            pending,
            processing,
            published,
            failed,
            dead,
            oldest_pending_age_seconds,
            oldest_failed_age_seconds,
        ) = row;

        Self {
            pending,
            processing,
            published,
            failed,
            dead,
            oldest_pending_age_seconds,
            oldest_failed_age_seconds,
        }
    }
}

impl From<SummaryCountRow> for AdminRuntimeFunctionSummary {
    fn from(row: SummaryCountRow) -> Self {
        let (
            pending,
            running,
            completed,
            failed,
            dead,
            oldest_pending_age_seconds,
            oldest_failed_age_seconds,
        ) = row;

        Self {
            pending,
            running,
            completed,
            failed,
            dead,
            oldest_pending_age_seconds,
            oldest_failed_age_seconds,
        }
    }
}

impl From<SummaryItemRow> for AdminRuntimeSummaryItem {
    fn from(row: SummaryItemRow) -> Self {
        let (
            item_type,
            id,
            name,
            status,
            attempts,
            max_attempts,
            correlation_id,
            created_at,
            last_error,
        ) = row;

        Self {
            item_type,
            id,
            name,
            status,
            attempts,
            max_attempts,
            correlation_id,
            created_at,
            last_error,
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

impl From<OutboxDetailRow> for AdminOutboxEventDetail {
    fn from(row: OutboxDetailRow) -> Self {
        let (
            id,
            event_name,
            event_version,
            source_module,
            aggregate_type,
            aggregate_id,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            published_at,
            last_error,
            correlation_id,
            causation_id,
            occurred_at,
            created_at,
            payload,
            headers,
        ) = row;

        let actor = headers
            .get("actor")
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default()));
        let trace = headers
            .get("trace")
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default()));

        Self {
            id,
            event_name,
            event_version,
            source_module,
            aggregate_type,
            aggregate_id,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            published_at,
            last_error,
            correlation_id,
            causation_id,
            occurred_at,
            created_at,
            payload,
            actor,
            trace,
            headers,
        }
    }
}

impl From<FunctionRunDetailRow> for AdminFunctionRunDetail {
    fn from(row: FunctionRunDetailRow) -> Self {
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
            input_json,
            actor,
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
            input_json,
            actor,
        }
    }
}

impl From<TimelineRow> for AdminRuntimeTimelineItem {
    fn from(row: TimelineRow) -> Self {
        let (
            item_type,
            id,
            name,
            status,
            attempts,
            max_attempts,
            created_at,
            started_at,
            completed_at,
            last_error,
            correlation_id,
        ) = row;

        Self {
            item_type,
            id,
            name,
            status,
            attempts,
            max_attempts,
            created_at,
            started_at,
            completed_at,
            last_error,
            correlation_id,
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

async fn fetch_outbox_event_detail(
    ctx: &AppContext,
    request_ctx: &platform_core::RequestContext,
    id: &str,
) -> Result<AdminOutboxEventDetail, ApiErrorResponse> {
    let row = sqlx::query(
        r#"
        select
            id,
            event_name,
            event_version,
            source_module,
            aggregate_type,
            aggregate_id,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            published_at,
            last_error,
            correlation_id,
            causation_id,
            occurred_at,
            created_at,
            payload,
            headers
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

    outbox_detail_from_row(&row).map_err(|source| query_error(source, request_ctx))
}

fn outbox_detail_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<AdminOutboxEventDetail, sqlx::Error> {
    let headers: Value = row.try_get("headers")?;
    let actor = headers
        .get("actor")
        .cloned()
        .unwrap_or_else(|| Value::Object(Default::default()));
    let trace = headers
        .get("trace")
        .cloned()
        .unwrap_or_else(|| Value::Object(Default::default()));

    Ok(AdminOutboxEventDetail {
        id: row.try_get("id")?,
        event_name: row.try_get("event_name")?,
        event_version: row.try_get("event_version")?,
        source_module: row.try_get("source_module")?,
        aggregate_type: row.try_get("aggregate_type")?,
        aggregate_id: row.try_get("aggregate_id")?,
        status: row.try_get("status")?,
        attempts: row.try_get("attempts")?,
        max_attempts: row.try_get("max_attempts")?,
        available_at: row.try_get("available_at")?,
        locked_by: row.try_get("locked_by")?,
        published_at: row.try_get("published_at")?,
        last_error: row.try_get("last_error")?,
        correlation_id: row.try_get("correlation_id")?,
        causation_id: row.try_get("causation_id")?,
        occurred_at: row.try_get("occurred_at")?,
        created_at: row.try_get("created_at")?,
        payload: row.try_get("payload")?,
        actor,
        trace,
        headers,
    })
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

async fn fetch_summary_items(
    ctx: &AppContext,
    request_ctx: &platform_core::RequestContext,
    failures_only: bool,
) -> Result<Vec<AdminRuntimeSummaryItem>, ApiErrorResponse> {
    let rows = sqlx::query_as::<_, SummaryItemRow>(
        r#"
        select *
        from (
            select
                'outbox_event'::text as item_type,
                id,
                event_name as name,
                status,
                attempts,
                max_attempts,
                correlation_id,
                created_at,
                last_error
            from platform.outbox
            where (not $1 or status in ('failed', 'dead'))

            union all

            select
                'function_run'::text as item_type,
                id,
                function_name as name,
                status,
                attempts,
                max_attempts,
                correlation_id,
                created_at,
                last_error
            from runtime.function_runs
            where (not $1 or status in ('failed', 'dead'))
        ) summary_items
        order by created_at desc, item_type asc, id desc
        limit 10
        "#,
    )
    .bind(failures_only)
    .fetch_all(&ctx.db)
    .await
    .map_err(|source| query_error(source, request_ctx))?;

    Ok(rows.into_iter().map(Into::into).collect())
}

fn runtime_status(
    outbox: &AdminRuntimeOutboxSummary,
    functions: &AdminRuntimeFunctionSummary,
) -> &'static str {
    if outbox.dead > 0 || functions.dead > 0 {
        return "failing";
    }

    if outbox.failed > 0 || functions.failed > 0 {
        return "degraded";
    }

    "healthy"
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
