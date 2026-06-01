use crate::commands::create_user::IdentityCommands;
use crate::dto::{CreateUserRequest, CreateUserResponse, MeResponse};
use crate::public::{CreateUserCommand, IdentityService};
use crate::repositories::PostgresUserRepository;
use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use platform_core::AppContext;
use platform_http::responses::{DataResponse, json};
use platform_http::{ApiErrorResponse, HttpRequestContext, JsonBody, UserActor};
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

pub fn router() -> Router<AppContext> {
    Router::new()
        .route("/v1/identity/health", get(health))
        .route("/v1/identity/users", post(create_user))
        .route("/v1/identity/me", get(me))
}

async fn health() -> Json<DataResponse<&'static str>> {
    json("identity")
}

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

async fn me(user: UserActor) -> Json<DataResponse<MeResponse>> {
    json(MeResponse {
        user_id: user.user_id,
        scopes: user.scopes,
    })
}
