use platform_core::{AppResult, ExecutionContext};
use platform_runtime::{
    FunctionDefinition, FunctionHandler, Queue, RetryPolicy, RuntimeDescriptor,
};
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Debug)]
struct CleanupExpiredSessions;

#[async_trait::async_trait]
impl FunctionHandler for CleanupExpiredSessions {
    async fn call(&self, _ctx: ExecutionContext, _input: Value) -> AppResult<Value> {
        Ok(json!({ "expired_sessions": 0 }))
    }
}

pub fn descriptor() -> RuntimeDescriptor {
    RuntimeDescriptor {
        module: "identity",
        queues: vec![Queue::new("identity", 1)],
        functions: vec![FunctionDefinition {
            name: "identity.cleanup_expired_sessions.v1",
            version: 1,
            queue: "identity",
            retry_policy: RetryPolicy::default(),
            handler: Arc::new(CleanupExpiredSessions),
        }],
        triggers: Vec::new(),
        flows: Vec::new(),
    }
}
