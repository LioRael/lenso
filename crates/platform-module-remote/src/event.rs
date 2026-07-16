use crate::config::{RemoteModuleConfig, RemoteModuleTransport};
use crate::protocol::{
    RemoteEventHandleRequest, RemoteEventHandleResponse, RemoteEventResultAction,
};
use crate::response::{ResponseBodyPolicy, decode_json_response_with_policy};
use crate::validation::validate_path_segment;
use platform_core::{
    ActorContext, AppError, AppResult, ClaimedOutboxEvent, CorrelationId, ErrorCode, EventHandler,
    trace_context_from_headers,
};
use platform_runtime::{EnqueueFunctionRequest, FunctionRegistry, RuntimeClient};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

const MAX_EVENT_HANDLER_RESPONSE_BYTES: u64 = 1024 * 1024;
const MAX_EVENT_HANDLER_RESULT_ACTIONS: usize = 1;

#[derive(Debug, Clone)]
pub struct RemoteEventHandler {
    client: reqwest::Client,
    config: RemoteModuleConfig,
    handler_name: String,
    event_name: String,
    action_runner: Arc<dyn RemoteEventActionRunner>,
}

impl RemoteEventHandler {
    pub fn new(
        config: RemoteModuleConfig,
        handler_name: impl Into<String>,
        event_name: impl Into<String>,
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
                    format!("failed to build remote event handler client: {error}"),
                )
            })?;
        Ok(Self {
            client,
            config,
            handler_name,
            event_name,
            action_runner: Arc::new(RejectingRemoteEventActionRunner),
        })
    }

    #[must_use]
    pub fn with_host_action_runner(mut self, action_runner: RemoteEventHostActionRunner) -> Self {
        self.action_runner = Arc::new(action_runner);
        self
    }

    pub async fn invoke(&self, event: &ClaimedOutboxEvent) -> AppResult<()> {
        let request_body = RemoteEventHandleRequest {
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
        if self.config.transport == RemoteModuleTransport::Grpc {
            let response = crate::grpc::handle_event(&self.config, &request_body).await?;
            self.action_runner
                .run_actions(event, &self.handler_name, response.actions)
                .await?;
            return Ok(());
        }

        let mut request = self.client.post(self.invoke_url()).json(&request_body);
        if let Some(token) = &self.config.auth_token {
            request = request.bearer_auth(token);
        }

        let response = request.send().await.map_err(|error| {
            AppError::new(
                ErrorCode::ExternalDependency,
                format!(
                    "remote event handler {} request failed: {error}",
                    self.handler_name
                ),
            )
            .retryable()
        })?;

        let response = decode_json_response_with_policy::<RemoteEventHandleResponse>(
            response,
            "event handler invoke",
            false,
            ResponseBodyPolicy {
                max_bytes: Some(MAX_EVENT_HANDLER_RESPONSE_BYTES),
                require_json_content_type: true,
                allow_empty_success: true,
            },
        )
        .await?;
        if let Some(response) = response {
            self.action_runner
                .run_actions(event, &self.handler_name, response.actions)
                .await?;
        }
        Ok(())
    }

    fn invoke_url(&self) -> String {
        format!(
            "{}/events/handlers/{}/invoke",
            self.config.base_url, self.handler_name
        )
    }
}

#[async_trait::async_trait]
impl EventHandler for RemoteEventHandler {
    fn event_name(&self) -> &str {
        &self.event_name
    }

    async fn handle(&self, event: &ClaimedOutboxEvent) -> AppResult<()> {
        self.invoke(event).await
    }
}

#[async_trait::async_trait]
trait RemoteEventActionRunner: std::fmt::Debug + Send + Sync {
    async fn run_actions(
        &self,
        event: &ClaimedOutboxEvent,
        handler_name: &str,
        actions: Vec<RemoteEventResultAction>,
    ) -> AppResult<()>;
}

#[derive(Debug, Clone)]
pub struct RemoteEventHostActionRunner {
    runtime: RuntimeClient,
    function_registry: Arc<FunctionRegistry>,
    allowed_function_names: BTreeSet<String>,
}

impl RemoteEventHostActionRunner {
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
impl RemoteEventActionRunner for RemoteEventHostActionRunner {
    async fn run_actions(
        &self,
        event: &ClaimedOutboxEvent,
        handler_name: &str,
        actions: Vec<RemoteEventResultAction>,
    ) -> AppResult<()> {
        if actions.len() > MAX_EVENT_HANDLER_RESULT_ACTIONS {
            return Err(AppError::new(
                ErrorCode::Validation,
                format!(
                    "remote event handler {handler_name} returned too many result actions: {}",
                    actions.len()
                ),
            ));
        }

        for (index, action) in actions.into_iter().enumerate() {
            match action {
                RemoteEventResultAction::EnqueueFunction {
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

impl RemoteEventHostActionRunner {
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
                    "remote event handler {handler_name} requested runtime function {function_name} that is not declared by its module"
                ),
            ));
        }

        let definition = self.function_registry.get(&function_name).ok_or_else(|| {
            AppError::new(
                ErrorCode::Internal,
                format!("remote event handler {handler_name} requested unregistered runtime function {function_name}"),
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
                    "remote_event_handler:{}:{handler_name}:{action_index}",
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
            "remote event handler enqueued runtime function"
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
struct RejectingRemoteEventActionRunner;

#[async_trait::async_trait]
impl RemoteEventActionRunner for RejectingRemoteEventActionRunner {
    async fn run_actions(
        &self,
        _event: &ClaimedOutboxEvent,
        handler_name: &str,
        actions: Vec<RemoteEventResultAction>,
    ) -> AppResult<()> {
        if actions.is_empty() {
            return Ok(());
        }

        Err(AppError::new(
            ErrorCode::Validation,
            format!(
                "remote event handler {handler_name} returned result actions but host actions are not configured"
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
        "remote event handler name must be a stable path segment",
    )
}

pub(crate) fn validate_event_name(value: &str) -> AppResult<()> {
    validate_path_segment(value, "remote event name must be a stable path segment")
}

fn runtime_max_attempts_for_enqueue(max_attempts: u32) -> i32 {
    i32::try_from(max_attempts).unwrap_or(i32::MAX)
}
