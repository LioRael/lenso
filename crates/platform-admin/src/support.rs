#[allow(clippy::wildcard_imports)]
use super::*;
use chrono::{DateTime, Utc};
use platform_core::{AppError, ErrorCode};
use platform_http::{AdminActor, ApiErrorResponse};

const RUNTIME_STORIES_READ_CAPABILITY: &str = "runtime.stories.read";

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
        AdminActor::User { user_id, .. } => format!("user:{user_id}"),
        AdminActor::System => "system".to_owned(),
    }
}

pub(crate) fn ensure_runtime_read_capability(
    admin: &AdminActor,
    request_ctx: &platform_core::RequestContext,
) -> Result<(), ApiErrorResponse> {
    match admin {
        AdminActor::System | AdminActor::Service { .. } => Ok(()),
        AdminActor::User { scopes, .. }
            if scopes
                .iter()
                .any(|scope| scope == RUNTIME_STORIES_READ_CAPABILITY) =>
        {
            Ok(())
        }
        AdminActor::User { .. } => Err(ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::Forbidden,
                format!("missing runtime console capability: {RUNTIME_STORIES_READ_CAPABILITY}"),
            ),
            request_ctx,
        )),
    }
}

pub(crate) fn ensure_runtime_service_or_system(
    admin: &AdminActor,
    request_ctx: &platform_core::RequestContext,
) -> Result<(), ApiErrorResponse> {
    match admin {
        AdminActor::Service { .. } | AdminActor::System => Ok(()),
        AdminActor::User { .. } => Err(ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::Forbidden,
                "Service or system authentication is required",
            ),
            request_ctx,
        )),
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
