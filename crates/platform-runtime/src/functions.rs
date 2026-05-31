use crate::retries::RetryPolicy;
use async_trait::async_trait;
use platform_core::{AppResult, ExecutionContext};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::Arc;

#[async_trait]
pub trait FunctionHandler: Debug + Send + Sync {
    async fn call(&self, ctx: ExecutionContext, input: Value) -> AppResult<Value>;
}

#[derive(Debug, Clone)]
pub struct FunctionDefinition {
    pub name: &'static str,
    pub version: u16,
    pub queue: &'static str,
    pub retry_policy: RetryPolicy,
    pub handler: Arc<dyn FunctionHandler>,
}

#[derive(Debug, Default, Clone)]
pub struct FunctionRegistry {
    functions: BTreeMap<String, FunctionDefinition>,
}

impl FunctionRegistry {
    pub fn register(&mut self, function: FunctionDefinition) {
        self.functions.insert(function.name.to_owned(), function);
    }

    pub fn get(&self, name: &str) -> Option<&FunctionDefinition> {
        self.functions.get(name)
    }

    pub fn all(&self) -> impl Iterator<Item = &FunctionDefinition> {
        self.functions.values()
    }
}
