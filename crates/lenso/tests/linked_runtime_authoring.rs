#![cfg(feature = "host")]

use async_trait::async_trait;
use lenso::host::runtime::{
    AppContext, AppResult, ExecutionContext, FunctionDefinition, FunctionHandler, LinkedBinding,
    Module, RetryPolicy, RuntimeDescriptor,
};
use lenso::host::{HostComposition, HostLinkedModule, Migration};
use lenso::{ModuleManifest, RuntimeFunctionDeclaration, RuntimeSurface};
use platform_core::{
    AppConfig, AuthConfig, DatabaseConfig, HttpConfig, LoggingEventPublisher, ModuleSourcesConfig,
    RedisConfig, ServiceConfig, TelemetryConfig,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

const FUNCTION_NAME: &str = "fixture.external_runtime.v1";
const MIGRATIONS: &[Migration] = &[];

#[derive(Debug)]
struct ExternalHandler;

#[async_trait]
impl FunctionHandler for ExternalHandler {
    async fn call(&self, _ctx: ExecutionContext, input: Value) -> AppResult<Value> {
        Ok(input)
    }
}

fn manifest() -> ModuleManifest {
    ModuleManifest::builder("fixture/external-runtime")
        .runtime(RuntimeSurface {
            functions: vec![RuntimeFunctionDeclaration {
                name: FUNCTION_NAME.to_owned(),
                version: 1,
                queue: "fixture".to_owned(),
                input_schema: Some(format!("{FUNCTION_NAME}.input")),
                retry_policy: None,
                operation: None,
            }],
            schedules: Vec::new(),
            workflows: Vec::new(),
        })
        .build()
}

fn binding() -> LinkedBinding {
    LinkedBinding::builder()
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

#[tokio::test]
async fn external_crate_can_author_runtime_behavior_through_the_lenso_facade() {
    let linked = HostLinkedModule::linked("external-runtime", manifest, load, MIGRATIONS);
    let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
        .expect("lazy test pool");
    let context = AppContext::new(
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
    );
    let composition = HostComposition::new().with_linked_module(linked);
    let modules = lenso_bootstrap::modules_for_config_with_composition(&context, &composition)
        .expect("host should load external linked module");
    let registry = lenso_bootstrap::try_function_registry(&modules)
        .expect("host should admit manifest-declared runtime binding");

    assert!(registry.get(FUNCTION_NAME).is_some());
}
