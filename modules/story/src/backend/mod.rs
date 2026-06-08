use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::{DateTime, Duration, Utc};
use platform_core::{
    AppContext, AppError, ErrorCode, ExecutionLogRow, RequestContext, StoryDisplayDescriptor,
    StoryDisplaySource, TelemetrySpan, TelemetrySpanQuery,
};
use platform_http::{
    AdminActor, ApiErrorResponse, ApiOpenApiRouter, ErrorResponse, HttpRequestContext,
    OpenApiRouter, routes,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use std::sync::{OnceLock, RwLock};
use utoipa::{IntoParams, ToSchema};

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 100;

mod catalog;
mod dto;
mod graph;
mod heatmap;
mod queries;
mod rows;
mod support;
mod technical_ops;

pub use catalog::{install_default_story_display, install_story_display};
#[cfg(debug_assertions)]
pub use catalog::{reset_catalogs_for_test, story_display_catalog_snapshot};
#[allow(unused_imports)]
use dto::*;
#[allow(unused_imports)]
use graph::*;
#[allow(unused_imports)]
use heatmap::*;
#[allow(unused_imports)]
use queries::*;
#[allow(unused_imports)]
use rows::*;
#[allow(unused_imports)]
use support::*;
#[allow(unused_imports)]
use technical_ops::*;

pub fn router() -> ApiOpenApiRouter {
    OpenApiRouter::new()
        .routes(routes!(list_stories))
        .routes(routes!(get_story))
        .routes(routes!(get_story_heatmap))
        .routes(routes!(get_story_technical_operations))
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
async fn get_story_heatmap(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path(correlation_id): Path<String>,
    Query(query): Query<HeatmapQuery>,
) -> Result<Json<AdminRuntimeHeatmapResponse>, ApiErrorResponse> {
    if !runtime_story_exists(&ctx, &request_ctx, &correlation_id).await? {
        return Err(story_not_found(&request_ctx, &correlation_id));
    }

    let limit = normalized_limit(query.limit);
    let bucket_seconds = normalized_bucket_seconds(query.bucket_seconds);
    let rows = fetch_heatmap_rows(
        &ctx,
        &request_ctx,
        &query,
        limit,
        bucket_seconds,
        &correlation_id,
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
async fn get_story_technical_operations(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path(correlation_id): Path<String>,
) -> Result<Json<AdminRuntimeTechnicalOperationListResponse>, ApiErrorResponse> {
    let rows = fetch_story_rows(&ctx, &request_ctx, Some(&correlation_id), None, MAX_LIMIT).await?;
    if rows.is_empty() {
        return Err(story_not_found(&request_ctx, &correlation_id));
    }

    let spans = ctx
        .telemetry_spans
        .query_spans(TelemetrySpanQuery::by_correlation_id(&correlation_id))
        .await
        .map_err(|source| ApiErrorResponse::with_context(source, &request_ctx))?;
    let node_index = runtime_node_index(&rows);
    let mut data = technical_operations_from_spans(spans.clone(), &node_index);
    data.extend(
        remote_proxy_technical_operations(&ctx, &request_ctx, &correlation_id, &spans, &node_index)
            .await?,
    );
    data.extend(
        remote_runtime_technical_operations_by_correlation(
            &ctx,
            &request_ctx,
            &correlation_id,
            &node_index,
        )
        .await?,
    );
    data.extend(admin_action_technical_operations(&rows, &node_index));
    sort_technical_operations(&mut data);

    Ok(Json(AdminRuntimeTechnicalOperationListResponse {
        data,
        order: "started_at_asc",
    }))
}
