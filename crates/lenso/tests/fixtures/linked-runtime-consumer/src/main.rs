use async_trait::async_trait;
use lenso::host::outbox::{ClaimedOutboxEvent, EventHandler};
use lenso::host::runtime::{
    ActorContext, AppContext, AppResult, CorrelationId, ExecutionContext, ExecutionId,
    FunctionDefinition, FunctionHandler, LinkedBinding, Module, RetryPolicy, RuntimeDescriptor,
    TenantId, TraceContext,
};
use lenso::host::{HostBuilder, HostLinkedModule, Migration};
use lenso::{
    EventHandlerDeclaration, EventSurface, ModuleManifest, RuntimeFunctionDeclaration,
    RuntimeRetryPolicyDeclaration, RuntimeSurface, ScheduledFunctionDeclaration,
};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

const FUNCTION_NAME: &str = "fixture.reconcile.v1";
const SCHEDULE_NAME: &str = "fixture-reconcile-every-minute";
const EVENT_HANDLER_NAME: &str = "fixture.record-item-changed.v1";
const EVENT_NAME: &str = "fixture.item-changed.v1";
const MIGRATIONS: &[Migration] = &[];

#[derive(Debug)]
struct Reconcile;

#[async_trait]
impl FunctionHandler for Reconcile {
    async fn call(&self, context: ExecutionContext, input: Value) -> AppResult<Value> {
        let _: &ExecutionId = &context.execution_id;
        let _: &ActorContext = &context.actor;
        let _: &CorrelationId = &context.correlation_id;
        let _: &Option<TenantId> = &context.tenant_id;
        let _: &TraceContext = &context.trace;
        Ok(input)
    }
}

#[derive(Debug)]
struct RecordItemChanged;

#[async_trait]
impl EventHandler for RecordItemChanged {
    fn handler_name(&self) -> &str {
        EVENT_HANDLER_NAME
    }

    fn event_name(&self) -> &str {
        EVENT_NAME
    }

    async fn handle(&self, event: &ClaimedOutboxEvent) -> AppResult<()> {
        // Delivery is at least once. A real handler claims this stable id in
        // the same transaction as its side effect before acknowledging it.
        let _stable_delivery_id = &event.id;
        Ok(())
    }
}

fn manifest() -> ModuleManifest {
    ModuleManifest::builder("fixture/runtime-consumer")
        .events(EventSurface {
            handlers: vec![EventHandlerDeclaration {
                name: EVENT_HANDLER_NAME.to_owned(),
                event_name: EVENT_NAME.to_owned(),
                operation: None,
            }],
        })
        .runtime(RuntimeSurface {
            functions: vec![RuntimeFunctionDeclaration {
                name: FUNCTION_NAME.to_owned(),
                version: 1,
                queue: "fixture".to_owned(),
                input_schema: Some(FUNCTION_NAME.to_owned()),
                retry_policy: Some(RuntimeRetryPolicyDeclaration {
                    max_attempts: 3,
                    initial_delay_ms: 1_000,
                }),
                operation: None,
            }],
            schedules: vec![ScheduledFunctionDeclaration {
                name: SCHEDULE_NAME.to_owned(),
                function_name: FUNCTION_NAME.to_owned(),
                cron: "* * * * *".to_owned(),
                input: json!({ "reason": "scheduled" }),
            }],
            workflows: Vec::new(),
        })
        .build()
}

fn load(_context: &AppContext) -> AppResult<Module> {
    Ok(Module::linked(
        manifest(),
        LinkedBinding::builder()
            .event_handlers(vec![Arc::new(RecordItemChanged)])
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
    ))
}

fn linked_module() -> HostLinkedModule {
    HostLinkedModule::try_linked("runtime-consumer", manifest, load, MIGRATIONS)
}

#[tokio::main]
async fn main() {
    HostBuilder::new()
        .linked_module(linked_module())
        .run_worker_from_env()
        .await
        .expect("run Lenso worker");
}
