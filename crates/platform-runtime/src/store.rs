use async_trait::async_trait;
use platform_core::{AppResult, ExecutionContext};
use serde_json::Value;
use std::fmt::Debug;

#[async_trait]
pub trait RuntimeStore: Debug + Send + Sync {
    async fn enqueue_function(&self, ctx: ExecutionContext, input: Value) -> AppResult<()>;
}
