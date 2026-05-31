use crate::runtime::SEND_WELCOME_EMAIL;
use platform_core::{ActorContext, AppResult, ClaimedOutboxEvent, CorrelationId, EventHandler};
use platform_runtime::{EnqueueFunctionRequest, RuntimeClient};
use serde_json::{json, Value};
use std::sync::Arc;

pub const USER_REGISTERED: &str = "identity.user_registered.v1";

#[async_trait::async_trait]
pub trait RuntimeEnqueuer: std::fmt::Debug + Send + Sync {
    async fn enqueue_welcome_email(
        &self,
        event: &ClaimedOutboxEvent,
        payload: Value,
    ) -> AppResult<String>;
}

#[async_trait::async_trait]
impl RuntimeEnqueuer for RuntimeClient {
    async fn enqueue_welcome_email(
        &self,
        event: &ClaimedOutboxEvent,
        payload: Value,
    ) -> AppResult<String> {
        self.enqueue_function(EnqueueFunctionRequest {
            function_name: SEND_WELCOME_EMAIL.to_owned(),
            input_json: payload,
            correlation_id: CorrelationId::new(event.correlation_id.clone()),
            actor: actor_from_event(event),
            max_attempts: None,
        })
        .await
    }
}

#[derive(Debug, Clone)]
pub struct WelcomeEmailRequestedHandler {
    runtime: Arc<dyn RuntimeEnqueuer>,
}

impl WelcomeEmailRequestedHandler {
    pub fn new(runtime: impl RuntimeEnqueuer + 'static) -> Self {
        Self {
            runtime: Arc::new(runtime),
        }
    }
}

#[async_trait::async_trait]
impl EventHandler for WelcomeEmailRequestedHandler {
    fn event_name(&self) -> &'static str {
        USER_REGISTERED
    }

    async fn handle(&self, event: &ClaimedOutboxEvent) -> AppResult<()> {
        let payload = json!({
            "user_id": event.payload.get("user_id").cloned().unwrap_or_else(|| json!(event.aggregate_id)),
            "email": event.payload.get("email").cloned().unwrap_or(Value::Null),
            "display_name": event.payload.get("display_name").cloned().unwrap_or(Value::Null),
        });

        let function_run_id = self.runtime.enqueue_welcome_email(event, payload).await?;

        tracing::info!(
            user_id = %event.aggregate_id,
            correlation_id = %event.correlation_id,
            function_run_id = %function_run_id,
            "welcome email function enqueued"
        );
        Ok(())
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
