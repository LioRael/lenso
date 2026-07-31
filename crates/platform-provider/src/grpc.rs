use crate::config::ProviderConfig;
use crate::protocol::{
    ProviderDescriptor, ProviderInvocation, ProviderInvocationAcknowledgement,
    ProviderInvocationReference, ProviderOutcome,
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
) -> AppResult<ProviderOutcome> {
    let path = match binding {
        "http:invoke" => INVOKE_HTTP_ROUTE_PATH,
        "admin:list" => LIST_ADMIN_RECORDS_PATH,
        "admin:get" => GET_ADMIN_RECORD_PATH,
        "admin:query" => QUERY_ADMIN_VALUE_PATH,
        "admin:act" => INVOKE_ADMIN_ACTION_PATH,
        "runtime:invoke" => INVOKE_FUNCTION_PATH,
        "events:handle" => HANDLE_EVENT_PATH,
        _ => {
            return Err(AppError::new(
                ErrorCode::Validation,
                format!("unsupported Provider binding {binding}"),
            ));
        }
    };
    unary_json(config, path, "Provider invocation", invocation).await
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
