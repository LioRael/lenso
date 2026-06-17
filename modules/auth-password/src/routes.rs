use crate::dto::PasswordSessionResponse;
use crate::repositories::{AuthToken, PasswordAuthRepository};
use axum::Json;
use axum::extract::State;
use chrono::Duration;
use platform_core::AppContext;
use platform_http::responses::json;
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
        content = crate::dto::PasswordRegisterRequest,
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
            body = PasswordSessionResponse,
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
    JsonBody(input): JsonBody<crate::dto::PasswordRegisterRequest>,
) -> Result<Json<PasswordSessionResponse>, ApiErrorResponse> {
    let now = ctx.clock.now();
    let password_config = crate::config::AuthPasswordConfig::from_context(&ctx)
        .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?;
    let auth_token = PasswordAuthRepository::new(ctx.db.clone())
        .register(
            &input.identifier,
            &input.password,
            ctx.ids.new_id("usr"),
            ctx.ids.new_id("auth_identity"),
            ctx.ids.new_id("sess"),
            now,
            now + Duration::hours(SESSION_TTL_HOURS),
            &password_config,
        )
        .await
        .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?;

    Ok(json(auth_token_to_response(auth_token)))
}

#[utoipa::path(
    post,
    path = "/v1/auth/password/login",
    operation_id = "auth_password_login",
    tag = "auth",
    request_body(
        content = crate::dto::PasswordLoginRequest,
        content_type = "application/json",
        description = "Create a session or JWT for a password identity"
    ),
    params(
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier")
    ),
    responses(
        (
            status = 200,
            description = "Password authentication successful",
            body = PasswordSessionResponse,
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
    JsonBody(input): JsonBody<crate::dto::PasswordLoginRequest>,
) -> Result<Json<PasswordSessionResponse>, ApiErrorResponse> {
    let now = ctx.clock.now();
    let password_config = crate::config::AuthPasswordConfig::from_context(&ctx)
        .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?;
    let auth_token = PasswordAuthRepository::new(ctx.db.clone())
        .login(
            &input.identifier,
            &input.password,
            ctx.ids.new_id("sess"),
            now,
            now + Duration::hours(SESSION_TTL_HOURS),
            &password_config,
        )
        .await
        .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?;

    Ok(json(auth_token_to_response(auth_token)))
}

fn auth_token_to_response(token: AuthToken) -> PasswordSessionResponse {
    match token {
        AuthToken::Session(session) => PasswordSessionResponse {
            user_id: session.user_id.0,
            session_id: Some(session.id),
            token: session.token,
            expires_at: session.expires_at,
        },
        AuthToken::Jwt {
            user_id,
            token,
            expires_at,
        } => PasswordSessionResponse {
            user_id,
            session_id: None,
            token,
            expires_at,
        },
    }
}
