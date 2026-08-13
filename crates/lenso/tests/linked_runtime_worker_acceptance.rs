#![cfg(feature = "host")]

use platform_core::{
    ActorContext, CorrelationId, PLATFORM_MIGRATIONS, TenantId, TraceContext, apply_migrations,
};
use platform_runtime::{
    EnqueueFunctionRequest, FunctionTenancyMode, RUNTIME_MIGRATIONS, RuntimeClient,
    RuntimeScheduler, RuntimeWorker,
};
use platform_testing::TestDatabase;
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const FUNCTION_NAME: &str = "fixture.external_runtime.v1";
const MODULE_ID: &str = "fixture/external-runtime";
const QUEUE_NAME: &str = "fixture-maintenance";
const SCHEDULE_NAME: &str = "run-external-maintenance";
const EVENT_HANDLER_NAME: &str = "fixture.observe-item-changed.v1";
const EVENT_NAME: &str = "fixture.item-changed.v1";

mod external_fixture {
    use super::{
        EVENT_HANDLER_NAME, EVENT_NAME, FUNCTION_NAME, MODULE_ID, QUEUE_NAME, SCHEDULE_NAME,
    };
    use async_trait::async_trait;
    use lenso::host::outbox::{ClaimedOutboxEvent, EventHandler};
    use lenso::host::runtime::{
        AppError, AppResult, ErrorCode, ExecutionContext, FunctionDefinition, FunctionHandler,
        LinkedBinding, Module, RetryPolicy, RuntimeDescriptor,
    };
    use lenso::{
        EventHandlerDeclaration, EventSurface, ModuleManifest, RuntimeFunctionDeclaration,
        RuntimeRetryPolicyDeclaration, RuntimeSurface, ScheduledFunctionDeclaration,
    };
    use serde_json::{Value, json};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[derive(Debug)]
    struct RetryableHandler {
        observed: Arc<Mutex<Option<(ExecutionContext, Value)>>>,
    }

    #[async_trait]
    impl FunctionHandler for RetryableHandler {
        async fn call(&self, context: ExecutionContext, input: Value) -> AppResult<Value> {
            *self
                .observed
                .lock()
                .expect("observed execution lock should not be poisoned") = Some((context, input));
            Err(AppError::new(
                ErrorCode::ExternalDependency,
                "dependency temporarily unavailable",
            )
            .retryable())
        }
    }

    #[derive(Debug)]
    struct RetryOnceEventHandler {
        observed: Arc<Mutex<Vec<(String, Value)>>>,
    }

    #[async_trait]
    impl EventHandler for RetryOnceEventHandler {
        fn handler_name(&self) -> &str {
            EVENT_HANDLER_NAME
        }

        fn event_name(&self) -> &str {
            EVENT_NAME
        }

        async fn handle(&self, event: &ClaimedOutboxEvent) -> AppResult<()> {
            let mut observed = self
                .observed
                .lock()
                .expect("observed Event lock should not be poisoned");
            observed.push((event.id.clone(), event.payload.clone()));
            if observed.len() == 1 {
                return Err(AppError::new(
                    ErrorCode::ExternalDependency,
                    "Event dependency temporarily unavailable",
                )
                .retryable());
            }
            Ok(())
        }
    }

    pub fn module(
        observed_runtime: Arc<Mutex<Option<(ExecutionContext, Value)>>>,
        observed_events: Arc<Mutex<Vec<(String, Value)>>>,
    ) -> Module {
        let manifest = ModuleManifest::builder(MODULE_ID)
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
                    queue: QUEUE_NAME.to_owned(),
                    input_schema: Some(format!("{FUNCTION_NAME}.input")),
                    retry_policy: Some(RuntimeRetryPolicyDeclaration {
                        max_attempts: 4,
                        initial_delay_ms: 60_000,
                    }),
                    operation: None,
                }],
                schedules: vec![ScheduledFunctionDeclaration {
                    name: SCHEDULE_NAME.to_owned(),
                    function_name: FUNCTION_NAME.to_owned(),
                    cron: "* * * * *".to_owned(),
                    input: json!({ "source": "schedule" }),
                }],
                workflows: Vec::new(),
            })
            .build();
        let binding = LinkedBinding::builder()
            .event_handlers(vec![Arc::new(RetryOnceEventHandler {
                observed: observed_events,
            })])
            .runtime(RuntimeDescriptor {
                module: "external-runtime",
                functions: vec![FunctionDefinition {
                    name: FUNCTION_NAME.to_owned(),
                    version: 1,
                    queue: QUEUE_NAME.to_owned(),
                    retry_policy: RetryPolicy::fixed(4, Duration::from_secs(60)),
                    handler: Arc::new(RetryableHandler {
                        observed: observed_runtime,
                    }),
                }],
                ..RuntimeDescriptor::default()
            })
            .build();

        Module::linked(manifest, binding)
    }
}

#[tokio::test]
async fn external_scheduled_function_runs_with_host_context_and_retry_policy() {
    let Some(database) = TestDatabase::create().await else {
        return;
    };
    apply_runtime_stack_migrations(&database).await;

    let observed = Arc::new(Mutex::new(None));
    let observed_events = Arc::new(Mutex::new(Vec::new()));
    let modules = vec![external_fixture::module(
        observed.clone(),
        observed_events.clone(),
    )];
    let registry = lenso_bootstrap::try_function_registry(&modules)
        .expect("external Runtime binding should match its manifest");
    let event_handlers = lenso_bootstrap::try_event_handlers(&modules)
        .expect("external Event binding should match its manifest");
    let schedules = lenso_bootstrap::scheduled_functions(&modules, &registry)
        .expect("external schedule should be collected");

    assert_eq!(schedules.len(), 1);
    let schedule = &schedules[0];
    assert_eq!(
        schedule.schedule_key,
        format!("{MODULE_ID}:{SCHEDULE_NAME}")
    );
    assert_eq!(schedule.function_name, FUNCTION_NAME);
    assert_eq!(schedule.cron, "* * * * *");
    assert_eq!(schedule.input_json, json!({ "source": "schedule" }));
    assert_eq!(schedule.max_attempts, 4);

    let event = lenso::host::transaction::OutboxEvent {
        id: "evt_external_fixture".to_owned(),
        event_name: EVENT_NAME.to_owned(),
        event_version: 1,
        source_module: MODULE_ID.to_owned(),
        aggregate_type: "fixture_item".to_owned(),
        aggregate_id: "item_1".to_owned(),
        correlation_id: "corr_external_event".to_owned(),
        causation_id: None,
        occurred_at: chrono::Utc::now(),
        payload: json!({ "item_id": "item_1" }),
        headers: json!({}),
    };
    let mut transaction = lenso::host::transaction::LinkedTransaction::begin(&database.pool)
        .await
        .expect("Outbox transaction should begin");
    transaction
        .publish_outbox(&event)
        .await
        .expect("external Event should publish");
    transaction
        .commit()
        .await
        .expect("Outbox transaction should commit");
    let relay =
        lenso::host::outbox::OutboxRelay::new(database.pool.clone(), "external-module-acceptance");
    assert_eq!(
        relay
            .relay_once(&event_handlers, 1)
            .await
            .expect("first delivery should record the retryable failure"),
        1
    );
    tokio::time::sleep(Duration::from_millis(5_100)).await;
    assert_eq!(
        relay
            .relay_once(&event_handlers, 1)
            .await
            .expect("second delivery should succeed"),
        1
    );
    assert_eq!(
        *observed_events
            .lock()
            .expect("observed Event lock should not be poisoned"),
        vec![
            (event.id.clone(), event.payload.clone()),
            (event.id, event.payload),
        ]
    );

    let scheduler = RuntimeScheduler::new(database.pool.clone(), "acceptance-scheduler");
    assert!(
        scheduler
            .enqueue_due(&schedules)
            .await
            .expect("new schedule should initialize")
            .is_empty()
    );
    database
        .make_runtime_schedule_due(&schedule.schedule_key)
        .await;

    let run_ids = scheduler
        .enqueue_due(&schedules)
        .await
        .expect("due external schedule should enqueue");
    assert_eq!(run_ids.len(), 1);

    let worker = RuntimeWorker::new(
        database.pool.clone(),
        Arc::new(registry),
        "acceptance-worker",
    );
    assert_eq!(
        worker
            .claim_and_run_batch(1)
            .await
            .expect("Runtime worker should process the external function"),
        1
    );

    let (context, input) = observed
        .lock()
        .expect("observed execution lock should not be poisoned")
        .take()
        .expect("external handler should observe one execution");
    assert_eq!(context.execution_id.0, run_ids[0]);
    assert_eq!(context.function_name, FUNCTION_NAME);
    assert_eq!(context.attempt, 1);
    assert_eq!(context.queue, QUEUE_NAME);
    assert_eq!(
        context.causation_id.as_deref(),
        Some(format!("runtime_schedule:{}", schedule.schedule_key).as_str())
    );
    assert!(context.correlation_id.0.starts_with("corr_schedule_"));
    assert!(context.tenant_id.is_none());
    assert!(context.trace.trace_id.is_none());
    assert!(context.trace.span_id.is_none());
    match context.actor {
        ActorContext::Service { service_id, scopes } => {
            assert_eq!(service_id, "acceptance-scheduler");
            assert_eq!(scopes, vec!["runtime.functions.enqueue"]);
        }
        actor => panic!("scheduled function should run as the scheduler Service, got {actor:?}"),
    }
    assert_eq!(input["source"], "schedule");

    let traced_run_id = RuntimeClient::new(database.pool.clone())
        .enqueue_function(EnqueueFunctionRequest {
            function_name: FUNCTION_NAME.to_owned(),
            input_json: json!({ "source": "explicit-context" }),
            correlation_id: CorrelationId::new("corr_external_fixture"),
            actor: ActorContext::User {
                user_id: "usr_external_fixture".to_owned(),
                scopes: vec!["fixture:run".to_owned()],
            },
            tenant_id: Some(TenantId("tenant_external_fixture".to_owned())),
            tenancy_mode: FunctionTenancyMode::Required,
            trace: TraceContext {
                trace_id: Some("trace_external_fixture".to_owned()),
                span_id: Some("span_external_fixture".to_owned()),
                baggage: vec![("fixture".to_owned(), "linked-module".to_owned())],
            },
            causation_id: Some("evt_external_fixture".to_owned()),
            max_attempts: Some(4),
        })
        .await
        .expect("context-rich external function should enqueue");
    assert_eq!(
        worker
            .claim_and_run_batch(1)
            .await
            .expect("Runtime worker should process the context-rich run"),
        1
    );

    let (context, input) = observed
        .lock()
        .expect("observed execution lock should not be poisoned")
        .take()
        .expect("external handler should observe the context-rich execution");
    assert_eq!(context.execution_id.0, traced_run_id);
    assert_eq!(context.correlation_id.0, "corr_external_fixture");
    assert_eq!(
        context.causation_id.as_deref(),
        Some("evt_external_fixture")
    );
    assert_eq!(
        context.tenant_id.as_ref().map(|tenant| tenant.0.as_str()),
        Some("tenant_external_fixture")
    );
    assert_eq!(
        context.trace.trace_id.as_deref(),
        Some("trace_external_fixture")
    );
    assert_eq!(
        context.trace.span_id.as_deref(),
        Some("span_external_fixture")
    );
    assert_eq!(
        context.trace.baggage,
        vec![("fixture".to_owned(), "linked-module".to_owned())]
    );
    assert!(matches!(
        context.actor,
        ActorContext::User { ref user_id, ref scopes }
            if user_id == "usr_external_fixture" && scopes == &["fixture:run"]
    ));
    assert_eq!(input["source"], "explicit-context");

    database.cleanup().await;
}

async fn apply_runtime_stack_migrations(database: &TestDatabase) {
    let migrations = PLATFORM_MIGRATIONS
        .iter()
        .chain(RUNTIME_MIGRATIONS)
        .copied()
        .collect::<Vec<_>>();
    apply_migrations(&database.pool, &migrations)
        .await
        .expect("Runtime migrations should apply");
}
