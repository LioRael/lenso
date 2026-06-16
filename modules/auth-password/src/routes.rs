use crate::dto::{
    PasswordLoginRequest, PasswordRegisterRequest, PasswordSessionResponse,
    PasswordSessionResponseEnvelope,
};
use crate::repositories::PasswordAuthRepository;
use axum::Json;
use axum::extract::State;
use chrono::Duration;
use platform_core::AppContext;
use platform_http::responses::{DataResponse, json};
use platform_http::{
    ApiErrorResponse, ApiOpenApiRouter, ErrorResponse, HttpRequestContext, JsonBody, OpenApiRouter,
    routes,
};

const SESSION_TTL_HOURS: i64 = 12;

pub fn router() -> ApiOpenApiRouter {
    OpenApiRouter::new()
        .routes(routes!(register))
        .routes(routes!(login))
}

#[utoipa::path(
    post,
    path = "/v1/auth/password/register",
    operation_id = "auth_password_register",
    tag = "auth",
    request_body(
        content = PasswordRegisterRequest,
        content_type = "application/json",
        description = "Register a password identity for an identifier"
    ),
    params(
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier")
    ),
    responses(
        (
            status = 200,
            description = "Password identity registered",
            body = PasswordSessionResponseEnvelope,
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
            status = 409,
            description = "Identifier already exists",
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
async fn register(
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    JsonBody(input): JsonBody<PasswordRegisterRequest>,
) -> Result<Json<DataResponse<PasswordSessionResponse>>, ApiErrorResponse> {
    let now = ctx.clock.now();
    let session = PasswordAuthRepository::new(ctx.db.clone())
        .register(
            &input.identifier,
            &input.password,
            ctx.ids.new_id("usr"),
            ctx.ids.new_id("auth_identity"),
            ctx.ids.new_id("sess"),
            now,
            now + Duration::hours(SESSION_TTL_HOURS),
        )
        .await
        .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?;

    Ok(json(PasswordSessionResponse {
        user_id: session.user_id.0,
        session_id: session.id,
        token: session.token,
        expires_at: session.expires_at,
    }))
}

#[utoipa::path(
    post,
    path = "/v1/auth/password/login",
    operation_id = "auth_password_login",
    tag = "auth",
    request_body(
        content = PasswordLoginRequest,
        content_type = "application/json",
        description = "Create a session for a password identity"
    ),
    params(
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier")
    ),
    responses(
        (
            status = 200,
            description = "Password session created",
            body = PasswordSessionResponseEnvelope,
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
            status = 401,
            description = "Invalid identifier or password",
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
async fn login(
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    JsonBody(input): JsonBody<PasswordLoginRequest>,
) -> Result<Json<DataResponse<PasswordSessionResponse>>, ApiErrorResponse> {
    let now = ctx.clock.now();
    let session = PasswordAuthRepository::new(ctx.db.clone())
        .login(
            &input.identifier,
            &input.password,
            ctx.ids.new_id("sess"),
            now,
            now + Duration::hours(SESSION_TTL_HOURS),
        )
        .await
        .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?;

    Ok(json(PasswordSessionResponse {
        user_id: session.user_id.0,
        session_id: session.id,
        token: session.token,
        expires_at: session.expires_at,
    }))
}
