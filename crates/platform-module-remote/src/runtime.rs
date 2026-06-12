use crate::config::RemoteModuleConfig;
use crate::protocol::{RemoteFunctionInvokeRequest, RemoteFunctionInvokeResponse};
use crate::response::{ResponseBodyPolicy, decode_json_response_with_policy};
use platform_core::{AppError, AppResult, ErrorCode, ExecutionContext};
use platform_runtime::{FunctionHandlerObservability, RuntimeFunction};
use serde_json::Value;
use std::time::Duration;

const MAX_RUNTIME_FUNCTION_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct RemoteRuntimeFunction {
    client: reqwest::Client,
    config: RemoteModuleConfig,
    function_name: String,
}

impl RemoteRuntimeFunction {
    pub fn new(config: RemoteModuleConfig, function_name: impl Into<String>) -> AppResult<Self> {
        let function_name = function_name.into();
        validate_function_name(&function_name)?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to build remote runtime client: {error}"),
                )
            })?;
        Ok(Self {
            client,
            config,
            function_name,
        })
    }

    pub async fn invoke(&self, ctx: ExecutionContext, input: Value) -> AppResult<Value> {
        let request_body = RemoteFunctionInvokeRequest {
            request_id: ctx.execution_id.0.clone(),
            function_run_id: ctx.execution_id.0,
            function_name: self.function_name.clone(),
            attempt: ctx.attempt,
            correlation_id: ctx.correlation_id.0,
            causation_id: ctx.causation_id,
            actor: ctx.actor,
            trace: ctx.trace,
            input,
        };
        let mut request = self.client.post(self.invoke_url()).json(&request_body);
        if let Some(token) = &self.config.auth_token {
            request = request.bearer_auth(token);
        }

        let response = request.send().await.map_err(|error| {
            AppError::new(
                ErrorCode::ExternalDependency,
                format!(
                    "remote runtime function {} request failed: {error}",
                    self.function_name
                ),
            )
            .retryable()
        })?;

        let response = decode_json_response_with_policy::<RemoteFunctionInvokeResponse>(
            response,
            "runtime function invoke",
            false,
            ResponseBodyPolicy {
                max_bytes: Some(MAX_RUNTIME_FUNCTION_RESPONSE_BYTES),
                require_json_content_type: true,
                allow_empty_success: false,
            },
        )
        .await?
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::NotFound,
                format!("remote runtime function {} not found", self.function_name),
            )
        })?;
        Ok(response.output)
    }

    fn invoke_url(&self) -> String {
        format!(
            "{}/runtime/functions/{}/invoke",
            self.config.base_url, self.function_name
        )
    }
}

#[async_trait::async_trait]
impl RuntimeFunction for RemoteRuntimeFunction {
    async fn call(&self, ctx: ExecutionContext, input: Value) -> AppResult<Value> {
        self.invoke(ctx, input).await
    }

    fn observability(&self) -> Option<FunctionHandlerObservability> {
        Some(FunctionHandlerObservability::new(
            "remote_runtime",
            serde_json::json!({
                "module_name": &self.config.name,
                "function_name": &self.function_name,
                "remote_path": format!("/runtime/functions/{}/invoke", self.function_name),
                "timeout_ms": self.config.timeout_ms,
            }),
        ))
    }
}

pub(crate) fn validate_function_name(function_name: &str) -> AppResult<()> {
    let valid = !function_name.is_empty()
        && function_name.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || character == '.'
                || character == '_'
                || character == '-'
        });
    if valid {
        return Ok(());
    }

    Err(AppError::new(
        ErrorCode::Validation,
        "remote runtime function name must be a stable path segment",
    ))
}
