pub mod auth;
pub mod context;
pub mod errors;
pub mod health;
pub mod json;
pub mod responses;
pub mod routes;

pub use auth::{
    AdminActor, AuthenticatedActor, CONSOLE_ADMIN_SCOPE, OptionalActor, ServiceActor, UserActor,
};
pub use context::{HttpRequestContext, request_context_middleware};
pub use errors::ApiErrorBody as ErrorResponse;
pub use errors::{ApiErrorBody, ApiErrorResponse, ErrorBody, IntoApiError, ValidationErrorDetail};
pub use json::JsonBody;
pub use routes::{ApiOpenApiRouter, ApiRouter, base_router};

pub use utoipa_axum::router::OpenApiRouter;
pub use utoipa_axum::routes;
