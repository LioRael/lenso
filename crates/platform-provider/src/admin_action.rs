use crate::ProviderHostEffectCoordinator;
use crate::config::ProviderConfig;
use crate::invocation::{self, InvocationContext};
use crate::protocol::{
    ProviderActionInvokeResponse, ProviderAdminActionInvokeRequest, ProviderInvocationMode,
    ProviderOperationKind,
};
use platform_core::{ActorContext, AppError, AppResult, ErrorCode, TraceContext};
use platform_module::AdminActionSource;
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ProviderAdminActionSource {
    client: reqwest::Client,
    config: ProviderConfig,
    effects: ProviderHostEffectCoordinator,
}

impl ProviderAdminActionSource {
    pub fn new(config: ProviderConfig) -> AppResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to build Provider Service client: {error}"),
                )
            })?;
        Ok(Self {
            client,
            config,
            effects: ProviderHostEffectCoordinator::rejecting(),
        })
    }

    #[must_use]
    pub fn with_effect_coordinator(mut self, effects: ProviderHostEffectCoordinator) -> Self {
        self.effects = effects;
        self
    }
}

#[async_trait::async_trait]
impl AdminActionSource for ProviderAdminActionSource {
    async fn invoke(&self, action: &str, input: Value) -> AppResult<Value> {
        validate_action_name(action)?;
        let invocation_id = uuid::Uuid::now_v7().to_string();
        let invocation = invocation::build(
            &self.config,
            ProviderOperationKind::AdminAction,
            action,
            "1",
            ProviderInvocationMode::Durable,
            InvocationContext {
                request_id: invocation_id.clone(),
                invocation_id,
                attempt: 1,
                actor: ActorContext::System,
                correlation_id: uuid::Uuid::now_v7().to_string(),
                causation_id: None,
                trace: TraceContext::default(),
            },
            serde_json::to_value(ProviderAdminActionInvokeRequest {
                action: action.to_owned(),
                input,
            })
            .map_err(|error| AppError::new(ErrorCode::Internal, error.to_string()))?,
        )?;
        let outcome = invocation::send(
            &self.client,
            &self.config,
            &self.effects,
            "admin:act",
            &invocation,
        )
        .await?;
        let envelope: ProviderActionInvokeResponse =
            serde_json::from_value(invocation::result(&invocation, outcome)?).map_err(|error| {
                AppError::new(
                    ErrorCode::ExternalDependency,
                    format!("Provider admin action result violated its contract: {error}"),
                )
            })?;
        Ok(envelope.result)
    }
}

fn validate_action_name(action: &str) -> AppResult<()> {
    let valid = !action.is_empty()
        && action.chars().all(|character| {
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
        "provider admin action name must be a stable path segment",
    ))
}
