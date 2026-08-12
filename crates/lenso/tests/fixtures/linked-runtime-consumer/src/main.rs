use async_trait::async_trait;
use lenso::host::runtime::{
    AppContext, AppResult, ExecutionContext, FunctionDefinition, FunctionHandler, LinkedBinding,
    Module, RetryPolicy, RuntimeDescriptor,
};
use lenso::host::{HostBuilder, HostLinkedModule, Migration};
use lenso::{
    ModuleManifest, RuntimeFunctionDeclaration, RuntimeRetryPolicyDeclaration, RuntimeSurface,
};
use serde_json::Value;
use std::{sync::Arc, time::Duration};

const FUNCTION_NAME: &str = "fixture.reconcile.v1";
const MIGRATIONS: &[Migration] = &[];

#[derive(Debug)]
struct Reconcile;

#[async_trait]
impl FunctionHandler for Reconcile {
    async fn call(&self, _context: ExecutionContext, input: Value) -> AppResult<Value> {
        Ok(input)
    }
}

fn manifest() -> ModuleManifest {
    ModuleManifest::builder("fixture/runtime-consumer")
        .runtime(RuntimeSurface {
            functions: vec![RuntimeFunctionDeclaration {
                name: FUNCTION_NAME.to_owned(),
                version: 1,
                queue: "fixture".to_owned(),
                input_schema: None,
                retry_policy: Some(RuntimeRetryPolicyDeclaration {
                    max_attempts: 3,
                    initial_delay_ms: 1_000,
                }),
                operation: None,
            }],
            schedules: Vec::new(),
            workflows: Vec::new(),
        })
        .build()
}

fn load(_context: &AppContext) -> Module {
    Module::linked(
        manifest(),
        LinkedBinding::builder()
            .runtime(RuntimeDescriptor {
                module: "runtime-consumer",
                functions: vec![FunctionDefinition {
                    name: FUNCTION_NAME.to_owned(),
                    version: 1,
                    queue: "fixture".to_owned(),
                    retry_policy: RetryPolicy::fixed(3, Duration::from_secs(1)),
                    handler: Arc::new(Reconcile),
                }],
                ..RuntimeDescriptor::default()
            })
            .build(),
    )
}

fn linked_module() -> HostLinkedModule {
    HostLinkedModule::linked("runtime-consumer", manifest, load, MIGRATIONS)
}

#[tokio::main]
async fn main() {
    HostBuilder::new()
        .linked_module(linked_module())
        .run_worker_from_env()
        .await
        .expect("run Lenso worker");
}
