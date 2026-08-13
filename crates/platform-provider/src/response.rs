use crate::protocol::{ProviderErrorDetail, ProviderErrorEnvelope};
use platform_core::error::ErrorDetail;
use platform_core::{AppError, AppResult, ErrorCode};
use reqwest::header::CONTENT_TYPE;
use reqwest::{Response, StatusCode};

pub(crate) const MAX_PROVIDER_JSON_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;

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
                    format!("provider {operation} response was invalid JSON: {error}"),
                )
            });
    }

    if let Ok(envelope) = serde_json::from_slice::<ProviderErrorEnvelope>(&body) {
        return Err(provider_error(status, envelope));
    }

    if status == StatusCode::NOT_FOUND && not_found_as_none {
        return Ok(None);
    }

    Err(fallback_status_error(status, operation))
}

/// Reads a Provider JSON response while preserving whether the failure was an
/// ambiguous body-read loss after a successful POST or a fully received
/// semantic HTTP response. Only the former is safe for GET recovery.
pub(crate) async fn decode_invocation_response<T: serde::de::DeserializeOwned>(
    response: Response,
    operation: &str,
    policy: ResponseBodyPolicy,
) -> Result<Option<T>, InvocationResponseError> {
    let status = response.status();
    if let Some(max_bytes) = policy.max_bytes {
        ensure_content_length(&response, operation, max_bytes)
            .map_err(InvocationResponseError::Received)?;
    }
    let content_type_error = if policy.require_json_content_type && status.is_success() {
        json_content_type_error(&response, operation)
    } else {
        None
    };
    let body = read_invocation_response_body(response, operation, policy.max_bytes).await?;

    if status.is_success() {
        if policy.allow_empty_success && status == StatusCode::NO_CONTENT && body.is_empty() {
            return Ok(None);
        }
        if let Some(error) = content_type_error {
            return Err(InvocationResponseError::Received(error));
        }
        return serde_json::from_slice::<T>(&body)
            .map(Some)
            .map_err(|error| {
                InvocationResponseError::Received(AppError::new(
                    ErrorCode::ExternalDependency,
                    format!("provider {operation} response was invalid JSON: {error}"),
                ))
            });
    }

    if let Ok(envelope) = serde_json::from_slice::<ProviderErrorEnvelope>(&body) {
        return Err(InvocationResponseError::Received(provider_error(
            status, envelope,
        )));
    }
    Err(InvocationResponseError::Received(fallback_status_error(
        status, operation,
    )))
}

#[derive(Debug)]
pub(crate) enum InvocationResponseError {
    Ambiguous(AppError),
    Received(AppError),
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
            format!("provider {operation} response body could not be read: {error}"),
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

async fn read_invocation_response_body(
    mut response: Response,
    operation: &str,
    max_bytes: Option<u64>,
) -> Result<Vec<u8>, InvocationResponseError> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|source| {
        InvocationResponseError::Ambiguous(
            AppError::new(
                ErrorCode::ExternalDependency,
                format!("provider {operation} response body could not be read"),
            )
            .with_source(source)
            .retryable(),
        )
    })? {
        let next_len = body.len() as u64 + chunk.len() as u64;
        if let Some(max_bytes) = max_bytes
            && next_len > max_bytes
        {
            return Err(InvocationResponseError::Received(response_too_large(
                operation, next_len, max_bytes,
            )));
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
            "provider {operation} response body exceeded {max_bytes} bytes: {actual_bytes} bytes"
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
            format!("provider {operation} response content-type was not JSON: {content_type}"),
        ),
        None => AppError::new(
            ErrorCode::ExternalDependency,
            format!("provider {operation} response content-type was missing"),
        ),
    }
}

pub(crate) fn provider_error(status: StatusCode, envelope: ProviderErrorEnvelope) -> AppError {
    let provider = envelope.error;
    let retryable =
        provider.retryable || status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS;
    let retry_after_ms = retryable.then_some(provider.retry_after_ms).flatten();
    let mut error = AppError::new(
        error_code_from_provider(&provider.code, status),
        provider.message,
    )
    .with_retry_after_ms(retry_after_ms)
    .with_provider_trace_reference(provider.provider_trace_reference);
    error.details = provider
        .details
        .into_iter()
        .map(provider_detail)
        .chain([
            ErrorDetail {
                field: Some("provider_status".to_owned()),
                reason: status.as_u16().to_string(),
            },
            ErrorDetail {
                field: Some("provider_code".to_owned()),
                reason: provider.code,
            },
        ])
        .collect();
    if retryable {
        error = error.retryable();
    }
    error
}

fn provider_detail(detail: ProviderErrorDetail) -> ErrorDetail {
    ErrorDetail {
        field: detail.field,
        reason: detail.reason,
    }
}

pub(crate) fn fallback_status_error(status: StatusCode, operation: &str) -> AppError {
    let mut error = AppError::new(
        error_code_from_status(status),
        format!("provider {operation} returned status {status}"),
    );
    error.details = vec![ErrorDetail {
        field: Some("provider_status".to_owned()),
        reason: status.as_u16().to_string(),
    }];
    if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
        error = error.retryable();
    }
    error
}

fn error_code_from_provider(code: &str, status: StatusCode) -> ErrorCode {
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
                max_bytes: Some(MAX_PROVIDER_JSON_RESPONSE_BYTES),
                require_json_content_type: true,
                allow_empty_success: false,
            },
        )
        .await
        .expect_err("chunked body must exceed the policy limit");

        assert!(error.to_string().contains("exceeded"));
    }

    #[tokio::test]
    async fn received_retryable_error_is_not_classified_as_ambiguous_response_loss() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("test server address");
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            let body = serde_json::json!({
                "error": {
                    "code": "rate_limited",
                    "message": "Provider throttled",
                    "retryable": true,
                    "retryAfterMs": 2500,
                    "providerTraceReference": "provider-trace-1",
                    "details": []
                }
            })
            .to_string();
            write!(
                stream,
                "HTTP/1.1 429 Too Many Requests\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("write response");
        });

        let response = reqwest::Client::new()
            .post(format!("http://{address}/invoke"))
            .send()
            .await
            .expect("request should receive the semantic response");
        let error = decode_invocation_response::<serde_json::Value>(
            response,
            "invocation",
            ResponseBodyPolicy {
                max_bytes: Some(MAX_PROVIDER_JSON_RESPONSE_BYTES),
                require_json_content_type: true,
                allow_empty_success: false,
            },
        )
        .await
        .expect_err("received semantic failure should fail");

        match error {
            InvocationResponseError::Received(error) => {
                assert!(error.retryable);
                assert_eq!(error.retry_after_ms, Some(2_500));
                assert_eq!(
                    error.provider_trace_reference.as_deref(),
                    Some("provider-trace-1")
                );
            }
            InvocationResponseError::Ambiguous(_) => {
                panic!("received 429 must not trigger invocation recovery")
            }
        }
    }

    #[test]
    fn http_error_envelope_preserves_provider_retry_metadata() {
        let error = provider_error(
            StatusCode::TOO_MANY_REQUESTS,
            ProviderErrorEnvelope {
                error: crate::ProviderErrorBody {
                    code: "rate_limited".to_owned(),
                    message: "Provider throttled".to_owned(),
                    retryable: true,
                    retry_after_ms: Some(2_500),
                    provider_trace_reference: Some("provider-trace-1".to_owned()),
                    details: Vec::new(),
                },
            },
        );

        assert_eq!(error.code, ErrorCode::RateLimited);
        assert!(error.retryable);
        assert_eq!(error.retry_after_ms, Some(2_500));
        assert_eq!(
            error.provider_trace_reference.as_deref(),
            Some("provider-trace-1")
        );
    }
}
