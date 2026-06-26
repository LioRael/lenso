#[allow(clippy::wildcard_imports)]
use super::*;
use crate::module::STORY_CONSOLE_CAPABILITY;

pub(super) fn ensure_story_read_capability(
    admin: &AdminActor,
    request_ctx: &RequestContext,
) -> Result<(), ApiErrorResponse> {
    match admin {
        AdminActor::System | AdminActor::Service { .. } => Ok(()),
        AdminActor::User { scopes, .. }
            if scopes.iter().any(|scope| scope == STORY_CONSOLE_CAPABILITY) =>
        {
            Ok(())
        }
        AdminActor::User { .. } => Err(ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::Forbidden,
                format!("missing runtime console capability: {STORY_CONSOLE_CAPABILITY}"),
            ),
            request_ctx,
        )),
    }
}

pub(super) fn normalized_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

pub(super) fn normalized_bucket_seconds(bucket_seconds: Option<i64>) -> i64 {
    bucket_seconds.unwrap_or(300).clamp(60, 3600)
}

pub(super) fn page_info(limit: i64, next_created_before: Option<DateTime<Utc>>) -> PageInfo {
    PageInfo {
        limit,
        next_created_before,
    }
}

pub(super) fn story_not_found(
    request_ctx: &RequestContext,
    correlation_id: &str,
) -> ApiErrorResponse {
    ApiErrorResponse::with_context(
        AppError::new(
            ErrorCode::NotFound,
            format!("Runtime story {correlation_id} was not found"),
        ),
        request_ctx,
    )
}

pub(super) fn query_error(source: sqlx::Error, request_ctx: &RequestContext) -> ApiErrorResponse {
    ApiErrorResponse::with_context(
        AppError::new(ErrorCode::Internal, "Runtime story query failed").with_source(source),
        request_ctx,
    )
}
