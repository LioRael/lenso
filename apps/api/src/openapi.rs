use crate::admin_runtime::{
    AdminFunctionRun, AdminFunctionRunListResponse, AdminFunctionRunResponse, AdminOutboxEvent,
    AdminOutboxListResponse, FunctionRunQuery, OutboxQuery, PageInfo,
};
use identity::dto::{
    CreateUserRequest, CreateUserResponse, CreateUserResponseEnvelope, MeResponse,
    MeResponseEnvelope,
};
use platform_http::{ErrorBody, ErrorResponse, ValidationErrorDetail};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Lenso API",
        version = "1.0.0",
        description = "Rust-first modular monolith API contract"
    ),
    paths(
        identity_create_user_contract,
        identity_me_contract,
        admin_runtime_list_outbox_contract,
        admin_runtime_retry_outbox_contract,
        admin_runtime_list_function_runs_contract,
        admin_runtime_get_function_run_contract,
        admin_runtime_retry_function_run_contract
    ),
    components(
        schemas(
            AdminFunctionRun,
            AdminFunctionRunListResponse,
            AdminFunctionRunResponse,
            AdminOutboxEvent,
            AdminOutboxListResponse,
            CreateUserRequest,
            CreateUserResponse,
            CreateUserResponseEnvelope,
            MeResponse,
            MeResponseEnvelope,
            PageInfo,
            ErrorResponse,
            ErrorBody,
            ValidationErrorDetail
        )
    ),
    tags(
        (name = "identity", description = "Identity domain APIs"),
        (name = "admin-runtime", description = "Read-only runtime console APIs")
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
#[allow(dead_code)]
fn identity_me_contract() {}

#[utoipa::path(
    get,
    path = "/admin/runtime/outbox",
    operation_id = "admin_runtime_list_outbox",
    tag = "admin-runtime",
    params(
        ("authorization" = String, Header, description = "Development service bearer token, for example `Bearer dev-service:admin`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier"),
        OutboxQuery
    ),
    responses(
        (
            status = 200,
            description = "Outbox events",
            body = AdminOutboxListResponse,
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
            status = 403,
            description = "Service or system authentication is required",
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
#[allow(dead_code)]
fn admin_runtime_list_outbox_contract() {}

#[utoipa::path(
    post,
    path = "/admin/runtime/outbox/{id}/retry",
    operation_id = "admin_runtime_retry_outbox",
    tag = "admin-runtime",
    params(
        ("id" = String, Path, description = "Outbox event identifier"),
        ("authorization" = String, Header, description = "Development service bearer token, for example `Bearer dev-service:admin`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier")
    ),
    responses(
        (
            status = 200,
            description = "Outbox event retry was scheduled",
            body = AdminOutboxEvent,
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
            status = 403,
            description = "Service or system authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 404,
            description = "Outbox event was not found",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 409,
            description = "Outbox event status cannot be retried",
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
#[allow(dead_code)]
fn admin_runtime_retry_outbox_contract() {}

#[utoipa::path(
    get,
    path = "/admin/runtime/functions",
    operation_id = "admin_runtime_list_function_runs",
    tag = "admin-runtime",
    params(
        ("authorization" = String, Header, description = "Development service bearer token, for example `Bearer dev-service:admin`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier"),
        FunctionRunQuery
    ),
    responses(
        (
            status = 200,
            description = "Runtime function runs",
            body = AdminFunctionRunListResponse,
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
            status = 403,
            description = "Service or system authentication is required",
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
#[allow(dead_code)]
fn admin_runtime_list_function_runs_contract() {}

#[utoipa::path(
    get,
    path = "/admin/runtime/functions/{id}",
    operation_id = "admin_runtime_get_function_run",
    tag = "admin-runtime",
    params(
        ("id" = String, Path, description = "Runtime function run identifier"),
        ("authorization" = String, Header, description = "Development service bearer token, for example `Bearer dev-service:admin`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier")
    ),
    responses(
        (
            status = 200,
            description = "Runtime function run",
            body = AdminFunctionRunResponse,
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
            status = 403,
            description = "Service or system authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 404,
            description = "Function run was not found",
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
#[allow(dead_code)]
fn admin_runtime_get_function_run_contract() {}

#[utoipa::path(
    post,
    path = "/admin/runtime/functions/{id}/retry",
    operation_id = "admin_runtime_retry_function_run",
    tag = "admin-runtime",
    params(
        ("id" = String, Path, description = "Runtime function run identifier"),
        ("authorization" = String, Header, description = "Development service bearer token, for example `Bearer dev-service:admin`"),
        ("x-request-id" = Option<String>, Header, description = "Optional caller-provided request identifier"),
        ("x-correlation-id" = Option<String>, Header, description = "Optional caller-provided correlation identifier")
    ),
    responses(
        (
            status = 200,
            description = "Runtime function run retry was scheduled",
            body = AdminFunctionRunResponse,
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
            status = 403,
            description = "Service or system authentication is required",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 404,
            description = "Function run was not found",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 409,
            description = "Function run status cannot be retried",
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
#[allow(dead_code)]
fn admin_runtime_retry_function_run_contract() {}
