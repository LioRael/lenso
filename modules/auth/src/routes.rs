use crate::dto::{CreateDevSessionRequest, CreateDevSessionResponse, RevokeSessionResponse};
use crate::models::AuthUserId;
use crate::repositories::PostgresAuthUserRepository;
use crate::resolver::first_session_token;
use axum::Json;
use axum::extract::State;
use axum::http::header::{AUTHORIZATION, COOKIE};
use axum::http::{HeaderMap, HeaderName};
use chrono::Duration;
use platform_core::error::ErrorDetail;
use platform_core::{
    ActorResolutionRequest, AppContext, AppError, ErrorCode, is_local_development_environment,
};
use platform_http::responses::json;
use platform_http::{
    ApiErrorResponse, ApiOpenApiRouter, ErrorResponse, HttpRequestContext, JsonBody, OpenApiRouter,
    routes,
};

const DEV_SESSION_TTL_HOURS: i64 = 12;

pub fn router() -> ApiOpenApiRouter {
    OpenApiRouter::new()
        .routes(routes!(create_dev_session))
        .routes(routes!(revoke_session))
}

#[utoipa::path(
    post,
    path = "/v1/auth/dev/sessions",
    operation_id = "auth_create_dev_session",
    tag = "auth",
    request_body(
        content = CreateDevSessionRequest,
        content_type = "application/json",
        description = "Create a local-development auth session for a user id"
    ),
    params(
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier")
    ),
    responses(
        (
            status = 200,
            description = "Development session created",
            body = CreateDevSessionResponse,
            content_type = "application/json",
            headers(
                ("x-request-id" = String, description = "Request identifier for this HTTP request"),
                ("x-correlation-id" = String, description = "Correlation identifier shared across related work")
            )
        ),
        (
            status = 400,
            description = "Request validation failed",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 403,
            description = "Development sessions are only available in local environments",
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
async fn create_dev_session(
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    JsonBody(input): JsonBody<CreateDevSessionRequest>,
) -> Result<Json<CreateDevSessionResponse>, ApiErrorResponse> {
    if !is_local_development_environment(&ctx.config.service.environment) {
        return Err(ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::Forbidden,
                "Development auth sessions are only available in local environments",
            ),
            &request_ctx,
        ));
    }

    let user_id = input.user_id.trim();
    if user_id.is_empty() {
        return Err(ApiErrorResponse::with_context(
            AppError::validation(
                "Request validation failed",
                vec![ErrorDetail {
                    field: Some("user_id".to_owned()),
                    reason: "user_id must not be empty".to_owned(),
                }],
            ),
            &request_ctx,
        ));
    }

    let now = ctx.clock.now();
    let session = PostgresAuthUserRepository::new(ctx.db.clone())
        .create_dev_session(
            AuthUserId(user_id.to_owned()),
            ctx.ids.new_id("sess"),
            ctx.ids.new_id("dev_session_token"),
            now,
            now + Duration::hours(DEV_SESSION_TTL_HOURS),
        )
        .await
        .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?;

    Ok(json(CreateDevSessionResponse {
        user_id: session.user_id.0,
        session_id: session.id,
        token: session.token,
        expires_at: session.expires_at,
    }))
}

#[utoipa::path(
    post,
    path = "/v1/auth/sessions/revoke",
    operation_id = "auth_revoke_session",
    tag = "auth",
    params(
        ("authorization" = Option<String>, Header, description = "Bearer session token"),
        ("cookie" = Option<String>, Header, description = "Cookie header containing `lenso_session`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier")
    ),
    responses(
        (
            status = 200,
            description = "Session revoke attempted",
            body = RevokeSessionResponse,
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
            status = 500,
            description = "Internal server error",
            body = ErrorResponse,
            content_type = "application/json"
        )
    )
)]
async fn revoke_session(
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    headers: HeaderMap,
) -> Result<Json<RevokeSessionResponse>, ApiErrorResponse> {
    let request = ActorResolutionRequest {
        authorization: header_value(&headers, AUTHORIZATION),
        cookie: header_value(&headers, COOKIE),
    };
    let Some(token) = first_session_token(&request) else {
        return Err(ApiErrorResponse::with_context(
            AppError::new(ErrorCode::Unauthorized, "Authentication is required"),
            &request_ctx,
        ));
    };

    let revoked = PostgresAuthUserRepository::new(ctx.db.clone())
        .revoke_session_token(&token, ctx.clock.now())
        .await
        .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?;

    Ok(json(RevokeSessionResponse { revoked }))
}

fn header_value(headers: &HeaderMap, name: HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
}
