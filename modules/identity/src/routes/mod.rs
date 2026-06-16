use crate::commands::create_user::IdentityCommands;
use crate::dto::{
    CreateUserRequest, CreateUserResponse, CreateUserResponseEnvelope, MeResponse,
    MeResponseEnvelope,
};
use crate::public::{CreateUserCommand, IdentityService};
use crate::repositories::PostgresUserRepository;
use axum::Json;
use axum::extract::State;
use axum::routing::get;
use platform_core::AppContext;
use platform_http::responses::{DataResponse, json};
use platform_http::{
    ApiErrorResponse, ApiOpenApiRouter, ErrorResponse, HttpRequestContext, JsonBody, OpenApiRouter,
    UserActor, routes,
};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct IdentityRouteState {
    commands: Arc<IdentityCommands>,
}

impl IdentityRouteState {
    pub fn new(ctx: &AppContext) -> Self {
        Self {
            commands: Arc::new(IdentityCommands::new(
                Arc::new(PostgresUserRepository::new(ctx.db.clone())),
                ctx.events.clone(),
                ctx.clock.clone(),
                ctx.ids.clone(),
            )),
        }
    }
}

pub fn router() -> ApiOpenApiRouter {
    OpenApiRouter::new()
        .routes(routes!(create_user))
        .routes(routes!(me))
        .route("/v1/identity/health", get(health))
}

async fn health() -> Json<DataResponse<&'static str>> {
    json("identity")
}

#[utoipa::path(
    post,
    path = "/v1/identity/users",
    operation_id = "identity_create_user",
    tag = "identity",
    request_body(
        content = CreateUserRequest,
        content_type = "application/json",
        description = "Create a new identity user"
    ),
    params(
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier")
    ),
    responses(
        (
            status = 200,
            description = "User created",
            body = CreateUserResponseEnvelope,
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
            content_type = "application/json",
            headers(
                ("x-request-id" = String, description = "Request identifier for this HTTP request"),
                ("x-correlation-id" = String, description = "Correlation identifier shared across related work")
            )
        ),
        (
            status = 409,
            description = "User already exists",
            body = ErrorResponse,
            content_type = "application/json",
            headers(
                ("x-request-id" = String, description = "Request identifier for this HTTP request"),
                ("x-correlation-id" = String, description = "Correlation identifier shared across related work")
            )
        ),
        (
            status = 500,
            description = "Internal server error",
            body = ErrorResponse,
            content_type = "application/json",
            headers(
                ("x-request-id" = String, description = "Request identifier for this HTTP request"),
                ("x-correlation-id" = String, description = "Correlation identifier shared across related work")
            )
        )
    )
)]
async fn create_user(
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    JsonBody(input): JsonBody<CreateUserRequest>,
) -> Result<Json<DataResponse<CreateUserResponse>>, ApiErrorResponse> {
    let state = IdentityRouteState::new(&ctx);
    let user = state
        .commands
        .create_user(
            &request_ctx,
            CreateUserCommand {
                email: input.email,
                display_name: input.display_name,
            },
        )
        .await
        .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?;

    Ok(json(user.into()))
}

#[utoipa::path(
    get,
    path = "/v1/identity/me",
    operation_id = "identity_me",
    tag = "identity",
    params(
        ("authorization" = String, Header, description = "Development bearer token, for example `Bearer dev-user:user_123`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier")
    ),
    responses(
        (
            status = 200,
            description = "Current authenticated user",
            body = MeResponseEnvelope,
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
            content_type = "application/json",
            headers(
                ("x-request-id" = String, description = "Request identifier for this HTTP request"),
                ("x-correlation-id" = String, description = "Correlation identifier shared across related work")
            )
        ),
        (
            status = 403,
            description = "User authentication is required",
            body = ErrorResponse,
            content_type = "application/json",
            headers(
                ("x-request-id" = String, description = "Request identifier for this HTTP request"),
                ("x-correlation-id" = String, description = "Correlation identifier shared across related work")
            )
        ),
        (
            status = 500,
            description = "Internal server error",
            body = ErrorResponse,
            content_type = "application/json",
            headers(
                ("x-request-id" = String, description = "Request identifier for this HTTP request"),
                ("x-correlation-id" = String, description = "Correlation identifier shared across related work")
            )
        )
    )
)]
async fn me(user: UserActor) -> Json<DataResponse<MeResponse>> {
    json(MeResponse {
        user_id: user.user_id,
        scopes: user.scopes,
    })
}
