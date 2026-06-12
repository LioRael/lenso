#[allow(clippy::wildcard_imports)]
use super::*;
use chrono::{DateTime, Utc};
use platform_core::{AppError, ErrorCode};
use platform_http::{AdminActor, ApiErrorResponse};

pub(crate) fn query_error(
    source: sqlx::Error,
    request_ctx: &platform_core::RequestContext,
) -> ApiErrorResponse {
    ApiErrorResponse::with_context(
        AppError::new(ErrorCode::Internal, "Runtime console query failed").with_source(source),
        request_ctx,
    )
}

pub(crate) fn admin_audit_label(actor: &AdminActor) -> String {
    match actor {
        AdminActor::Service { service_id, .. } => format!("service:{service_id}"),
        AdminActor::System => "system".to_owned(),
    }
}

pub(crate) fn normalized_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

pub(crate) fn normalized_bucket_seconds(bucket_seconds: Option<i64>) -> i64 {
    bucket_seconds.unwrap_or(300).clamp(60, 3600)
}

pub(crate) fn page_info(limit: i64, next_created_before: Option<DateTime<Utc>>) -> PageInfo {
    PageInfo {
        limit,
        next_created_before,
    }
}
