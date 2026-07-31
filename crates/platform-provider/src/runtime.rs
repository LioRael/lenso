use crate::config::ProviderConfig;
use crate::invocation::{self, InvocationContext};
use crate::protocol::{ProviderFunctionInvokeRequest, ProviderFunctionInvokeResponse};
use crate::protocol::{ProviderInvocationMode, ProviderOperationKind};
use crate::validation::validate_path_segment;
use platform_core::{AppError, AppResult, ErrorCode, ExecutionContext};
use platform_runtime::{FunctionHandlerObservability, RuntimeFunction};
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ProviderRuntimeFunction {
    client: reqwest::Client,
    config: ProviderConfig,
    function_name: String,
}

impl ProviderRuntimeFunction {
    pub fn new(config: ProviderConfig, function_name: impl Into<String>) -> AppResult<Self> {
        let function_name = function_name.into();
        validate_function_name(&function_name)?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to build provider runtime client: {error}"),
                )
            })?;
        Ok(Self {
            client,
            config,
            function_name,
        })
    }

    pub async fn invoke(&self, ctx: ExecutionContext, input: Value) -> AppResult<Value> {
        let invocation_id = ctx.execution_id.0.clone();
        let request_body = ProviderFunctionInvokeRequest {
            request_id: ctx.execution_id.0.clone(),
            function_run_id: ctx.execution_id.0.clone(),
            function_name: self.function_name.clone(),
            attempt: ctx.attempt,
            correlation_id: ctx.correlation_id.0,
            causation_id: ctx.causation_id,
            actor: ctx.actor.clone(),
            trace: ctx.trace.clone(),
            input,
        };
        let invocation = invocation::build(
            &self.config,
            ProviderOperationKind::RuntimeFunction,
            &self.function_name,
            "1",
            ProviderInvocationMode::Durable,
            InvocationContext {
                invocation_id,
                request_id: request_body.request_id.clone(),
                attempt: request_body.attempt,
                actor: request_body.actor.clone(),
                correlation_id: request_body.correlation_id.clone(),
                causation_id: request_body.causation_id.clone(),
                trace: request_body.trace.clone(),
            },
            serde_json::to_value(request_body).map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("encode Provider payload: {error}"),
                )
            })?,
        )?;
        let outcome =
            invocation::send(&self.client, &self.config, "runtime:invoke", &invocation).await?;
        let value = invocation::result(&invocation, outcome)?;
        let response: ProviderFunctionInvokeResponse =
            serde_json::from_value(value).map_err(|error| {
                AppError::new(
                    ErrorCode::ExternalDependency,
                    format!("Provider runtime result violated its contract: {error}"),
                )
            })?;
        Ok(response.output)
    }
}

#[async_trait::async_trait]
impl RuntimeFunction for ProviderRuntimeFunction {
    async fn call(&self, ctx: ExecutionContext, input: Value) -> AppResult<Value> {
        self.invoke(ctx, input).await
    }

    fn observability(&self) -> Option<FunctionHandlerObservability> {
        Some(FunctionHandlerObservability::new(
            "provider_runtime",
            serde_json::json!({
                "module_name": &self.config.name,
                "function_name": &self.function_name,
                "provider_path": format!("/exports/{}/runtime:invoke", self.config.export_key),
                "timeout_ms": self.config.timeout_ms,
            }),
        ))
    }
}

pub(crate) fn validate_function_name(function_name: &str) -> AppResult<()> {
    validate_path_segment(
        function_name,
        "provider runtime function name must be a stable path segment",
    )
}
