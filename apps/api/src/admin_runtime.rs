use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use platform_core::{AppContext, AppError, ErrorCode, TelemetrySpan, TelemetrySpanQuery};
use platform_http::responses::{DataResponse, json};
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
        .route(
            "/admin/runtime/timeline/{correlation_id}",
            get(get_timeline),
        )
        .route("/admin/runtime/heatmap", get(get_heatmap))
        .route("/admin/runtime/stories", get(list_stories))
        .route("/admin/runtime/stories/{correlation_id}", get(get_story))
        .route(
            "/admin/runtime/stories/{correlation_id}/technical-operations",
            get(get_story_technical_operations),
        )
        .route(
            "/admin/runtime/executions/{node_id}/technical-operations",
            get(get_execution_technical_operations),
        )
        .route("/admin/runtime/outbox", get(list_outbox))
        .route("/admin/runtime/outbox/{id}", get(get_outbox_event))
        .route("/admin/runtime/outbox/{id}/retry", post(retry_outbox_event))
        .route("/admin/runtime/functions", get(list_function_runs))
        .route("/admin/runtime/functions/{id}", get(get_function_run))
        .route(
            "/admin/runtime/functions/{id}/retry",
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

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct StoryQuery {
    pub limit: Option<i64>,
    pub created_before: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct HeatmapQuery {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub bucket_seconds: Option<i64>,
    pub status: Option<String>,
    pub event_name: Option<String>,
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
#[schema(as = AdminRuntimeStoryListResponse)]
pub struct AdminRuntimeStoryListResponse {
    pub data: Vec<AdminRuntimeStoryListItem>,
    pub page: PageInfo,
    pub order: &'static str,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminRuntimeStoryDetailResponse)]
pub struct AdminRuntimeStoryDetailResponse {
    pub data: AdminRuntimeStoryDetail,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminRuntimeHeatmapResponse)]
pub struct AdminRuntimeHeatmapResponse {
    pub data: Vec<AdminRuntimeHeatmapCell>,
    pub bucket_seconds: i64,
    pub page: PageInfo,
    pub order: &'static str,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminRuntimeTechnicalOperationListResponse)]
pub struct AdminRuntimeTechnicalOperationListResponse {
    pub data: Vec<AdminRuntimeTechnicalOperation>,
    pub order: &'static str,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminRuntimeTechnicalOperation {
    pub id: String,
    pub story_id: String,
    pub correlation_id: String,
    pub related_node_id: Option<String>,
    pub category: String,
    pub name: String,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub duration_ms: i64,
    pub attributes: Value,
    pub source: String,
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
#[schema(as = AdminRuntimeOutboxItem)]
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
#[schema(as = AdminRuntimeFunctionRunItem)]
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
    pub related_node_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AdminRuntimeStoryListItem {
    pub title: String,
    pub correlation_id: String,
    pub status: String,
    pub duration: i64,
    pub node_count: usize,
    pub error_count: usize,
    pub services: Vec<String>,
    pub pattern: Vec<String>,
    pub root_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminRuntimeStoryDetail {
    pub summary: AdminRuntimeStoryListItem,
    pub nodes: Vec<AdminRuntimeStoryNode>,
    pub edges: Vec<AdminRuntimeStoryEdge>,
    pub timeline_items: Vec<AdminRuntimeTimelineItem>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AdminRuntimeStoryNode {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub name: String,
    pub status: String,
    pub service: String,
    pub timestamp: DateTime<Utc>,
    pub duration_ms: i64,
    pub error: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminRuntimeStoryEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    #[serde(rename = "type")]
    pub edge_type: String,
    pub label: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminRuntimeHeatmapCell {
    pub bucket_start: DateTime<Utc>,
    pub bucket_end: DateTime<Utc>,
    pub service: String,
    pub node_type: String,
    pub total_count: i64,
    pub error_count: i64,
    pub retry_count: i64,
    pub dead_count: i64,
    pub avg_duration_ms: Option<i64>,
    pub max_duration_ms: Option<i64>,
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

async fn get_heatmap(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Query(query): Query<HeatmapQuery>,
) -> Result<Json<AdminRuntimeHeatmapResponse>, ApiErrorResponse> {
    let limit = normalized_limit(query.limit);
    let bucket_seconds = normalized_bucket_seconds(query.bucket_seconds);
    let rows = sqlx::query_as::<_, HeatmapRow>(
        r#"
        with runtime_items as (
            select
                created_at,
                source_module as service,
                'event'::text as node_type,
                status,
                attempts,
                case
                    when locked_at is not null and published_at is not null then
                        greatest(
                            0,
                            extract(epoch from published_at - locked_at)::bigint * 1000
                        )
                    else null::bigint
                end as duration_ms
            from platform.outbox
            where ($1::timestamptz is null or created_at < $1)
              and ($4::timestamptz is null or created_at >= $4)
              and ($5::timestamptz is null or created_at < $5)
              and ($6::text is null or status = $6)
              and ($7::text is null or event_name = $7)
              and ($8::text is null)

            union all

            select
                created_at,
                split_part(function_name, '.', 1) as service,
                'function'::text as node_type,
                status,
                attempts,
                case
                    when coalesce(started_at, locked_at) is not null and completed_at is not null then
                        greatest(
                            0,
                            extract(epoch from completed_at - coalesce(started_at, locked_at))::bigint * 1000
                        )
                    else null::bigint
                end as duration_ms
            from runtime.function_runs
            where ($1::timestamptz is null or created_at < $1)
              and ($4::timestamptz is null or created_at >= $4)
              and ($5::timestamptz is null or created_at < $5)
              and ($6::text is null or status = $6)
              and ($7::text is null)
              and ($8::text is null or function_name = $8)
        ),
        heatmap as (
            select
                to_timestamp(
                    floor(extract(epoch from created_at) / $2::double precision) * $2
                )::timestamptz as bucket_start,
                service,
                node_type,
                count(*)::bigint as total_count,
                count(*) filter (where status in ('failed', 'dead'))::bigint as error_count,
                count(*) filter (where attempts > 1)::bigint as retry_count,
                count(*) filter (where status = 'dead')::bigint as dead_count,
                avg(duration_ms)::bigint as avg_duration_ms,
                max(duration_ms)::bigint as max_duration_ms
            from runtime_items
            group by bucket_start, service, node_type
        )
        select
            bucket_start,
            bucket_start + ($2::bigint * interval '1 second') as bucket_end,
            service,
            node_type,
            total_count,
            error_count,
            retry_count,
            dead_count,
            avg_duration_ms,
            max_duration_ms
        from heatmap
        order by bucket_start desc, service asc, node_type asc
        limit $3
        "#,
    )
    .bind(query.created_before)
    .bind(bucket_seconds)
    .bind(limit)
    .bind(query.from)
    .bind(query.to)
    .bind(query.status)
    .bind(query.event_name)
    .bind(query.function_name)
    .fetch_all(&ctx.db)
    .await
    .map_err(|source| query_error(source, &request_ctx))?;

    let data: Vec<AdminRuntimeHeatmapCell> = rows.into_iter().map(Into::into).collect();
    Ok(Json(AdminRuntimeHeatmapResponse {
        page: page_info(limit, data.last().map(|cell| cell.bucket_start)),
        data,
        bucket_seconds,
        order: "bucket_start_desc",
    }))
}

async fn list_stories(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Query(query): Query<StoryQuery>,
) -> Result<Json<AdminRuntimeStoryListResponse>, ApiErrorResponse> {
    let limit = normalized_limit(query.limit);
    let rows = fetch_story_rows(&ctx, &request_ctx, None, query.created_before, limit).await?;
    let stories = build_story_summaries(rows);

    Ok(Json(AdminRuntimeStoryListResponse {
        page: page_info(limit, stories.last().map(|story| story.updated_at)),
        data: stories,
        order: "updated_at_desc",
    }))
}

async fn get_story(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path(correlation_id): Path<String>,
) -> Result<Json<AdminRuntimeStoryDetailResponse>, ApiErrorResponse> {
    let rows = fetch_story_rows(&ctx, &request_ctx, Some(&correlation_id), None, MAX_LIMIT).await?;
    if rows.is_empty() {
        return Err(ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::NotFound,
                format!("Runtime story {correlation_id} was not found"),
            ),
            &request_ctx,
        ));
    }

    Ok(Json(AdminRuntimeStoryDetailResponse {
        data: build_story_detail(rows),
    }))
}

async fn get_story_technical_operations(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path(correlation_id): Path<String>,
) -> Result<Json<AdminRuntimeTechnicalOperationListResponse>, ApiErrorResponse> {
    let rows = fetch_story_rows(&ctx, &request_ctx, Some(&correlation_id), None, MAX_LIMIT).await?;
    if rows.is_empty() {
        return Err(ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::NotFound,
                format!("Runtime story {correlation_id} was not found"),
            ),
            &request_ctx,
        ));
    }

    let spans = ctx
        .telemetry_spans
        .query_spans(TelemetrySpanQuery::by_correlation_id(&correlation_id))
        .await
        .map_err(|source| ApiErrorResponse::with_context(source, &request_ctx))?;
    let data = technical_operations_from_spans(spans, &runtime_node_index(&rows));

    Ok(Json(AdminRuntimeTechnicalOperationListResponse {
        data,
        order: "started_at_asc",
    }))
}

async fn get_execution_technical_operations(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path(node_id): Path<String>,
) -> Result<Json<AdminRuntimeTechnicalOperationListResponse>, ApiErrorResponse> {
    let node = fetch_runtime_node_ref(&ctx, &request_ctx, &node_id).await?;
    let query = match node.item_type.as_str() {
        "function" => TelemetrySpanQuery::by_function_run_id(&node.id),
        _ => TelemetrySpanQuery::by_outbox_event_id(&node.id),
    };
    let spans = ctx
        .telemetry_spans
        .query_spans(query)
        .await
        .map_err(|source| ApiErrorResponse::with_context(source, &request_ctx))?;
    let data = technical_operations_from_spans(spans, &RuntimeNodeIndex::single(node.id));

    Ok(Json(AdminRuntimeTechnicalOperationListResponse {
        data,
        order: "started_at_asc",
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

type HeatmapRow = (
    DateTime<Utc>,
    DateTime<Utc>,
    String,
    String,
    i64,
    i64,
    i64,
    i64,
    Option<i64>,
    Option<i64>,
);

type RuntimeNodeRefTuple = (String, String, String);

#[derive(Debug, Clone)]
struct RuntimeNodeRef {
    id: String,
    item_type: String,
}

#[derive(Debug, Clone)]
struct StoryWorkRow {
    item_type: String,
    id: String,
    name: String,
    status: String,
    attempts: i32,
    max_attempts: i32,
    correlation_id: String,
    causation_id: Option<String>,
    service: String,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    last_error: Option<String>,
    metadata: Value,
}

type StoryWorkTuple = (
    String,
    String,
    String,
    String,
    i32,
    i32,
    String,
    Option<String>,
    String,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    Option<String>,
    Value,
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
        let related_node_id = Some(id.clone());

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
            related_node_id,
        }
    }
}

impl From<StoryWorkTuple> for StoryWorkRow {
    fn from(row: StoryWorkTuple) -> Self {
        let (
            item_type,
            id,
            name,
            status,
            attempts,
            max_attempts,
            correlation_id,
            causation_id,
            service,
            created_at,
            started_at,
            completed_at,
            last_error,
            metadata,
        ) = row;

        Self {
            item_type,
            id,
            name,
            status,
            attempts,
            max_attempts,
            correlation_id,
            causation_id,
            service,
            created_at,
            started_at,
            completed_at,
            last_error,
            metadata,
        }
    }
}

impl From<HeatmapRow> for AdminRuntimeHeatmapCell {
    fn from(row: HeatmapRow) -> Self {
        let (
            bucket_start,
            bucket_end,
            service,
            node_type,
            total_count,
            error_count,
            retry_count,
            dead_count,
            avg_duration_ms,
            max_duration_ms,
        ) = row;

        Self {
            bucket_start,
            bucket_end,
            service,
            node_type,
            total_count,
            error_count,
            retry_count,
            dead_count,
            avg_duration_ms,
            max_duration_ms,
        }
    }
}

impl From<RuntimeNodeRefTuple> for RuntimeNodeRef {
    fn from(row: RuntimeNodeRefTuple) -> Self {
        let (id, item_type, _correlation_id) = row;
        Self { id, item_type }
    }
}

impl From<&StoryWorkRow> for AdminRuntimeTimelineItem {
    fn from(row: &StoryWorkRow) -> Self {
        Self {
            item_type: timeline_item_type(&row.item_type, &row.status, row.attempts).to_owned(),
            id: row.id.clone(),
            name: row.name.clone(),
            status: row.status.clone(),
            attempts: row.attempts,
            max_attempts: row.max_attempts,
            created_at: row.created_at,
            started_at: row.started_at,
            completed_at: row.completed_at,
            last_error: row.last_error.clone(),
            correlation_id: row.correlation_id.clone(),
            related_node_id: Some(row.id.clone()),
        }
    }
}

fn timeline_item_type(item_type: &str, status: &str, attempts: i32) -> &'static str {
    if status == "dead" {
        return "dead_letter";
    }
    if status == "failed" {
        return "failure";
    }
    if attempts > 1 {
        return "retry";
    }
    match item_type {
        "event" | "outbox_event" => "outbox_event",
        "function" | "function_run" => "function_run",
        _ => "runtime",
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

async fn fetch_runtime_node_ref(
    ctx: &AppContext,
    request_ctx: &platform_core::RequestContext,
    node_id: &str,
) -> Result<RuntimeNodeRef, ApiErrorResponse> {
    let row = sqlx::query_as::<_, RuntimeNodeRefTuple>(
        r#"
        select id, item_type, correlation_id
        from (
            select
                id,
                'event'::text as item_type,
                correlation_id
            from platform.outbox
            where id = $1

            union all

            select
                id,
                'function'::text as item_type,
                correlation_id
            from runtime.function_runs
            where id = $1
        ) runtime_nodes
        order by item_type asc
        limit 1
        "#,
    )
    .bind(node_id)
    .fetch_optional(&ctx.db)
    .await
    .map_err(|source| query_error(source, request_ctx))?
    .ok_or_else(|| {
        ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::NotFound,
                format!("Runtime execution node {node_id} was not found"),
            ),
            request_ctx,
        )
    })?;

    Ok(row.into())
}

async fn fetch_story_rows(
    ctx: &AppContext,
    request_ctx: &platform_core::RequestContext,
    correlation_id: Option<&str>,
    created_before: Option<DateTime<Utc>>,
    limit: i64,
) -> Result<Vec<StoryWorkRow>, ApiErrorResponse> {
    let rows = sqlx::query_as::<_, StoryWorkTuple>(
        r#"
        with story_keys as (
            select correlation_id, max(updated_at) as updated_at
            from (
                select
                    correlation_id,
                    coalesce(published_at, locked_at, created_at) as updated_at
                from platform.outbox
                where ($1::text is null or correlation_id = $1)

                union all

                select
                    correlation_id,
                    coalesce(completed_at, started_at, locked_at, created_at) as updated_at
                from runtime.function_runs
                where ($1::text is null or correlation_id = $1)
            ) story_items
            group by correlation_id
            having ($2::timestamptz is null or max(updated_at) < $2)
            order by updated_at desc, correlation_id asc
            limit $3
        )
        select *
        from (
            select
                'event'::text as item_type,
                id,
                event_name as name,
                status,
                attempts,
                max_attempts,
                correlation_id,
                causation_id,
                source_module as service,
                created_at,
                locked_at as started_at,
                published_at as completed_at,
                last_error,
                headers as metadata
            from platform.outbox
            where correlation_id in (select correlation_id from story_keys)

            union all

            select
                'function'::text as item_type,
                id,
                function_name as name,
                status,
                attempts,
                max_attempts,
                correlation_id,
                null::text as causation_id,
                split_part(function_name, '.', 1) as service,
                created_at,
                coalesce(started_at, locked_at) as started_at,
                completed_at,
                last_error,
                input_json as metadata
            from runtime.function_runs
            where correlation_id in (select correlation_id from story_keys)
        ) story_rows
        order by correlation_id asc, created_at asc, item_type asc, id asc
        "#,
    )
    .bind(correlation_id)
    .bind(created_before)
    .bind(limit)
    .fetch_all(&ctx.db)
    .await
    .map_err(|source| query_error(source, request_ctx))?;

    Ok(rows.into_iter().map(Into::into).collect())
}

fn build_story_summaries(rows: Vec<StoryWorkRow>) -> Vec<AdminRuntimeStoryListItem> {
    let mut grouped: Vec<(String, Vec<StoryWorkRow>)> = Vec::new();
    for row in rows {
        if let Some((_, items)) = grouped
            .iter_mut()
            .find(|(correlation_id, _)| correlation_id == &row.correlation_id)
        {
            items.push(row);
        } else {
            grouped.push((row.correlation_id.clone(), vec![row]));
        }
    }

    let mut summaries: Vec<AdminRuntimeStoryListItem> = grouped
        .into_iter()
        .map(|(_, items)| build_story_summary(&items))
        .collect();
    summaries.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.correlation_id.cmp(&right.correlation_id))
    });
    summaries
}

fn build_story_detail(rows: Vec<StoryWorkRow>) -> AdminRuntimeStoryDetail {
    let summary = build_story_summary(&rows);
    let edges = build_story_edges(&rows);
    let connected_ids = connected_node_ids(&edges);
    let nodes: Vec<AdminRuntimeStoryNode> = rows
        .iter()
        .map(|row| build_story_node(row, &connected_ids))
        .collect();
    let timeline_items = rows.iter().map(Into::into).collect();

    AdminRuntimeStoryDetail {
        summary,
        nodes,
        edges,
        timeline_items,
    }
}

#[derive(Debug, Clone, Default)]
struct RuntimeNodeIndex {
    ids: std::collections::BTreeSet<String>,
}

impl RuntimeNodeIndex {
    fn single(id: String) -> Self {
        Self {
            ids: std::collections::BTreeSet::from([id]),
        }
    }

    fn contains(&self, id: &str) -> bool {
        self.ids.contains(id)
    }
}

fn runtime_node_index(rows: &[StoryWorkRow]) -> RuntimeNodeIndex {
    RuntimeNodeIndex {
        ids: rows.iter().map(|row| row.id.clone()).collect(),
    }
}

fn technical_operations_from_spans(
    spans: Vec<TelemetrySpan>,
    node_index: &RuntimeNodeIndex,
) -> Vec<AdminRuntimeTechnicalOperation> {
    let mut operations = spans
        .into_iter()
        .map(|span| technical_operation_from_span(span, node_index))
        .collect::<Vec<_>>();
    operations.sort_by(|left, right| {
        left.started_at
            .cmp(&right.started_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    operations
}

fn technical_operation_from_span(
    span: TelemetrySpan,
    node_index: &RuntimeNodeIndex,
) -> AdminRuntimeTechnicalOperation {
    let correlation_id = span_attribute(&span.attributes, "lenso.correlation_id")
        .or_else(|| span_attribute(&span.attributes, "lenso.story_id"))
        .unwrap_or("unknown")
        .to_owned();
    let story_id = span_attribute(&span.attributes, "lenso.story_id")
        .unwrap_or(&correlation_id)
        .to_owned();
    let duration_ms = span
        .ended_at
        .signed_duration_since(span.started_at)
        .num_milliseconds()
        .max(0);
    let attributes = safe_span_attributes(&span.attributes);
    let category = technical_operation_category(&span);
    let related_node_id = related_node_id(&span.attributes, node_index);
    let status = technical_operation_status(&span);

    AdminRuntimeTechnicalOperation {
        attributes,
        category,
        correlation_id,
        duration_ms,
        ended_at: span.ended_at,
        id: span.id,
        name: span.name,
        related_node_id,
        source: "otel".to_owned(),
        started_at: span.started_at,
        status,
        story_id,
    }
}

fn related_node_id(attributes: &Value, node_index: &RuntimeNodeIndex) -> Option<String> {
    for key in ["lenso.function_run_id", "lenso.outbox_event_id"] {
        let Some(id) = span_attribute(attributes, key) else {
            continue;
        };
        if node_index.contains(id) {
            return Some(id.to_owned());
        }
    }

    None
}

fn technical_operation_category(span: &TelemetrySpan) -> String {
    if has_attribute_with_prefix(&span.attributes, "redis.")
        || span_attribute(&span.attributes, "db.system") == Some("redis")
    {
        return "redis".to_owned();
    }
    if has_attribute_with_prefix(&span.attributes, "db.") {
        return "db".to_owned();
    }
    if has_attribute_with_prefix(&span.attributes, "http.")
        || matches!(
            span.name.split_whitespace().next(),
            Some("GET" | "POST" | "PUT" | "PATCH" | "DELETE")
        )
    {
        return "http".to_owned();
    }
    if has_attribute_with_prefix(&span.attributes, "aws.s3.")
        || has_attribute_with_prefix(&span.attributes, "s3.")
    {
        return "s3".to_owned();
    }
    if has_attribute_with_prefix(&span.attributes, "aws.ses.")
        || has_attribute_with_prefix(&span.attributes, "ses.")
    {
        return "ses".to_owned();
    }

    match span_attribute(&span.attributes, "lenso.execution.kind") {
        Some("worker_loop" | "outbox_claim" | "function_claim") => "worker".to_owned(),
        Some("outbox_event" | "function_run" | "runtime") => "runtime".to_owned(),
        _ if has_attribute_with_prefix(&span.attributes, "rpc.")
            || has_attribute_with_prefix(&span.attributes, "peer.")
            || has_attribute_with_prefix(&span.attributes, "net.peer.") =>
        {
            "external".to_owned()
        }
        _ => "unknown".to_owned(),
    }
}

fn technical_operation_status(span: &TelemetrySpan) -> String {
    let raw = span
        .status
        .as_deref()
        .or_else(|| span_attribute(&span.attributes, "otel.status_code"));
    match raw.map(str::to_ascii_lowercase).as_deref() {
        Some("ok" | "success") => "ok".to_owned(),
        Some("error" | "err" | "failed" | "failure") => "error".to_owned(),
        Some("unset" | "unknown") | None => {
            if span.attributes.get("error.type").is_some() {
                "error".to_owned()
            } else {
                "unknown".to_owned()
            }
        }
        Some(_) => "unknown".to_owned(),
    }
}

fn safe_span_attributes(attributes: &Value) -> Value {
    let Some(map) = attributes.as_object() else {
        return Value::Object(Default::default());
    };

    let mut safe = serde_json::Map::new();
    for (key, value) in map {
        if is_safe_span_attribute(key) && is_safe_attribute_value(value) {
            safe.insert(key.clone(), value.clone());
        }
    }

    Value::Object(safe)
}

fn is_safe_span_attribute(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    if [
        "authorization",
        "cookie",
        "password",
        "secret",
        "token",
        "api_key",
        "email",
        "statement",
        "query",
        "body",
        "payload",
    ]
    .iter()
    .any(|unsafe_part| lower.contains(unsafe_part))
    {
        return false;
    }

    key.starts_with("lenso.")
        || matches!(
            key,
            "otel.status_code"
                | "error.type"
                | "http.request.method"
                | "http.route"
                | "http.response.status_code"
                | "url.scheme"
                | "server.address"
                | "server.port"
                | "network.peer.address"
                | "network.peer.port"
                | "net.peer.name"
                | "net.peer.port"
                | "db.system"
                | "db.name"
                | "db.namespace"
                | "db.operation"
                | "db.operation.name"
                | "db.collection.name"
                | "db.sql.table"
                | "rpc.system"
                | "rpc.service"
                | "rpc.method"
                | "aws.s3.bucket"
                | "aws.s3.bucket.name"
                | "s3.bucket"
                | "s3.bucket.name"
                | "aws.ses.operation"
                | "ses.operation"
        )
}

fn is_safe_attribute_value(value: &Value) -> bool {
    matches!(
        value,
        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null
    )
}

fn has_attribute_with_prefix(attributes: &Value, prefix: &str) -> bool {
    attributes
        .as_object()
        .is_some_and(|map| map.keys().any(|key| key.starts_with(prefix)))
}

fn span_attribute<'a>(attributes: &'a Value, key: &str) -> Option<&'a str> {
    attributes.get(key).and_then(Value::as_str)
}

fn build_story_summary(rows: &[StoryWorkRow]) -> AdminRuntimeStoryListItem {
    let created_at = rows
        .iter()
        .map(|row| row.created_at)
        .min()
        .unwrap_or_else(Utc::now);
    let updated_at = rows
        .iter()
        .map(story_row_end_timestamp)
        .max()
        .unwrap_or(created_at);
    let services = rows.iter().fold(Vec::new(), |mut services, row| {
        if !services.contains(&row.service) {
            services.push(row.service.clone());
        }
        services
    });
    let pattern = collapse_story_pattern(rows.iter().map(|row| row.item_type.clone()));
    let duration_ms = updated_at
        .signed_duration_since(created_at)
        .num_milliseconds()
        .max(0);

    AdminRuntimeStoryListItem {
        title: rows
            .first()
            .map(|row| row.name.clone())
            .unwrap_or_else(|| "Runtime Story".to_owned()),
        correlation_id: rows
            .first()
            .map(|row| row.correlation_id.clone())
            .unwrap_or_default(),
        status: story_status(rows).to_owned(),
        duration: duration_ms,
        node_count: rows.len(),
        error_count: rows
            .iter()
            .filter(|row| matches!(row.status.as_str(), "failed" | "dead"))
            .count(),
        services,
        pattern,
        root_error: story_root_error(rows),
        created_at,
        updated_at,
    }
}

fn build_story_node(
    row: &StoryWorkRow,
    connected_ids: &std::collections::BTreeSet<String>,
) -> AdminRuntimeStoryNode {
    let component = if connected_ids.contains(&row.id) {
        "connected"
    } else {
        "orphan"
    };
    AdminRuntimeStoryNode {
        id: row.id.clone(),
        node_type: row.item_type.clone(),
        name: row.name.clone(),
        status: row.status.clone(),
        service: row.service.clone(),
        timestamp: row.created_at,
        duration_ms: row_duration_ms(row),
        error: row.last_error.clone(),
        metadata: serde_json::json!({
            "attempts": row.attempts,
            "max_attempts": row.max_attempts,
            "correlation_id": row.correlation_id,
            "causation_id": row.causation_id,
            "component": component,
            "source_metadata": row.metadata,
        }),
    }
}

fn build_story_edges(rows: &[StoryWorkRow]) -> Vec<AdminRuntimeStoryEdge> {
    let ids = rows
        .iter()
        .map(|row| row.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    rows.iter()
        .filter_map(|current| {
            let source = explicit_causal_source(current, &ids)?;

            Some(AdminRuntimeStoryEdge {
                id: format!("{source}:{}:causation", current.id),
                source: source.to_owned(),
                target: current.id.clone(),
                edge_type: "causation".to_owned(),
                label: None,
            })
        })
        .collect()
}

fn explicit_causal_source(
    row: &StoryWorkRow,
    ids: &std::collections::BTreeSet<&str>,
) -> Option<String> {
    if let Some(source) = row.causation_id.as_deref().filter(|id| ids.contains(id)) {
        return Some(source.to_owned());
    }

    for key in [
        "outbox_event_id",
        "event_id",
        "causation_id",
        "parent_id",
        "source_id",
        "function_run_id",
    ] {
        if let Some(source) = json_string(&row.metadata, key).filter(|id| ids.contains(*id)) {
            return Some(source.to_owned());
        }
    }

    if let Some(headers) = row.metadata.get("headers") {
        for key in ["outbox_event_id", "event_id", "causation_id", "parent_id"] {
            if let Some(source) = json_string(headers, key).filter(|id| ids.contains(*id)) {
                return Some(source.to_owned());
            }
        }
    }

    None
}

fn json_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn connected_node_ids(edges: &[AdminRuntimeStoryEdge]) -> std::collections::BTreeSet<String> {
    edges
        .iter()
        .flat_map(|edge| [edge.source.clone(), edge.target.clone()])
        .collect()
}

fn collapse_story_pattern(types: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut pattern = Vec::new();
    for node_type in types {
        if pattern.last() != Some(&node_type) {
            pattern.push(node_type);
        }
    }
    pattern
}

fn story_status(rows: &[StoryWorkRow]) -> &'static str {
    if rows.iter().any(|row| row.status == "dead") {
        return "dead";
    }
    if rows.iter().any(|row| row.status == "failed") {
        return "failed";
    }
    if rows
        .iter()
        .any(|row| matches!(row.status.as_str(), "processing" | "running"))
    {
        return "running";
    }
    if rows
        .iter()
        .all(|row| matches!(row.status.as_str(), "published" | "completed"))
    {
        return "completed";
    }
    "pending"
}

fn story_root_error(rows: &[StoryWorkRow]) -> Option<String> {
    rows.iter()
        .filter(|row| matches!(row.status.as_str(), "failed" | "dead"))
        .min_by_key(|row| row.created_at)
        .map(|row| {
            let error = row
                .last_error
                .clone()
                .unwrap_or_else(|| format!("{} runtime work", row.status));
            format!("{}: {error}", row.name)
        })
}

fn story_row_end_timestamp(row: &StoryWorkRow) -> DateTime<Utc> {
    row.completed_at.unwrap_or(row.created_at)
}

fn row_duration_ms(row: &StoryWorkRow) -> i64 {
    let Some(started_at) = row.started_at else {
        return 0;
    };
    row.completed_at
        .unwrap_or(started_at)
        .signed_duration_since(started_at)
        .num_milliseconds()
        .max(0)
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

#[cfg(test)]
mod tests {
    use super::*;
    use platform_core::TelemetrySpan;

    #[test]
    fn story_edges_do_not_guess_sequence_edges_for_unlinked_work() {
        let rows = vec![
            story_row("event", "evt_1", None, "2026-05-31T00:00:00Z"),
            story_row("function", "fnrun_1", None, "2026-05-31T00:00:10Z"),
        ];

        assert!(build_story_edges(&rows).is_empty());
    }

    #[test]
    fn story_edges_preserve_explicit_causality() {
        let mut rows = vec![
            story_row("event", "evt_parent", None, "2026-05-31T00:00:00Z"),
            story_row(
                "function",
                "fnrun_child",
                Some("evt_parent"),
                "2026-05-31T00:00:10Z",
            ),
        ];
        rows[1].causation_id = None;
        rows[1].metadata = serde_json::json!({ "outbox_event_id": "evt_parent" });

        let edges = build_story_edges(&rows);

        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].source, "evt_parent");
        assert_eq!(edges[0].target, "fnrun_child");
        assert_eq!(edges[0].edge_type, "causation");
    }

    #[test]
    fn story_detail_marks_orphan_and_connected_components() {
        let rows = vec![
            story_row("event", "evt_parent", None, "2026-05-31T00:00:00Z"),
            story_row(
                "function",
                "fnrun_child",
                Some("evt_parent"),
                "2026-05-31T00:00:10Z",
            ),
            story_row("event", "evt_orphan", None, "2026-05-31T00:00:20Z"),
        ];

        let detail = build_story_detail(rows);

        let components = detail
            .nodes
            .iter()
            .map(|node| {
                (
                    node.id.as_str(),
                    node.metadata["component"].as_str().unwrap_or_default(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            components,
            vec![
                ("evt_parent", "connected"),
                ("fnrun_child", "connected"),
                ("evt_orphan", "orphan"),
            ]
        );
    }

    #[test]
    fn timeline_item_type_preserves_failure_retry_and_dead_letter_kinds() {
        assert_eq!(timeline_item_type("event", "published", 1), "outbox_event");
        assert_eq!(
            timeline_item_type("outbox_event", "published", 1),
            "outbox_event"
        );
        assert_eq!(
            timeline_item_type("function", "completed", 1),
            "function_run"
        );
        assert_eq!(
            timeline_item_type("function_run", "completed", 1),
            "function_run"
        );
        assert_eq!(timeline_item_type("function", "completed", 2), "retry");
        assert_eq!(timeline_item_type("function", "failed", 2), "failure");
        assert_eq!(timeline_item_type("event", "dead", 3), "dead_letter");
    }

    #[test]
    fn story_summary_cursor_uses_stable_updated_at_boundaries() {
        let mut rows = vec![
            story_row("event", "evt_a", None, "2026-05-31T00:00:00Z"),
            story_row("event", "evt_b", None, "2026-05-31T00:01:00Z"),
        ];
        rows[1].completed_at = Some(parse_time("2026-05-31T00:03:00Z"));

        let summaries = build_story_summaries(rows);

        assert_eq!(summaries[0].correlation_id, "corr_test");
        assert_eq!(summaries[0].updated_at, parse_time("2026-05-31T00:03:00Z"));
    }

    #[test]
    fn technical_operation_dto_serializes_business_friendly_shape() {
        let operation = AdminRuntimeTechnicalOperation {
            attributes: serde_json::json!({ "db.system": "postgresql" }),
            category: "db".to_owned(),
            correlation_id: "corr_test".to_owned(),
            duration_ms: 25,
            ended_at: parse_time("2026-05-31T00:00:00.025Z"),
            id: "span_1".to_owned(),
            name: "INSERT runtime.function_runs".to_owned(),
            related_node_id: Some("fnrun_test".to_owned()),
            source: "otel".to_owned(),
            started_at: parse_time("2026-05-31T00:00:00Z"),
            status: "ok".to_owned(),
            story_id: "corr_test".to_owned(),
        };

        let value = serde_json::to_value(operation).expect("operation should serialize");

        assert_eq!(value["source"], "otel");
        assert_eq!(value["category"], "db");
        assert_eq!(value["related_node_id"], "fnrun_test");
        assert_eq!(value["attributes"]["db.system"], "postgresql");
    }

    #[test]
    fn telemetry_span_maps_known_function_run_to_execution_node() {
        let rows = vec![story_row(
            "function",
            "fnrun_test",
            None,
            "2026-05-31T00:00:00Z",
        )];
        let operations = technical_operations_from_spans(
            vec![telemetry_span(
                "span_function",
                "SELECT identity.users",
                serde_json::json!({
                    "lenso.correlation_id": "corr_test",
                    "lenso.function_run_id": "fnrun_test",
                    "db.system": "postgresql"
                }),
            )],
            &runtime_node_index(&rows),
        );

        assert_eq!(operations.len(), 1);
        assert_eq!(operations[0].related_node_id.as_deref(), Some("fnrun_test"));
        assert_eq!(operations[0].category, "db");
    }

    #[test]
    fn telemetry_span_maps_known_outbox_event_to_execution_node() {
        let rows = vec![story_row("event", "evt_test", None, "2026-05-31T00:00:00Z")];
        let operations = technical_operations_from_spans(
            vec![telemetry_span(
                "span_outbox",
                "Publish event",
                serde_json::json!({
                    "lenso.correlation_id": "corr_test",
                    "lenso.outbox_event_id": "evt_test",
                    "lenso.execution.kind": "outbox_event"
                }),
            )],
            &runtime_node_index(&rows),
        );

        assert_eq!(operations[0].related_node_id.as_deref(), Some("evt_test"));
        assert_eq!(operations[0].category, "runtime");
    }

    #[test]
    fn unknown_telemetry_span_remains_story_level_unlinked_operation() {
        let operations = technical_operations_from_spans(
            vec![telemetry_span(
                "span_unlinked",
                "GET https://api.example.test",
                serde_json::json!({
                    "lenso.correlation_id": "corr_test",
                    "http.request.method": "GET"
                }),
            )],
            &runtime_node_index(&[]),
        );

        assert_eq!(operations[0].related_node_id, None);
        assert_eq!(operations[0].category, "http");
    }

    #[test]
    fn technical_operation_attributes_are_safe_subset_only() {
        let operations = technical_operations_from_spans(
            vec![telemetry_span(
                "span_sensitive",
                "INSERT identity.users",
                serde_json::json!({
                    "lenso.correlation_id": "corr_test",
                    "db.system": "postgresql",
                    "db.statement": "insert into users(email, password) values('a@example.test', 'secret')",
                    "http.request.header.authorization": "Bearer secret",
                    "user.email": "a@example.test"
                }),
            )],
            &runtime_node_index(&[]),
        );

        assert_eq!(operations[0].attributes["db.system"], "postgresql");
        assert!(operations[0].attributes.get("db.statement").is_none());
        assert!(
            operations[0]
                .attributes
                .get("http.request.header.authorization")
                .is_none()
        );
        assert!(operations[0].attributes.get("user.email").is_none());
    }

    fn story_row(
        item_type: &str,
        id: &str,
        causation_id: Option<&str>,
        created_at: &str,
    ) -> StoryWorkRow {
        StoryWorkRow {
            item_type: item_type.to_owned(),
            id: id.to_owned(),
            name: id.to_owned(),
            status: if item_type == "event" {
                "published".to_owned()
            } else {
                "completed".to_owned()
            },
            attempts: 1,
            max_attempts: 3,
            correlation_id: "corr_test".to_owned(),
            causation_id: causation_id.map(ToOwned::to_owned),
            service: "runtime".to_owned(),
            created_at: parse_time(created_at),
            started_at: Some(parse_time(created_at)),
            completed_at: Some(parse_time(created_at)),
            last_error: None,
            metadata: Value::Object(Default::default()),
        }
    }

    fn parse_time(value: &str) -> DateTime<Utc> {
        value.parse().expect("test timestamp should parse")
    }

    fn telemetry_span(id: &str, name: &str, attributes: Value) -> TelemetrySpan {
        TelemetrySpan {
            attributes,
            ended_at: parse_time("2026-05-31T00:00:01Z"),
            id: id.to_owned(),
            name: name.to_owned(),
            started_at: parse_time("2026-05-31T00:00:00Z"),
            status: Some("ok".to_owned()),
        }
    }
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

fn normalized_bucket_seconds(bucket_seconds: Option<i64>) -> i64 {
    bucket_seconds.unwrap_or(300).clamp(60, 3600)
}

fn page_info(limit: i64, next_created_before: Option<DateTime<Utc>>) -> PageInfo {
    PageInfo {
        limit,
        next_created_before,
    }
}
