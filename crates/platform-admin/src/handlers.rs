#[allow(clippy::wildcard_imports)]
use super::*;
use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::Duration;
use platform_core::{
    AppContext, AppError, ErrorCode, ExecutionLogQuery as ProviderExecutionLogQuery,
    ExecutionLogRow, TelemetrySpanQuery,
};
use platform_http::responses::json;
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
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
) -> Result<Json<AdminRuntimeSummaryResponse>, ApiErrorResponse> {
    ensure_runtime_read_capability(&admin, &request_ctx)?;

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
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Query(query): Query<HeatmapQuery>,
) -> Result<Json<AdminRuntimeHeatmapResponse>, ApiErrorResponse> {
    ensure_runtime_read_capability(&admin, &request_ctx)?;

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

async fn remote_runtime_technical_operations_by_execution(
    ctx: &AppContext,
    request_ctx: &platform_core::RequestContext,
    node_id: &str,
    node_index: &RuntimeNodeIndex,
) -> Result<Vec<AdminRuntimeTechnicalOperation>, ApiErrorResponse> {
    let logs = ctx
        .execution_logs
        .query_execution_logs(ProviderExecutionLogQuery {
            execution_id: node_id.to_owned(),
            occurred_before: None,
            limit: MAX_LIMIT,
        })
        .await
        .map_err(|source| ApiErrorResponse::with_context(source, request_ctx))?;

    Ok(logs
        .into_iter()
        .filter(|log| {
            span_attribute(&log.attributes, "source") == Some("remote_runtime")
                && log.execution_id == node_id
        })
        .map(|log| remote_runtime_log_to_technical_operation(log, node_index))
        .collect())
}

fn remote_runtime_log_to_technical_operation(
    log: ExecutionLogRow,
    node_index: &RuntimeNodeIndex,
) -> AdminRuntimeTechnicalOperation {
    let duration_ms = json_i64_attribute(&log.attributes, "duration_ms").unwrap_or(0);
    let ended_at = log.occurred_at + Duration::milliseconds(duration_ms.max(0));
    let related_node_id = if node_index.contains(&log.execution_id) {
        Some(log.execution_id.clone())
    } else {
        None
    };
    let module_name = span_attribute(&log.attributes, "module_name").map(ToOwned::to_owned);
    let function_name = span_attribute(&log.attributes, "function_name")
        .unwrap_or(log.execution_name.as_str())
        .to_owned();
    let status = match json_bool_attribute(&log.attributes, "success") {
        Some(true) => "ok",
        Some(false) => "error",
        _ if log.severity == "error" => "error",
        _ => "ok",
    };

    AdminRuntimeTechnicalOperation {
        attributes: log.attributes,
        category: "external".to_owned(),
        correlation_id: log.correlation_id.clone(),
        duration_ms,
        ended_at,
        id: format!("remote_runtime:{}", log.id),
        name: module_name
            .map(|module| format!("{module} {function_name}"))
            .unwrap_or(function_name),
        related_node_id,
        source: "remote_runtime".to_owned(),
        started_at: log.occurred_at,
        status: status.to_owned(),
        story_id: log.story_id,
    }
}

fn json_bool_attribute(attributes: &serde_json::Value, key: &str) -> Option<bool> {
    attributes.get(key).and_then(serde_json::Value::as_bool)
}

fn json_i64_attribute(attributes: &serde_json::Value, key: &str) -> Option<i64> {
    attributes
        .get(key)
        .and_then(serde_json::Value::as_i64)
        .or_else(|| {
            attributes
                .get(key)
                .and_then(serde_json::Value::as_u64)
                .and_then(|value| i64::try_from(value).ok())
        })
}

fn sort_technical_operations(data: &mut [AdminRuntimeTechnicalOperation]) {
    data.sort_by(|left, right| {
        left.started_at
            .cmp(&right.started_at)
            .then_with(|| left.id.cmp(&right.id))
    });
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
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path(node_id): Path<String>,
) -> Result<Json<AdminRuntimeTechnicalOperationListResponse>, ApiErrorResponse> {
    ensure_runtime_read_capability(&admin, &request_ctx)?;

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
    let node_index = RuntimeNodeIndex::single(node.id.clone());
    let mut data = technical_operations_from_spans(spans, &node_index);
    data.extend(
        remote_runtime_technical_operations_by_execution(&ctx, &request_ctx, &node.id, &node_index)
            .await?,
    );
    sort_technical_operations(&mut data);

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
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path(node_id): Path<String>,
) -> Result<Json<AdminRuntimeExecutionPayloadResponse>, ApiErrorResponse> {
    ensure_runtime_read_capability(&admin, &request_ctx)?;

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
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path(node_id): Path<String>,
    Query(query): Query<ExecutionLogQuery>,
) -> Result<Json<AdminRuntimeExecutionLogListResponse>, ApiErrorResponse> {
    ensure_runtime_read_capability(&admin, &request_ctx)?;

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
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Query(query): Query<RemoteProxyCallQuery>,
) -> Result<Json<AdminRemoteProxyCallListResponse>, ApiErrorResponse> {
    ensure_runtime_read_capability(&admin, &request_ctx)?;

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
          and ($3::text is null or correlation_id = $3)
          and ($4::timestamptz is null or occurred_at < $4)
        order by occurred_at desc, id desc
        limit $5
        "#,
    )
    .bind(query.module_name)
    .bind(query.success)
    .bind(query.correlation_id)
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
    path = "/admin/runtime/admin-actions",
    operation_id = "admin_runtime_list_admin_action_invocations",
    tag = "admin-runtime",
    params(
        ("authorization" = String, Header, description = "Development service bearer token, for example `Bearer dev-service:admin`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier"),
        AdminActionInvocationQuery
    ),
    responses(
        (
            status = 200,
            description = "Recent declarative admin action invocations",
            body = AdminActionInvocationListResponse,
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
pub(crate) async fn list_admin_action_invocations(
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Query(query): Query<AdminActionInvocationQuery>,
) -> Result<Json<AdminActionInvocationListResponse>, ApiErrorResponse> {
    ensure_runtime_read_capability(&admin, &request_ctx)?;

    let limit = normalized_limit(query.limit);
    let rows = sqlx::query_as::<_, AdminActionInvocationTuple>(
        r#"
        select
            id,
            service as module_name,
            coalesce(metadata ->> 'action_name', name) as action_name,
            coalesce(metadata ->> 'label', name) as label,
            metadata ->> 'capability' as capability,
            duration_ms,
            status <> 'failed' as success,
            metadata ->> 'error_code' as error_code,
            metadata ->> 'error_message' as error_message,
            metadata ->> 'request_id' as request_id,
            correlation_id,
            trace_id,
            span_id,
            metadata ->> 'input_summary' as input_summary,
            metadata ->> 'result_summary' as result_summary,
            started_at as occurred_at
        from platform.story_events
        where source_type = 'admin_action'
          and ($1::text is null or service = $1)
          and ($2::text is null or metadata ->> 'action_name' = $2)
          and ($3::text is null or metadata ->> 'capability' = $3)
          and ($4::text is null or correlation_id = $4)
          and ($5::boolean is null or (status <> 'failed') = $5)
          and ($6::timestamptz is null or started_at < $6)
        order by started_at desc, id desc
        limit $7
        "#,
    )
    .bind(query.module_name)
    .bind(query.action_name)
    .bind(query.capability)
    .bind(query.correlation_id)
    .bind(query.success)
    .bind(query.created_before)
    .bind(limit)
    .fetch_all(&ctx.db)
    .await
    .map_err(|source| query_error(source, &request_ctx))?;

    let data = rows
        .into_iter()
        .map(AdminActionInvocation::from)
        .collect::<Vec<_>>();
    Ok(Json(AdminActionInvocationListResponse {
        page: page_info(limit, data.last().map(|item| item.occurred_at)),
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
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Query(query): Query<OutboxQuery>,
) -> Result<Json<AdminOutboxListResponse>, ApiErrorResponse> {
    ensure_runtime_read_capability(&admin, &request_ctx)?;

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
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Query(query): Query<FunctionRunQuery>,
) -> Result<Json<AdminFunctionRunListResponse>, ApiErrorResponse> {
    ensure_runtime_read_capability(&admin, &request_ctx)?;

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

    let data: Vec<AdminFunctionRun> = rows
        .into_iter()
        .map(Into::into)
        .map(enrich_function_run)
        .collect();
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
            body = AdminOutboxEventDetail,
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
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path(id): Path<String>,
) -> Result<Json<AdminOutboxEventDetail>, ApiErrorResponse> {
    ensure_runtime_read_capability(&admin, &request_ctx)?;

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
            body = AdminFunctionRunDetail,
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
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path(id): Path<String>,
) -> Result<Json<AdminFunctionRunDetail>, ApiErrorResponse> {
    ensure_runtime_read_capability(&admin, &request_ctx)?;

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

    Ok(enrich_function_run_detail(row.into()))
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
) -> Result<Json<AdminOutboxEvent>, ApiErrorResponse> {
    ensure_runtime_service_or_system(&admin, &request_ctx)?;

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
            body = AdminFunctionRun,
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
) -> Result<Json<AdminFunctionRun>, ApiErrorResponse> {
    ensure_runtime_service_or_system(&admin, &request_ctx)?;

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

    Ok(json(enrich_function_run(row.into())))
}
