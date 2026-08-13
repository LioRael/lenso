#![cfg(feature = "host")]

use async_trait::async_trait;
use lenso::host::outbox::{ClaimedOutboxEvent, EventHandler};
use lenso::host::runtime::{
    AppContext, AppResult, ExecutionContext, FunctionDefinition, FunctionHandler, LinkedBinding,
    Module, RetryPolicy, RuntimeDescriptor,
};
use lenso::host::{HostComposition, HostLinkedModule, Migration};
use lenso::{
    EventHandlerDeclaration, EventSurface, ModuleManifest, RuntimeFunctionDeclaration,
    RuntimeSurface, ScheduledFunctionDeclaration,
};
use platform_core::{
    AppConfig, AuthConfig, DatabaseConfig, HttpConfig, LoggingEventPublisher, ModuleSourcesConfig,
    RedisConfig, ServiceConfig, TelemetryConfig,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

const FUNCTION_NAME: &str = "fixture.external_runtime.v1";
const EVENT_HANDLER_NAME: &str = "fixture.external_event_handler.v1";
const EVENT_NAME: &str = "fixture.external_event.v1";
const SCHEDULE_NAME: &str = "fixture-external-runtime";
const MIGRATIONS: &[Migration] = &[];

#[derive(Debug)]
struct ExternalHandler;

#[async_trait]
impl FunctionHandler for ExternalHandler {
    async fn call(&self, _ctx: ExecutionContext, input: Value) -> AppResult<Value> {
        Ok(input)
    }
}

#[derive(Debug)]
struct ExternalEventHandler;

#[async_trait]
impl EventHandler for ExternalEventHandler {
    fn handler_name(&self) -> &str {
        EVENT_HANDLER_NAME
    }

    fn event_name(&self) -> &str {
        EVENT_NAME
    }

    async fn handle(&self, _event: &ClaimedOutboxEvent) -> AppResult<()> {
        Ok(())
    }
}

fn manifest() -> ModuleManifest {
    ModuleManifest::builder("fixture/external-runtime")
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
                input_schema: Some(format!("{FUNCTION_NAME}.input")),
                retry_policy: None,
                operation: None,
            }],
            schedules: vec![ScheduledFunctionDeclaration {
                name: SCHEDULE_NAME.to_owned(),
                function_name: FUNCTION_NAME.to_owned(),
                cron: "*/5 * * * *".to_owned(),
                input: serde_json::json!({"source": "fixture"}),
            }],
            workflows: Vec::new(),
        })
        .build()
}

fn binding() -> LinkedBinding {
    LinkedBinding::builder()
        .event_handlers(vec![Arc::new(ExternalEventHandler)])
        .runtime(RuntimeDescriptor {
            module: "external-runtime",
            functions: vec![FunctionDefinition {
                name: FUNCTION_NAME.to_owned(),
                version: 1,
                queue: "fixture".to_owned(),
                retry_policy: RetryPolicy::fixed(3, Duration::from_secs(1)),
                handler: Arc::new(ExternalHandler),
            }],
            ..RuntimeDescriptor::default()
        })
        .build()
}

fn load(_context: &AppContext) -> Module {
    Module::linked(manifest(), binding())
}

fn test_context() -> AppContext {
    let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
        .expect("lazy test pool");
    AppContext::new(
        AppConfig {
            service: ServiceConfig::default(),
            database: DatabaseConfig {
                url: "postgres://localhost/lenso_test".to_owned(),
                max_connections: 1,
            },
            redis: RedisConfig::default(),
            http: HttpConfig::default(),
            telemetry: TelemetryConfig::default(),
            auth: AuthConfig::default(),
            module_sources: ModuleSourcesConfig {
                linked_profile: "core".to_owned(),
            },
            modules: BTreeMap::new(),
        },
        db,
        Arc::new(LoggingEventPublisher),
    )
}

#[tokio::test]
async fn external_crate_can_author_runtime_behavior_through_the_lenso_facade() {
    let linked = HostLinkedModule::linked("external-runtime", manifest, load, MIGRATIONS);
    let context = test_context();
    let composition = HostComposition::new().with_linked_module(linked);
    let modules = lenso_bootstrap::modules_for_config_with_composition(&context, &composition)
        .expect("host should load external linked module");
    let registry = lenso_bootstrap::try_function_registry(&modules)
        .expect("host should admit manifest-declared runtime binding");
    let event_handlers = lenso_bootstrap::try_event_handlers(&modules)
        .expect("host should admit manifest-declared Event binding");
    let schedules = lenso_bootstrap::scheduled_functions(&modules, &registry)
        .expect("host should collect manifest-declared schedule");

    assert!(registry.get(FUNCTION_NAME).is_some());
    assert_eq!(event_handlers.handler_count(EVENT_NAME), 1);
    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0].schedule_name, SCHEDULE_NAME);
    assert_eq!(schedules[0].function_name, FUNCTION_NAME);
}
