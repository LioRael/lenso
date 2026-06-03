use crate::protocol::{RemoteErrorDetail, RemoteErrorEnvelope};
use platform_core::error::ErrorDetail;
use platform_core::{AppError, AppResult, ErrorCode};
use reqwest::{Response, StatusCode};

pub(crate) async fn decode_json_response<T: serde::de::DeserializeOwned>(
    response: Response,
    operation: &str,
    not_found_as_none: bool,
) -> AppResult<Option<T>> {
    let status = response.status();
    let body = response.text().await.map_err(|error| {
        AppError::new(
            ErrorCode::ExternalDependency,
            format!("remote {operation} response body could not be read: {error}"),
        )
        .retryable()
    })?;

    if status.is_success() {
        return serde_json::from_str::<T>(&body).map(Some).map_err(|error| {
            AppError::new(
                ErrorCode::ExternalDependency,
                format!("remote {operation} response was invalid JSON: {error}"),
            )
        });
    }

    if let Ok(envelope) = serde_json::from_str::<RemoteErrorEnvelope>(&body) {
        return Err(remote_error(status, envelope));
    }

    if status == StatusCode::NOT_FOUND && not_found_as_none {
        return Ok(None);
    }

    Err(fallback_status_error(status, operation))
}

fn remote_error(status: StatusCode, envelope: RemoteErrorEnvelope) -> AppError {
    let remote = envelope.error;
    let mut error = AppError::new(error_code_from_remote(&remote.code, status), remote.message);
    error.details = remote
        .details
        .into_iter()
        .map(remote_detail)
        .chain([
            ErrorDetail {
                field: Some("remote_status".to_owned()),
                reason: status.as_u16().to_string(),
            },
            ErrorDetail {
                field: Some("remote_code".to_owned()),
                reason: remote.code,
            },
        ])
        .collect();
    if remote.retryable || status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
        error = error.retryable();
    }
    error
}

fn remote_detail(detail: RemoteErrorDetail) -> ErrorDetail {
    ErrorDetail {
        field: detail.field,
        reason: detail.reason,
    }
}

fn fallback_status_error(status: StatusCode, operation: &str) -> AppError {
    let mut error = AppError::new(
        error_code_from_status(status),
        format!("remote {operation} returned status {status}"),
    );
    error.details = vec![ErrorDetail {
        field: Some("remote_status".to_owned()),
        reason: status.as_u16().to_string(),
    }];
    if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
        error = error.retryable();
    }
    error
}

fn error_code_from_remote(code: &str, status: StatusCode) -> ErrorCode {
    if status.is_server_error() {
        return ErrorCode::ExternalDependency;
    }

    match code {
        "validation" | "validation_failed" => ErrorCode::Validation,
        "unauthorized" => ErrorCode::Unauthorized,
        "forbidden" => ErrorCode::Forbidden,
        "not_found" => ErrorCode::NotFound,
        "conflict" => ErrorCode::Conflict,
        "rate_limited" => ErrorCode::RateLimited,
        "external_dependency" | "external_dependency_failure" => ErrorCode::ExternalDependency,
        "internal" | "internal_error" => ErrorCode::Internal,
        _ => error_code_from_status(status),
    }
}

fn error_code_from_status(status: StatusCode) -> ErrorCode {
    match status {
        StatusCode::BAD_REQUEST => ErrorCode::Validation,
        StatusCode::UNAUTHORIZED => ErrorCode::Unauthorized,
        StatusCode::FORBIDDEN => ErrorCode::Forbidden,
        StatusCode::NOT_FOUND => ErrorCode::NotFound,
        StatusCode::CONFLICT => ErrorCode::Conflict,
        StatusCode::TOO_MANY_REQUESTS => ErrorCode::RateLimited,
        _ => ErrorCode::ExternalDependency,
    }
}
