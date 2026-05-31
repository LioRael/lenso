use identity::dto::{CreateUserRequest, CreateUserResponse, CreateUserResponseEnvelope};
use platform_http::{ErrorBody, ErrorResponse, ValidationErrorDetail};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Lenso API",
        version = "1.0.0",
        description = "Rust-first modular monolith API contract"
    ),
    paths(identity_create_user_contract),
    components(
        schemas(
            CreateUserRequest,
            CreateUserResponse,
            CreateUserResponseEnvelope,
            ErrorResponse,
            ErrorBody,
            ValidationErrorDetail
        )
    ),
    tags(
        (name = "identity", description = "Identity domain APIs")
    )
)]
struct ApiDoc;

pub fn openapi_document() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
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
#[allow(dead_code)]
fn identity_create_user_contract() {}
