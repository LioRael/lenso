use axum::http::StatusCode;
use axum::{Json, Router, routing::post};
use platform_core::{
    ActorContext, CorrelationId, PLATFORM_MIGRATIONS, TraceContext, apply_migrations,
};
use platform_module_remote::{RemoteModuleConfig, RemoteRuntimeFunction};
use platform_runtime::{
    EnqueueFunctionRequest, FunctionDefinition, FunctionRegistry, RUNTIME_MIGRATIONS, RetryPolicy,
    RuntimeClient, RuntimeWorker,
};
use platform_testing::TestDatabase;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

#[tokio::test]
async fn worker_completes_remote_runtime_function() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_runtime_stack_migrations(&db).await;

    let remote = spawn_remote(runtime_success_router()).await;
    let mut registry = FunctionRegistry::default();
    registry.register(remote_definition(
        &remote,
        "remote_crm.sync_contact.v1",
        3,
        0,
    ));

    let run_id = enqueue(&db.pool, "remote_crm.sync_contact.v1", 3).await;

    let worker = RuntimeWorker::new(db.pool.clone(), Arc::new(registry), "worker-remote");
    let count = worker
        .claim_and_run_batch(10)
        .await
        .expect("remote function should run");

    assert_eq!(count, 1);
    assert_eq!(
        run_status(&db.pool, "remote_crm.sync_contact.v1").await,
        "completed"
    );
    assert!(
        execution_log_bodies(&db.pool, "remote_crm.sync_contact.v1")
            .await
            .contains(&"Function run completed".to_owned())
    );
    let operation = remote_runtime_operation_log(&db.pool, "remote_crm.sync_contact.v1").await;
    assert_eq!(operation["source"], "remote_runtime");
    assert_eq!(operation["module_name"], "remote-crm");
    assert_eq!(operation["function_name"], "remote_crm.sync_contact.v1");
    assert_eq!(operation["request_id"], run_id);
    assert_eq!(operation["success"], true);
    assert!(
        operation["duration_ms"]
            .as_i64()
            .is_some_and(|value| value >= 0)
    );

    db.cleanup().await;
}

#[tokio::test]
async fn worker_retries_remote_runtime_failure() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_runtime_stack_migrations(&db).await;

    let remote = spawn_remote(runtime_retryable_failure_router()).await;
    let mut registry = FunctionRegistry::default();
    registry.register(remote_definition(
        &remote,
        "remote_crm.sync_contact.v1",
        3,
        0,
    ));
    enqueue(&db.pool, "remote_crm.sync_contact.v1", 3).await;

    RuntimeWorker::new(db.pool.clone(), Arc::new(registry), "worker-remote")
        .claim_and_run_batch(10)
        .await
        .expect("worker should handle remote failure");

    assert_eq!(
        run_status_and_attempts(&db.pool, "remote_crm.sync_contact.v1").await,
        ("failed".to_owned(), 1)
    );

    db.cleanup().await;
}

#[tokio::test]
async fn worker_marks_remote_runtime_timeout_failed() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_runtime_stack_migrations(&db).await;

    let remote = spawn_remote(runtime_slow_router()).await;
    let mut registry = FunctionRegistry::default();
    registry.register(remote_definition_with_timeout(
        &remote,
        "remote_crm.sync_contact.v1",
        10,
    ));
    enqueue(&db.pool, "remote_crm.sync_contact.v1", 3).await;

    RuntimeWorker::new(db.pool.clone(), Arc::new(registry), "worker-remote")
        .claim_and_run_batch(10)
        .await
        .expect("worker should handle timeout");

    assert_eq!(
        run_status_and_attempts(&db.pool, "remote_crm.sync_contact.v1").await,
        ("failed".to_owned(), 1)
    );

    db.cleanup().await;
}

#[tokio::test]
async fn worker_marks_unregistered_remote_function_dead() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_runtime_stack_migrations(&db).await;
    enqueue(&db.pool, "remote_crm.missing.v1", 1).await;

    RuntimeWorker::new(
        db.pool.clone(),
        Arc::new(FunctionRegistry::default()),
        "worker-remote",
    )
    .claim_and_run_batch(10)
    .await
    .expect("worker should handle missing registration");

    assert_eq!(
        run_status_and_attempts(&db.pool, "remote_crm.missing.v1").await,
        ("dead".to_owned(), 1)
    );

    db.cleanup().await;
}

async fn spawn_remote(router: Router) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind test server");
    let address = listener.local_addr().expect("test server address");
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("test server should run");
    });
    format!("http://{address}")
}

fn runtime_success_router() -> Router {
    Router::new().route(
        "/runtime/functions/remote_crm.sync_contact.v1/invoke",
        post(runtime_success),
    )
}

fn runtime_retryable_failure_router() -> Router {
    Router::new().route(
        "/runtime/functions/remote_crm.sync_contact.v1/invoke",
        post(runtime_retryable_failure),
    )
}

fn runtime_slow_router() -> Router {
    Router::new().route(
        "/runtime/functions/remote_crm.sync_contact.v1/invoke",
        post(runtime_slow),
    )
}

async fn runtime_success(Json(body): Json<Value>) -> Json<Value> {
    Json(json!({
        "output": {
            "synced": true,
            "function_run_id": body["function_run_id"],
            "contact_id": body["input"]["contact_id"],
        }
    }))
}

async fn runtime_retryable_failure() -> (StatusCode, Json<Value>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({
            "error": {
                "code": "external_dependency_failure",
                "message": "remote CRM was unavailable",
                "retryable": true,
                "details": [{ "field": "upstream", "reason": "timeout" }]
            }
        })),
    )
}

async fn runtime_slow() -> Json<Value> {
    tokio::time::sleep(Duration::from_millis(100)).await;
    Json(json!({ "output": {} }))
}

fn remote_definition(
    base_url: &str,
    function_name: &str,
    max_attempts: u32,
    initial_delay_ms: u64,
) -> FunctionDefinition {
    FunctionDefinition {
        name: function_name.to_owned(),
        version: 1,
        queue: "remote-crm".to_owned(),
        retry_policy: RetryPolicy::fixed(max_attempts, Duration::from_millis(initial_delay_ms)),
        handler: Arc::new(
            RemoteRuntimeFunction::new(
                RemoteModuleConfig::new("remote-crm", base_url),
                function_name,
            )
            .expect("remote runtime function"),
        ),
    }
}

fn remote_definition_with_timeout(
    base_url: &str,
    function_name: &str,
    timeout_ms: u64,
) -> FunctionDefinition {
    FunctionDefinition {
        name: function_name.to_owned(),
        version: 1,
        queue: "remote-crm".to_owned(),
        retry_policy: RetryPolicy::fixed(3, Duration::ZERO),
        handler: Arc::new(
            RemoteRuntimeFunction::new(
                RemoteModuleConfig::new("remote-crm", base_url).with_timeout_ms(timeout_ms),
                function_name,
            )
            .expect("remote runtime function"),
        ),
    }
}

async fn enqueue(pool: &platform_core::DbPool, function_name: &str, max_attempts: i32) -> String {
    RuntimeClient::new(pool.clone())
        .enqueue_function(EnqueueFunctionRequest {
            function_name: function_name.to_owned(),
            input_json: json!({ "contact_id": "contact_1" }),
            correlation_id: CorrelationId::new("corr_remote_runtime_1"),
            actor: ActorContext::System,
            tenant_id: Some(platform_core::TenantId("tenant_01".to_owned())),
            tenancy_mode: platform_runtime::FunctionTenancyMode::Required,
            trace: TraceContext {
                trace_id: Some("trace_remote_runtime_1".to_owned()),
                span_id: Some("span_remote_runtime_1".to_owned()),
                baggage: Vec::new(),
            },
            causation_id: Some("httpreq_remote_runtime_1".to_owned()),
            max_attempts: Some(max_attempts),
        })
        .await
        .expect("function should enqueue")
}

async fn apply_runtime_stack_migrations(db: &TestDatabase) {
    let migrations = PLATFORM_MIGRATIONS
        .iter()
        .chain(RUNTIME_MIGRATIONS)
        .copied()
        .collect::<Vec<_>>();
    apply_migrations(&db.pool, &migrations)
        .await
        .expect("runtime migrations should apply");
}

async fn run_status(pool: &platform_core::DbPool, function_name: &str) -> String {
    sqlx::query_scalar("select status from runtime.function_runs where function_name = $1")
        .bind(function_name)
        .fetch_one(pool)
        .await
        .expect("status should query")
}

async fn run_status_and_attempts(
    pool: &platform_core::DbPool,
    function_name: &str,
) -> (String, i32) {
    sqlx::query_as("select status, attempts from runtime.function_runs where function_name = $1")
        .bind(function_name)
        .fetch_one(pool)
        .await
        .expect("status and attempts should query")
}

async fn execution_log_bodies(pool: &platform_core::DbPool, function_name: &str) -> Vec<String> {
    sqlx::query_scalar(
        r#"
        select log.body
        from platform.execution_logs log
        where log.execution_name = $1
        order by log.occurred_at asc
        "#,
    )
    .bind(function_name)
    .fetch_all(pool)
    .await
    .expect("execution logs should query")
}

async fn remote_runtime_operation_log(pool: &platform_core::DbPool, function_name: &str) -> Value {
    sqlx::query_scalar(
        r#"
        select log.attributes
        from platform.execution_logs log
        where log.execution_name = $1
            and log.attributes ->> 'source' = 'remote_runtime'
        order by log.occurred_at asc
        limit 1
        "#,
    )
    .bind(function_name)
    .fetch_one(pool)
    .await
    .expect("remote runtime operation log should query")
}
