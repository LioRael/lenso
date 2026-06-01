use platform_core::{AppResult, ExecutionContext};
use platform_runtime::{
    FunctionDefinition, FunctionHandler, Queue, RetryPolicy, RuntimeDescriptor,
};
use serde_json::{Value, json};
use std::sync::Arc;

pub const SEND_WELCOME_EMAIL: &str = "notifications.send_welcome_email.v1";

#[derive(Debug)]
struct SendWelcomeEmail;

#[async_trait::async_trait]
impl FunctionHandler for SendWelcomeEmail {
    async fn call(&self, ctx: ExecutionContext, input: Value) -> AppResult<Value> {
        tracing::info!(
            function_name = %ctx.function_name,
            correlation_id = %ctx.correlation_id.0,
            user_id = input
                .get("user_id")
                .and_then(|value| value.as_str())
                .unwrap_or_default(),
            "welcome email function executed"
        );
        Ok(json!({ "email_requested": true }))
    }
}

pub fn descriptor() -> RuntimeDescriptor {
    RuntimeDescriptor {
        module: "notifications",
        queues: vec![Queue::new("notifications", 1)],
        functions: vec![FunctionDefinition {
            name: SEND_WELCOME_EMAIL,
            version: 1,
            queue: "notifications",
            retry_policy: RetryPolicy::default(),
            handler: Arc::new(SendWelcomeEmail),
        }],
        triggers: Vec::new(),
        flows: Vec::new(),
    }
}
