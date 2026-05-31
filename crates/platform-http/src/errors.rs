use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use platform_core::{AppError, ErrorCode, RequestContext};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = ErrorResponse)]
pub struct ApiErrorBody {
    pub error: ErrorBody,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    pub request_id: Option<String>,
    pub correlation_id: Option<String>,
    pub details: Vec<ValidationErrorDetail>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ValidationErrorDetail {
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
        let body = ApiErrorBody {
            error: ErrorBody {
                code: error.code.as_str().to_owned(),
                message: error.public_message,
                request_id,
                correlation_id,
                details: error
                    .details
                    .into_iter()
                    .map(|detail| ValidationErrorDetail {
                        field: detail.field,
                        reason: detail.reason,
                    })
                    .collect(),
            },
        };
        (status, Json(body)).into_response()
    }
}
