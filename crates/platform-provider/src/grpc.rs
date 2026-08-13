use crate::config::ProviderConfig;
use crate::protocol::{
    ProviderDescriptor, ProviderErrorEnvelope, ProviderInvocation,
    ProviderInvocationAcknowledgement, ProviderInvocationReference, ProviderOutcome,
};
use platform_core::{AppError, AppResult, ErrorCode};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::time::Duration;
use tonic::codegen::GrpcMethod;
use tonic::codegen::http::uri::PathAndQuery;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Endpoint};
use tonic::{Code, Request, Status};

const DESCRIBE_PROVIDER_PATH: &str = "/lenso.provider.v1.Provider/DescribeProvider";
const LIST_ADMIN_RECORDS_PATH: &str = "/lenso.provider.v1.Provider/ListAdminRecords";
const GET_ADMIN_RECORD_PATH: &str = "/lenso.provider.v1.Provider/GetAdminRecord";
const INVOKE_ADMIN_ACTION_PATH: &str = "/lenso.provider.v1.Provider/InvokeAdminAction";
const QUERY_ADMIN_VALUE_PATH: &str = "/lenso.provider.v1.Provider/QueryAdminValue";
const INVOKE_HTTP_ROUTE_PATH: &str = "/lenso.provider.v1.Provider/InvokeHttpRoute";
const INVOKE_FUNCTION_PATH: &str = "/lenso.provider.v1.Provider/InvokeRuntimeFunction";
const HANDLE_EVENT_PATH: &str = "/lenso.provider.v1.Provider/HandleEvent";
const GET_INVOCATION_PATH: &str = "/lenso.provider.v1.Provider/GetInvocation";
const ACKNOWLEDGE_INVOCATION_PATH: &str = "/lenso.provider.v1.Provider/AcknowledgeInvocation";
const MAX_GRPC_MESSAGE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, PartialEq, prost::Message)]
struct JsonEnvelope {
    // The first gRPC lane reuses stable JSON envelopes; typed proto can replace this later.
    #[prost(string, tag = "1")]
    payload_json: String,
}

pub(crate) async fn fetch_descriptor(config: &ProviderConfig) -> AppResult<ProviderDescriptor> {
    unary_json(
        config,
        DESCRIBE_PROVIDER_PATH,
        "Provider descriptor",
        &serde_json::json!({}),
    )
    .await
}

pub(crate) async fn invoke(
    config: &ProviderConfig,
    binding: &str,
    invocation: &ProviderInvocation,
) -> Result<ProviderOutcome, GrpcInvocationError> {
    let path = match binding {
        "http:invoke" => INVOKE_HTTP_ROUTE_PATH,
        "admin:list" => LIST_ADMIN_RECORDS_PATH,
        "admin:get" => GET_ADMIN_RECORD_PATH,
        "admin:query" => QUERY_ADMIN_VALUE_PATH,
        "admin:act" => INVOKE_ADMIN_ACTION_PATH,
        "runtime:invoke" => INVOKE_FUNCTION_PATH,
        "events:handle" => HANDLE_EVENT_PATH,
        _ => {
            return Err(GrpcInvocationError::Received(AppError::new(
                ErrorCode::Validation,
                format!("unsupported Provider binding {binding}"),
            )));
        }
    };
    unary_invocation_json(config, path, "Provider invocation", invocation).await
}

#[derive(Debug)]
pub(crate) enum GrpcInvocationError {
    /// The request may have reached the Provider, so GET recovery is required
    /// before the Host can decide whether to retry it.
    Ambiguous(AppError),
    /// The Host received a definitive semantic result or failed before sending.
    Received(AppError),
}

pub(crate) async fn get_invocation(
    config: &ProviderConfig,
    invocation_id: &str,
) -> AppResult<ProviderOutcome> {
    unary_json(
        config,
        GET_INVOCATION_PATH,
        "Provider invocation recovery",
        &ProviderInvocationReference {
            invocation_id: invocation_id.to_owned(),
        },
    )
    .await
}

pub(crate) async fn acknowledge_invocation(
    config: &ProviderConfig,
    acknowledgement: &ProviderInvocationAcknowledgement,
) -> AppResult<ProviderInvocationReference> {
    unary_json(
        config,
        ACKNOWLEDGE_INVOCATION_PATH,
        "Provider invocation acknowledgement",
        acknowledgement,
    )
    .await
}

async fn unary_json<TRequest, TResponse>(
    config: &ProviderConfig,
    path: &'static str,
    operation: &'static str,
    request: &TRequest,
) -> AppResult<TResponse>
where
    TRequest: Serialize,
    TResponse: DeserializeOwned,
{
    let mut client = connect(config, operation).await?;
    let mut request = Request::new(JsonEnvelope {
        payload_json: serde_json::to_string(request).map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("provider {operation} gRPC request could not be encoded: {error}"),
            )
        })?,
    });
    request.set_timeout(Duration::from_millis(config.timeout_ms));
    apply_auth(&mut request, config.auth_token.as_deref(), operation)?;

    client.ready().await.map_err(|error| {
        status_error(
            Status::unknown(format!("provider gRPC service was not ready: {}", error)),
            operation,
        )
    })?;
    request.extensions_mut().insert(GrpcMethod::new(
        "lenso.provider.v1.Provider",
        method_name(path),
    ));
    let codec = tonic_prost::ProstCodec::<JsonEnvelope, JsonEnvelope>::default();
    let response = client
        .unary(request, PathAndQuery::from_static(path), codec)
        .await
        .map_err(|status| status_error(status, operation))?
        .into_inner();

    serde_json::from_str(&response.payload_json).map_err(|error| {
        AppError::new(
            ErrorCode::ExternalDependency,
            format!("provider {operation} gRPC response was invalid JSON: {error}"),
        )
    })
}

async fn unary_invocation_json<TRequest, TResponse>(
    config: &ProviderConfig,
    path: &'static str,
    operation: &'static str,
    request: &TRequest,
) -> Result<TResponse, GrpcInvocationError>
where
    TRequest: Serialize,
    TResponse: DeserializeOwned,
{
    let mut client = connect(config, operation)
        .await
        .map_err(GrpcInvocationError::Received)?;
    let mut request = Request::new(JsonEnvelope {
        payload_json: serde_json::to_string(request)
            .map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("provider {operation} gRPC request could not be encoded: {error}"),
                )
            })
            .map_err(GrpcInvocationError::Received)?,
    });
    request.set_timeout(Duration::from_millis(config.timeout_ms));
    apply_auth(&mut request, config.auth_token.as_deref(), operation)
        .map_err(GrpcInvocationError::Received)?;

    client
        .ready()
        .await
        .map_err(|error| {
            status_error(
                Status::unknown(format!("provider gRPC service was not ready: {error}")),
                operation,
            )
        })
        .map_err(GrpcInvocationError::Received)?;
    request.extensions_mut().insert(GrpcMethod::new(
        "lenso.provider.v1.Provider",
        method_name(path),
    ));
    let codec = tonic_prost::ProstCodec::<JsonEnvelope, JsonEnvelope>::default();
    let response = client
        .unary(request, PathAndQuery::from_static(path), codec)
        .await
        .map_err(|status| invocation_status_error(status, operation))?
        .into_inner();

    serde_json::from_str(&response.payload_json).map_err(|error| {
        GrpcInvocationError::Received(AppError::new(
            ErrorCode::ExternalDependency,
            format!("provider {operation} gRPC response was invalid JSON: {error}"),
        ))
    })
}

async fn connect(
    config: &ProviderConfig,
    operation: &'static str,
) -> AppResult<tonic::client::Grpc<Channel>> {
    let timeout = Duration::from_millis(config.timeout_ms);
    let endpoint = Endpoint::new(config.base_url.clone())
        .map_err(|error| {
            AppError::new(
                ErrorCode::Validation,
                format!(
                    "provider {operation} gRPC endpoint was invalid: {} ({})",
                    config.base_url, error
                ),
            )
        })?
        .connect_timeout(timeout)
        .timeout(timeout);

    let channel = endpoint.connect().await.map_err(|error| {
        AppError::new(
            ErrorCode::ExternalDependency,
            format!("provider {operation} gRPC connection failed: {error}"),
        )
        .retryable()
    })?;

    Ok(tonic::client::Grpc::new(channel)
        .max_decoding_message_size(MAX_GRPC_MESSAGE_BYTES)
        .max_encoding_message_size(MAX_GRPC_MESSAGE_BYTES))
}

fn apply_auth<T>(
    request: &mut Request<T>,
    token: Option<&str>,
    operation: &'static str,
) -> AppResult<()> {
    let Some(token) = token else {
        return Ok(());
    };
    let value = MetadataValue::try_from(format!("Bearer {token}").as_str()).map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("provider {operation} gRPC auth metadata was invalid: {error}"),
        )
    })?;
    request.metadata_mut().insert("authorization", value);
    Ok(())
}

fn status_error(status: Status, operation: &'static str) -> AppError {
    if let Ok(envelope) = serde_json::from_str::<ProviderErrorEnvelope>(status.message()) {
        return crate::response::provider_error(http_status_from_code(status.code()), envelope);
    }
    let code = status.code();
    let mut error = AppError::new(
        error_code_from_status(code),
        format!("provider {operation} gRPC failed: {}", status.message()),
    );
    if status_is_retryable(code) {
        error = error.retryable();
    }
    error
}

fn invocation_status_error(status: Status, operation: &'static str) -> GrpcInvocationError {
    let received_provider_envelope =
        serde_json::from_str::<ProviderErrorEnvelope>(status.message()).is_ok();
    let ambiguous = !received_provider_envelope && status_is_ambiguous(status.code());
    let error = status_error(status, operation);
    if ambiguous {
        GrpcInvocationError::Ambiguous(error)
    } else {
        GrpcInvocationError::Received(error)
    }
}

fn http_status_from_code(code: Code) -> reqwest::StatusCode {
    match code {
        Code::InvalidArgument | Code::FailedPrecondition | Code::OutOfRange => {
            reqwest::StatusCode::BAD_REQUEST
        }
        Code::Unauthenticated => reqwest::StatusCode::UNAUTHORIZED,
        Code::PermissionDenied => reqwest::StatusCode::FORBIDDEN,
        Code::NotFound => reqwest::StatusCode::NOT_FOUND,
        Code::AlreadyExists | Code::Aborted => reqwest::StatusCode::CONFLICT,
        Code::ResourceExhausted => reqwest::StatusCode::TOO_MANY_REQUESTS,
        Code::Unavailable => reqwest::StatusCode::SERVICE_UNAVAILABLE,
        _ => reqwest::StatusCode::BAD_GATEWAY,
    }
}

fn error_code_from_status(code: Code) -> ErrorCode {
    match code {
        Code::InvalidArgument | Code::FailedPrecondition | Code::OutOfRange => {
            ErrorCode::Validation
        }
        Code::Unauthenticated => ErrorCode::Unauthorized,
        Code::PermissionDenied => ErrorCode::Forbidden,
        Code::NotFound => ErrorCode::NotFound,
        Code::AlreadyExists | Code::Aborted => ErrorCode::Conflict,
        Code::ResourceExhausted => ErrorCode::RateLimited,
        _ => ErrorCode::ExternalDependency,
    }
}

fn status_is_retryable(code: Code) -> bool {
    matches!(
        code,
        Code::Unavailable | Code::DeadlineExceeded | Code::ResourceExhausted | Code::Unknown
    )
}

fn status_is_ambiguous(code: Code) -> bool {
    matches!(
        code,
        Code::Cancelled
            | Code::Unknown
            | Code::DeadlineExceeded
            | Code::ResourceExhausted
            | Code::Internal
            | Code::Unavailable
            | Code::DataLoss
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grpc_error_envelope_preserves_provider_retry_metadata() {
        let status = Status::resource_exhausted(
            serde_json::json!({
                "error": {
                    "code": "rate_limited",
                    "message": "Provider throttled",
                    "retryable": true,
                    "retryAfterMs": 2500,
                    "providerTraceReference": "provider-trace-1",
                    "details": []
                }
            })
            .to_string(),
        );

        let error = status_error(status, "test");
        assert_eq!(error.code, ErrorCode::RateLimited);
        assert!(error.retryable);
        assert_eq!(error.retry_after_ms, Some(2_500));
        assert_eq!(
            error.provider_trace_reference.as_deref(),
            Some("provider-trace-1")
        );
    }

    #[test]
    fn received_retryable_error_envelope_does_not_request_get_recovery() {
        let status = Status::resource_exhausted(
            serde_json::json!({
                "error": {
                    "code": "rate_limited",
                    "message": "Provider throttled",
                    "retryable": true,
                    "retryAfterMs": 2500,
                    "providerTraceReference": "provider-trace-1",
                    "details": []
                }
            })
            .to_string(),
        );

        match invocation_status_error(status, "test") {
            GrpcInvocationError::Received(error) => {
                assert!(error.retryable);
                assert_eq!(error.retry_after_ms, Some(2_500));
            }
            GrpcInvocationError::Ambiguous(_) => {
                panic!("received Provider error envelope must not trigger GET recovery")
            }
        }
    }

    #[test]
    fn ambiguous_transport_status_requests_get_recovery() {
        match invocation_status_error(Status::unavailable("connection lost"), "test") {
            GrpcInvocationError::Ambiguous(error) => assert!(error.retryable),
            GrpcInvocationError::Received(_) => {
                panic!("ambiguous transport status must trigger GET recovery")
            }
        }
    }

    #[test]
    fn ambiguous_non_retryable_transport_status_requests_fail_closed_recovery() {
        match invocation_status_error(Status::internal("stream reset"), "test") {
            GrpcInvocationError::Ambiguous(error) => assert!(!error.retryable),
            GrpcInvocationError::Received(_) => {
                panic!("ambiguous stream reset must trigger GET recovery")
            }
        }
    }

    #[test]
    fn bare_retryable_status_without_provider_envelope_requests_get_recovery() {
        match invocation_status_error(Status::resource_exhausted("server busy"), "test") {
            GrpcInvocationError::Ambiguous(error) => assert!(error.retryable),
            GrpcInvocationError::Received(_) => {
                panic!("retryable status without a Provider envelope must trigger GET recovery")
            }
        }
    }

    #[test]
    fn decoded_grpc_response_with_invalid_json_is_received_not_ambiguous() {
        let error = serde_json::from_str::<ProviderOutcome>("not-json")
            .map_err(|source| {
                GrpcInvocationError::Received(AppError::new(
                    ErrorCode::ExternalDependency,
                    format!("provider test gRPC response was invalid JSON: {source}"),
                ))
            })
            .expect_err("invalid response JSON should fail");

        match error {
            GrpcInvocationError::Received(error) => assert!(!error.retryable),
            GrpcInvocationError::Ambiguous(_) => {
                panic!("fully received invalid JSON must not trigger GET recovery")
            }
        }
    }
}

fn method_name(path: &str) -> &'static str {
    match path {
        DESCRIBE_PROVIDER_PATH => "DescribeProvider",
        LIST_ADMIN_RECORDS_PATH => "ListAdminRecords",
        GET_ADMIN_RECORD_PATH => "GetAdminRecord",
        INVOKE_ADMIN_ACTION_PATH => "InvokeAdminAction",
        QUERY_ADMIN_VALUE_PATH => "QueryAdminValue",
        INVOKE_HTTP_ROUTE_PATH => "InvokeHttpRoute",
        INVOKE_FUNCTION_PATH => "InvokeRuntimeFunction",
        HANDLE_EVENT_PATH => "HandleEvent",
        GET_INVOCATION_PATH => "GetInvocation",
        ACKNOWLEDGE_INVOCATION_PATH => "AcknowledgeInvocation",
        _ => "Unknown",
    }
}
