use crate::protocol::{RemoteErrorDetail, RemoteErrorEnvelope};
use platform_core::error::ErrorDetail;
use platform_core::{AppError, AppResult, ErrorCode};
use reqwest::header::CONTENT_TYPE;
use reqwest::{Response, StatusCode};

pub(crate) const MAX_REMOTE_JSON_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ResponseBodyPolicy {
    pub max_bytes: Option<u64>,
    pub require_json_content_type: bool,
    pub allow_empty_success: bool,
}

pub(crate) async fn decode_json_response_with_policy<T: serde::de::DeserializeOwned>(
    response: Response,
    operation: &str,
    not_found_as_none: bool,
    policy: ResponseBodyPolicy,
) -> AppResult<Option<T>> {
    let status = response.status();
    if let Some(max_bytes) = policy.max_bytes {
        ensure_content_length(&response, operation, max_bytes)?;
    }
    let content_type_error = if policy.require_json_content_type && status.is_success() {
        json_content_type_error(&response, operation)
    } else {
        None
    };

    let body = read_response_body(response, operation, policy.max_bytes).await?;

    if status.is_success() {
        if policy.allow_empty_success && status == StatusCode::NO_CONTENT && body.is_empty() {
            return Ok(None);
        }
        if let Some(error) = content_type_error {
            return Err(error);
        }
        return serde_json::from_slice::<T>(&body)
            .map(Some)
            .map_err(|error| {
                AppError::new(
                    ErrorCode::ExternalDependency,
                    format!("remote {operation} response was invalid JSON: {error}"),
                )
            });
    }

    if let Ok(envelope) = serde_json::from_slice::<RemoteErrorEnvelope>(&body) {
        return Err(remote_error(status, envelope));
    }

    if status == StatusCode::NOT_FOUND && not_found_as_none {
        return Ok(None);
    }

    Err(fallback_status_error(status, operation))
}

async fn read_response_body(
    mut response: Response,
    operation: &str,
    max_bytes: Option<u64>,
) -> AppResult<Vec<u8>> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| {
        AppError::new(
            ErrorCode::ExternalDependency,
            format!("remote {operation} response body could not be read: {error}"),
        )
        .retryable()
    })? {
        let next_len = body.len() as u64 + chunk.len() as u64;
        if let Some(max_bytes) = max_bytes
            && next_len > max_bytes
        {
            return Err(response_too_large(operation, next_len, max_bytes));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn ensure_content_length(response: &Response, operation: &str, max_bytes: u64) -> AppResult<()> {
    if let Some(content_length) = response.content_length()
        && content_length > max_bytes
    {
        return Err(response_too_large(operation, content_length, max_bytes));
    }
    Ok(())
}

fn response_too_large(operation: &str, actual_bytes: u64, max_bytes: u64) -> AppError {
    AppError::new(
        ErrorCode::ExternalDependency,
        format!(
            "remote {operation} response body exceeded {max_bytes} bytes: {actual_bytes} bytes"
        ),
    )
    .retryable()
}

fn json_content_type_error(response: &Response, operation: &str) -> Option<AppError> {
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    let Some(content_type) = content_type else {
        return Some(invalid_content_type(operation, None));
    };

    let media_type = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if media_type == "application/json"
        || (media_type.starts_with("application/") && media_type.ends_with("+json"))
    {
        return None;
    }

    Some(invalid_content_type(operation, Some(content_type)))
}

fn invalid_content_type(operation: &str, content_type: Option<&str>) -> AppError {
    match content_type {
        Some(content_type) => AppError::new(
            ErrorCode::ExternalDependency,
            format!("remote {operation} response content-type was not JSON: {content_type}"),
        ),
        None => AppError::new(
            ErrorCode::ExternalDependency,
            format!("remote {operation} response content-type was missing"),
        ),
    }
}

pub(crate) fn remote_error(status: StatusCode, envelope: RemoteErrorEnvelope) -> AppError {
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

pub(crate) fn fallback_status_error(status: StatusCode, operation: &str) -> AppError {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[tokio::test]
    async fn chunked_response_larger_than_policy_is_rejected() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("test server address");
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n",
                )
                .expect("write response headers");
            let chunk = vec![b'x'; 64 * 1024];
            for _ in 0..65 {
                stream
                    .write_all(format!("{:X}\r\n", chunk.len()).as_bytes())
                    .expect("write chunk length");
                stream.write_all(&chunk).expect("write chunk");
                stream.write_all(b"\r\n").expect("write chunk terminator");
            }
            let _ = stream.write_all(b"0\r\n\r\n");
        });

        let response = reqwest::get(format!("http://{address}"))
            .await
            .expect("response");
        let error = decode_json_response_with_policy::<serde_json::Value>(
            response,
            "chunked test",
            false,
            ResponseBodyPolicy {
                max_bytes: Some(MAX_REMOTE_JSON_RESPONSE_BYTES),
                require_json_content_type: true,
                allow_empty_success: false,
            },
        )
        .await
        .expect_err("chunked body must exceed the policy limit");

        assert!(error.to_string().contains("exceeded"));
    }
}
