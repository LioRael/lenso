use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use platform_core::{AppError, ErrorCode, RequestContext};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = ErrorResponse)]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    pub problem_type: String,
    pub title: String,
    #[schema(minimum = 100, maximum = 599)]
    pub status: u16,
    pub detail: String,
    pub code: String,
    pub request_id: Option<String>,
    pub correlation_id: Option<String>,
    pub errors: Vec<ProblemErrorDetail>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_actions: Option<Vec<String>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProblemErrorDetail {
    pub field: Option<String>,
    pub reason: String,
}

pub trait IntoApiError {
    fn status_code(&self) -> StatusCode;
}

impl IntoApiError for AppError {
    fn status_code(&self) -> StatusCode {
        match self.code {
            ErrorCode::Validation => StatusCode::BAD_REQUEST,
            ErrorCode::Unauthorized => StatusCode::UNAUTHORIZED,
            ErrorCode::Forbidden => StatusCode::FORBIDDEN,
            ErrorCode::NotFound => StatusCode::NOT_FOUND,
            ErrorCode::Conflict => StatusCode::CONFLICT,
            ErrorCode::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            ErrorCode::ExternalDependency => StatusCode::BAD_GATEWAY,
            ErrorCode::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[derive(Debug)]
pub struct ApiErrorResponse {
    pub error: AppError,
    pub context: Option<RequestContext>,
}

impl From<AppError> for ApiErrorResponse {
    fn from(error: AppError) -> Self {
        Self {
            error,
            context: None,
        }
    }
}

impl ApiErrorResponse {
    pub fn with_context(error: AppError, context: &RequestContext) -> Self {
        Self {
            error,
            context: Some(context.clone()),
        }
    }
}

impl IntoResponse for ApiErrorResponse {
    fn into_response(self) -> Response {
        let error = self.error;
        let status = error.status_code();
        let request_id = self.context.as_ref().map(|ctx| ctx.request_id.0.clone());
        let correlation_id = self
            .context
            .as_ref()
            .map(|ctx| ctx.correlation_id.0.clone());
        tracing::warn!(
            error_code = error.code.as_str(),
            status = status.as_u16(),
            request_id = request_id.as_deref().unwrap_or(""),
            correlation_id = correlation_id.as_deref().unwrap_or(""),
            "HTTP request failed"
        );
        let code = error.code;
        let body = ProblemDetails {
            problem_type: problem_type(code),
            title: problem_title(code).to_owned(),
            status: status.as_u16(),
            detail: error.public_message,
            code: code.as_str().to_owned(),
            request_id,
            correlation_id,
            errors: error
                .details
                .into_iter()
                .map(|detail| ProblemErrorDetail {
                    field: detail.field,
                    reason: detail.reason,
                })
                .collect(),
            next_actions: None,
        };
        let mut response = (status, Json(body)).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        if let Ok(value) = HeaderValue::from_str(code.as_str()) {
            response.headers_mut().insert("x-lenso-error-code", value);
        }
        response
    }
}

fn problem_title(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::Validation => "Validation failed",
        ErrorCode::Unauthorized => "Unauthorized",
        ErrorCode::Forbidden => "Forbidden",
        ErrorCode::NotFound => "Not found",
        ErrorCode::Conflict => "Conflict",
        ErrorCode::RateLimited => "Rate limited",
        ErrorCode::ExternalDependency => "External dependency failure",
        ErrorCode::Internal => "Internal error",
    }
}

fn problem_type(code: ErrorCode) -> String {
    format!("https://lenso.dev/problems/{}", code.as_str())
}
