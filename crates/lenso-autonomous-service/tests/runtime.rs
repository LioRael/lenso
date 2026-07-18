use axum::{body::Body, routing::post};
use http::{Request, StatusCode};
use lenso_autonomous_service::{ServiceRuntimeConfig, prepare_runtime, service_router};
use lenso_service::{
    AutonomousServiceContract, AutonomousServiceStore, AutonomousServiceWorkload,
    ServiceTenancyMode, WorkloadRole,
};
use platform_testing::TestDatabase;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;
use tower::ServiceExt as _;
use utoipa_axum::router::OpenApiRouter;

fn service() -> AutonomousServiceContract {
    service_named("support")
}

fn service_named(service_id: &str) -> AutonomousServiceContract {
    let mut service = AutonomousServiceContract::new(
        service_id,
        vec![
            AutonomousServiceWorkload::new(
                format!("{service_id}-api"),
                service_id,
                WorkloadRole::API,
            ),
            AutonomousServiceWorkload::new(
                format!("{service_id}-migrate"),
                service_id,
                WorkloadRole::MIGRATION,
            ),
            AutonomousServiceWorkload::new(
                format!("{service_id}-worker"),
                service_id,
                WorkloadRole::WORKER,
            ),
        ],
        ServiceTenancyMode::None,
        vec!["local".to_owned()],
    );
    service.stores = vec![AutonomousServiceStore::new("primary", service_id)];
    service
}

#[tokio::test]
async fn shutdown_releases_only_the_service_workers_claims() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let state = prepare_runtime(
        &service(),
        &ServiceRuntimeConfig::new("support", "primary", "support"),
        db.pool.clone(),
        platform_core::PLATFORM_MIGRATIONS,
    )
    .await
    .expect("Service migrations should apply");
    platform_core::apply_migrations(&db.pool, platform_runtime::RUNTIME_MIGRATIONS)
        .await
        .unwrap();
    sqlx::query(
        r#"
        insert into platform.outbox (
            id, event_name, event_version, source_module, aggregate_type, aggregate_id,
            correlation_id, occurred_at, payload, headers, status, locked_by, locked_at
        ) values
            ('support-event', 'ticket.opened', 1, 'tickets', 'ticket', '1', 'story-1', now(), '{}', '{}', 'processing', 'support/support-worker', now()),
            ('billing-event', 'invoice.sent', 1, 'billing', 'invoice', '2', 'story-2', now(), '{}', '{}', 'processing', 'billing/billing-worker', now())
        "#,
    )
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        insert into runtime.function_runs (
            id, function_name, input_json, correlation_id, actor, status, locked_by, locked_at
        ) values
            ('support-run', 'tickets.notify', '{}', 'story-1', '{}', 'processing', 'support/support-worker', now()),
            ('billing-run', 'billing.notify', '{}', 'story-2', '{}', 'processing', 'billing/billing-worker', now())
        "#,
    )
    .execute(&db.pool)
    .await
    .unwrap();

    lenso_autonomous_service::release_worker_claims(&state)
        .await
        .unwrap();

    let support_outbox: (String, Option<String>) =
        sqlx::query_as("select status, locked_by from platform.outbox where id = 'support-event'")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let billing_outbox: (String, Option<String>) =
        sqlx::query_as("select status, locked_by from platform.outbox where id = 'billing-event'")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let support_run: (String, Option<String>) = sqlx::query_as(
        "select status, locked_by from runtime.function_runs where id = 'support-run'",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();

    assert_eq!(support_outbox, ("pending".to_owned(), None));
    assert_eq!(support_run, ("pending".to_owned(), None));
    assert_eq!(
        billing_outbox,
        (
            "processing".to_owned(),
            Some("billing/billing-worker".to_owned())
        )
    );
    db.cleanup().await;
}

#[tokio::test]
async fn store_owner_is_rejected_before_business_migrations_run() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    prepare_runtime(
        &service_named("support"),
        &ServiceRuntimeConfig::new("support", "primary", "support"),
        db.pool.clone(),
        platform_core::PLATFORM_MIGRATIONS,
    )
    .await
    .expect("first Service should claim Store");
    let forbidden_migration = platform_core::Migration {
        name: "billing/0001_forbidden",
        sql: "create schema billing_should_not_exist;",
    };

    let error = prepare_runtime(
        &service_named("billing"),
        &ServiceRuntimeConfig::new("billing", "primary", "billing"),
        db.pool.clone(),
        &[forbidden_migration],
    )
    .await
    .expect_err("another Service must not reuse the Store");

    assert_eq!(
        error.code,
        lenso_autonomous_service::RuntimeErrorCode::StoreAlreadyOwned
    );
    let mutated: bool = sqlx::query_scalar(
        "select exists (select 1 from information_schema.schemata where schema_name = 'billing_should_not_exist')",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert!(!mutated);

    db.cleanup().await;
}

#[tokio::test]
async fn api_operation_persists_service_local_story_segment() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let state = prepare_runtime(
        &service(),
        &ServiceRuntimeConfig::new("support", "primary", "support"),
        db.pool.clone(),
        platform_core::PLATFORM_MIGRATIONS,
    )
    .await
    .expect("Service migrations should apply");
    let app = service_router(
        OpenApiRouter::new().route("/tickets", post(|| async { StatusCode::CREATED })),
        state,
    );

    let operation = app
        .clone()
        .oneshot(Request::post("/tickets").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(operation.status(), StatusCode::CREATED);
    drop(operation);

    let segment: (String, String, String, String, String, i32, i32) = sqlx::query_as(
        r#"
        select story_id, service_id, workload_id, operation, status,
               evidence_revision, attempt
        from platform.service_story_segments
        where service_id = 'support'
        "#,
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert!(!segment.0.is_empty());
    assert_eq!(segment.1, "support");
    assert_eq!(segment.2, "support-api");
    assert_eq!(segment.3, "POST /tickets");
    assert_eq!(segment.4, "succeeded");
    assert_eq!((segment.5, segment.6), (1, 1));

    drop(app);
    db.cleanup().await;
}

#[tokio::test]
async fn service_worker_runs_module_outbox_and_function_work_with_local_evidence() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let state = prepare_runtime(
        &service(),
        &ServiceRuntimeConfig::new("support", "primary", "support"),
        db.pool.clone(),
        &[],
    )
    .await
    .expect("Service runtime should prepare its owned Store");

    let mut transaction = db.pool.begin().await.unwrap();
    sqlx::query("create table if not exists support_tickets (id text primary key)")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("insert into support_tickets (id) values ('ticket-1')")
        .execute(&mut *transaction)
        .await
        .unwrap();
    platform_core::OutboxPublisher
        .publish_in_tx(
            &mut transaction,
            &platform_core::OutboxEvent {
                id: "ticket-opened-1".to_owned(),
                event_name: "ticket.opened.v1".to_owned(),
                event_version: 1,
                source_module: "tickets".to_owned(),
                aggregate_type: "ticket".to_owned(),
                aggregate_id: "ticket-1".to_owned(),
                correlation_id: "story-1".to_owned(),
                causation_id: None,
                occurred_at: chrono::Utc::now(),
                payload: serde_json::json!({"ticketId": "ticket-1"}),
                headers: serde_json::json!({}),
            },
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    let function_calls = Arc::new(AtomicUsize::new(0));
    let mut functions = platform_runtime::FunctionRegistry::default();
    functions.register(platform_runtime::FunctionDefinition {
        name: "tickets.notify.v1".to_owned(),
        version: 1,
        queue: "tickets".to_owned(),
        retry_policy: platform_runtime::RetryPolicy::default(),
        handler: Arc::new(CountingFunction(function_calls.clone())),
    });
    platform_runtime::RuntimeClient::new(db.pool.clone())
        .enqueue_function(platform_runtime::EnqueueFunctionRequest {
            function_name: "tickets.notify.v1".to_owned(),
            input_json: serde_json::json!({"ticketId": "ticket-1"}),
            correlation_id: platform_core::CorrelationId::new("story-1"),
            actor: platform_core::ActorContext::System,
            tenant_id: Some(platform_core::TenantId("tenant_01".to_owned())),
            tenancy_mode: platform_runtime::FunctionTenancyMode::Required,
            trace: platform_core::TraceContext::default(),
            causation_id: Some("ticket-opened-1".to_owned()),
            max_attempts: Some(1),
        })
        .await
        .unwrap();
    let event_calls = Arc::new(AtomicUsize::new(0));
    let mut events = platform_core::EventHandlerRegistry::new();
    events.register(Arc::new(CountingEvent(event_calls.clone())));
    let shutdown = platform_core::Shutdown::new();
    let worker_shutdown = shutdown.clone();
    let worker_state = state.clone();
    let worker = tokio::spawn(async move {
        lenso_autonomous_service::run_worker(
            worker_state,
            Arc::new(functions),
            events,
            lenso_autonomous_service::ServiceWorkerConfig {
                poll_interval: Duration::from_millis(5),
                batch_size: 10,
            },
            worker_shutdown,
        )
        .await
        .unwrap();
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    shutdown.signal();
    worker.await.unwrap();

    assert_eq!(event_calls.load(Ordering::SeqCst), 1);
    assert_eq!(function_calls.load(Ordering::SeqCst), 1);
    let outcomes: Vec<(String, String)> = sqlx::query_as(
        "select operation, status from platform.service_story_segments where workload_id = 'support-worker' order by operation",
    )
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert!(outcomes.contains(&("event ticket.opened.v1".to_owned(), "published".to_owned())));
    assert!(outcomes.contains(&(
        "function tickets.notify.v1".to_owned(),
        "completed".to_owned()
    )));
    let phase: String = sqlx::query_scalar(
        "select phase from platform.service_worker_health where service_id = 'support' and workload_id = 'support-worker'",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(phase, "stopped");
    db.cleanup().await;
}

#[derive(Debug)]
struct CountingFunction(Arc<AtomicUsize>);

#[async_trait::async_trait]
impl platform_runtime::RuntimeFunction for CountingFunction {
    async fn call(
        &self,
        _ctx: platform_core::ExecutionContext,
        _input: serde_json::Value,
    ) -> platform_core::AppResult<serde_json::Value> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(serde_json::json!({"delivered": true}))
    }
}

#[derive(Debug)]
struct CountingEvent(Arc<AtomicUsize>);

#[async_trait::async_trait]
impl platform_core::EventHandler for CountingEvent {
    fn event_name(&self) -> &str {
        "ticket.opened.v1"
    }

    async fn handle(
        &self,
        _event: &platform_core::ClaimedOutboxEvent,
    ) -> platform_core::AppResult<()> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
