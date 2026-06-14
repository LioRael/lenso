use crate::config::RemoteModuleConfig;
use crate::protocol::RemoteEventHandleRequest;
use crate::response::{ResponseBodyPolicy, decode_json_response_with_policy};
use crate::validation::validate_path_segment;
use platform_core::{
    ActorContext, AppError, AppResult, ClaimedOutboxEvent, ErrorCode, EventHandler,
    trace_context_from_headers,
};
use serde_json::Value;
use std::time::Duration;

const MAX_EVENT_HANDLER_RESPONSE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct RemoteEventHandler {
    client: reqwest::Client,
    config: RemoteModuleConfig,
    handler_name: String,
    event_name: String,
}

impl RemoteEventHandler {
    pub fn new(
        config: RemoteModuleConfig,
        handler_name: impl Into<String>,
        event_name: impl Into<String>,
    ) -> AppResult<Self> {
        let handler_name = handler_name.into();
        let event_name = event_name.into();
        validate_path_segment(
            &handler_name,
            "remote event handler name must be a stable path segment",
        )?;
        validate_path_segment(
            &event_name,
            "remote event name must be a stable path segment",
        )?;
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
        })
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

        decode_json_response_with_policy::<Value>(
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

fn actor_from_event(event: &ClaimedOutboxEvent) -> ActorContext {
    event
        .headers
        .get("actor")
        .cloned()
        .and_then(|actor| serde_json::from_value(actor).ok())
        .unwrap_or_default()
}
