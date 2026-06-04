#[allow(clippy::wildcard_imports)]
use super::*;
use axum::Json;
use axum::extract::{Path, Query, State};
use platform_core::{
    AppContext, AppError, ErrorCode, ExecutionLogQuery as ProviderExecutionLogQuery,
    TelemetrySpanQuery,
};
use platform_http::responses::{DataResponse, json};
use platform_http::{AdminActor, ApiErrorResponse, ErrorResponse, HttpRequestContext};

#[utoipa::path(
    get,
    path = "/admin/runtime/summary",
    operation_id = "admin_runtime_get_summary",
    tag = "admin-runtime",
    params(
        ("authorization" = String, Header, description = "Development service bearer token, for example `Bearer dev-service:admin`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier")
    ),
    responses(
        (
            status = 200,
            description = "Compact runtime health summary",
            body = AdminRuntimeSummaryResponse,
            content_type = "application/json",
            headers(
                ("x-request-id" = String, description = "Request identifier for this HTTP request"),
                ("x-correlation-id" = String, description = "Correlation identifier shared across related work")
            )
        ),
        (
            status = 401,
            description = "Authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 403,
            description = "Service or system authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 500,
            description = "Internal server error",
            body = ErrorResponse,
            content_type = "application/json"
        )
    )
)]
pub(crate) async fn get_summary(
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

#[utoipa::path(
    get,
    path = "/admin/runtime/heatmap",
    operation_id = "admin_runtime_get_heatmap",
    tag = "admin-runtime",
    params(
        ("authorization" = String, Header, description = "Development service bearer token, for example `Bearer dev-service:admin`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier"),
        HeatmapQuery
    ),
    responses(
        (
            status = 200,
            description = "Runtime heatmap cells grouped by time bucket, service, and node type",
            body = AdminRuntimeHeatmapResponse,
            content_type = "application/json",
            headers(
                ("x-request-id" = String, description = "Request identifier for this HTTP request"),
                ("x-correlation-id" = String, description = "Correlation identifier shared across related work")
            )
        ),
        (
            status = 401,
            description = "Authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 403,
            description = "Service or system authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 500,
            description = "Internal server error",
            body = ErrorResponse,
            content_type = "application/json"
        )
    )
)]
pub(crate) async fn get_heatmap(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Query(query): Query<HeatmapQuery>,
) -> Result<Json<AdminRuntimeHeatmapResponse>, ApiErrorResponse> {
    let limit = normalized_limit(query.limit);
    let bucket_seconds = normalized_bucket_seconds(query.bucket_seconds);
    let rows = fetch_heatmap_rows(&ctx, &request_ctx, &query, limit, bucket_seconds, None).await?;

    let data: Vec<AdminRuntimeHeatmapCell> = rows.into_iter().map(Into::into).collect();
    Ok(Json(AdminRuntimeHeatmapResponse {
        page: page_info(limit, data.last().map(|cell| cell.bucket_start)),
        data,
        bucket_seconds,
        order: "bucket_start_desc",
    }))
}

#[utoipa::path(
    get,
    path = "/admin/runtime/stories/{correlation_id}/heatmap",
    operation_id = "admin_runtime_get_story_heatmap",
    tag = "admin-runtime",
    params(
        ("authorization" = String, Header, description = "Development service bearer token, for example `Bearer dev-service:admin`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier"),
        ("correlation_id" = String, Path, description = "Story correlation identifier"),
        HeatmapQuery
    ),
    responses(
        (
            status = 200,
            description = "Runtime heatmap cells scoped to a single story correlation identifier",
            body = AdminRuntimeHeatmapResponse,
            content_type = "application/json",
            headers(
                ("x-request-id" = String, description = "Request identifier for this HTTP request"),
                ("x-correlation-id" = String, description = "Correlation identifier shared across related work")
            )
        ),
        (
            status = 401,
            description = "Authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 403,
            description = "Service or system authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 404,
            description = "Runtime story not found",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 500,
            description = "Internal server error",
            body = ErrorResponse,
            content_type = "application/json"
        )
    )
)]
pub(crate) async fn get_story_heatmap(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path(correlation_id): Path<String>,
    Query(query): Query<HeatmapQuery>,
) -> Result<Json<AdminRuntimeHeatmapResponse>, ApiErrorResponse> {
    if !runtime_story_exists(&ctx, &request_ctx, &correlation_id).await? {
        return Err(ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::NotFound,
                format!("Runtime story {correlation_id} was not found"),
            ),
            &request_ctx,
        ));
    }

    let limit = normalized_limit(query.limit);
    let bucket_seconds = normalized_bucket_seconds(query.bucket_seconds);
    let rows = fetch_heatmap_rows(
        &ctx,
        &request_ctx,
        &query,
        limit,
        bucket_seconds,
        Some(&correlation_id),
    )
    .await?;

    let data: Vec<AdminRuntimeHeatmapCell> = rows.into_iter().map(Into::into).collect();
    Ok(Json(AdminRuntimeHeatmapResponse {
        page: page_info(limit, data.last().map(|cell| cell.bucket_start)),
        data,
        bucket_seconds,
        order: "bucket_start_desc",
    }))
}

async fn fetch_heatmap_rows(
    ctx: &AppContext,
    request_ctx: &platform_core::RequestContext,
    query: &HeatmapQuery,
    limit: i64,
    bucket_seconds: i64,
    correlation_id: Option<&str>,
) -> Result<Vec<HeatmapRow>, ApiErrorResponse> {
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
              and ($9::text is null or correlation_id = $9)

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
              and ($9::text is null or correlation_id = $9)

            union all

            select
                started_at as created_at,
                service,
                node_type,
                status,
                1 as attempts,
                duration_ms
            from platform.story_events
            where ($1::timestamptz is null or started_at < $1)
              and ($4::timestamptz is null or started_at >= $4)
              and ($5::timestamptz is null or started_at < $5)
              and ($6::text is null or status = $6)
              and ($7::text is null)
              and ($8::text is null)
              and ($9::text is null or correlation_id = $9)
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
    .bind(query.status.as_deref())
    .bind(query.event_name.as_deref())
    .bind(query.function_name.as_deref())
    .bind(correlation_id)
    .fetch_all(&ctx.db)
    .await
    .map_err(|source| query_error(source, request_ctx))?;

    Ok(rows)
}

async fn runtime_story_exists(
    ctx: &AppContext,
    request_ctx: &platform_core::RequestContext,
    correlation_id: &str,
) -> Result<bool, ApiErrorResponse> {
    let exists = sqlx::query_scalar::<_, bool>(
        r#"
        select exists (
            select 1
            from platform.outbox
            where correlation_id = $1

            union all

            select 1
            from runtime.function_runs
            where correlation_id = $1

            union all

            select 1
            from platform.story_events
            where correlation_id = $1
        )
        "#,
    )
    .bind(correlation_id)
    .fetch_one(&ctx.db)
    .await
    .map_err(|source| query_error(source, request_ctx))?;

    Ok(exists)
}

#[utoipa::path(
    get,
    path = "/admin/runtime/stories",
    operation_id = "admin_runtime_list_stories",
    tag = "admin-runtime",
    params(
        ("authorization" = String, Header, description = "Development service bearer token, for example `Bearer dev-service:admin`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier"),
        StoryQuery
    ),
    responses(
        (
            status = 200,
            description = "Runtime stories grouped by correlation identifier",
            body = AdminRuntimeStoryListResponse,
            content_type = "application/json",
            headers(
                ("x-request-id" = String, description = "Request identifier for this HTTP request"),
                ("x-correlation-id" = String, description = "Correlation identifier shared across related work")
            )
        ),
        (
            status = 401,
            description = "Authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 403,
            description = "Service or system authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 500,
            description = "Internal server error",
            body = ErrorResponse,
            content_type = "application/json"
        )
    )
)]
pub(crate) async fn list_stories(
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

#[utoipa::path(
    get,
    path = "/admin/runtime/stories/{correlation_id}",
    operation_id = "admin_runtime_get_story",
    tag = "admin-runtime",
    params(
        ("correlation_id" = String, Path, description = "Correlation identifier shared by related runtime work"),
        ("authorization" = String, Header, description = "Development service bearer token, for example `Bearer dev-service:admin`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier")
    ),
    responses(
        (
            status = 200,
            description = "Runtime story detail with nodes, edges, and timeline items",
            body = AdminRuntimeStoryDetailResponse,
            content_type = "application/json",
            headers(
                ("x-request-id" = String, description = "Request identifier for this HTTP request"),
                ("x-correlation-id" = String, description = "Correlation identifier shared across related work")
            )
        ),
        (
            status = 401,
            description = "Authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 403,
            description = "Service or system authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 404,
            description = "Runtime story not found",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 500,
            description = "Internal server error",
            body = ErrorResponse,
            content_type = "application/json"
        )
    )
)]
pub(crate) async fn get_story(
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

#[utoipa::path(
    get,
    path = "/admin/runtime/stories/{correlation_id}/technical-operations",
    operation_id = "admin_runtime_get_story_technical_operations",
    tag = "admin-runtime",
    params(
        ("correlation_id" = String, Path, description = "Correlation identifier shared by related runtime work"),
        ("authorization" = String, Header, description = "Development service bearer token, for example `Bearer dev-service:admin`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier")
    ),
    responses(
        (
            status = 200,
            description = "Technical operations observed for the runtime story",
            body = AdminRuntimeTechnicalOperationListResponse,
            content_type = "application/json",
            headers(
                ("x-request-id" = String, description = "Request identifier for this HTTP request"),
                ("x-correlation-id" = String, description = "Correlation identifier shared across related work")
            )
        ),
        (
            status = 401,
            description = "Authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 403,
            description = "Service or system authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 404,
            description = "Runtime story not found",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 500,
            description = "Internal server error",
            body = ErrorResponse,
            content_type = "application/json"
        )
    )
)]
pub(crate) async fn get_story_technical_operations(
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

#[utoipa::path(
    get,
    path = "/admin/runtime/executions/{node_id}/technical-operations",
    operation_id = "admin_runtime_get_execution_technical_operations",
    tag = "admin-runtime",
    params(
        ("node_id" = String, Path, description = "Runtime execution node identifier"),
        ("authorization" = String, Header, description = "Development service bearer token, for example `Bearer dev-service:admin`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier")
    ),
    responses(
        (
            status = 200,
            description = "Technical operations observed for the runtime execution node",
            body = AdminRuntimeTechnicalOperationListResponse,
            content_type = "application/json",
            headers(
                ("x-request-id" = String, description = "Request identifier for this HTTP request"),
                ("x-correlation-id" = String, description = "Correlation identifier shared across related work")
            )
        ),
        (
            status = 401,
            description = "Authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 403,
            description = "Service or system authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 404,
            description = "Runtime execution node not found",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 500,
            description = "Internal server error",
            body = ErrorResponse,
            content_type = "application/json"
        )
    )
)]
pub(crate) async fn get_execution_technical_operations(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path(node_id): Path<String>,
) -> Result<Json<AdminRuntimeTechnicalOperationListResponse>, ApiErrorResponse> {
    let node = fetch_runtime_node_ref(&ctx, &request_ctx, &node_id).await?;
    let query = match node.item_type.as_str() {
        "function" => TelemetrySpanQuery::by_function_run_id(&node.id),
        "event" => TelemetrySpanQuery::by_outbox_event_id(&node.id),
        _ => TelemetrySpanQuery::by_correlation_id(&node.correlation_id),
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

#[utoipa::path(
    get,
    path = "/admin/runtime/executions/{node_id}/payload",
    operation_id = "admin_runtime_get_execution_payload",
    tag = "admin-runtime",
    params(
        ("node_id" = String, Path, description = "Runtime execution node identifier"),
        ("authorization" = String, Header, description = "Development service bearer token, for example `Bearer dev-service:admin`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier")
    ),
    responses(
        (
            status = 200,
            description = "Redacted payload captured for the runtime execution node",
            body = AdminRuntimeExecutionPayloadResponse,
            content_type = "application/json",
            headers(
                ("x-request-id" = String, description = "Request identifier for this HTTP request"),
                ("x-correlation-id" = String, description = "Correlation identifier shared across related work")
            )
        ),
        (
            status = 401,
            description = "Authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 403,
            description = "Service or system authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 404,
            description = "Runtime execution node not found",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 500,
            description = "Internal server error",
            body = ErrorResponse,
            content_type = "application/json"
        )
    )
)]
pub(crate) async fn get_execution_payload(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path(node_id): Path<String>,
) -> Result<Json<AdminRuntimeExecutionPayloadResponse>, ApiErrorResponse> {
    let node = fetch_runtime_node_ref(&ctx, &request_ctx, &node_id).await?;
    let data = match node.item_type.as_str() {
        "function" => {
            let detail = fetch_function_run_detail(&ctx, &request_ctx, &node.id).await?;
            execution_payload_from_function(detail)
        }
        "event" => {
            let detail = fetch_outbox_event_detail(&ctx, &request_ctx, &node.id).await?;
            execution_payload_from_outbox(detail)
        }
        _ => {
            let detail = fetch_story_event_detail(&ctx, &request_ctx, &node.id).await?;
            execution_payload_from_story_event(detail)
        }
    };

    Ok(Json(AdminRuntimeExecutionPayloadResponse { data }))
}

#[utoipa::path(
    get,
    path = "/admin/runtime/executions/{node_id}/logs",
    operation_id = "admin_runtime_get_execution_logs",
    tag = "admin-runtime",
    params(
        ("node_id" = String, Path, description = "Runtime execution node identifier"),
        ("authorization" = String, Header, description = "Development service bearer token, for example `Bearer dev-service:admin`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier"),
        ExecutionLogQuery
    ),
    responses(
        (
            status = 200,
            description = "Structured logs recorded for the runtime execution node",
            body = AdminRuntimeExecutionLogListResponse,
            content_type = "application/json",
            headers(
                ("x-request-id" = String, description = "Request identifier for this HTTP request"),
                ("x-correlation-id" = String, description = "Correlation identifier shared across related work")
            )
        ),
        (
            status = 401,
            description = "Authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 403,
            description = "Service or system authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 404,
            description = "Runtime execution node not found",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 500,
            description = "Internal server error",
            body = ErrorResponse,
            content_type = "application/json"
        )
    )
)]
pub(crate) async fn get_execution_logs(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path(node_id): Path<String>,
    Query(query): Query<ExecutionLogQuery>,
) -> Result<Json<AdminRuntimeExecutionLogListResponse>, ApiErrorResponse> {
    let node = fetch_runtime_node_ref(&ctx, &request_ctx, &node_id).await?;
    if node.item_type == "http_request" {
        return Ok(Json(AdminRuntimeExecutionLogListResponse {
            page: page_info(normalized_limit(query.limit), None),
            data: Vec::new(),
            order: "occurred_at_asc",
        }));
    }
    let limit = normalized_limit(query.limit);
    let data = ctx
        .execution_logs
        .query_execution_logs(ProviderExecutionLogQuery {
            execution_id: node_id,
            occurred_before: query.created_before,
            limit,
        })
        .await
        .map_err(|source| ApiErrorResponse::with_context(source, &request_ctx))?
        .into_iter()
        .map(AdminRuntimeExecutionLog::from)
        .collect::<Vec<_>>();

    Ok(Json(AdminRuntimeExecutionLogListResponse {
        page: page_info(limit, data.first().map(|log| log.occurred_at)),
        data,
        order: "occurred_at_asc",
    }))
}

#[utoipa::path(
    get,
    path = "/admin/runtime/timeline/{correlation_id}",
    operation_id = "admin_runtime_get_timeline",
    tag = "admin-runtime",
    params(
        ("correlation_id" = String, Path, description = "Correlation identifier shared by related runtime work"),
        ("authorization" = String, Header, description = "Development service bearer token, for example `Bearer dev-service:admin`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier"),
        TimelineQuery
    ),
    responses(
        (
            status = 200,
            description = "Runtime timeline items ordered by created_at ascending",
            body = AdminRuntimeTimelineResponse,
            content_type = "application/json",
            headers(
                ("x-request-id" = String, description = "Request identifier for this HTTP request"),
                ("x-correlation-id" = String, description = "Correlation identifier shared across related work")
            )
        ),
        (
            status = 401,
            description = "Authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 403,
            description = "Service or system authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 500,
            description = "Internal server error",
            body = ErrorResponse,
            content_type = "application/json"
        )
    )
)]
pub(crate) async fn get_timeline(
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

            union all

            select
                node_type as item_type,
                id,
                name,
                status,
                1 as attempts,
                1 as max_attempts,
                started_at as created_at,
                started_at,
                completed_at,
                error as last_error,
                correlation_id
            from platform.story_events
            where correlation_id = $1
              and ($2::timestamptz is null or started_at < $2)
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

#[utoipa::path(
    get,
    path = "/admin/runtime/remote-proxy-calls",
    operation_id = "admin_runtime_list_remote_proxy_calls",
    tag = "admin-runtime",
    params(
        ("authorization" = String, Header, description = "Development service bearer token, for example `Bearer dev-service:admin`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier"),
        RemoteProxyCallQuery
    ),
    responses(
        (
            status = 200,
            description = "Recent remote module HTTP proxy calls",
            body = AdminRemoteProxyCallListResponse,
            content_type = "application/json",
            headers(
                ("x-request-id" = String, description = "Request identifier for this HTTP request"),
                ("x-correlation-id" = String, description = "Correlation identifier shared across related work")
            )
        ),
        (
            status = 401,
            description = "Authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 403,
            description = "Service or system authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 500,
            description = "Internal server error",
            body = ErrorResponse,
            content_type = "application/json"
        )
    )
)]
pub(crate) async fn list_remote_proxy_calls(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Query(query): Query<RemoteProxyCallQuery>,
) -> Result<Json<AdminRemoteProxyCallListResponse>, ApiErrorResponse> {
    let limit = normalized_limit(query.limit);
    let rows = sqlx::query(
        r#"
        select
            id,
            module_name,
            method,
            declared_path,
            remote_path,
            capability,
            remote_status,
            duration_ms,
            success,
            error_code,
            retryable,
            request_id,
            correlation_id,
            trace_id,
            span_id,
            path_params,
            error_details,
            occurred_at
        from platform.remote_http_proxy_calls
        where ($1::text is null or module_name = $1)
          and ($2::boolean is null or success = $2)
          and ($3::timestamptz is null or occurred_at < $3)
        order by occurred_at desc, id desc
        limit $4
        "#,
    )
    .bind(query.module_name)
    .bind(query.success)
    .bind(query.created_before)
    .bind(limit)
    .fetch_all(&ctx.db)
    .await
    .map_err(|source| query_error(source, &request_ctx))?;

    let data = rows
        .into_iter()
        .map(|row| remote_proxy_call_from_row(&row))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| query_error(source, &request_ctx))?;
    Ok(Json(AdminRemoteProxyCallListResponse {
        page: page_info(limit, data.last().map(|call| call.occurred_at)),
        data,
    }))
}

#[utoipa::path(
    get,
    path = "/admin/runtime/outbox",
    operation_id = "admin_runtime_list_outbox",
    tag = "admin-runtime",
    params(
        ("authorization" = String, Header, description = "Development service bearer token, for example `Bearer dev-service:admin`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier"),
        OutboxQuery
    ),
    responses(
        (
            status = 200,
            description = "Outbox events",
            body = AdminOutboxListResponse,
            content_type = "application/json",
            headers(
                ("x-request-id" = String, description = "Request identifier for this HTTP request"),
                ("x-correlation-id" = String, description = "Correlation identifier shared across related work")
            )
        ),
        (
            status = 401,
            description = "Authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 403,
            description = "Service or system authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 500,
            description = "Internal server error",
            body = ErrorResponse,
            content_type = "application/json"
        )
    )
)]
pub(crate) async fn list_outbox(
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

#[utoipa::path(
    get,
    path = "/admin/runtime/functions",
    operation_id = "admin_runtime_list_function_runs",
    tag = "admin-runtime",
    params(
        ("authorization" = String, Header, description = "Development service bearer token, for example `Bearer dev-service:admin`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier"),
        FunctionRunQuery
    ),
    responses(
        (
            status = 200,
            description = "Runtime function runs",
            body = AdminFunctionRunListResponse,
            content_type = "application/json",
            headers(
                ("x-request-id" = String, description = "Request identifier for this HTTP request"),
                ("x-correlation-id" = String, description = "Correlation identifier shared across related work")
            )
        ),
        (
            status = 401,
            description = "Authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 403,
            description = "Service or system authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 500,
            description = "Internal server error",
            body = ErrorResponse,
            content_type = "application/json"
        )
    )
)]
pub(crate) async fn list_function_runs(
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

#[utoipa::path(
    get,
    path = "/admin/runtime/outbox/{id}",
    operation_id = "admin_runtime_get_outbox",
    tag = "admin-runtime",
    params(
        ("id" = String, Path, description = "Outbox event identifier"),
        ("authorization" = String, Header, description = "Development service bearer token, for example `Bearer dev-service:admin`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier")
    ),
    responses(
        (
            status = 200,
            description = "Outbox event detail",
            body = AdminOutboxEventDetailResponse,
            content_type = "application/json",
            headers(
                ("x-request-id" = String, description = "Request identifier for this HTTP request"),
                ("x-correlation-id" = String, description = "Correlation identifier shared across related work")
            )
        ),
        (
            status = 401,
            description = "Authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 403,
            description = "Service or system authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 404,
            description = "Outbox event was not found",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 500,
            description = "Internal server error",
            body = ErrorResponse,
            content_type = "application/json"
        )
    )
)]
pub(crate) async fn get_outbox_event(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path(id): Path<String>,
) -> Result<Json<DataResponse<AdminOutboxEventDetail>>, ApiErrorResponse> {
    let row = fetch_outbox_event_detail(&ctx, &request_ctx, &id).await?;
    Ok(json(row))
}

#[utoipa::path(
    get,
    path = "/admin/runtime/functions/{id}",
    operation_id = "admin_runtime_get_function_run",
    tag = "admin-runtime",
    params(
        ("id" = String, Path, description = "Runtime function run identifier"),
        ("authorization" = String, Header, description = "Development service bearer token, for example `Bearer dev-service:admin`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier")
    ),
    responses(
        (
            status = 200,
            description = "Runtime function run",
            body = AdminFunctionRunResponse,
            content_type = "application/json",
            headers(
                ("x-request-id" = String, description = "Request identifier for this HTTP request"),
                ("x-correlation-id" = String, description = "Correlation identifier shared across related work")
            )
        ),
        (
            status = 401,
            description = "Authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 403,
            description = "Service or system authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 404,
            description = "Function run was not found",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 500,
            description = "Internal server error",
            body = ErrorResponse,
            content_type = "application/json"
        )
    )
)]
pub(crate) async fn get_function_run(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path(id): Path<String>,
) -> Result<Json<DataResponse<AdminFunctionRunDetail>>, ApiErrorResponse> {
    let row = fetch_function_run_detail(&ctx, &request_ctx, &id).await?;
    Ok(json(row))
}

pub(crate) async fn fetch_function_run_detail(
    ctx: &AppContext,
    request_ctx: &platform_core::RequestContext,
    id: &str,
) -> Result<AdminFunctionRunDetail, ApiErrorResponse> {
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

    Ok(row.into())
}

pub(crate) async fn fetch_story_event_detail(
    ctx: &AppContext,
    request_ctx: &platform_core::RequestContext,
    id: &str,
) -> Result<StoryEventDetail, ApiErrorResponse> {
    let row = sqlx::query_as::<_, StoryEventDetailRow>(
        r#"
        select
            id,
            node_type,
            name,
            status,
            service,
            correlation_id,
            causation_id,
            started_at,
            completed_at,
            duration_ms,
            error,
            metadata,
            trace_id,
            span_id
        from platform.story_events
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
                format!("Story event {id} was not found"),
            ),
            request_ctx,
        )
    })?;

    Ok(row.into())
}

#[utoipa::path(
    post,
    path = "/admin/runtime/outbox/{id}/retry",
    operation_id = "admin_runtime_retry_outbox",
    tag = "admin-runtime",
    params(
        ("id" = String, Path, description = "Outbox event identifier"),
        ("authorization" = String, Header, description = "Development service bearer token, for example `Bearer dev-service:admin`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier")
    ),
    responses(
        (
            status = 200,
            description = "Outbox event retry was scheduled",
            body = AdminOutboxEvent,
            content_type = "application/json",
            headers(
                ("x-request-id" = String, description = "Request identifier for this HTTP request"),
                ("x-correlation-id" = String, description = "Correlation identifier shared across related work")
            )
        ),
        (
            status = 401,
            description = "Authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 403,
            description = "Service or system authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 404,
            description = "Outbox event was not found",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 409,
            description = "Outbox event status cannot be retried",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 500,
            description = "Internal server error",
            body = ErrorResponse,
            content_type = "application/json"
        )
    )
)]
pub(crate) async fn retry_outbox_event(
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

#[utoipa::path(
    post,
    path = "/admin/runtime/functions/{id}/retry",
    operation_id = "admin_runtime_retry_function_run",
    tag = "admin-runtime",
    params(
        ("id" = String, Path, description = "Runtime function run identifier"),
        ("authorization" = String, Header, description = "Development service bearer token, for example `Bearer dev-service:admin`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier")
    ),
    responses(
        (
            status = 200,
            description = "Runtime function run retry was scheduled",
            body = AdminFunctionRunResponse,
            content_type = "application/json",
            headers(
                ("x-request-id" = String, description = "Request identifier for this HTTP request"),
                ("x-correlation-id" = String, description = "Correlation identifier shared across related work")
            )
        ),
        (
            status = 401,
            description = "Authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 403,
            description = "Service or system authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 404,
            description = "Function run was not found",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 409,
            description = "Function run status cannot be retried",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 500,
            description = "Internal server error",
            body = ErrorResponse,
            content_type = "application/json"
        )
    )
)]
pub(crate) async fn retry_function_run(
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
