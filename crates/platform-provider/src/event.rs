use crate::ProviderHostEffectCoordinator;
use crate::config::ProviderConfig;
use crate::invocation::{self, InvocationContext};
use crate::protocol::{
    ProviderEventHandleRequest, ProviderEventHandleResponse, ProviderEventResultAction,
    ProviderInvocationMode, ProviderOperationKind,
};
use crate::validation::validate_path_segment;
use platform_core::{
    ActorContext, AppError, AppResult, ClaimedOutboxEvent, CorrelationId, ErrorCode, EventHandler,
    trace_context_from_headers,
};
use platform_runtime::{EnqueueFunctionRequest, FunctionRegistry, RuntimeClient};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

const MAX_EVENT_HANDLER_RESULT_ACTIONS: usize = 1;

#[derive(Debug, Clone)]
pub struct ProviderEventHandler {
    client: reqwest::Client,
    config: ProviderConfig,
    handler_name: String,
    event_name: String,
    action_runner: Arc<dyn ProviderEventActionRunner>,
    effects: ProviderHostEffectCoordinator,
}

impl ProviderEventHandler {
    pub fn new(
        config: ProviderConfig,
        handler_name: impl Into<String>,
        event_name: impl Into<String>,
        effects: ProviderHostEffectCoordinator,
    ) -> AppResult<Self> {
        let handler_name = handler_name.into();
        let event_name = event_name.into();
        validate_event_handler_name(&handler_name)?;
        validate_event_name(&event_name)?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to build provider event handler client: {error}"),
                )
            })?;
        Ok(Self {
            client,
            config,
            handler_name,
            event_name,
            action_runner: Arc::new(RejectingProviderEventActionRunner),
            effects,
        })
    }

    #[must_use]
    pub fn with_host_action_runner(mut self, action_runner: ProviderEventHostActionRunner) -> Self {
        self.action_runner = Arc::new(action_runner);
        self
    }

    pub async fn invoke(&self, event: &ClaimedOutboxEvent) -> AppResult<()> {
        let request_body = ProviderEventHandleRequest {
            request_id: format!("{}:{}", event.id, self.handler_name),
            outbox_event_id: event.id.clone(),
            handler_name: self.handler_name.clone(),
            event_name: event.event_name.clone(),
            event_version: event.event_version,
            source_module: event.source_module.clone(),
            aggregate_type: event.aggregate_type.clone(),
            aggregate_id: event.aggregate_id.clone(),
            correlation_id: event.correlation_id.clone(),
            causation_id: event.causation_id.clone(),
            occurred_at: event.occurred_at.to_rfc3339(),
            actor: actor_from_event(event),
            trace: trace_context_from_headers(&event.headers),
            payload: event.payload.clone(),
            headers: event.headers.clone(),
        };
        let invocation = invocation::build(
            &self.config,
            ProviderOperationKind::EventHandler,
            &self.handler_name,
            event.event_version.to_string(),
            ProviderInvocationMode::Durable,
            InvocationContext {
                invocation_id: request_body.request_id.clone(),
                request_id: request_body.request_id.clone(),
                attempt: 1,
                actor: request_body.actor.clone(),
                correlation_id: request_body.correlation_id.clone(),
                causation_id: request_body.causation_id.clone(),
                trace: request_body.trace.clone(),
            },
            serde_json::to_value(&request_body).map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("encode Provider payload: {error}"),
                )
            })?,
        )?;
        let outcome = invocation::send(
            &self.client,
            &self.config,
            &self.effects,
            "events:handle",
            &invocation,
        )
        .await?;
        let value = invocation::result(&invocation, outcome)?;
        let response: ProviderEventHandleResponse = if value.is_null() {
            ProviderEventHandleResponse::default()
        } else {
            serde_json::from_value(value).map_err(|error| {
                AppError::new(
                    ErrorCode::ExternalDependency,
                    format!("Provider event result violated its contract: {error}"),
                )
            })?
        };
        self.action_runner
            .run_actions(event, &self.handler_name, response.actions)
            .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl EventHandler for ProviderEventHandler {
    fn handler_name(&self) -> &str {
        &self.handler_name
    }

    fn event_name(&self) -> &str {
        &self.event_name
    }

    async fn handle(&self, event: &ClaimedOutboxEvent) -> AppResult<()> {
        self.invoke(event).await
    }
}

#[async_trait::async_trait]
trait ProviderEventActionRunner: std::fmt::Debug + Send + Sync {
    async fn run_actions(
        &self,
        event: &ClaimedOutboxEvent,
        handler_name: &str,
        actions: Vec<ProviderEventResultAction>,
    ) -> AppResult<()>;
}

#[derive(Debug, Clone)]
pub struct ProviderEventHostActionRunner {
    runtime: RuntimeClient,
    function_registry: Arc<FunctionRegistry>,
    allowed_function_names: BTreeSet<String>,
}

impl ProviderEventHostActionRunner {
    #[must_use]
    pub fn new(
        runtime: RuntimeClient,
        function_registry: Arc<FunctionRegistry>,
        allowed_function_names: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            runtime,
            function_registry,
            allowed_function_names: allowed_function_names.into_iter().collect(),
        }
    }
}

#[async_trait::async_trait]
impl ProviderEventActionRunner for ProviderEventHostActionRunner {
    async fn run_actions(
        &self,
        event: &ClaimedOutboxEvent,
        handler_name: &str,
        actions: Vec<ProviderEventResultAction>,
    ) -> AppResult<()> {
        if actions.len() > MAX_EVENT_HANDLER_RESULT_ACTIONS {
            return Err(AppError::new(
                ErrorCode::Validation,
                format!(
                    "provider event handler {handler_name} returned too many result actions: {}",
                    actions.len()
                ),
            ));
        }

        for (index, action) in actions.into_iter().enumerate() {
            match action {
                ProviderEventResultAction::EnqueueFunction {
                    function_name,
                    input,
                } => {
                    self.enqueue_function(event, handler_name, index, function_name, input)
                        .await?;
                }
            }
        }

        Ok(())
    }
}

impl ProviderEventHostActionRunner {
    async fn enqueue_function(
        &self,
        event: &ClaimedOutboxEvent,
        handler_name: &str,
        action_index: usize,
        function_name: String,
        input: serde_json::Value,
    ) -> AppResult<()> {
        if !self.allowed_function_names.contains(&function_name) {
            return Err(AppError::new(
                ErrorCode::Validation,
                format!(
                    "provider event handler {handler_name} requested runtime function {function_name} that is not declared by its module"
                ),
            ));
        }

        let definition = self.function_registry.get(&function_name).ok_or_else(|| {
            AppError::new(
                ErrorCode::Internal,
                format!("provider event handler {handler_name} requested unregistered runtime function {function_name}"),
            )
        })?;
        let run_id = self
            .runtime
            .enqueue_function(EnqueueFunctionRequest {
                function_name: function_name.clone(),
                input_json: input,
                correlation_id: CorrelationId::new(event.correlation_id.clone()),
                actor: actor_from_event(event),
                tenant_id: tenant_from_event(event),
                tenancy_mode: platform_runtime::FunctionTenancyMode::Optional,
                trace: trace_context_from_headers(&event.headers),
                causation_id: Some(format!(
                    "provider_event_handler:{}:{handler_name}:{action_index}",
                    event.id
                )),
                max_attempts: Some(runtime_max_attempts_for_enqueue(
                    definition.retry_policy.max_attempts,
                )),
            })
            .await?;

        tracing::info!(
            outbox_event_id = %event.id,
            handler_name = %handler_name,
            function_name = %function_name,
            function_run_id = %run_id,
            "provider event handler enqueued runtime function"
        );

        Ok(())
    }
}

fn tenant_from_event(event: &ClaimedOutboxEvent) -> Option<platform_core::TenantId> {
    event
        .headers
        .get("tenant_id")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
}

#[derive(Debug)]
struct RejectingProviderEventActionRunner;

#[async_trait::async_trait]
impl ProviderEventActionRunner for RejectingProviderEventActionRunner {
    async fn run_actions(
        &self,
        _event: &ClaimedOutboxEvent,
        handler_name: &str,
        actions: Vec<ProviderEventResultAction>,
    ) -> AppResult<()> {
        if actions.is_empty() {
            return Ok(());
        }

        Err(AppError::new(
            ErrorCode::Validation,
            format!(
                "provider event handler {handler_name} returned result actions but host actions are not configured"
            ),
        ))
    }
}

fn actor_from_event(event: &ClaimedOutboxEvent) -> ActorContext {
    event
        .headers
        .get("actor")
        .cloned()
        .and_then(|actor| serde_json::from_value(actor).ok())
        .unwrap_or_default()
}

pub(crate) fn validate_event_handler_name(value: &str) -> AppResult<()> {
    validate_path_segment(
        value,
        "provider event handler name must be a stable path segment",
    )
}

pub(crate) fn validate_event_name(value: &str) -> AppResult<()> {
    validate_path_segment(value, "provider event name must be a stable path segment")
}

fn runtime_max_attempts_for_enqueue(max_attempts: u32) -> i32 {
    i32::try_from(max_attempts).unwrap_or(i32::MAX)
}
